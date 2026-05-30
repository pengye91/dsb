// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie

use k8s_openapi::api::core::v1::ServicePort;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use std::collections::HashMap;

use crate::core::types::SandboxConfig;
pub(super) fn merge_sandbox_environment(
    config: &SandboxConfig,
    proxy_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut env = proxy_env.clone();
    for (k, v) in &config.environment {
        env.insert(k.clone(), v.clone());
    }
    env
}

/// When sandboxes use an HTTP(S) proxy, ensure in-cluster and loopback traffic bypass it.
///
/// Chromium/CDP and Kubernetes Services must not be forced through a corporate proxy; otherwise
/// `Page.goto` to the public Internet hangs while loopback/CDP still works.
pub(super) fn augment_no_proxy_for_kubernetes_cluster(env: &mut HashMap<String, String>) {
    let using_proxy = env.contains_key("HTTP_PROXY")
        || env.contains_key("http_proxy")
        || env.contains_key("HTTPS_PROXY")
        || env.contains_key("https_proxy")
        || env.contains_key("ALL_PROXY")
        || env.contains_key("all_proxy");
    if !using_proxy {
        return;
    }

    const CLUSTER_LOCAL_BYPASS: &str =
        "localhost,127.0.0.1,127.0.0.0/8,::1,.svc.cluster.local,.cluster.local";

    for key in ["NO_PROXY", "no_proxy"] {
        if let Some(existing) = env.get(key) {
            if existing.contains(".svc.cluster.local") {
                continue;
            }
            let merged = if existing.trim().is_empty() {
                CLUSTER_LOCAL_BYPASS.to_string()
            } else {
                format!("{existing},{CLUSTER_LOCAL_BYPASS}")
            };
            env.insert(key.to_string(), merged);
        } else {
            env.insert(key.to_string(), CLUSTER_LOCAL_BYPASS.to_string());
        }
    }
}

/// Creates a ServicePort for a given name and port number.
pub(super) fn make_service_port(name: &str, port: u16) -> ServicePort {
    ServicePort {
        port: port as i32,
        target_port: Some(IntOrString::Int(port as i32)),
        protocol: Some("TCP".to_string()),
        name: Some(name.to_string()),
        ..Default::default()
    }
}

/// Maps a Kubernetes pod phase to a Docker-compatible state string.
pub(super) fn phase_to_state(phase: &str) -> String {
    match phase {
        "Pending" => "created".to_string(),
        "Running" => "running".to_string(),
        "Succeeded" => "exited".to_string(),
        "Failed" => "exited".to_string(),
        "Unknown" => "unknown".to_string(),
        _ => "unknown".to_string(),
    }
}
