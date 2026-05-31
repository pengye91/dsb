// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Sandbox Custom Resource Definition for Kubernetes.
//!
//! The Sandbox CRD represents a DSB sandbox in the Kubernetes API.
//! The lightweight operator watches these CRDs and reconciles them into Pods and Services.

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Sandbox CRD specification.
///
/// This defines the desired state of a sandbox.
#[derive(CustomResource, Serialize, Deserialize, Default, Debug, Clone, JsonSchema)]
#[kube(
    group = "dsb.io",
    version = "v1",
    kind = "Sandbox",
    namespaced,
    status = "SandboxStatus",
    shortname = "sb",
    printcolumn = r#"{"name":"Image","type":"string","jsonPath":".spec.image"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#,
    printcolumn = r#"{"name":"Node","type":"string","jsonPath":".status.nodeName"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
pub struct SandboxSpec {
    /// Container image to run in the sandbox.
    pub image: String,

    /// Environment variables to set in the container.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Ports to expose from the container.
    #[serde(default)]
    pub ports: Vec<PortSpec>,

    /// Volumes to mount in the container.
    #[serde(default)]
    pub volumes: Vec<VolumeSpec>,

    /// Container resource requirements.
    #[serde(default)]
    pub resources: Option<ResourceSpec>,

    /// Command to run in the container (overrides ENTRYPOINT).
    #[serde(default)]
    pub command: Option<Vec<String>>,

    /// Arguments to the ENTRYPOINT.
    #[serde(default)]
    pub args: Option<Vec<String>>,

    /// Labels to apply to the Pod and Service.
    #[serde(default)]
    pub labels: HashMap<String, String>,

    /// Whether the sandbox needs GPU access.
    #[serde(default)]
    pub gpu: bool,

    /// Inactivity timeout in minutes (stored for SandboxService layer to use).
    #[serde(default)]
    pub inactivity_timeout_minutes: Option<u64>,

    /// The DSB sandbox ID (UUID) that maps to the database record.
    pub sandbox_id: String,

    /// API key hash for multi-tenancy filtering.
    #[serde(default)]
    pub api_key_hash: Option<String>,

    /// Whether DSB features (browser, web, vnc, etc.) were requested for this sandbox.
    ///
    /// When true, the image is expected to run supervisord with tool_proxy on :8080,
    /// so the K8s backend should wait for the health check and configure a readiness probe.
    /// When false, the image may be a plain image (e.g., ubuntu:22.04) that does not run
    /// tool_proxy, so the backend should skip the health check to avoid a 15s timeout.
    #[serde(default)]
    pub has_dsb_features: bool,
}

/// Port specification for a sandbox container.
#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
pub struct PortSpec {
    /// Port number inside the container.
    pub container_port: u16,
    /// Protocol (TCP or UDP).
    #[serde(default = "default_protocol")]
    pub protocol: String,
    /// Optional service port override (defaults to container_port).
    pub service_port: Option<u16>,
}

fn default_protocol() -> String {
    "TCP".to_string()
}

/// Volume specification for a sandbox container.
#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct VolumeSpec {
    /// PVC name or volume source name.
    pub name: String,
    /// Mount path inside the container.
    pub mount_path: String,
    /// Volume type: "pvc", "configmap", "secret", "emptydir".
    #[serde(default = "default_volume_type")]
    pub volume_type: String,
    /// Source name (PVC name, ConfigMap name, etc.).
    pub source_name: Option<String>,
    /// Whether the volume is read-only.
    #[serde(default)]
    pub read_only: bool,
}

fn default_volume_type() -> String {
    "pvc".to_string()
}

/// Resource requirements for a sandbox container.
#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct ResourceSpec {
    /// CPU request (e.g., "500m").
    pub cpu_request: Option<String>,
    /// Memory request (e.g., "1Gi").
    pub memory_request: Option<String>,
    /// CPU limit (e.g., "2000m").
    pub cpu_limit: Option<String>,
    /// Memory limit (e.g., "4Gi").
    pub memory_limit: Option<String>,
}

/// Sandbox CRD status.
///
/// This represents the observed state of a sandbox.
#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema, Default)]
pub struct SandboxStatus {
    /// Current phase of the sandbox (Pending, Running, Stopped, Failed).
    pub phase: Option<String>,
    /// Name of the Pod running the sandbox.
    pub pod_name: Option<String>,
    /// Name of the Service exposing the sandbox.
    pub service_name: Option<String>,
    /// Container ID (same as pod name in K8s).
    pub container_id: Option<String>,
    /// Node where the Pod is scheduled.
    pub node_name: Option<String>,
    /// Pod IP address.
    pub pod_ip: Option<String>,
    /// Human-readable message about the current status.
    pub message: Option<String>,
}
