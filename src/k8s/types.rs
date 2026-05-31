// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Kubernetes-specific type definitions for the DSB backend.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result of creating a sandbox on K8s.
#[derive(Debug, Clone)]
pub struct CreateSandboxResult {
    /// The CRD name (used as the sandbox identifier in K8s).
    pub crd_name: String,
    /// The pod name (set after start() creates the Pod).
    pub pod_name: Option<String>,
    /// The service name for accessing the sandbox.
    pub service_name: Option<String>,
}

/// Configuration for creating a K8s sandbox Pod.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodConfig {
    /// Sandbox image.
    pub image: String,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Container ports.
    pub ports: Vec<u16>,
    /// Resource requirements.
    pub cpu_request: Option<String>,
    /// Memory request (e.g. "512Mi").
    pub memory_request: Option<String>,
    /// CPU limit (e.g. "1000m").
    pub cpu_limit: Option<String>,
    /// Memory limit (e.g. "1Gi").
    pub memory_limit: Option<String>,
    /// Command override.
    pub command: Option<Vec<String>>,
    /// Labels to apply.
    pub labels: HashMap<String, String>,
    /// Whether GPU is needed.
    pub gpu: bool,
}

/// Standard ports exposed by DSB sandbox containers.
pub mod sandbox_ports {
    /// Tool proxy HTTP port.
    pub const TOOL_PROXY: u16 = 8080;
    /// VNC port.
    pub const VNC: u16 = 5901;
    /// noVNC WebSocket port.
    pub const NOVNC: u16 = 6080;
    /// Agent browser port.
    pub const AGENT_BROWSER: u16 = 3000;
}

/// Label keys used by DSB on K8s resources.
pub mod labels {
    /// Managed-by label.
    pub const MANAGED_BY: &str = "dsb.io/managed-by";
    /// Sandbox ID label.
    pub const SANDBOX_ID: &str = "dsb.io/sandbox-id";
    /// API key hash label for multi-tenancy.
    pub const API_KEY_HASH: &str = "dsb.io/api-key-hash";
    /// Component label (sandbox-pod, sandbox-service).
    pub const COMPONENT: &str = "dsb.io/component";
}

/// Generates a K8s-safe name for a sandbox resource.
/// K8s names must be lowercase, DNS-compatible, max 63 chars.
pub fn sandbox_resource_name(sandbox_id: &str) -> String {
    let name = format!("dsb-sb-{}", sandbox_id);
    let truncated: String = name.chars().take(63).collect();
    truncated.to_lowercase()
}

/// Generates a K8s-safe service name for a sandbox.
pub fn sandbox_service_name(sandbox_id: &str) -> String {
    let name = format!("dsb-svc-{}", sandbox_id);
    let truncated: String = name.chars().take(63).collect();
    truncated.to_lowercase()
}
