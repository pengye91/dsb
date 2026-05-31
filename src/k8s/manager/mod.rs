// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Kubernetes implementation of the SandboxManager trait.
//!
//! This module implements the [`SandboxManager`] trait using the Kubernetes API
//! via `kube-rs`. It manages sandbox Pods, Services, and Custom Resource Definitions.
//!
//! # Architecture
//!
//! The K8s backend follows this model:
//! - `create()` creates a Sandbox CRD object in K8s
//! - `start()` creates a Pod + Service from the CRD spec, returns the pod name as container_id
//! - `stop()` deletes the Pod + Service but keeps the CRD
//! - `delete()` deletes the CRD (and associated Pod + Service via owner references)

use async_trait::async_trait;
use k8s_openapi::api::core::v1::{PersistentVolumeClaim, Pod, Service};
use kube::api::{Api, AttachParams, DeleteParams, ListParams, PostParams};
use kube::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::Config;
use crate::core::types::{ImageDetails, ImageSummary};
use crate::core::manager::{
    ExecCommandResult, ManagerError, ManagerResult, SandboxManager, TerminalStream,
};
use crate::core::types::{ContainerStats, KubernetesInfo, SandboxConfig, SandboxInfo};
use crate::k8s::crd::Sandbox;
use crate::k8s::types::{labels, sandbox_ports, sandbox_resource_name, sandbox_service_name};

mod builders;
mod exec;
mod helpers;

#[cfg(test)]
mod tests;

pub use exec::K8sTerminalStream;
use helpers::phase_to_state;
use exec::RemoteExec;

/// Kubernetes backend for DSB sandbox management.
pub struct KubernetesManager {
    /// kube-rs client for K8s API calls.
    client: Client,
    /// DSB configuration.
    config: Arc<Config>,
    /// K8s namespace for sandbox resources.
    namespace: String,
}
#[async_trait]
impl SandboxManager for KubernetesManager {
    /// Creates a new sandbox by creating a Sandbox CRD object in K8s.
    ///
    /// The CRD stores the desired state; `start()` will create the actual Pod and Service.
    async fn create(
        &self,
        sandbox_id: Option<&Uuid>,
        config: &SandboxConfig,
    ) -> ManagerResult<String> {
        let id = sandbox_id
            .map(|u| u.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let crd_name = sandbox_resource_name(&id);

        info!(sandbox_id = %id, crd_name = %crd_name, "Creating sandbox CRD");

        // Build SandboxSpec from SandboxConfig
        let spec = self.build_sandbox_spec(&id, config);

        // Create Sandbox CRD
        let sandbox = Sandbox::new(&crd_name, spec);
        let api: Api<Sandbox> = Api::namespaced(self.client.clone(), &self.namespace);

        api.create(&PostParams::default(), &sandbox)
            .await
            .map_err(|e| self.map_kube_error(e, "create_crd"))?;

        info!(sandbox_id = %id, crd_name = %crd_name, "Sandbox CRD created");
        Ok(id)
    }

    /// Starts a sandbox by delegating Pod/Service creation to the operator.
    ///
    /// The CRD is already created by `create()`. We just wait for the operator
    /// to reconcile and the Pod to become ready, then update the CRD status.
    async fn start(&self, id: &str) -> ManagerResult<()> {
        let crd_name = sandbox_resource_name(id);

        info!(sandbox_id = %id, crd_name = %crd_name, "Starting sandbox via operator reconciliation");

        // The operator will create the Pod and Service automatically since the CRD was created.
        // We just need to wait for the pod to be ready.
        self.wait_for_pod_ready(&crd_name).await?;

        // Fetch CRD spec to determine whether tool_proxy is expected.
        // - Custom commands override the default entrypoint, so tool_proxy won't run.
        // - Plain images (no DSB features) don't have supervisord/tool_proxy at all.
        // Only wait for tool_proxy health when features were requested, indicating the
        // image is expected to run supervisord with tool_proxy on :8080.
        let api: Api<Sandbox> = Api::namespaced(self.client.clone(), &self.namespace);
        let sandbox = api.get(&crd_name).await.map_err(|e| {
            ManagerError::Api(format!("Failed to get CRD {} for tool_proxy check: {}", crd_name, e))
        })?;
        let has_custom_command = sandbox
            .spec
            .command
            .as_ref()
            .map(|c| !c.is_empty())
            .unwrap_or(false);
        let has_dsb_features = sandbox.spec.has_dsb_features
            || sandbox.spec.image == self.config.docker.default_image;

        if !has_custom_command && has_dsb_features {
            // Pod Ready can flip true before tool_proxy listens; wait for HTTP health so tool calls work.
            self.wait_for_sandbox_tool_proxy(id).await?;
        } else {
            let reason = if has_custom_command {
                "custom command provided"
            } else {
                "no DSB features requested (plain image)"
            };
            info!(sandbox_id = %id, reason = reason, "Skipping tool_proxy health check");
        }

        // Update CRD status
        self.update_status(&crd_name, "Running").await?;

        info!(sandbox_id = %id, crd_name = %crd_name, "Sandbox started");
        Ok(())
    }

    /// Stops a sandbox by delegating to the operator (via CRD state update) or manual deletion.
    ///
    /// Updates the CRD status to "Stopped". The operator will observe this and
    /// delete the Pod and Service.
    async fn stop(&self, id: &str) -> ManagerResult<()> {
        let crd_name = sandbox_resource_name(id);
        let pod_name = sandbox_resource_name(id);
        let svc_name = sandbox_service_name(id);

        info!(sandbox_id = %id, crd_name = %crd_name, "Stopping sandbox");

        // Update CRD status to Stopped. The operator will handle deleting the resources.
        self.update_status(&crd_name, "Stopped").await?;

        // Also delete Pod manually just in case the operator is slow or misses it
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);
        match pods.delete(&pod_name, &DeleteParams::default()).await {
            Ok(_) => debug!(pod_name = %pod_name, "Pod deleted"),
            Err(e) => {
                // Ignore 404 - pod may already be gone
                if let kube::Error::Api(api_err) = &e {
                    if api_err.code != 404 {
                        warn!(pod_name = %pod_name, error = %e, "Failed to delete pod");
                    }
                }
            }
        }

        // Delete Service manually
        let services: Api<Service> = Api::namespaced(self.client.clone(), &self.namespace);
        match services.delete(&svc_name, &DeleteParams::default()).await {
            Ok(_) => debug!(svc_name = %svc_name, "Service deleted"),
            Err(e) => {
                if let kube::Error::Api(api_err) = &e {
                    if api_err.code != 404 {
                        warn!(svc_name = %svc_name, error = %e, "Failed to delete service");
                    }
                }
            }
        }

        info!(sandbox_id = %id, crd_name = %crd_name, "Sandbox stopped");
        Ok(())
    }

    /// Deletes a sandbox by removing the CRD and all associated resources.
    ///
    /// Explicitly deletes the Pod and Service before removing the CRD.
    async fn delete(&self, id: &str) -> ManagerResult<()> {
        let crd_name = sandbox_resource_name(id);

        info!(sandbox_id = %id, crd_name = %crd_name, "Deleting sandbox");

        // Delete Pod explicitly (owner refs should cascade, but be explicit)
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);
        let _ = pods.delete(&crd_name, &DeleteParams::default()).await;

        // Delete Service explicitly
        let services: Api<Service> = Api::namespaced(self.client.clone(), &self.namespace);
        let svc_name = sandbox_service_name(id);
        let _ = services.delete(&svc_name, &DeleteParams::default()).await;

        // Delete CRD
        let api: Api<Sandbox> = Api::namespaced(self.client.clone(), &self.namespace);
        api.delete(&crd_name, &DeleteParams::default())
            .await
            .map_err(|e| self.map_kube_error(e, "delete_crd"))?;

        info!(sandbox_id = %id, crd_name = %crd_name, "Sandbox deleted");
        Ok(())
    }

    /// Executes a command within a running sandbox pod and captures the output.
    ///
    /// Uses the kube-rs exec API without TTY to capture stdout and stderr.
    async fn exec(&self, id: &str, cmd: Vec<String>) -> ManagerResult<String> {
        let result = RemoteExec::new(self.client.clone(), self.namespace.clone())
            .exec_in_pod(&sandbox_resource_name(id), cmd, None, None)
            .await?;
        Ok(result.output)
    }

    /// Retrieves resource usage statistics for a sandbox pod.
    ///
    /// Uses exec to read cgroup memory and CPU stats from the pod's filesystem.
    /// Falls back to zeroed defaults if the stats cannot be read (e.g., cgroup
    /// paths differ or exec fails).
    ///
    /// TODO: Integrate with the K8s metrics API (metrics.k8s.io/PodMetrics) for
    /// more accurate and efficient stats collection when a metrics-server is
    /// available in the cluster.
    async fn stats(&self, id: &str) -> ManagerResult<ContainerStats> {
        let pod_name = sandbox_resource_name(id);

        // Read memory usage via cgroup v2 (preferred) or cgroup v1
        let mem_usage_output = self
            .exec(
                id,
                vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    "cat /sys/fs/cgroup/memory.current 2>/dev/null || cat /sys/fs/cgroup/memory/memory.usage_in_bytes 2>/dev/null || echo 0"
                        .to_string(),
                ],
            )
            .await
            .unwrap_or_else(|_| "0".to_string());

        let mem_limit_output = self
            .exec(
                id,
                vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    "cat /sys/fs/cgroup/memory.max 2>/dev/null || cat /sys/fs/cgroup/memory/memory.limit_in_bytes 2>/dev/null || echo 0"
                        .to_string(),
                ],
            )
            .await
            .unwrap_or_else(|_| "0".to_string());

        // Parse memory values (output may contain trailing newline or "max" for unlimited)
        let mem_usage_bytes: u64 = mem_usage_output
            .trim()
            .trim_end_matches('\n')
            .parse()
            .unwrap_or(0);
        let mem_limit_bytes: u64 = if mem_limit_output.trim() == "max" {
            0u64 // unlimited
        } else {
            mem_limit_output
                .trim()
                .trim_end_matches('\n')
                .parse()
                .unwrap_or(0)
        };

        let memory_usage_mb = mem_usage_bytes / (1024 * 1024);
        let memory_limit_mb = if mem_limit_bytes > 0 {
            mem_limit_bytes / (1024 * 1024)
        } else {
            0
        };
        let memory_percent = if mem_limit_bytes > 0 && mem_usage_bytes > 0 {
            (mem_usage_bytes as f64 / mem_limit_bytes as f64) * 100.0
        } else {
            0.0
        };

        // For CPU percent, read cgroup v2 cpu.stat or v1 cpuacct.usage.
        // A single snapshot is not sufficient for an accurate CPU percentage,
        // so we report 0.0 for now. A proper implementation would take two
        // samples and compute the delta.
        let cpu_percent = 0.0f64;

        debug!(
            pod_name = %pod_name,
            memory_usage_mb = memory_usage_mb,
            memory_limit_mb = memory_limit_mb,
            "Collected pod resource stats"
        );

        Ok(ContainerStats {
            cpu_percent,
            memory_usage_mb,
            memory_limit_mb,
            memory_percent,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
            block_read_bytes: 0,
            block_write_bytes: 0,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Checks if a sandbox is currently running by inspecting pod phase.
    async fn is_running(&self, id: &str) -> ManagerResult<bool> {
        let pod_name = sandbox_resource_name(id);
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        let pod = pods
            .get(&pod_name)
            .await
            .map_err(|e| self.map_kube_error(e, "is_running"))?;

        let running = pod
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|phase| phase == "Running")
            .unwrap_or(false);

        Ok(running)
    }

    /// Gets the exit information for a sandbox (exit code and OOM killed status).
    ///
    /// Reads the pod's container status to determine if the container terminated
    /// and whether it was OOM killed.
    async fn get_exit_info(&self, id: &str) -> ManagerResult<(i64, bool)> {
        let pod_name = sandbox_resource_name(id);
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        let pod = pods
            .get(&pod_name)
            .await
            .map_err(|e| self.map_kube_error(e, "get_exit_info"))?;

        if let Some(status) = &pod.status {
            if let Some(container_statuses) = &status.container_statuses {
                if let Some(cs) = container_statuses.first() {
                    if let Some(state) = &cs.state {
                        if let Some(terminated) = &state.terminated {
                            let exit_code = terminated.exit_code as i64;
                            let oom_killed = terminated.reason.as_deref() == Some("OOMKilled");
                            return Ok((exit_code, oom_killed));
                        }
                    }
                }
            }
        }

        // Pod not yet terminated or no status available
        Ok((0, false))
    }

    /// Gets the working directory of a sandbox.
    ///
    /// For K8s sandboxes, the working directory is always `/workspace`.
    async fn get_workdir(&self, _id: &str) -> ManagerResult<String> {
        Ok("/workspace".to_string())
    }

    /// Lists all sandboxes managed by DSB.
    ///
    /// Uses label selector `dsb.io/managed-by=dsb` to find DSB-managed pods.
    async fn list(
        &self,
        all: bool,
        _filters: Option<HashMap<String, Vec<String>>>,
    ) -> ManagerResult<Vec<SandboxInfo>> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        let lp = ListParams::default().labels(&format!("{}=dsb", labels::MANAGED_BY));
        let pod_list = pods
            .list(&lp)
            .await
            .map_err(|e| self.map_kube_error(e, "list"))?;

        let mut result = Vec::new();
        for pod in pod_list {
            let phase = pod
                .status
                .as_ref()
                .and_then(|s| s.phase.clone())
                .unwrap_or_else(|| "Unknown".to_string());

            // Filter out non-running pods unless `all` is true
            if !all && phase != "Running" {
                continue;
            }

            let sandbox_id = pod
                .metadata
                .labels
                .as_ref()
                .and_then(|l| l.get(labels::SANDBOX_ID).cloned())
                .unwrap_or_default();

            let image = pod
                .spec
                .as_ref()
                .and_then(|s| s.containers.first())
                .and_then(|c| c.image.clone());

            let created = pod
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|t| t.0.timestamp());

            let state = phase_to_state(&phase);
            let status = phase.clone();

            // Extract K8s-specific info from pod status and spec
            let pod_ip = pod.status.as_ref().and_then(|s| s.pod_ip.clone());
            let node_name = pod.spec.as_ref().and_then(|s| s.node_name.clone());

            // Convert BTreeMap labels to HashMap for SandboxInfo
            let labels_map: HashMap<String, String> = pod
                .metadata
                .labels
                .unwrap_or_default()
                .into_iter()
                .collect();

            result.push(SandboxInfo {
                id: sandbox_id,
                name: pod.metadata.name.clone(),
                image,
                state: Some(state),
                status: Some(status),
                created,
                ports: Vec::new(),
                labels: labels_map,
                node_name,
                pod_ip,
            });
        }

        Ok(result)
    }

    /// Removes a PersistentVolumeClaim by name.
    ///
    /// Deletes the PVC from the configured K8s namespace. This will also
    /// reclaim the underlying storage depending on the PVC's reclaim policy.
    async fn remove_volume(&self, name: &str) -> ManagerResult<()> {
        let pvcs: Api<PersistentVolumeClaim> =
            Api::namespaced(self.client.clone(), &self.namespace);

        pvcs.delete(name, &DeleteParams::default())
            .await
            .map_err(|e| self.map_kube_error(e, "delete_pvc"))?;

        info!(pvc_name = name, "PVC deleted");
        Ok(())
    }

    /// Gets detailed information about a specific image.
    ///
    /// Image feature detection requires Docker inspect. On K8s, images are
    /// managed by the pre-pull DaemonSet and cannot be inspected at runtime.
    async fn get_image_features(&self, _image: &str) -> ManagerResult<ImageDetails> {
        Err(ManagerError::NotSupported(
            "Image feature detection is not supported on Kubernetes backend. Use Docker backend for feature detection.".to_string(),
        ))
    }

    /// Lists all available images in the backend.
    ///
    /// On K8s, node images cannot be easily listed. Returns an empty list.
    /// Dashboard/image management features are Docker-specific.
    async fn list_images(&self) -> ManagerResult<Vec<ImageSummary>> {
        Ok(vec![])
    }

    /// Pulls an image from a remote registry.
    ///
    /// Not supported on K8s. Images must be pre-pulled via DaemonSet.
    async fn pull_image(&self, image: &str) -> ManagerResult<()> {
        Err(ManagerError::NotSupported(
            format!("Image pulling is not supported on Kubernetes. Images must be pre-pulled via DaemonSet. Image: {}", image),
        ))
    }

    /// Pulls an image with a progress callback.
    ///
    /// Not supported on K8s. Images must be pre-pulled via DaemonSet.
    async fn pull_image_with_progress(
        &self,
        image: &str,
        _callback: Box<dyn FnMut(String, Option<u64>, Option<u64>) + Send + 'static>,
    ) -> ManagerResult<()> {
        Err(ManagerError::NotSupported(
            format!("Image pulling is not supported on Kubernetes. Images must be pre-pulled via DaemonSet. Image: {}", image),
        ))
    }

    /// Deletes an image from the backend.
    ///
    /// Not supported on K8s. Image lifecycle is managed by the DaemonSet, not at runtime.
    async fn delete_image(&self, id: &str) -> ManagerResult<()> {
        Err(ManagerError::NotSupported(
            format!("Image deletion is not supported on Kubernetes. Image lifecycle is managed by DaemonSet. Image: {}", id),
        ))
    }

    /// Checks if an image exists locally.
    ///
    /// On K8s, images are pre-pulled via DaemonSet. Assume they exist.
    async fn image_exists(&self, _image: &str) -> ManagerResult<bool> {
        Ok(true)
    }

    /// Executes an HTTP request within a running sandbox and returns the JSON response.
    ///
    /// Makes a simple HTTP call to the Service ClusterIP.
    async fn exec_http(
        &self,
        id: &str,
        path: &str,
        method: &str,
        body: Option<Value>,
        timeout_secs: Option<u64>,
    ) -> ManagerResult<Value> {
        let svc_name = sandbox_service_name(id);
        let namespace = &self.namespace;
        // Use the same FQDN pattern as `get_sandbox_address` so tool-proxy HTTP matches
        // VNC/terminal DNS behavior and avoids ambiguous resolver handling of `svc.ns` names.
        let url = format!(
            "http://{}.{}.svc.cluster.local:{}{}",
            svc_name,
            namespace,
            sandbox_ports::TOOL_PROXY,
            path
        );

        let per_attempt_timeout = timeout_secs
            .map(std::time::Duration::from_secs)
            .unwrap_or_else(|| std::time::Duration::from_secs(30));

        let http_client = reqwest::Client::builder()
            .timeout(per_attempt_timeout)
            .build()
            .map_err(|e| ManagerError::Api(format!("Failed to create HTTP client: {}", e)))?;

        // Transient errors before an HTTP response is received (DNS, RST, kube-proxy delay, etc.).
        fn is_transient_sandbox_http_error(msg: &str) -> bool {
            let m = msg.to_lowercase();
            m.contains("error sending request")
                || m.contains("connection refused")
                || m.contains("connection reset")
                || m.contains("broken pipe")
                || m.contains("timed out")
                || m.contains("timeout")
                || m.contains("dns")
                || m.contains("hyper::")
        }

        // kube-proxy / EndpointSlice programming can lag pod Ready; widen retries with backoff.
        const MAX_ATTEMPTS: u32 = 12u32;
        const INITIAL_BACKOFF_MS: u64 = 100;

        for attempt in 1..=MAX_ATTEMPTS {
            let request = match method {
                "POST" => http_client.post(&url).json(&body).send(),
                "PUT" => http_client.put(&url).json(&body).send(),
                "PATCH" => http_client.patch(&url).json(&body).send(),
                "DELETE" => http_client.delete(&url).send(),
                _ => http_client.get(&url).send(),
            };

            let response = match request.await {
                Ok(r) => r,
                Err(e) => {
                    let msg = e.to_string();
                    if attempt < MAX_ATTEMPTS && is_transient_sandbox_http_error(&msg) {
                        let backoff_ms =
                            (INITIAL_BACKOFF_MS * (1u64 << (attempt - 1).min(5))).min(2000);
                        warn!(
                            sandbox_id = %id,
                            url = %url,
                            attempt,
                            max_attempts = MAX_ATTEMPTS,
                            error = %msg,
                            backoff_ms,
                            "Transient HTTP error to sandbox tool_proxy, retrying"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                        continue;
                    }
                    return Err(ManagerError::Api(format!(
                        "HTTP request to sandbox failed: {}",
                        msg
                    )));
                }
            };

            let status = response.status();
            let body_text = response.text().await.map_err(|e| {
                ManagerError::Api(format!("Failed to read sandbox HTTP body: {}", e))
            })?;

            if !status.is_success() {
                let snippet: String = body_text.chars().take(500).collect();
                return Err(ManagerError::Api(format!(
                    "HTTP request to sandbox returned {}: {}",
                    status, snippet
                )));
            }

            let value: Value = serde_json::from_str(&body_text).map_err(|e| {
                ManagerError::Api(format!(
                    "Failed to parse sandbox HTTP response as JSON: {} (body starts: {})",
                    e,
                    body_text.chars().take(200).collect::<String>()
                ))
            })?;

            return Ok(value);
        }

        unreachable!("exec_http retry loop always returns inside");
    }

    /// Executes a command with stdin input within a running sandbox pod.
    ///
    /// Uses the kube-rs exec API without TTY, writing stdin data before
    /// capturing stdout and stderr.
    async fn exec_with_stdin(
        &self,
        id: &str,
        cmd: Vec<String>,
        stdin: Option<String>,
        timeout_secs: Option<u64>,
    ) -> ManagerResult<String> {
        let result = RemoteExec::new(self.client.clone(), self.namespace.clone())
            .exec_in_pod(
                &sandbox_resource_name(id),
                cmd,
                stdin.map(|s| s.into_bytes()),
                timeout_secs,
            )
            .await?;
        Ok(result.output)
    }

    /// Executes a command with stdin input and returns both output and exit code.
    ///
    /// Overrides the default implementation to capture the real exit code
    /// from the K8s pod exec process status.
    async fn exec_with_stdin_result(
        &self,
        id: &str,
        cmd: Vec<String>,
        stdin: Option<String>,
        timeout_secs: Option<u64>,
    ) -> ManagerResult<ExecCommandResult> {
        RemoteExec::new(self.client.clone(), self.namespace.clone())
            .exec_in_pod(
                &sandbox_resource_name(id),
                cmd,
                stdin.map(|s| s.into_bytes()),
                timeout_secs,
            )
            .await
    }

    /// Opens an interactive terminal session with a running sandbox pod.
    ///
    /// Creates a TTY-enabled exec session and returns a `K8sTerminalStream`
    /// that implements the `TerminalStream` trait.
    async fn exec_terminal(
        &self,
        id: &str,
        shell: Option<String>,
    ) -> ManagerResult<Box<dyn TerminalStream + Send>> {
        let pod_name = sandbox_resource_name(id);
        let shell_cmd = shell.unwrap_or_else(|| "bash".to_string());

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        // Verify pod exists and is running
        let pod = pods.get(&pod_name).await.map_err(|e| {
            if let kube::Error::Api(api_err) = &e {
                if api_err.code == 404 {
                    return ManagerError::NotFound(format!("Pod: {}", pod_name));
                }
            }
            ManagerError::Api(format!("Failed to get pod: {}", e))
        })?;

        let running = pod
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|phase| phase == "Running")
            .unwrap_or(false);

        if !running {
            return Err(ManagerError::OperationFailed(format!(
                "Pod {} is not running",
                pod_name
            )));
        }

        // Use AttachParams with TTY enabled for interactive terminal
        // Note: stderr must be false when tty is true
        let ap = AttachParams::interactive_tty().container("sandbox".to_string());

        let mut attached = pods
            .exec(&pod_name, vec![shell_cmd], &ap)
            .await
            .map_err(|e| {
                ManagerError::OperationFailed(format!("Failed to exec terminal: {}", e))
            })?;

        let stdin_writer = attached.stdin().ok_or_else(|| {
            ManagerError::OperationFailed("Failed to get stdin stream".to_string())
        })?;
        let stdout_reader = attached.stdout().ok_or_else(|| {
            ManagerError::OperationFailed("Failed to get stdout stream".to_string())
        })?;
        let terminal_size_tx = attached.terminal_size().ok_or_else(|| {
            ManagerError::OperationFailed("Failed to get terminal resize channel".to_string())
        })?;

        // We need to keep the AttachedProcess alive for the streams to work.
        // Spawn a task that holds it and waits for completion.
        tokio::spawn(async move {
            let _ = attached.join().await;
        });

        Ok(Box::new(K8sTerminalStream {
            stdin: Box::new(stdin_writer),
            stdout: Box::new(stdout_reader),
            terminal_size_tx: Some(terminal_size_tx),
        }))
    }

    /// Uploads a tar archive to a sandbox pod at the specified path.
    ///
    /// K8s has no equivalent to Docker's PUT /containers/{id}/archive endpoint,
    /// so we base64-encode the tar data and pipe it through exec using:
    /// `echo '<base64>' | base64 -d | tar -xf - -C <path>`.
    ///
    /// This approach works for reasonably sized archives. For very large files,
    /// consider using `kubectl cp` or a sidecar container with an HTTP endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if the path contains potentially dangerous characters
    /// (path traversal or shell injection patterns).
    async fn upload_archive(&self, id: &str, path: &str, tar_data: Vec<u8>) -> ManagerResult<()> {
        // Sanitize the path to prevent shell injection and path traversal
        if path.contains("..") || path.contains(';') || path.contains('&') || path.contains('|') {
            return Err(ManagerError::OperationFailed(
                "Invalid path: contains forbidden characters".to_string(),
            ));
        }

        let pod_name = sandbox_resource_name(id);

        debug!(
            pod_name = %pod_name,
            path = path,
            tar_size = tar_data.len(),
            "Uploading archive via raw binary stdin"
        );

        // Send raw tar bytes through stdin. kube-rs exec places command arguments
        // in the URI query string (~8KB limit), but stdin is streamed over the
        // WebSocket with no practical limit. Using raw binary avoids the 33%
        // base64 overhead and potential encoding-related buffer issues.
        let cmd = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("sudo tar -xf - -C '{}'", path),
        ];

        let tar_size = tar_data.len();
        let result = RemoteExec::new(self.client.clone(), self.namespace.clone())
            .exec_in_pod(&pod_name, cmd, Some(tar_data), Some(120))
            .await?;

        if result.exit_code != 0 {
            debug!(
                pod_name = %pod_name,
                exit_code = result.exit_code,
                output = %result.output,
                "Tar archive extraction failed"
            );
            return Err(ManagerError::OperationFailed(format!(
                "Failed to extract tar archive in pod {}: exit code {}, output: {}",
                pod_name, result.exit_code, result.output
            )));
        }

        info!(
            pod_name = %pod_name,
            path = path,
            size = tar_size,
            "Archive uploaded successfully"
        );

        Ok(())
    }

    /// Returns the network address (host:port) for accessing a sandbox on a specific port.
    ///
    /// For Kubernetes, returns the full Service DNS name:
    /// `dsb-svc-{sandbox-id}.{namespace}.svc.cluster.local:{port}`
    async fn get_sandbox_address(&self, id: &str, port: u16) -> ManagerResult<String> {
        let svc_name = sandbox_service_name(id);
        let namespace = &self.namespace;
        Ok(format!(
            "{}.{}.svc.cluster.local:{}",
            svc_name, namespace, port
        ))
    }

    /// Gets Kubernetes-specific status information for a sandbox.
    ///
    /// Fetches the Sandbox CRD and extracts node_name, pod_ip, service_name,
    /// and message from the CRD status.
    async fn get_sandbox_k8s_status(
        &self,
        sandbox_id: &Uuid,
    ) -> ManagerResult<Option<KubernetesInfo>> {
        let crd_name = sandbox_resource_name(&sandbox_id.to_string());
        let api: Api<Sandbox> = Api::namespaced(self.client.clone(), &self.namespace);

        let sandbox = match api.get(&crd_name).await {
            Ok(s) => s,
            Err(e) => {
                // If 404, sandbox doesn't exist or CRD not created yet
                if let kube::Error::Api(api_err) = &e {
                    if api_err.code == 404 {
                        return Ok(None);
                    }
                }
                return Err(self.map_kube_error(e, "get_sandbox_k8s_status"));
            }
        };

        // Extract status from the CRD
        let status = match &sandbox.status {
            Some(s) => s,
            None => return Ok(None),
        };

        // Only return info if we have some K8s-specific data
        if status.node_name.is_none()
            && status.pod_ip.is_none()
            && status.service_name.is_none()
            && status.message.is_none()
        {
            return Ok(None);
        }

        Ok(Some(KubernetesInfo {
            node_name: status.node_name.clone(),
            pod_ip: status.pod_ip.clone(),
            service_name: status.service_name.clone(),
            message: status.message.clone(),
        }))
    }
}
