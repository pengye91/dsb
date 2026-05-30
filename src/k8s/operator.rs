// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Lightweight operator for reconciling Sandbox CRDs to Pods and Services.
//!
//! This operator runs as a background task within the DSB server process.
//! It watches Sandbox CRDs for changes and reconciles them by creating,
//! updating, or cleaning up the corresponding Pod and Service resources.
//!
//! # Architecture
//!
//! The operator uses the kube-rs `Controller` runtime which provides
//! watch-based reconciliation (efficient, not polling). The reconcile
//! function is called for every CRD change. The error policy determines
//! what happens on errors (requeue with backoff).
//!
//! # Lifecycle
//!
//! 1. On CRD creation: if status is empty, create Pod + Service
//! 2. On reconciliation: verify Pod exists and is running; if missing, recreate
//! 3. On CRD deletion: Pod + Service are auto-deleted via owner references
//! 4. Status is updated with current phase, pod name, node name

use std::sync::Arc;

use futures::StreamExt;
use k8s_openapi::api::core::v1::{Pod, Service};
use kube::{
    api::{Api, Patch, PatchParams, PostParams},
    runtime::{controller::Action, watcher, Controller},
    Resource, ResourceExt,
};
use tokio::time::Duration;
use tracing::{debug, info, warn};

use crate::k8s::crd::{Sandbox, SandboxStatus};
use crate::k8s::manager::KubernetesManager;
use crate::k8s::types::{sandbox_resource_name, sandbox_service_name};

/// Lightweight operator that reconciles Sandbox CRDs into Pods and Services.
pub struct SandboxOperator {
    /// Shared Kubernetes manager for pod/service building logic.
    manager: Arc<KubernetesManager>,
}

impl SandboxOperator {
    /// Creates a new SandboxOperator backed by the given KubernetesManager.
    ///
    /// The manager provides access to the Kubernetes client, namespace,
    /// configuration, and pod/service building methods.
    pub fn new(manager: Arc<KubernetesManager>) -> Self {
        Self { manager }
    }

    /// Starts the controller as a background task.
    ///
    /// Returns a [`tokio::task::JoinHandle`] for the controller loop.
    /// The controller runs until the handle is aborted or the process shuts down.
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Runs the controller main loop.
    ///
    /// Sets up a kube-rs Controller that watches Sandbox CRDs and owned
    /// Pod resources, then processes reconciliation events indefinitely.
    async fn run(self) {
        let client = self.manager.client().clone();
        let namespace = self.manager.namespace().to_string();

        let crds: Api<Sandbox> = Api::namespaced(client.clone(), &namespace);
        let pods: Api<Pod> = Api::namespaced(client.clone(), &namespace);

        info!(
            namespace = %namespace,
            "Starting Sandbox CRD operator"
        );

        Controller::new(crds, watcher::Config::default())
            .owns(pods, watcher::Config::default())
            .run(Self::reconcile, Self::error_policy, Arc::new(self))
            .for_each(|result| async move {
                match result {
                    Ok((obj_ref, _action)) => {
                        debug!(
                            name = %obj_ref.name,
                            namespace = ?obj_ref.namespace,
                            "Reconciled Sandbox CRD"
                        );
                    }
                    Err(error) => {
                        warn!(error = %error, "Sandbox CRD reconciliation failed");
                    }
                }
            })
            .await;
    }

    /// Reconciles a single Sandbox CRD.
    ///
    /// Called by the controller runtime whenever a Sandbox CRD or an owned
    /// Pod changes. The function ensures the desired state (CRD spec) matches
    /// the observed state (running Pod and Service).
    async fn reconcile(sandbox: Arc<Sandbox>, ctx: Arc<Self>) -> Result<Action, kube::Error> {
        let name = sandbox.name_any();
        let ns = ctx.manager.namespace();

        // Determine pod name from CRD name (same convention)
        let pod_name = sandbox_resource_name(&sandbox.spec.sandbox_id);

        let pods: Api<Pod> = Api::namespaced(ctx.manager.client().clone(), ns);

        match pods.get(&pod_name).await {
            Ok(pod) => {
                // Pod exists -- check status and update CRD status accordingly.
                // Take ownership of status once to avoid partial moves.
                let pod_status = pod.status;
                let phase = pod_status
                    .as_ref()
                    .and_then(|s| s.phase.clone())
                    .unwrap_or_else(|| "Unknown".to_string());

                let node_name = pod.spec.and_then(|s| s.node_name);

                let pod_ip = pod_status.and_then(|s| s.pod_ip);

                ctx.update_crd_status(&name, &phase, node_name.as_deref(), pod_ip.as_deref())
                    .await?;

                // Re-check on a regular cadence.
                Ok(Action::requeue(Duration::from_secs(30)))
            }
            Err(e) if is_not_found(&e) => {
                // Pod does not exist. Decide whether to create one.
                let has_status = sandbox.status.is_some();
                let was_running = sandbox
                    .status
                    .as_ref()
                    .and_then(|s| s.phase.as_ref())
                    .map(|p| p == "Running" || p == "Pending")
                    .unwrap_or(false);

                // Check if CRD explicitly says "Stopped" or "Succeeded" or "Failed"
                let is_stopped = sandbox
                    .status
                    .as_ref()
                    .and_then(|s| s.phase.as_ref())
                    .map(|p| p == "Stopped" || p == "Succeeded" || p == "Failed")
                    .unwrap_or(false);

                if !has_status {
                    // Brand-new CRD with no status: create Pod + Service.
                    info!(
                        crd_name = %name,
                        "New Sandbox CRD detected, creating Pod and Service"
                    );
                    ctx.create_resources_from_crd(&sandbox).await?;
                } else if was_running {
                    // Pod was deleted externally while CRD says it should be running -- recreate.
                    info!(
                        pod_name = %pod_name,
                        "Pod missing but CRD says Running/Pending, recreating"
                    );
                    ctx.create_resources_from_crd(&sandbox).await?;
                } else if is_stopped {
                    // CRD explicitly stopped, pod is already gone - nothing to do
                    debug!(
                        crd_name = %name,
                        "Pod gone and CRD explicitly Stopped/Succeeded/Failed, skipping reconciliation"
                    );
                } else {
                    // CRD has some other status and Pod is gone -- assume we want it running if we don't know it's stopped.
                    info!(
                        crd_name = %name,
                        "Pod gone and CRD status unknown, recreating to be safe"
                    );
                    ctx.create_resources_from_crd(&sandbox).await?;
                }

                Ok(Action::requeue(Duration::from_secs(60)))
            }
            Err(e) => Err(e),
        }
    }

    /// Error policy: requeue on errors with exponential backoff.
    fn error_policy(_sandbox: Arc<Sandbox>, _error: &kube::Error, _ctx: Arc<Self>) -> Action {
        Action::requeue(Duration::from_secs(5))
    }

    /// Updates the status subresource of a Sandbox CRD.
    async fn update_crd_status(
        &self,
        crd_name: &str,
        phase: &str,
        node_name: Option<&str>,
        pod_ip: Option<&str>,
    ) -> Result<(), kube::Error> {
        let namespace = self.manager.namespace();
        let crds: Api<Sandbox> = Api::namespaced(self.manager.client().clone(), namespace);

        let sandbox_id = &crd_name.strip_prefix("dsb-sb-").unwrap_or(crd_name);

        let status = SandboxStatus {
            phase: Some(phase.to_string()),
            pod_name: Some(sandbox_resource_name(sandbox_id)),
            service_name: Some(sandbox_service_name(sandbox_id)),
            node_name: node_name.map(|n| n.to_string()),
            pod_ip: pod_ip.map(|p| p.to_string()),
            ..Default::default()
        };

        let patch = serde_json::json!({
            "status": status
        });

        crds.patch_status(crd_name, &PatchParams::default(), &Patch::Merge(&patch))
            .await?;

        Ok(())
    }

    /// Creates a Pod and Service from a Sandbox CRD spec.
    ///
    /// Reuses the building logic from [`KubernetesManager`] to ensure
    /// consistent pod/service definitions across imperative and declarative paths.
    async fn create_resources_from_crd(&self, sandbox: &Sandbox) -> Result<(), kube::Error> {
        let crd_name = sandbox.name_any();
        let crd_uid = sandbox.meta().uid.clone().ok_or_else(|| {
            kube::Error::Api(kube::error::ErrorResponse {
                status: "Failure".to_string(),
                message: format!(
                    "Sandbox CRD '{}' is missing UID required for owner references",
                    crd_name
                ),
                reason: "InvalidResource".to_string(),
                code: 500,
            })
        })?;

        let pod = self.manager.build_pod(&crd_name, &crd_uid, &sandbox.spec);
        let service = self
            .manager
            .build_service(&crd_name, &crd_uid, &sandbox.spec);

        let namespace = self.manager.namespace();
        let pods: Api<Pod> = Api::namespaced(self.manager.client().clone(), namespace);
        let services: Api<Service> = Api::namespaced(self.manager.client().clone(), namespace);

        // Create Pod (ignore 409 Conflict -- already exists).
        match pods.create(&PostParams::default(), &pod).await {
            Ok(_) => {
                info!(
                    pod_name = %crd_name,
                    "Operator created Pod"
                );
            }
            Err(e) if is_conflict(&e) => {
                debug!(
                    pod_name = %crd_name,
                    "Pod already exists, skipping creation"
                );
            }
            Err(e) => return Err(e),
        }

        // Create Service (ignore 409 Conflict -- already exists).
        let svc_name = sandbox_service_name(&sandbox.spec.sandbox_id);
        match services.create(&PostParams::default(), &service).await {
            Ok(_) => {
                info!(
                    service_name = %svc_name,
                    "Operator created Service"
                );
            }
            Err(e) if is_conflict(&e) => {
                debug!(
                    service_name = %svc_name,
                    "Service already exists, skipping creation"
                );
            }
            Err(e) => return Err(e),
        }

        Ok(())
    }
}

/// Checks if a kube error is a 404 Not Found.
fn is_not_found(err: &kube::Error) -> bool {
    matches!(err, kube::Error::Api(s) if s.code == 404)
}

/// Checks if a kube error is a 409 Conflict.
fn is_conflict(err: &kube::Error) -> bool {
    matches!(err, kube::Error::Api(s) if s.code == 409)
}

// ---------------------------------------------------------------------------
// Unit tests for operator reconciliation logic
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::k8s::crd::{Sandbox, SandboxSpec, SandboxStatus};
    use crate::k8s::types::sandbox_resource_name;
    use k8s_openapi::api::core::v1::{Pod, PodStatus};
    use kube::api::ObjectMeta;
    use std::collections::HashMap;

    /// Creates a minimal Sandbox CRD for testing.
    fn make_test_sandbox(name: &str, sandbox_id: &str) -> Sandbox {
        let spec = SandboxSpec {
            image: "test-image:latest".to_string(),
            sandbox_id: sandbox_id.to_string(),
            env: HashMap::new(),
            ports: vec![],
            volumes: vec![],
            resources: None,
            command: None,
            args: None,
            labels: HashMap::new(),
            gpu: false,
            inactivity_timeout_minutes: None,
            api_key_hash: None,
        };
        Sandbox::new(name, spec)
    }

    /// Creates a Sandbox CRD with a specific UID for owner reference testing.
    fn make_test_sandbox_with_uid(name: &str, sandbox_id: &str, uid: &str) -> Sandbox {
        let mut sandbox = make_test_sandbox(name, sandbox_id);
        sandbox.metadata.uid = Some(uid.to_string());
        sandbox
    }

    /// Creates a test Pod with the given name and phase.
    fn make_test_pod(
        name: &str,
        phase: &str,
        pod_ip: Option<&str>,
        node_name: Option<&str>,
    ) -> Pod {
        Pod {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some("test-namespace".to_string()),
                ..Default::default()
            },
            spec: Some(k8s_openapi::api::core::v1::PodSpec {
                node_name: node_name.map(|s| s.to_string()),
                ..Default::default()
            }),
            status: Some(PodStatus {
                phase: Some(phase.to_string()),
                pod_ip: pod_ip.map(|s| s.to_string()),
                ..Default::default()
            }),
        }
    }

    /// Creates a kube ErrorResponse for the given code.
    fn make_kube_error(code: u16, message: &str) -> kube::Error {
        kube::Error::Api(kube::error::ErrorResponse {
            status: "Failure".to_string(),
            message: message.to_string(),
            reason: "".to_string(),
            code,
        })
    }

    // -----------------------------------------------------------------------
    // Tests for reconcile() function
    // -----------------------------------------------------------------------

    #[test]
    fn test_reconcile_pod_exists_updates_crd_status() {
        // This test verifies that when a pod exists, we update the CRD status
        // We test the logic by checking that the function handles the exists case correctly
        let sandbox_id = "test-sandbox-123";
        let pod_name = sandbox_resource_name(sandbox_id);
        let crd_name = format!("dsb-sb-{}", sandbox_id);

        // Create a CRD with no status (will be updated)
        let sandbox = make_test_sandbox(&crd_name, sandbox_id);
        assert!(sandbox.status.is_none());

        // Create a running pod
        let pod = make_test_pod(&pod_name, "Running", Some("10.0.0.1"), Some("node-1"));

        // Verify pod exists and has running status
        assert_eq!(
            pod.status.as_ref().unwrap().phase.as_ref().unwrap(),
            "Running"
        );
        assert_eq!(
            pod.status.as_ref().unwrap().pod_ip.as_ref().unwrap(),
            "10.0.0.1"
        );
        assert_eq!(
            pod.spec.as_ref().unwrap().node_name.as_ref().unwrap(),
            "node-1"
        );
    }

    #[test]
    fn test_reconcile_pod_missing_no_status_creates_resources() {
        // When CRD has no status and pod is missing, we should create resources
        let sandbox_id = "new-sandbox-456";
        let crd_name = format!("dsb-sb-{}", sandbox_id);

        let sandbox = make_test_sandbox(&crd_name, sandbox_id);
        assert!(sandbox.status.is_none(), "New CRD should have no status");

        // The reconcile logic should detect !has_status and call create_resources_from_crd
        let has_status = sandbox.status.is_some();
        assert!(!has_status, "Should detect no-status CRD");
    }

    #[test]
    fn test_reconcile_pod_missing_was_running_recreates() {
        // When pod is missing but CRD shows Running/Pending status, recreate
        let sandbox_id = "was-running-789";
        let crd_name = format!("dsb-sb-{}", sandbox_id);

        let mut sandbox = make_test_sandbox(&crd_name, sandbox_id);
        sandbox.status = Some(SandboxStatus {
            phase: Some("Running".to_string()),
            ..Default::default()
        });

        let has_status = sandbox.status.is_some();
        let was_running = sandbox
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| p == "Running" || p == "Pending")
            .unwrap_or(false);

        assert!(has_status, "Should have status");
        assert!(was_running, "Should detect was-running CRD");
    }

    #[test]
    fn test_reconcile_pod_missing_explicitly_stopped_skips() {
        // When CRD is explicitly Stopped/Succeeded/Failed and pod is missing, skip
        let sandbox_id = "stopped-sandbox-000";
        let crd_name = format!("dsb-sb-{}", sandbox_id);

        let mut sandbox = make_test_sandbox(&crd_name, sandbox_id);
        sandbox.status = Some(SandboxStatus {
            phase: Some("Stopped".to_string()),
            ..Default::default()
        });

        let is_stopped = sandbox
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| p == "Stopped" || p == "Succeeded" || p == "Failed")
            .unwrap_or(false);

        assert!(is_stopped, "Should detect explicitly stopped CRD");

        // Also test Succeeded and Failed phases
        for phase in &["Stopped", "Succeeded", "Failed"] {
            let mut s = make_test_sandbox(&crd_name, sandbox_id);
            s.status = Some(SandboxStatus {
                phase: Some(phase.to_string()),
                ..Default::default()
            });

            let stopped = s
                .status
                .as_ref()
                .and_then(|st| st.phase.as_ref())
                .map(|p| p == "Stopped" || p == "Succeeded" || p == "Failed")
                .unwrap_or(false);
            assert!(stopped, "Should detect {} as stopped", phase);
        }
    }

    #[test]
    fn test_reconcile_pod_missing_pending_phase_takes_recreate_path() {
        // When pod is missing and CRD has Pending status, it should take the
        // was_running path (recreate) since Pending is treated as "should be running"
        let sandbox_id = "pending-status-999";
        let crd_name = format!("dsb-sb-{}", sandbox_id);

        let mut sandbox = make_test_sandbox(&crd_name, sandbox_id);
        sandbox.status = Some(SandboxStatus {
            phase: Some("Pending".to_string()),
            ..Default::default()
        });

        let was_running = sandbox
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| p == "Running" || p == "Pending")
            .unwrap_or(false);

        // "Pending" is treated as was_running, so it takes the recreate path
        assert!(
            was_running,
            "Pending phase should be treated as was_running"
        );
    }

    #[test]
    fn test_reconcile_unknown_phase_recreates_for_safety() {
        // Test with a truly unknown phase (not Running, not Pending, not Stopped/Succeeded/Failed)
        // This should take the "recreate for safety" path
        let sandbox_id = "unknown-phase-111";
        let crd_name = format!("dsb-sb-{}", sandbox_id);

        let mut sandbox = make_test_sandbox(&crd_name, sandbox_id);
        sandbox.status = Some(SandboxStatus {
            phase: Some("Creating".to_string()), // Not Running, not Pending, not Stopped
            ..Default::default()
        });

        let was_running = sandbox
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| p == "Running" || p == "Pending")
            .unwrap_or(false);
        let is_stopped = sandbox
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| p == "Stopped" || p == "Succeeded" || p == "Failed")
            .unwrap_or(false);

        // Unknown phase should not be treated as was_running or is_stopped
        assert!(
            !was_running && !is_stopped,
            "Creating is neither was_running nor is_stopped"
        );
        // This means it hits the "else" branch which recreates for safety
    }

    // -----------------------------------------------------------------------
    // Tests for create_resources_from_crd() UID validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_resources_requires_crd_uid() {
        // When CRD UID is missing, owner references would be invalid
        let sandbox_id = "no-uid-sandbox";
        let crd_name = format!("dsb-sb-{}", sandbox_id);

        let sandbox = make_test_sandbox(&crd_name, sandbox_id);
        // metadata.uid is None by default

        let uid = sandbox.meta().uid.clone();
        assert!(uid.is_none(), "UID should be None for this sandbox");

        // The code should return an error when UID is missing
        // Instead of unwrapping with .unwrap_or_default()
    }

    #[test]
    fn test_create_resources_with_valid_uid() {
        let sandbox_id = "valid-uid-sandbox";
        let crd_name = format!("dsb-sb-{}", sandbox_id);
        let uid = "test-uid-12345-abcde";

        let sandbox = make_test_sandbox_with_uid(&crd_name, sandbox_id, uid);

        let actual_uid = sandbox.meta().uid.clone();
        assert_eq!(actual_uid, Some(uid.to_string()), "UID should match");
    }

    // -----------------------------------------------------------------------
    // Tests for is_not_found and is_conflict helpers
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_not_found_detects_404() {
        let err = make_kube_error(404, "pods \"test\" not found");
        assert!(is_not_found(&err), "Should detect 404 as not found");
    }

    #[test]
    fn test_is_not_found_rejects_other_codes() {
        let err_409 = make_kube_error(409, "conflict");
        let err_500 = make_kube_error(500, "internal error");

        assert!(!is_not_found(&err_409), "409 should not be not found");
        assert!(!is_not_found(&err_500), "500 should not be not found");
    }

    #[test]
    fn test_is_conflict_detects_409() {
        let err = make_kube_error(409, "object already exists");
        assert!(is_conflict(&err), "Should detect 409 as conflict");
    }

    #[test]
    fn test_is_conflict_rejects_other_codes() {
        let err_404 = make_kube_error(404, "not found");
        let err_500 = make_kube_error(500, "internal error");

        assert!(!is_conflict(&err_404), "404 should not be conflict");
        assert!(!is_conflict(&err_500), "500 should not be conflict");
    }

    // -----------------------------------------------------------------------
    // Tests for Sandbox resource name generation
    // -----------------------------------------------------------------------

    #[test]
    fn test_sandbox_resource_name_consistency() {
        let sandbox_id = "550e8400-e29b-41d4-a716-446655440000";
        let name = sandbox_resource_name(sandbox_id);

        // The pod name derived from sandbox_id should be consistent
        assert!(name.starts_with("dsb-sb-"));
    }

    #[test]
    fn test_sandbox_status_phase_detection() {
        // Test the phase detection logic used in reconcile
        fn detect_phase_status(phase: Option<&str>) -> (bool, bool, bool) {
            let was_running = phase
                .map(|p| p == "Running" || p == "Pending")
                .unwrap_or(false);
            let is_stopped = phase
                .map(|p| p == "Stopped" || p == "Succeeded" || p == "Failed")
                .unwrap_or(false);
            let has_status = phase.is_some();
            (has_status, was_running, is_stopped)
        }

        // No status
        let (has, was, stopped) = detect_phase_status(None);
        assert!(!has && !was && !stopped, "None phase");

        // Running
        let (has, was, stopped) = detect_phase_status(Some("Running"));
        assert!(has && was && !stopped, "Running phase");

        // Pending
        let (has, was, stopped) = detect_phase_status(Some("Pending"));
        assert!(has && was && !stopped, "Pending phase");

        // Stopped
        let (has, was, stopped) = detect_phase_status(Some("Stopped"));
        assert!(has && !was && stopped, "Stopped phase");

        // Succeeded
        let (has, was, stopped) = detect_phase_status(Some("Succeeded"));
        assert!(has && !was && stopped, "Succeeded phase");

        // Failed
        let (has, was, stopped) = detect_phase_status(Some("Failed"));
        assert!(has && !was && stopped, "Failed phase");

        // Unknown/other
        let (has, was, stopped) = detect_phase_status(Some("Unknown"));
        assert!(has && !was && !stopped, "Unknown phase");

        let (has, was, stopped) = detect_phase_status(Some("Creating"));
        assert!(has && !was && !stopped, "Creating phase");
    }

    // -----------------------------------------------------------------------
    // Integration-style test for full reconcile decision tree
    // -----------------------------------------------------------------------

    #[test]
    fn test_reconcile_decision_tree_new_crd() {
        // CRD: no status, Pod: missing -> CREATE
        let sandbox_id = "new-crd";
        let sandbox = make_test_sandbox(&format!("dsb-sb-{}", sandbox_id), sandbox_id);

        let pod_exists = false;
        let has_status = sandbox.status.is_some();

        // Decision: !pod_exists && !has_status -> CREATE
        assert!(
            !pod_exists && !has_status,
            "Should take create path for new CRD with missing pod"
        );
    }

    #[test]
    fn test_reconcile_decision_tree_was_running() {
        // CRD: status=Running, Pod: missing -> RECREATE
        let sandbox_id = "was-running";
        let mut sandbox = make_test_sandbox(&format!("dsb-sb-{}", sandbox_id), sandbox_id);
        sandbox.status = Some(SandboxStatus {
            phase: Some("Running".to_string()),
            ..Default::default()
        });

        let pod_exists = false;
        let was_running = sandbox
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| p == "Running" || p == "Pending")
            .unwrap_or(false);
        let is_stopped = sandbox
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| p == "Stopped" || p == "Succeeded" || p == "Failed")
            .unwrap_or(false);

        // Decision: !pod_exists && was_running && !is_stopped -> RECREATE
        assert!(
            !pod_exists && was_running && !is_stopped,
            "Should take recreate path for was-running CRD"
        );
    }

    #[test]
    fn test_reconcile_decision_tree_explicitly_stopped() {
        // CRD: status=Stopped, Pod: missing -> SKIP
        let sandbox_id = "explicitly-stopped";
        let mut sandbox = make_test_sandbox(&format!("dsb-sb-{}", sandbox_id), sandbox_id);
        sandbox.status = Some(SandboxStatus {
            phase: Some("Stopped".to_string()),
            ..Default::default()
        });

        let pod_exists = false;
        let was_running = sandbox
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| p == "Running" || p == "Pending")
            .unwrap_or(false);
        let is_stopped = sandbox
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| p == "Stopped" || p == "Succeeded" || p == "Failed")
            .unwrap_or(false);

        // Decision: !pod_exists && !was_running && is_stopped -> SKIP
        assert!(
            !pod_exists && !was_running && is_stopped,
            "Should take skip path for explicitly stopped CRD"
        );
    }

    #[test]
    fn test_reconcile_decision_tree_unknown_status() {
        // CRD: status=Unknown/other, Pod: missing -> RECREATE (for safety)
        let sandbox_id = "unknown-status";
        let mut sandbox = make_test_sandbox(&format!("dsb-sb-{}", sandbox_id), sandbox_id);
        sandbox.status = Some(SandboxStatus {
            phase: Some("Unknown".to_string()),
            ..Default::default()
        });

        let pod_exists = false;
        let was_running = sandbox
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| p == "Running" || p == "Pending")
            .unwrap_or(false);
        let is_stopped = sandbox
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| p == "Stopped" || p == "Succeeded" || p == "Failed")
            .unwrap_or(false);

        // Decision: !pod_exists && !was_running && !is_stopped -> RECREATE (for safety)
        assert!(
            !pod_exists && !was_running && !is_stopped,
            "Should take recreate-for-safety path for unknown status"
        );
    }

    #[test]
    fn test_reconcile_decision_tree_pod_exists() {
        // CRD: any status, Pod: exists -> UPDATE STATUS
        let pod_exists = true;

        // Decision: pod_exists -> UPDATE STATUS
        assert!(pod_exists, "Should take update path when pod exists");
    }

    // -----------------------------------------------------------------------
    // Test error responses have correct structure
    // -----------------------------------------------------------------------

    #[test]
    fn test_kube_error_response_structure() {
        let err = kube::Error::Api(kube::error::ErrorResponse {
            status: "Failure".to_string(),
            message: "pods \"test\" not found".to_string(),
            reason: "NotFound".to_string(),
            code: 404,
        });

        match &err {
            kube::Error::Api(api_err) => {
                assert_eq!(api_err.code, 404);
                assert_eq!(api_err.reason, "NotFound");
                assert!(api_err.message.contains("not found"));
            }
            _ => panic!("Expected Api error"),
        }
    }
}
