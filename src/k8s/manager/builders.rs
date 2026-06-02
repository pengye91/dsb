// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie

use k8s_openapi::api::core::v1::{
    Affinity, Container, ContainerPort, EnvVar, HTTPGetAction, NodeAffinity,
    NodeSelectorRequirement, NodeSelectorTerm, Pod, PodSecurityContext, PodSpec,
    PreferredSchedulingTerm, Probe, ResourceRequirements, Service, ServiceSpec, Toleration, Volume,
    VolumeMount,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{Api, ObjectMeta, Patch, PatchParams, Resource};
use kube::Client;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::core::manager::{ManagerError, ManagerResult};
use crate::core::types::SandboxConfig;
use crate::k8s::crd::{PortSpec, Sandbox, SandboxSpec, SandboxStatus};
use crate::k8s::types::{labels, sandbox_ports, sandbox_resource_name, sandbox_service_name};

use super::helpers::{
    augment_no_proxy_for_kubernetes_cluster, make_service_port, merge_sandbox_environment,
};
use super::KubernetesManager;
impl KubernetesManager {
    /// Creates a new KubernetesManager instance.
    ///
    /// # Arguments
    ///
    /// * `client` - A kube-rs Client for interacting with the Kubernetes API.
    /// * `config` - Shared DSB configuration reference.
    pub fn new(client: Client, config: Arc<Config>) -> Self {
        let namespace = config.sandbox.kubernetes.namespace.clone();
        Self {
            client,
            config,
            namespace,
        }
    }

    /// Returns a reference to the kube client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Returns a reference to the namespace.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Builds a SandboxSpec from a SandboxConfig for CRD creation.
    pub(super) fn build_sandbox_spec(
        &self,
        sandbox_id: &str,
        config: &SandboxConfig,
    ) -> SandboxSpec {
        let mut merged_env = merge_sandbox_environment(config, &self.config.docker.proxy_env);
        augment_no_proxy_for_kubernetes_cluster(&mut merged_env);

        // Build ports from SandboxConfig port_mappings
        let ports: Vec<PortSpec> = config
            .port_mappings
            .iter()
            .map(|pm| PortSpec {
                container_port: pm.container_port,
                protocol: match pm.protocol {
                    crate::core::types::PortProtocol::Tcp => "TCP".to_string(),
                    crate::core::types::PortProtocol::Udp => "UDP".to_string(),
                },
                service_port: Some(pm.host_port),
            })
            .collect();

        // Build resource spec from SandboxConfig resource_limits
        let resources = {
            let rl = &config.resource_limits;
            let has_any =
                rl.memory_mb.is_some() || rl.cpu_quota.is_some() || rl.cpu_shares.is_some();
            if has_any {
                Some(crate::k8s::crd::ResourceSpec {
                    cpu_request: rl.cpu_shares.map(|_| "500m".to_string()),
                    memory_request: rl.memory_mb.map(|m| format!("{}Mi", m)),
                    cpu_limit: rl.cpu_quota.map(|q| format!("{}m", q / 100)),
                    memory_limit: rl.memory_mb.map(|m| format!("{}Mi", m)),
                })
            } else {
                None
            }
        };

        // Build volumes — always add shared static files PVC when PVC name is configured
        let mut volumes = Vec::new();
        if !self.config.sandbox.kubernetes.pvc_name.is_empty() {
            volumes.push(crate::k8s::crd::VolumeSpec {
                name: "static-files".to_string(),
                mount_path: "/public".to_string(),
                volume_type: "pvc".to_string(),
                source_name: Some(self.config.sandbox.kubernetes.pvc_name.clone()),
                read_only: false,
            });
        }

        // Determine whether DSB features were requested. On K8s, image inspection is not
        // supported so we cannot detect features from image labels. Instead, we rely on the
        // user's explicit feature request. When features are enabled, the image is expected
        // to run supervisord with tool_proxy, so the backend should wait for the health check.
        // When no features are requested, the image may be a plain image (e.g., ubuntu:22.04)
        // that does not run tool_proxy, and we should skip the health check to avoid a timeout.
        let has_dsb_features = config.enable_all_features
            || config
                .features
                .iter()
                .any(|f| matches!(f.as_str(), "browser" | "web" | "databend" | "vnc" | "ssh"));

        SandboxSpec {
            image: config.image.clone(),
            env: merged_env,
            ports,
            volumes,
            resources,
            command: config.command.clone(),
            args: None,
            labels: HashMap::new(),
            gpu: false,
            inactivity_timeout_minutes: config.inactivity_timeout_minutes,
            sandbox_id: sandbox_id.to_string(),
            api_key_hash: None,
            has_dsb_features,
        }
    }

    /// Builds a Pod from a SandboxSpec and the owning CRD's UID.
    pub fn build_pod(&self, crd_name: &str, crd_uid: &str, spec: &SandboxSpec) -> Pod {
        let sandbox_id = &spec.sandbox_id;

        // Add standard DSB ports
        let mut all_ports: Vec<ContainerPort> = vec![
            ContainerPort {
                container_port: sandbox_ports::TOOL_PROXY as i32,
                name: Some("tool-proxy".to_string()),
                ..Default::default()
            },
            ContainerPort {
                container_port: sandbox_ports::VNC as i32,
                name: Some("vnc".to_string()),
                ..Default::default()
            },
            ContainerPort {
                container_port: sandbox_ports::NOVNC as i32,
                name: Some("novnc".to_string()),
                ..Default::default()
            },
            ContainerPort {
                container_port: sandbox_ports::AGENT_BROWSER as i32,
                name: Some("agent-browser".to_string()),
                ..Default::default()
            },
        ];

        // Add custom ports from spec
        let custom_ports: Vec<ContainerPort> = spec
            .ports
            .iter()
            .map(|p| ContainerPort {
                container_port: p.container_port as i32,
                protocol: Some(p.protocol.clone()),
                ..Default::default()
            })
            .collect();
        all_ports.extend(custom_ports);

        // Build environment variables
        let env_vars: Vec<EnvVar> = spec
            .env
            .iter()
            .map(|(k, v)| EnvVar {
                name: k.clone(),
                value: Some(v.clone()),
                ..Default::default()
            })
            .collect();

        // Build resource requirements
        let k8s_resources = self.build_resource_requirements(spec);

        // Build labels as BTreeMap (required by k8s_openapi)
        let mut pod_labels = BTreeMap::new();
        pod_labels.insert(labels::MANAGED_BY.to_string(), "dsb".to_string());
        pod_labels.insert(labels::SANDBOX_ID.to_string(), sandbox_id.clone());
        pod_labels.insert(labels::COMPONENT.to_string(), "sandbox-pod".to_string());
        // Network policy labels can be added via spec.labels if needed
        // Merge custom labels from spec
        for (k, v) in &spec.labels {
            pod_labels.insert(k.clone(), v.clone());
        }

        // Build owner reference pointing to the Sandbox CRD
        let owner_refs = vec![OwnerReference {
            api_version: Sandbox::api_version(&()).to_string(),
            kind: Sandbox::kind(&()).to_string(),
            name: crd_name.to_string(),
            uid: crd_uid.to_string(),
            controller: Some(true),
            block_owner_deletion: Some(true),
        }];

        // When the default image entrypoint runs tool_proxy on :8080, gate Pod Ready on HTTP /health
        // so Services get endpoints before DSB sends the first tool POST (avoids transient connect errors).
        // Only configure the readiness probe when DSB features are expected (tool_proxy will be running).
        // Plain images (e.g., ubuntu:22.04) don't run tool_proxy, so a readiness probe against :8080
        // would never pass and would cause wait_for_pod_ready to time out.
        // On K8s, image labels are not inspectable, so also match against the configured default image.
        let custom_entrypoint = spec
            .command
            .as_ref()
            .map(|c| !c.is_empty())
            .unwrap_or(false);
        let is_dsb_image_for_probe =
            spec.has_dsb_features || spec.image == self.config.docker.default_image;
        let readiness_probe = if !custom_entrypoint && is_dsb_image_for_probe {
            Some(Probe {
                http_get: Some(HTTPGetAction {
                    path: Some("/health".to_string()),
                    port: IntOrString::Int(i32::from(sandbox_ports::TOOL_PROXY)),
                    scheme: Some("HTTP".to_string()),
                    ..Default::default()
                }),
                initial_delay_seconds: Some(3),
                period_seconds: Some(2),
                timeout_seconds: Some(2),
                failure_threshold: Some(30),
                ..Default::default()
            })
        } else {
            None
        };

        let command = if spec.command.as_ref().is_none_or(|c| c.is_empty()) {
            // Determine whether this image is expected to run DSB tooling
            // (supervisord + tool_proxy).  On K8s we cannot inspect image labels,
            // so we rely on either explicit feature flags OR a match against the
            // configured default sandbox image.
            let is_dsb_image =
                spec.has_dsb_features || spec.image == self.config.docker.default_image;

            if is_dsb_image {
                // DSB-aware images (e.g., dsb/sandbox) run supervisord as their
                // entrypoint which starts tool_proxy and other services.  Let the
                // image's default ENTRYPOINT run — do NOT override with sleep.
                None
            } else {
                // Plain images (no features, no custom command) have entrypoints
                // that exit immediately (e.g., ubuntu's /bin/bash exits without a
                // TTY).  Inject a keep-alive so the pod stays Running for exec.
                Some(vec!["sleep".to_string(), "infinity".to_string()])
            }
        } else {
            spec.command.clone()
        };

        let container = Container {
            name: "sandbox".to_string(),
            image: Some(spec.image.clone()),
            image_pull_policy: Some("Always".to_string()),
            command,
            args: spec.args.clone(),
            ports: Some(all_ports),
            env: Some(env_vars),
            resources: Some(k8s_resources),
            readiness_probe,
            ..Default::default()
        };

        let mut pod_spec = PodSpec {
            containers: vec![container],
            restart_policy: Some("Never".to_string()),
            security_context: Some(PodSecurityContext {
                fs_group: Some(1000), // Ensures mounted volumes (like EFS) are writable by sandbox user
                ..Default::default()
            }),
            ..Default::default()
        };

        // Mount volumes from spec
        if !spec.volumes.is_empty() {
            let mut volumes = Vec::new();
            let mut volume_mounts = Vec::new();

            for vol in &spec.volumes {
                let vol_name = vol.name.clone();
                let mount_path = vol.mount_path.clone();

                let volume = match vol.volume_type.as_str() {
                    "pvc" => {
                        let claim_name =
                            vol.source_name.clone().unwrap_or_else(|| vol.name.clone());
                        Volume {
                            name: vol_name.clone(),
                            persistent_volume_claim: Some(
                                k8s_openapi::api::core::v1::PersistentVolumeClaimVolumeSource {
                                    claim_name,
                                    read_only: Some(vol.read_only),
                                },
                            ),
                            ..Default::default()
                        }
                    }
                    "configmap" => {
                        let source_name =
                            vol.source_name.clone().unwrap_or_else(|| vol.name.clone());
                        Volume {
                            name: vol_name.clone(),
                            config_map: Some(k8s_openapi::api::core::v1::ConfigMapVolumeSource {
                                name: source_name,
                                ..Default::default()
                            }),
                            ..Default::default()
                        }
                    }
                    "secret" => {
                        let source_name =
                            vol.source_name.clone().unwrap_or_else(|| vol.name.clone());
                        Volume {
                            name: vol_name.clone(),
                            secret: Some(k8s_openapi::api::core::v1::SecretVolumeSource {
                                secret_name: Some(source_name),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }
                    }
                    "emptydir" => Volume {
                        name: vol_name.clone(),
                        empty_dir: Some(k8s_openapi::api::core::v1::EmptyDirVolumeSource::default()),
                        ..Default::default()
                    },
                    _ => {
                        warn!(
                            volume_type = %vol.volume_type,
                            "Unknown volume type, skipping"
                        );
                        continue;
                    }
                };

                volumes.push(volume);
                volume_mounts.push(VolumeMount {
                    name: vol_name,
                    mount_path,
                    read_only: Some(vol.read_only),
                    ..Default::default()
                });
            }

            if !volumes.is_empty() {
                pod_spec.volumes = Some(volumes);
                if let Some(first_container) = pod_spec.containers.first_mut() {
                    first_container.volume_mounts = Some(volume_mounts);
                }
            }
        }

        // Apply node selector from config
        if !self.config.sandbox.kubernetes.node_selector.is_empty() {
            pod_spec.node_selector = Some(
                self.config
                    .sandbox
                    .kubernetes
                    .node_selector
                    .clone()
                    .into_iter()
                    .collect(),
            );
        }

        // Apply tolerations from config
        if !self.config.sandbox.kubernetes.tolerations.is_empty() {
            let mut tolerations = Vec::new();
            for toleration_json in &self.config.sandbox.kubernetes.tolerations {
                match serde_json::from_value(toleration_json.clone()) {
                    Ok(t) => tolerations.push(t),
                    Err(e) => warn!(
                        error = %e,
                        "Failed to parse toleration from config, skipping"
                    ),
                }
            }
            pod_spec.tolerations = Some(tolerations);
        }

        // GPU: add nvidia.com/gpu resource request, limit, node affinity, and tolerations
        if spec.gpu {
            let gpu_config = &self.config.sandbox.kubernetes.gpu;

            // Add GPU resource request AND limit so scheduler pre-reserves the GPU
            if let Some(container) = pod_spec.containers.first_mut() {
                let mut existing = container.resources.take().unwrap_or_default();
                let gpu_qty = Quantity(gpu_config.resource_request.clone());

                // Set GPU in requests (so scheduler knows what to reserve)
                let requests = existing.requests.get_or_insert_with(BTreeMap::new);
                requests.insert("nvidia.com/gpu".to_string(), gpu_qty.clone());

                // Set GPU in limits (so container can't exceed allocated GPU)
                let limits = existing.limits.get_or_insert_with(BTreeMap::new);
                limits.insert("nvidia.com/gpu".to_string(), gpu_qty);

                container.resources = Some(existing);
            }

            // Add GPU tolerations: first the default nvidia.com/gpu toleration,
            // then any custom tolerations from config
            let mut all_gpu_tolerations: Vec<Toleration> = Vec::new();

            // Default GPU toleration for nvidia.com/gpu:NoSchedule
            all_gpu_tolerations.push(Toleration {
                key: Some("nvidia.com/gpu".to_string()),
                operator: Some("Exists".to_string()),
                value: None,
                effect: Some("NoSchedule".to_string()),
                ..Default::default()
            });

            // Add custom GPU tolerations from config
            for toleration_json in &gpu_config.tolerations {
                match serde_json::from_value(toleration_json.clone()) {
                    Ok(t) => all_gpu_tolerations.push(t),
                    Err(e) => warn!(
                        error = %e,
                        "Failed to parse GPU toleration from config, skipping"
                    ),
                }
            }

            // Merge with existing tolerations
            if let Some(ref mut tols) = pod_spec.tolerations {
                tols.extend(all_gpu_tolerations);
            } else {
                pod_spec.tolerations = Some(all_gpu_tolerations);
            }

            // Add GPU node affinity to prefer scheduling on GPU nodes
            if !gpu_config.node_selector.is_empty() {
                let mut node_selector_terms = Vec::new();
                for (key, value) in &gpu_config.node_selector {
                    node_selector_terms.push(NodeSelectorRequirement {
                        key: key.clone(),
                        operator: "In".to_string(),
                        values: Some(vec![value.clone()]),
                    });
                }

                let gpu_affinity = Affinity {
                    node_affinity: Some(NodeAffinity {
                        preferred_during_scheduling_ignored_during_execution: Some(vec![
                            PreferredSchedulingTerm {
                                weight: 100,
                                preference: NodeSelectorTerm {
                                    match_expressions: Some(node_selector_terms),
                                    ..Default::default()
                                },
                            },
                        ]),
                        ..Default::default()
                    }),
                    ..Default::default()
                };

                // Merge with existing affinity if present
                if let Some(ref mut aff) = pod_spec.affinity {
                    // Merge node affinities
                    if let Some(ref mut existing_node_aff) = aff.node_affinity {
                        if let Some(ref mut existing_pref) =
                            existing_node_aff.preferred_during_scheduling_ignored_during_execution
                        {
                            if let Some(gpu_pref) =
                                gpu_affinity.node_affinity.as_ref().and_then(|na| {
                                    na.preferred_during_scheduling_ignored_during_execution
                                        .clone()
                                })
                            {
                                existing_pref.extend(gpu_pref);
                            }
                        }
                    } else {
                        aff.node_affinity = gpu_affinity.node_affinity.clone();
                    }
                } else {
                    pod_spec.affinity = Some(gpu_affinity);
                }
            }
        }

        Pod {
            metadata: ObjectMeta {
                name: Some(crd_name.to_string()),
                namespace: Some(self.namespace.clone()),
                labels: Some(pod_labels),
                owner_references: Some(owner_refs),
                ..Default::default()
            },
            spec: Some(pod_spec),
            status: None,
        }
    }

    /// Builds a Service from a SandboxSpec and the owning CRD's UID.
    pub fn build_service(&self, crd_name: &str, crd_uid: &str, spec: &SandboxSpec) -> Service {
        let sandbox_id = &spec.sandbox_id;
        let svc_name = sandbox_service_name(sandbox_id);

        // Build service ports for the standard DSB ports
        let service_ports = vec![
            make_service_port("tool-proxy", sandbox_ports::TOOL_PROXY),
            make_service_port("vnc", sandbox_ports::VNC),
            make_service_port("novnc", sandbox_ports::NOVNC),
            make_service_port("agent-browser", sandbox_ports::AGENT_BROWSER),
        ];

        // Build selector matching pod labels as BTreeMap
        let mut selector = BTreeMap::new();
        selector.insert(labels::MANAGED_BY.to_string(), "dsb".to_string());
        selector.insert(labels::SANDBOX_ID.to_string(), sandbox_id.clone());
        selector.insert(labels::COMPONENT.to_string(), "sandbox-pod".to_string());

        // Build owner reference
        let owner_refs = vec![OwnerReference {
            api_version: Sandbox::api_version(&()).to_string(),
            kind: Sandbox::kind(&()).to_string(),
            name: crd_name.to_string(),
            uid: crd_uid.to_string(),
            controller: Some(true),
            block_owner_deletion: Some(true),
        }];

        Service {
            metadata: ObjectMeta {
                name: Some(svc_name),
                namespace: Some(self.namespace.clone()),
                owner_references: Some(owner_refs),
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                selector: Some(selector),
                ports: Some(service_ports),
                ..Default::default()
            }),
            status: None,
        }
    }

    /// Builds K8s ResourceRequirements from a SandboxSpec.
    pub(super) fn build_resource_requirements(&self, spec: &SandboxSpec) -> ResourceRequirements {
        let resource_defaults = &self.config.sandbox.kubernetes.resource_defaults;

        let cpu_request = spec
            .resources
            .as_ref()
            .and_then(|r| r.cpu_request.clone())
            .unwrap_or_else(|| resource_defaults.cpu_request.clone());
        let memory_request = spec
            .resources
            .as_ref()
            .and_then(|r| r.memory_request.clone())
            .unwrap_or_else(|| resource_defaults.memory_request.clone());
        let cpu_limit = spec
            .resources
            .as_ref()
            .and_then(|r| r.cpu_limit.clone())
            .unwrap_or_else(|| resource_defaults.cpu_limit.clone());
        let memory_limit = spec
            .resources
            .as_ref()
            .and_then(|r| r.memory_limit.clone())
            .unwrap_or_else(|| resource_defaults.memory_limit.clone());

        let mut requests = BTreeMap::new();
        requests.insert("cpu".to_string(), Quantity(cpu_request));
        requests.insert("memory".to_string(), Quantity(memory_request));

        let mut limits = BTreeMap::new();
        limits.insert("cpu".to_string(), Quantity(cpu_limit));
        limits.insert("memory".to_string(), Quantity(memory_limit));

        ResourceRequirements {
            requests: Some(requests),
            limits: Some(limits),
            claims: None,
        }
    }

    /// Waits for a Pod to become ready.
    ///
    /// Polls pod status every 2 seconds until the pod is ready or a timeout is reached.
    pub(super) async fn wait_for_pod_ready(&self, pod_name: &str) -> ManagerResult<()> {
        let timeout_secs = self.config.sandbox.kubernetes.pod_ready_timeout_secs;
        let check_interval = std::time::Duration::from_secs(2);
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        loop {
            let elapsed = start.elapsed();
            if elapsed > timeout {
                let detail = match pods.get(pod_name).await {
                    Ok(pod) => Self::format_pod_scheduling_diagnostic(&pod),
                    Err(e) => format!("(could not re-fetch pod: {e})"),
                };
                return Err(ManagerError::Timeout(format!(
                    "Pod {pod_name} not ready within {timeout_secs} seconds. {detail}",
                )));
            }

            let pod = match pods.get(pod_name).await {
                Ok(pod) => pod,
                Err(e) => {
                    // If pod doesn't exist yet, wait for operator to create it
                    if let kube::Error::Api(ae) = &e {
                        if ae.code == 404 {
                            debug!(
                                pod_name = pod_name,
                                elapsed_secs = elapsed.as_secs(),
                                "Pod not found yet, waiting for operator to create it"
                            );
                            tokio::time::sleep(check_interval).await;
                            continue;
                        }
                    }
                    return Err(ManagerError::Api(format!(
                        "Failed to get pod status: {}",
                        e
                    )));
                }
            };

            if let Some(status) = &pod.status {
                // Check for failed states first
                if let Some(container_statuses) = &status.container_statuses {
                    if let Some(cs) = container_statuses.first() {
                        if let Some(state) = &cs.state {
                            if let Some(wait) = &state.waiting {
                                let reason = wait.reason.as_deref().unwrap_or("Unknown");
                                if reason == "ImagePullBackOff"
                                    || reason == "ErrImagePull"
                                    || reason == "CrashLoopBackOff"
                                {
                                    return Err(ManagerError::OperationFailed(format!(
                                        "Pod {} failed: {} - {}",
                                        pod_name,
                                        reason,
                                        wait.message.as_deref().unwrap_or("no message")
                                    )));
                                }
                            }
                            // Check if terminated
                            if let Some(terminated) = &state.terminated {
                                if terminated.exit_code != 0 {
                                    return Err(ManagerError::OperationFailed(format!(
                                        "Pod {} terminated with exit code {}",
                                        pod_name, terminated.exit_code
                                    )));
                                }
                                // Pod finished successfully (exit_code == 0) —
                                // e.g. a short-lived command like `echo hello`.
                                // The pod won't become Ready, so return Ok.
                                info!(
                                    pod_name = pod_name,
                                    exit_code = 0,
                                    "Pod terminated successfully (short-lived command), marking ready"
                                );
                                return Ok(());
                            }
                        }
                    }
                }

                // Check if all containers are ready
                let ready = status
                    .conditions
                    .as_ref()
                    .and_then(|conditions| {
                        conditions
                            .iter()
                            .find(|c| c.type_ == "Ready" && c.status == "True")
                    })
                    .is_some();

                if ready {
                    info!(pod_name = pod_name, "Pod is ready");
                    return Ok(());
                }
            }

            debug!(
                pod_name = pod_name,
                elapsed_secs = elapsed.as_secs(),
                "Waiting for pod to be ready"
            );
            tokio::time::sleep(check_interval).await;
        }
    }

    /// Waits until the sandbox `tool_proxy` HTTP server accepts `GET /health`.
    ///
    /// Kubernetes can mark the Pod Ready before `tool_proxy` binds to :8080, which makes the
    /// first `POST /exec` fail with transient connection errors and pushes clients toward exec
    /// fallbacks that may not work in all clusters.
    ///
    /// Uses a single-attempt HTTP check (no retry storm) so that sandboxes without
    /// tool_proxy (custom commands) fail fast instead of hanging for minutes.
    pub(super) async fn wait_for_sandbox_tool_proxy(&self, sandbox_id: &str) -> ManagerResult<()> {
        const MAX_ATTEMPTS: u32 = 30;
        const SLEEP_MS: u64 = 500;
        let url = format!(
            "http://{}.{}.svc.cluster.local:{}/health",
            sandbox_service_name(sandbox_id),
            self.namespace,
            sandbox_ports::TOOL_PROXY
        );

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .map_err(|e| ManagerError::Api(format!("Failed to create HTTP client: {}", e)))?;

        for attempt in 1..=MAX_ATTEMPTS {
            let healthy = match http_client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(body) => body.get("status").and_then(|v| v.as_str()) == Some("healthy"),
                        Err(_) => false,
                    }
                }
                Ok(_) => false,
                Err(err) => {
                    debug!(
                        sandbox_id = %sandbox_id,
                        attempt,
                        error = %err,
                        "Tool proxy not reachable over HTTP yet, retrying"
                    );
                    false
                }
            };

            if healthy {
                if attempt > 1 {
                    info!(
                        sandbox_id = %sandbox_id,
                        attempts = attempt,
                        "Sandbox tool_proxy HTTP is ready"
                    );
                }
                return Ok(());
            }

            if attempt == MAX_ATTEMPTS {
                return Err(ManagerError::Timeout(format!(
                    "Sandbox {sandbox_id} tool_proxy did not respond healthy on GET /health within {} attempts (~{} ms)",
                    MAX_ATTEMPTS,
                    u64::from(MAX_ATTEMPTS) * SLEEP_MS
                )));
            }
            tokio::time::sleep(std::time::Duration::from_millis(SLEEP_MS)).await;
        }

        unreachable!("MAX_ATTEMPTS is a positive constant");
    }

    /// Snapshot of Pod phase, conditions, and container waiting state for timeout errors.
    pub(super) fn format_pod_scheduling_diagnostic(pod: &Pod) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(st) = &pod.status {
            if let Some(phase) = &st.phase {
                parts.push(format!("phase={phase}"));
            }
            if let Some(conds) = &st.conditions {
                for c in conds {
                    parts.push(format!(
                        "cond {}={} reason={:?} msg={:?}",
                        c.type_, c.status, c.reason, c.message
                    ));
                }
            }
            if let Some(css) = &st.container_statuses {
                for cs in css {
                    if let Some(state) = &cs.state {
                        if let Some(w) = &state.waiting {
                            parts.push(format!(
                                "container {:?} waiting {:?}: {:?}",
                                cs.name, w.reason, w.message
                            ));
                        }
                    }
                }
            }
        }
        if parts.is_empty() {
            "(no pod.status yet)".to_string()
        } else {
            parts.join("; ")
        }
    }

    /// Updates the status subresource of a Sandbox CRD.
    pub(super) async fn update_status(&self, crd_name: &str, phase: &str) -> ManagerResult<()> {
        let api: Api<Sandbox> = Api::namespaced(self.client.clone(), &self.namespace);

        let sandbox_id = crd_name.strip_prefix("dsb-sb-").unwrap_or(crd_name);

        let status = SandboxStatus {
            phase: Some(phase.to_string()),
            pod_name: Some(sandbox_resource_name(sandbox_id)),
            service_name: Some(sandbox_service_name(sandbox_id)),
            ..Default::default()
        };

        let patch = serde_json::json!({
            "status": status
        });

        api.patch_status(crd_name, &PatchParams::default(), &Patch::Merge(&patch))
            .await
            .map_err(|e| {
                warn!(crd_name = crd_name, error = %e, "Failed to update CRD status");
                ManagerError::Api(format!("Failed to update status: {}", e))
            })?;

        Ok(())
    }

    /// Maps kube-rs errors to ManagerError variants.
    pub(super) fn map_kube_error(&self, err: kube::Error, context: &str) -> ManagerError {
        match &err {
            kube::Error::Api(api_err) => match api_err.code {
                404 => ManagerError::NotFound(format!(
                    "{}: resource not found - {}",
                    context, api_err.message
                )),
                409 => ManagerError::Conflict(format!(
                    "{}: resource already exists - {}",
                    context, api_err.message
                )),
                408 | 429 => ManagerError::Timeout(format!(
                    "{}: request timeout/rate limited - {}",
                    context, api_err.message
                )),
                _ => ManagerError::Api(format!(
                    "{}: K8s API error ({}): {}",
                    context, api_err.code, api_err.message
                )),
            },
            _ => ManagerError::Api(format!("{}: {}", context, err)),
        }
    }
}
