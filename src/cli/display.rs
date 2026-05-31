// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! CLI display formatting utilities for sandbox information.
//!
//! This module provides functions for formatting and displaying sandbox details
//! in a user-friendly way in the CLI.

use crate::core::types::{
    PortProtocol, ResourceLimits, SandboxConfig, SandboxResponse, VolumeMount,
};

/// Prints comprehensive sandbox details to stdout.
///
/// This function displays all sandbox information including:
/// - Basic info (ID, state, container ID, timestamps)
/// - Full configuration (image, command, ports, volumes, env, resources, features)
///
/// # Example
///
/// ```
/// # use dsb::cli::display::print_sandbox_details;
/// # use dsb::core::types::SandboxResponse;
/// # use dsb::core::types::{SandboxConfig, SandboxState, PullPolicy};
/// # let config = SandboxConfig::default();
/// # let sandbox = SandboxResponse {
/// #     id: uuid::Uuid::new_v4(),
/// #     state: SandboxState::Running,
/// #     config,
/// #     container_id: Some("container-123".to_string()),
/// #     created_at: chrono::Utc::now(),
/// #     updated_at: chrono::Utc::now(),
/// #     deleted_at: None,
/// #     deleted_by: None,
/// #     api_key_id: None,
/// #     kubernetes: None,
/// # };
/// # // Suppress output from doctest
/// # let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
/// print_sandbox_details(&sandbox);
/// # }));
/// ```
pub fn print_sandbox_details(sandbox: &SandboxResponse) {
    println!("✓ Sandbox ready!");
    println!();
    println!("Sandbox Details:");
    println!("  ID: {}", sandbox.id);
    println!("  State: {}", format_state(sandbox.state));
    if let Some(ref container_id) = sandbox.container_id {
        println!("  Container: {}", container_id);
    }
    println!(
        "  Created: {}",
        sandbox.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!(
        "  Updated: {}",
        sandbox.updated_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!();

    print_config_summary(&sandbox.config);
}

/// Prints sandbox configuration details.
fn print_config_summary(config: &SandboxConfig) {
    println!("Configuration:");
    println!("  Image: {}", config.image);
    if let Some(ref name) = config.name {
        println!("  Name: {}", name);
    }

    if let Some(ref command) = config.command {
        println!("  Command: {}", command.join(" "));
    }

    if !config.port_mappings.is_empty() {
        println!();
        println!("  Port Mappings:");
        for mapping in &config.port_mappings {
            println!(
                "    {} (host) → {} (container) [{}]",
                mapping.host_port,
                mapping.container_port,
                format_protocol(&mapping.protocol)
            );
        }
    }

    if !config.volumes.is_empty() {
        println!();
        println!("  Volume Mounts:");
        for volume in &config.volumes {
            println!("    {}", format_volume(volume));
        }
    }

    if !config.environment.is_empty() {
        println!();
        println!("  Environment Variables:");
        for (key, value) in &config.environment {
            println!("    {}={}", key, value);
        }
    }

    print_resource_limits(&config.resource_limits);
    print_features(config);
    print_other_settings(config);
}

/// Prints resource limits if any are set.
fn print_resource_limits(limits: &ResourceLimits) {
    let has_limits = limits.memory_mb.is_some()
        || limits.cpu_quota.is_some()
        || limits.cpu_shares.is_some()
        || limits.pids_limit.is_some();

    if !has_limits {
        return;
    }

    println!();
    println!("  Resource Limits:");
    if let Some(memory_mb) = limits.memory_mb {
        println!("    Memory: {} MB", memory_mb);
    }
    if let Some(cpu_quota) = limits.cpu_quota {
        println!("    CPU Quota: {}", cpu_quota);
    }
    if let Some(cpu_shares) = limits.cpu_shares {
        println!("    CPU Shares: {}", cpu_shares);
    }
    if let Some(pids_limit) = limits.pids_limit {
        println!("    PIDs Limit: {}", pids_limit);
    }
}

/// Prints feature-related settings.
fn print_features(config: &SandboxConfig) {
    println!();
    println!("  Features:");
    println!("    Static Server: enabled");
    if !config.features.is_empty() {
        println!("    Enabled Features: {}", config.features.join(", "));
    } else {
        println!("    Enabled Features: none");
    }
    println!(
        "    Enable All Features: {}",
        if config.enable_all_features {
            "yes"
        } else {
            "no"
        }
    );
}

/// Prints other configuration settings.
fn print_other_settings(config: &SandboxConfig) {
    println!();
    println!("  Other:");
    println!("    Pull Policy: {}", config.pull_policy.as_str());
    if let Some(timeout) = config.inactivity_timeout_minutes {
        println!("    Inactivity Timeout: {} minutes", timeout);
    } else {
        println!("    Inactivity Timeout: disabled");
    }
}

/// Formats a volume mount for display.
fn format_volume(volume: &VolumeMount) -> String {
    match volume {
        VolumeMount::Bind {
            host_path,
            container_path,
            read_only,
        } => {
            format!(
                "{} → {} ({})",
                host_path,
                container_path,
                if *read_only { "ro" } else { "rw" }
            )
        }
        VolumeMount::Named {
            name,
            container_path,
            read_only,
        } => {
            format!(
                "{} (named volume) → {} ({})",
                name,
                container_path,
                if *read_only { "ro" } else { "rw" }
            )
        }
    }
}

/// Formats a port protocol for display.
fn format_protocol(protocol: &PortProtocol) -> &'static str {
    match protocol {
        PortProtocol::Tcp => "tcp",
        PortProtocol::Udp => "udp",
    }
}

/// Formats sandbox state for display.
fn format_state(state: crate::core::types::SandboxState) -> &'static str {
    match state {
        crate::core::types::SandboxState::Creating => "creating",
        crate::core::types::SandboxState::Created => "created",
        crate::core::types::SandboxState::Starting => "starting",
        crate::core::types::SandboxState::Running => "running",
        crate::core::types::SandboxState::Stopped => "stopped",
        crate::core::types::SandboxState::Error => "error",
        crate::core::types::SandboxState::Destroying => "destroying",
        crate::core::types::SandboxState::Destroyed => "destroyed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{PortMapping, PullPolicy, SandboxState};
    use chrono::Utc;
    use std::collections::HashMap;

    #[test]
    fn test_format_volume_bind() {
        let volume = VolumeMount::Bind {
            host_path: "/host/data".to_string(),
            container_path: "/container/data".to_string(),
            read_only: false,
        };
        assert_eq!(format_volume(&volume), "/host/data → /container/data (rw)");
    }

    #[test]
    fn test_format_volume_bind_readonly() {
        let volume = VolumeMount::Bind {
            host_path: "/host/data".to_string(),
            container_path: "/container/data".to_string(),
            read_only: true,
        };
        assert_eq!(format_volume(&volume), "/host/data → /container/data (ro)");
    }

    #[test]
    fn test_format_volume_named() {
        let volume = VolumeMount::Named {
            name: "cache-volume".to_string(),
            container_path: "/cache".to_string(),
            read_only: false,
        };
        assert_eq!(
            format_volume(&volume),
            "cache-volume (named volume) → /cache (rw)"
        );
    }

    #[test]
    fn test_format_protocol_tcp() {
        assert_eq!(format_protocol(&PortProtocol::Tcp), "tcp");
    }

    #[test]
    fn test_format_protocol_udp() {
        assert_eq!(format_protocol(&PortProtocol::Udp), "udp");
    }

    #[test]
    fn test_print_sandbox_details_minimal() {
        let sandbox = SandboxResponse {
            id: uuid::Uuid::new_v4(),
            state: SandboxState::Running,
            config: SandboxConfig {
                image: "nginx:latest".to_string(),
                ..Default::default()
            },
            container_id: Some("abc123".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
            deleted_by: None,
            api_key_id: None,
            kubernetes: None,
        };

        // Just ensure it doesn't panic
        print_sandbox_details(&sandbox);
    }

    #[test]
    fn test_print_sandbox_details_full() {
        let mut env = HashMap::new();
        env.insert("NODE_ENV".to_string(), "production".to_string());

        let sandbox = SandboxResponse {
            id: uuid::Uuid::new_v4(),
            state: SandboxState::Running,
            config: SandboxConfig {
                image: "nginx:latest".to_string(),
                name: Some("test-sandbox".to_string()),
                command: Some(vec![
                    "nginx".to_string(),
                    "-g".to_string(),
                    "daemon off;".to_string(),
                ]),
                environment: env,
                port_mappings: vec![PortMapping {
                    host_port: 8080,
                    container_port: 80,
                    protocol: PortProtocol::Tcp,
                }],
                exposed_ports: vec![],
                volumes: vec![VolumeMount::Bind {
                    host_path: "/host/data".to_string(),
                    container_path: "/data".to_string(),
                    read_only: false,
                }],
                resource_limits: ResourceLimits {
                    memory_mb: Some(512),
                    cpu_shares: Some(1024),
                    ..Default::default()
                },
                inactivity_timeout_minutes: Some(30),
                pull_policy: PullPolicy::Always,
                features: vec!["vnc".to_string()],
                enable_all_features: false,
                vnc_resolution: None,
            },
            container_id: Some("abc123".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
            deleted_by: None,
            api_key_id: None,
            kubernetes: None,
        };

        // Just ensure it doesn't panic
        print_sandbox_details(&sandbox);
    }
}
