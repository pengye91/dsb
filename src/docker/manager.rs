// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! # Docker Container Management
//!
//! This module provides a high-level interface for managing Docker containers
//! using the Bollard library.
//!
//! ## Overview
//!
//! The [`DockerManager`] abstracts away the complexity of the Docker API and provides
//! simple methods for:
//!
//! - Creating containers with custom configurations
//! - Starting and stopping containers
//! - Removing containers and their volumes
//! - Pulling images from registries
//! - Executing commands inside running containers
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────┐
//! │              DockerManager                          │
//! │                                                      │
//! │  ┌────────────────────────────────────────────┐     │
//! │  │     Arc<Docker> (Bollard client)           │     │
//! │  └────────────────────────────────────────────┘     │
//! │                       │                              │
//! │                       ▼                              │
//! │  ┌────────────────────────────────────────────┐     │
//! │  │         Docker Daemon                      │     │
//! │  │  (via UNIX socket / TCP)                   │     │
//! │  └────────────────────────────────────────────┘     │
//! └──────────────────────────────────────────────────────┘
//! ```
//!
//! ## Thread Safety
//!
//! `DockerManager` uses `Arc<Docker>` internally, making it safe and cheap to clone.
//! Multiple threads can share a single manager instance.
//!
//! ## Usage Example
//!
//! ```rust,no_run,ignore
//! use dsb::docker::DockerManager;
//! use dsb::core::{SandboxConfig, PortMapping, PortProtocol};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), dsb::docker::DockerManagerError> {
//!     let docker = DockerManager::new()?;
//!
//!     // Create a container
//!     let config = SandboxConfig {
//!         image: "nginx:latest".to_string(),
//!         name: Some("my-nginx".to_string()),
//!         port_mappings: vec![
//!             PortMapping {
//!                 host_port: 8080,
//!                 container_port: 80,
//!                 protocol: PortProtocol::Tcp,
//!             }
//!         ],
//!         ..Default::default()
//!     };
//!
//!     let container_id = docker.create_container(&config).await?;
//!     println!("Created container: {}", container_id);
//!
//!     // Start it
//!     docker.start_container(&container_id).await?;
//!
//!     // Execute a command
//!     let output = docker.exec_container(
//!         &container_id,
//!         vec!["ls".to_string(), "-la".to_string()]
//!     ).await?;
//!     println!("Output:\n{}", output);
//!
//!     // Cleanup
//!     docker.remove_container(&container_id).await?;
//!
//!     Ok(())
//! }
//! ```

use bollard::models::ContainerSummary;
use bollard::Docker;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::Config;
use crate::core::types::SandboxInfo;

/// Converts a Docker `ContainerSummary` into a backend-agnostic `SandboxInfo`.
impl From<ContainerSummary> for SandboxInfo {
    fn from(cs: ContainerSummary) -> Self {
        let ports = cs
            .ports
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.public_port.map(|hp| (hp, p.private_port)))
            .collect();

        Self {
            id: cs.id.unwrap_or_default(),
            name: cs
                .names
                .and_then(|n| n.first().map(|s| s.trim_start_matches('/').to_string())),
            image: cs.image,
            state: cs.state.as_ref().map(|s| s.to_string()),
            status: cs.status,
            created: cs.created,
            ports,
            labels: cs.labels.unwrap_or_default(),
            node_name: None,
            pod_ip: None,
        }
    }
}

/// Expand tilde (~) to home directory in a path.
fn expand_tilde(path: &str, home_dir: Option<&str>) -> String {
    if path.starts_with("~/") {
        let home = home_dir
            .map(|s| s.to_string())
            .or_else(|| std::env::var("HOME").ok())
            .or_else(|| std::env::var("USERPROFILE").ok());
        if let Some(home) = home {
            return path.replacen("~", &home, 1);
        }
    }
    path.to_string()
}

/// High-level interface for managing Docker containers.
#[derive(Clone)]
pub struct DockerManager {
    pub(crate) docker: Arc<Docker>,
    pub(crate) config: Arc<Config>,
    pub(crate) http_client: reqwest::Client,
    pub(crate) ip_cache: Arc<std::sync::RwLock<HashMap<String, String>>>,
}

mod constructor;
mod container;
mod error;
mod exec;
mod image;
mod inspect;
mod sandbox;
mod terminal;
mod traits;

pub use error::DockerManagerError;
pub use terminal::DockerTerminalStream;

#[cfg(test)]
mod mock_tests;
#[cfg(test)]
mod tests;
