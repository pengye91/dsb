// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Feature Profile System
//!
//! This module provides automatic configuration discovery from Docker image labels.
//!
//! ## Concept
//!
//! Images declare their capabilities via Docker labels. DSB inspects these labels
//! and automatically applies ports, volumes, commands, and environment variables.
//!
//! ## Feature Metadata Flow
//!
//! 1. Image declares `com.dsb.features` label with JSON metadata
//! 2. DSB inspects image and extracts feature definitions
//! 3. User requests specific features (e.g., `--features vnc,browser`)
//! 4. DSB merges detected features with user configuration
//! 5. User configuration always takes precedence
//!
//! ## Example Label
//!
//! ```json
//! {
//!   "version": "1.0",
//!   "features": {
//!     "vnc": {
//!       "description": "VNC server with web client",
//!       "ports": [
//!         {"host": 5901, "container": 5901, "protocol": "tcp", "description": "x11vnc"},
//!         {"host": 6080, "container": 6080, "protocol": "tcp", "description": "noVNC"}
//!       ],
//!       "env": {"DISPLAY": ":1"},
//!       "enabled_by_default": true
//!     }
//!   },
//!   "default_command": ["sudo", "/usr/bin/supervisord", "-c", "/etc/supervisor/conf.d/supervisord.conf"]
//! }
//! ```

use crate::core::types::{PortMapping, PortProtocol, SandboxConfig, VolumeMount};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Label key for DSB features metadata
pub const DSB_FEATURES_LABEL: &str = "com.dsb.features";

/// Feature metadata stored in Docker image labels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageFeatureLabel {
    /// Schema version (for future compatibility)
    pub version: String,

    /// Map of feature name to feature definition
    #[serde(default)]
    pub features: HashMap<String, FeatureDefinition>,

    /// Default command to run (if features enabled)
    pub default_command: Option<Vec<String>>,
}

/// Definition of a single feature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDefinition {
    /// Human-readable description
    pub description: String,

    /// Ports required by this feature
    #[serde(default)]
    pub ports: Vec<FeaturePort>,

    /// Environment variables for this feature
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Volume mounts for this feature
    #[serde(default)]
    pub volumes: Vec<FeatureVolume>,

    /// Static file serving capability
    #[serde(default)]
    pub static_server: Option<FeatureStaticServer>,

    /// Whether this feature is enabled by default
    #[serde(default = "default_feature_enabled")]
    pub enabled_by_default: bool,
}

fn default_feature_enabled() -> bool {
    true
}

/// Port mapping definition within a feature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturePort {
    /// Host port (can use 0 for auto-allocation)
    pub host: u16,

    /// Container port
    pub container: u16,

    /// Protocol (tcp/udp)
    #[serde(default)]
    pub protocol: String,

    /// Human-readable description
    #[serde(default)]
    pub description: String,
}

/// Volume mount definition within a feature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVolume {
    /// Volume type (bind, named, or dynamic_bind for auto-generated paths)
    #[serde(rename = "type")]
    pub volume_type: String,

    /// Container path
    pub container_path: String,

    /// Host path (for bind mounts, or template for dynamic_bind)
    pub host_path: Option<String>,

    /// Read-only flag
    #[serde(default)]
    pub read_only: bool,

    /// Human-readable description
    #[serde(default)]
    pub description: String,
}

/// Static file server configuration within a feature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureStaticServer {
    /// Container path for static files (default: "/public")
    #[serde(default = "default_static_server_path")]
    pub container_path: String,

    /// Host path template (supports {sandbox_id} placeholder)
    /// If None, uses base_path from config + sandbox_id
    pub host_path_template: Option<String>,

    /// Human-readable description
    #[serde(default)]
    pub description: String,
}

fn default_static_server_path() -> String {
    "/public".to_string()
}

/// Feature profile extracted from image inspection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureProfile {
    /// All available features from the image
    pub available_features: HashMap<String, FeatureDefinition>,

    /// Features to enable (filtered by user request)
    pub enabled_features: Vec<String>,

    /// Default command from image
    pub default_command: Option<Vec<String>>,
}

impl FeatureProfile {
    /// Create empty feature profile (no labels found)
    pub fn empty() -> Self {
        Self {
            available_features: HashMap::new(),
            enabled_features: Vec::new(),
            default_command: None,
        }
    }

    /// Check if feature is enabled
    pub fn is_feature_enabled(&self, name: &str) -> bool {
        self.enabled_features.contains(&name.to_string())
    }

    /// Get all ports for enabled features
    pub fn get_ports(&self) -> Vec<PortMapping> {
        let mut ports = Vec::new();

        for feature_name in &self.enabled_features {
            if let Some(feature) = self.available_features.get(feature_name) {
                for port_def in &feature.ports {
                    ports.push(PortMapping {
                        host_port: port_def.host,
                        container_port: port_def.container,
                        protocol: if port_def.protocol.to_lowercase() == "udp" {
                            PortProtocol::Udp
                        } else {
                            PortProtocol::Tcp
                        },
                    });
                }
            }
        }

        ports
    }

    /// Get all volumes for enabled features
    pub fn get_volumes(&self, sandbox_id: &str) -> Vec<VolumeMount> {
        let mut volumes = Vec::new();

        for feature_name in &self.enabled_features {
            if let Some(feature) = self.available_features.get(feature_name) {
                for volume_def in &feature.volumes {
                    let volume = match volume_def.volume_type.as_str() {
                        "dynamic_bind" => {
                            // Auto-generate host path based on sandbox_id
                            let host_path = if let Some(template) = &volume_def.host_path {
                                // Replace {sandbox_id} placeholder
                                template.replace("{sandbox_id}", sandbox_id)
                            } else {
                                // Use default base path from config
                                format!("/var/lib/dsb/feature-data/{}/{}", sandbox_id, feature_name)
                            };

                            VolumeMount::Bind {
                                host_path,
                                container_path: volume_def.container_path.clone(),
                                read_only: volume_def.read_only,
                            }
                        }
                        "bind" => {
                            let host_path = volume_def.host_path.clone().unwrap_or_else(|| {
                                format!("/tmp/dsb/{}/{}", sandbox_id, feature_name)
                            });

                            VolumeMount::Bind {
                                host_path,
                                container_path: volume_def.container_path.clone(),
                                read_only: volume_def.read_only,
                            }
                        }
                        "named" => {
                            let name = volume_def
                                .host_path
                                .clone()
                                .unwrap_or_else(|| format!("dsb-{}-{}", feature_name, sandbox_id));

                            VolumeMount::Named {
                                name,
                                container_path: volume_def.container_path.clone(),
                                read_only: volume_def.read_only,
                            }
                        }
                        _ => continue, // Unknown type, skip
                    };

                    volumes.push(volume);
                }
            }
        }

        volumes
    }

    /// Get all environment variables for enabled features
    pub fn get_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();

        for feature_name in &self.enabled_features {
            if let Some(feature) = self.available_features.get(feature_name) {
                env.extend(feature.env.clone());
            }
        }

        env
    }

    /// Get default command (if any features require it)
    pub fn get_command(&self) -> Option<Vec<String>> {
        // Only use default command if at least one feature is enabled
        if self.enabled_features.is_empty() {
            None
        } else {
            self.default_command.clone()
        }
    }

    /// Check if any enabled feature has static server capability
    pub fn has_static_server(&self) -> bool {
        self.enabled_features.iter().any(|feature_name| {
            self.available_features
                .get(feature_name)
                .and_then(|f| f.static_server.as_ref())
                .is_some()
        })
    }

    /// Get static server configuration from enabled features
    /// Returns first match (features should not conflict)
    pub fn get_static_server_config(&self) -> Option<&FeatureStaticServer> {
        self.enabled_features.iter().find_map(|feature_name| {
            self.available_features
                .get(feature_name)
                .and_then(|f| f.static_server.as_ref())
        })
    }
}

/// Feature selection request from user
#[derive(Debug, Clone, Default)]
pub struct FeatureSelection {
    /// Explicitly enabled features
    pub enabled: Vec<String>,

    /// Explicitly disabled features
    pub disabled: Vec<String>,

    /// Enable all features by default?
    pub enable_all: bool,
}

impl FeatureSelection {
    /// Create a new, empty feature selection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the list of explicitly enabled features.
    pub fn with_enabled(mut self, features: Vec<String>) -> Self {
        self.enabled = features;
        self
    }

    /// Set the list of explicitly disabled features.
    pub fn with_disabled(mut self, features: Vec<String>) -> Self {
        self.disabled = features;
        self
    }

    /// Enable all available features by default.
    pub fn enable_all(mut self) -> Self {
        self.enable_all = true;
        self
    }
}

fn determine_enabled_features(
    available_features: &HashMap<String, FeatureDefinition>,
    selection: &FeatureSelection,
) -> Vec<String> {
    let mut enabled = Vec::new();

    let has_explicit_selection = !selection.enabled.is_empty();

    for (name, feature) in available_features {
        if selection.disabled.contains(name) {
            tracing::debug!("Feature '{}' is explicitly disabled", name);
            continue;
        }

        if selection.enabled.contains(name) {
            tracing::debug!("Feature '{}' is explicitly enabled", name);
            enabled.push(name.clone());
            continue;
        }

        if has_explicit_selection {
            tracing::debug!("Feature '{}' skipped (user has explicit selection)", name);
            continue;
        }

        if selection.enable_all && feature.enabled_by_default {
            tracing::debug!("Feature '{}' enabled by enable_all", name);
            enabled.push(name.clone());
            continue;
        }

        if !selection.enable_all && feature.enabled_by_default {
            tracing::debug!("Feature '{}' enabled by default", name);
            enabled.push(name.clone());
        }
    }

    enabled.sort();
    enabled.dedup();
    enabled
}

/// Build a feature profile from image label metadata and a user selection.
pub fn build_feature_profile(
    label: ImageFeatureLabel,
    selection: &FeatureSelection,
) -> FeatureProfile {
    let enabled_features = determine_enabled_features(&label.features, selection);

    FeatureProfile {
        available_features: label.features,
        enabled_features,
        default_command: label.default_command,
    }
}

/// Apply feature profile to sandbox config
///
/// This function merges detected features from image metadata with the user's
/// sandbox configuration. User-specified values always take precedence.
///
/// # Arguments
///
/// * `config` - Mutable reference to sandbox config (will be modified)
/// * `profile` - Feature profile detected from image
/// * `sandbox_id` - Sandbox ID for dynamic path generation
///
/// # Returns
///
/// * `Ok(())` if features applied successfully
/// * `Err(String)` if there's an error applying features
pub fn apply_feature_profile(
    config: &mut SandboxConfig,
    profile: &FeatureProfile,
    sandbox_id: &str,
) -> Result<(), String> {
    // Apply ports to exposed_ports (internal networking only, not published to host)
    let feature_ports = profile.get_ports();
    for port in feature_ports {
        // Add to exposed_ports for internal Docker networking
        if !config.exposed_ports.contains(&port.container_port) {
            tracing::debug!(
                "Exposing feature port {} internally (not published to host)",
                port.container_port
            );
            config.exposed_ports.push(port.container_port);
        } else {
            tracing::debug!("Port {} already in exposed_ports", port.container_port);
        }
    }

    // Apply volumes (merge with existing)
    let feature_volumes = profile.get_volumes(sandbox_id);
    for volume in feature_volumes {
        let container_path = match &volume {
            VolumeMount::Bind { container_path, .. } => container_path.clone(),
            VolumeMount::Named { container_path, .. } => container_path.clone(),
        };

        // Check if user already specified this container path
        let user_has_volume = config
            .volumes
            .iter()
            .any(|v| {
                matches!(v, VolumeMount::Bind { container_path: cp, .. } if *cp == container_path)
                    || matches!(v, VolumeMount::Named { container_path: cp, .. } if *cp == container_path)
            });

        if !user_has_volume {
            tracing::debug!("Applying feature volume: {}", container_path);
            config.volumes.push(volume);
        } else {
            tracing::debug!(
                "Skipping feature volume {} (user already specified)",
                container_path
            );
        }
    }

    // Apply environment variables (user config takes precedence)
    let feature_env = profile.get_env();
    for (key, value) in feature_env {
        config.environment.entry(key.clone()).or_insert_with(|| {
            tracing::debug!("Applying feature env: {}", key);
            value
        });
    }

    // Apply VNC resolution from config if specified (overrides feature default)
    if let Some(ref resolution) = config.vnc_resolution {
        tracing::debug!("Applying VNC resolution from config: {}", resolution);
        config
            .environment
            .insert("VNC_RESOLUTION".to_string(), resolution.clone());
    }

    // Apply command (only if user didn't specify one and features are enabled)
    if config.command.is_none() {
        if let Some(feature_command) = profile.get_command() {
            tracing::debug!("Applying feature command: {:?}", feature_command);
            config.command = Some(feature_command);
        }
    } else {
        tracing::debug!("Skipping feature command (user specified custom command)");
    }

    // Update config.features with the enabled features from profile
    // This ensures the features array reflects what was actually enabled
    if !profile.enabled_features.is_empty() {
        config.features = profile.enabled_features.clone();
        tracing::debug!(
            "Updated config.features with enabled features: {:?}",
            config.features
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_profile_empty() {
        let profile = FeatureProfile::empty();
        assert!(profile.available_features.is_empty());
        assert!(profile.enabled_features.is_empty());
        assert!(profile.default_command.is_none());
        assert!(!profile.is_feature_enabled("vnc"));
    }

    #[test]
    fn test_feature_profile_ports() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            FeatureDefinition {
                description: "VNC server".to_string(),
                ports: vec![FeaturePort {
                    host: 5901,
                    container: 5901,
                    protocol: "tcp".to_string(),
                    description: "x11vnc".to_string(),
                }],
                env: HashMap::new(),
                volumes: Vec::new(),
                static_server: None,
                enabled_by_default: true,
            },
        );

        let profile = FeatureProfile {
            available_features,
            enabled_features: vec!["vnc".to_string()],
            default_command: None,
        };

        let ports = profile.get_ports();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].host_port, 5901);
        assert_eq!(ports[0].container_port, 5901);
        assert_eq!(ports[0].protocol, PortProtocol::Tcp);
    }

    #[test]
    fn test_feature_profile_udp_port() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "syslog".to_string(),
            FeatureDefinition {
                description: "Syslog server".to_string(),
                ports: vec![FeaturePort {
                    host: 514,
                    container: 514,
                    protocol: "udp".to_string(),
                    description: "syslog".to_string(),
                }],
                env: HashMap::new(),
                volumes: Vec::new(),
                static_server: None,
                enabled_by_default: false,
            },
        );

        let profile = FeatureProfile {
            available_features,
            enabled_features: vec!["syslog".to_string()],
            default_command: None,
        };

        let ports = profile.get_ports();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].protocol, PortProtocol::Udp);
    }

    #[test]
    fn test_feature_profile_env() {
        let mut env = HashMap::new();
        env.insert("DISPLAY".to_string(), ":1".to_string());
        env.insert("VNC_PORT".to_string(), "5901".to_string());

        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            FeatureDefinition {
                description: "VNC server".to_string(),
                ports: Vec::new(),
                env,
                volumes: Vec::new(),
                static_server: None,
                enabled_by_default: true,
            },
        );

        let profile = FeatureProfile {
            available_features,
            enabled_features: vec!["vnc".to_string()],
            default_command: None,
        };

        let env_vars = profile.get_env();
        assert_eq!(env_vars.len(), 2);
        assert_eq!(env_vars.get("DISPLAY"), Some(&":1".to_string()));
        assert_eq!(env_vars.get("VNC_PORT"), Some(&"5901".to_string()));
    }

    #[test]
    fn test_feature_profile_command() {
        let profile = FeatureProfile {
            available_features: HashMap::new(),
            enabled_features: vec![],
            default_command: Some(vec!["sudo".to_string(), "/usr/bin/supervisord".to_string()]),
        };

        // No features enabled, should return None
        assert!(profile.get_command().is_none());

        let profile_with_features = FeatureProfile {
            available_features: HashMap::new(),
            enabled_features: vec!["vnc".to_string()],
            default_command: Some(vec!["sudo".to_string(), "/usr/bin/supervisord".to_string()]),
        };

        // Features enabled, should return command
        assert!(profile_with_features.get_command().is_some());
    }

    #[test]
    fn test_feature_volumes_dynamic_bind() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "static-server".to_string(),
            FeatureDefinition {
                description: "Static file server".to_string(),
                ports: Vec::new(),
                env: HashMap::new(),
                volumes: vec![FeatureVolume {
                    volume_type: "dynamic_bind".to_string(),
                    container_path: "/public".to_string(),
                    host_path: Some("/data/{sandbox_id}/public".to_string()),
                    read_only: false,
                    description: "Public files".to_string(),
                }],
                static_server: None,
                enabled_by_default: false,
            },
        );

        let profile = FeatureProfile {
            available_features,
            enabled_features: vec!["static-server".to_string()],
            default_command: None,
        };

        let volumes = profile.get_volumes("test-sandbox-id");
        assert_eq!(volumes.len(), 1);

        match &volumes[0] {
            VolumeMount::Bind {
                host_path,
                container_path,
                ..
            } => {
                assert!(host_path.contains("test-sandbox-id"));
                assert_eq!(container_path, "/public");
            }
            _ => panic!("Expected Bind volume"),
        }
    }

    #[test]
    fn test_feature_selection_default() {
        let selection = FeatureSelection::new();
        assert!(selection.enabled.is_empty());
        assert!(selection.disabled.is_empty());
        assert!(!selection.enable_all);
    }

    #[test]
    fn test_feature_selection_builder() {
        let selection = FeatureSelection::new()
            .with_enabled(vec!["vnc".to_string(), "browser".to_string()])
            .with_disabled(vec!["desktop".to_string()])
            .enable_all();

        assert_eq!(selection.enabled, vec!["vnc", "browser"]);
        assert_eq!(selection.disabled, vec!["desktop"]);
        assert!(selection.enable_all);
    }

    #[test]
    fn test_apply_feature_profile_ports() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            FeatureDefinition {
                description: "VNC server".to_string(),
                ports: vec![
                    FeaturePort {
                        host: 5901,
                        container: 5901,
                        protocol: "tcp".to_string(),
                        description: "x11vnc".to_string(),
                    },
                    FeaturePort {
                        host: 6080,
                        container: 6080,
                        protocol: "tcp".to_string(),
                        description: "noVNC".to_string(),
                    },
                ],
                env: HashMap::new(),
                volumes: Vec::new(),
                static_server: None,
                enabled_by_default: true,
            },
        );

        let profile = FeatureProfile {
            available_features,
            enabled_features: vec!["vnc".to_string()],
            default_command: None,
        };

        let mut config = SandboxConfig::default();

        apply_feature_profile(&mut config, &profile, "test-id").unwrap();

        // Feature ports should go to exposed_ports, not port_mappings
        assert_eq!(config.port_mappings.len(), 0);
        assert_eq!(config.exposed_ports, vec![5901, 6080]);
    }

    #[test]
    fn test_apply_feature_profile_user_port_precedence() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            FeatureDefinition {
                description: "VNC server".to_string(),
                ports: vec![FeaturePort {
                    host: 5901,
                    container: 5901,
                    protocol: "tcp".to_string(),
                    description: "x11vnc".to_string(),
                }],
                env: HashMap::new(),
                volumes: Vec::new(),
                static_server: None,
                enabled_by_default: true,
            },
        );

        let profile = FeatureProfile {
            available_features,
            enabled_features: vec!["vnc".to_string()],
            default_command: None,
        };

        let mut config = SandboxConfig {
            port_mappings: vec![PortMapping {
                host_port: 8080, // User mapped container port 5901 to host 8080
                container_port: 5901,
                protocol: PortProtocol::Tcp,
            }],
            ..Default::default()
        };

        apply_feature_profile(&mut config, &profile, "test-id").unwrap();

        // Should still only have 1 port mapping (user's)
        assert_eq!(config.port_mappings.len(), 1);
        assert_eq!(config.port_mappings[0].host_port, 8080); // User's mapping preserved
    }

    #[test]
    fn test_apply_feature_profile_env() {
        let mut env = HashMap::new();
        env.insert("DISPLAY".to_string(), ":1".to_string());

        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            FeatureDefinition {
                description: "VNC server".to_string(),
                ports: Vec::new(),
                env,
                volumes: Vec::new(),
                static_server: None,
                enabled_by_default: true,
            },
        );

        let profile = FeatureProfile {
            available_features,
            enabled_features: vec!["vnc".to_string()],
            default_command: None,
        };

        let mut config = SandboxConfig::default();

        apply_feature_profile(&mut config, &profile, "test-id").unwrap();

        assert_eq!(config.environment.get("DISPLAY"), Some(&":1".to_string()));
    }

    #[test]
    fn test_apply_feature_profile_user_env_precedence() {
        let mut env = HashMap::new();
        env.insert("DISPLAY".to_string(), ":1".to_string());

        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            FeatureDefinition {
                description: "VNC server".to_string(),
                ports: Vec::new(),
                env,
                volumes: Vec::new(),
                static_server: None,
                enabled_by_default: true,
            },
        );

        let profile = FeatureProfile {
            available_features,
            enabled_features: vec!["vnc".to_string()],
            default_command: None,
        };

        let mut config = SandboxConfig {
            environment: {
                let mut env = HashMap::new();
                env.insert("DISPLAY".to_string(), ":2".to_string()); // User override
                env
            },
            ..Default::default()
        };

        apply_feature_profile(&mut config, &profile, "test-id").unwrap();

        // User's value should take precedence
        assert_eq!(config.environment.get("DISPLAY"), Some(&":2".to_string()));
    }

    #[test]
    fn test_apply_feature_profile_command() {
        let profile = FeatureProfile {
            available_features: HashMap::new(),
            enabled_features: vec!["vnc".to_string()],
            default_command: Some(vec!["sudo".to_string(), "/usr/bin/supervisord".to_string()]),
        };

        let mut config = SandboxConfig {
            command: None,
            ..Default::default()
        };

        apply_feature_profile(&mut config, &profile, "test-id").unwrap();

        assert!(config.command.is_some());
        assert_eq!(
            config.command,
            Some(vec!["sudo".to_string(), "/usr/bin/supervisord".to_string()])
        );
    }

    #[test]
    fn test_apply_feature_profile_user_command_precedence() {
        let profile = FeatureProfile {
            available_features: HashMap::new(),
            enabled_features: vec!["vnc".to_string()],
            default_command: Some(vec!["sudo".to_string(), "/usr/bin/supervisord".to_string()]),
        };

        let mut config = SandboxConfig {
            command: Some(vec!["nginx".to_string()]), // User's command
            ..Default::default()
        };

        apply_feature_profile(&mut config, &profile, "test-id").unwrap();

        // User's command should take precedence
        assert_eq!(config.command, Some(vec!["nginx".to_string()]));
    }

    #[test]
    fn test_feature_static_server_declaration() {
        let feature_json = r#"{
            "version": "1.0",
            "features": {
                "webhost": {
                    "description": "Web hosting",
                    "static_server": {
                        "container_path": "/public",
                        "description": "Static file server"
                    },
                    "enabled_by_default": true
                }
            }
        }"#;

        let label: ImageFeatureLabel = serde_json::from_str(feature_json).unwrap();
        assert!(label.features["webhost"].static_server.is_some());
        assert_eq!(
            label.features["webhost"]
                .static_server
                .as_ref()
                .unwrap()
                .container_path,
            "/public"
        );
    }

    #[test]
    fn test_feature_profile_has_static_server() {
        let mut profile = FeatureProfile::empty();
        assert!(!profile.has_static_server());

        let feature = FeatureDefinition {
            description: "Web host".to_string(),
            ports: vec![],
            env: HashMap::new(),
            volumes: vec![],
            static_server: Some(FeatureStaticServer {
                container_path: "/public".to_string(),
                host_path_template: None,
                description: "Static files".to_string(),
            }),
            enabled_by_default: true,
        };

        profile
            .available_features
            .insert("webhost".to_string(), feature);
        profile.enabled_features.push("webhost".to_string());

        assert!(profile.has_static_server());
        assert!(profile.get_static_server_config().is_some());
    }

    #[test]
    fn test_feature_ports_go_to_exposed_not_mappings() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            FeatureDefinition {
                description: "VNC server".to_string(),
                ports: vec![
                    FeaturePort {
                        host: 5901,
                        container: 5901,
                        protocol: "tcp".to_string(),
                        description: "x11vnc".to_string(),
                    },
                    FeaturePort {
                        host: 6080,
                        container: 6080,
                        protocol: "tcp".to_string(),
                        description: "noVNC".to_string(),
                    },
                ],
                env: HashMap::new(),
                volumes: Vec::new(),
                static_server: None,
                enabled_by_default: true,
            },
        );

        let profile = FeatureProfile {
            available_features,
            enabled_features: vec!["vnc".to_string()],
            default_command: None,
        };

        let mut config = SandboxConfig::default();

        apply_feature_profile(&mut config, &profile, "test-id").unwrap();

        // Feature ports should NOT be in port_mappings (not published to host)
        assert_eq!(
            config.port_mappings.len(),
            0,
            "port_mappings should be empty"
        );

        // Feature ports should be in exposed_ports (internal networking)
        assert_eq!(config.exposed_ports, vec![5901, 6080]);
    }

    #[test]
    fn test_user_port_mappings_preserved_with_features() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            FeatureDefinition {
                description: "VNC server".to_string(),
                ports: vec![FeaturePort {
                    host: 5901,
                    container: 5901,
                    protocol: "tcp".to_string(),
                    description: "x11vnc".to_string(),
                }],
                env: HashMap::new(),
                volumes: Vec::new(),
                static_server: None,
                enabled_by_default: true,
            },
        );

        let profile = FeatureProfile {
            available_features,
            enabled_features: vec!["vnc".to_string()],
            default_command: None,
        };

        let mut config = SandboxConfig {
            port_mappings: vec![PortMapping {
                host_port: 8080,
                container_port: 5901,
                protocol: PortProtocol::Tcp,
            }],
            ..Default::default()
        };

        apply_feature_profile(&mut config, &profile, "test-id").unwrap();

        // User-specified port mapping should be preserved
        assert_eq!(config.port_mappings.len(), 1);
        assert_eq!(config.port_mappings[0].host_port, 8080);
        assert_eq!(config.port_mappings[0].container_port, 5901);

        // Feature port should be in exposed_ports
        assert!(config.exposed_ports.contains(&5901));
    }

    #[test]
    fn test_build_feature_profile_enables_default_features() {
        let label: ImageFeatureLabel = serde_json::from_str(
            r#"{
                "version": "1.0",
                "features": {
                    "browser": {
                        "description": "Browser",
                        "enabled_by_default": true
                    },
                    "databend": {
                        "description": "Databend",
                        "enabled_by_default": false
                    }
                },
                "default_command": ["supervisord"]
            }"#,
        )
        .unwrap();

        let profile = build_feature_profile(label, &FeatureSelection::new());

        assert_eq!(profile.enabled_features, vec!["browser".to_string()]);
        assert_eq!(
            profile.default_command,
            Some(vec!["supervisord".to_string()])
        );
    }
}
