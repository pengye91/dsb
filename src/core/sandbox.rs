// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Sandbox Service - Core Business Logic
//!
//! This module provides the primary business logic for managing sandbox lifecycles.
//!
//! ## Overview
//!
//! The [`SandboxService`] acts as the orchestrator between the HTTP API layer and
//! the container backend (Docker, Kubernetes, etc.). It handles:
//!
//! - Sandbox creation and initialization
//! - State transitions and lifecycle management
//! - Command execution within containers
//! - Cleanup and resource management
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    HTTP API Layer                       │
//! │                   (Axum Handlers)                       │
//! └────────────────────────────┬────────────────────────────┘
//!                               │
//!                               ▼
//! ┌─────────────────────────────────────────────────────────┐
//! │                 SandboxService                         │
//! │  ┌─────────────────────────────────────────────────┐   │
//! │  │  - Orchestrates backend operations              │   │
//! │  │  - Manages state transitions                    │   │
//! │  │  - Handles business logic                       │   │
//! │  └─────────────────────────────────────────────────┘   │
//! │           │                   │                          │
//! │           ▼                   ▼                          │
//! │  ┌──────────────┐    ┌──────────────┐                 │
//! │  │SandboxManager│    │  StateStore  │                 │
//! │  │  (Docker)    │    │              │                 │
//! │  └──────────────┘    └──────────────┘                 │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Sandbox Lifecycle
//!
//! ```text
//! create_sandbox()
//!       │
//!       ├─► Generate UUID
//!       ├─► Create Sandbox with state=Creating
//!       ├─► Store in StateStore
//!       │
//!       ├─► Check pull_policy
//!       │        │
//!       │        ├─► Always: Pull image
//!       │        ├─► Missing: Check if exists, pull if missing
//!       │        └─► Never: Skip pull
//!       │
//!       ├─► backend.create()
//!       │        │
//!       │        ├─► Success → state=Created
//!       │        │
//!       │        └─► Error → state=Error, save error_message
//!       │
//!       └─► backend.start()
//!                │
//!                ├─► Success → state=Running
//!                │
//!                └─► Error → state=Error, save error_message
//! ```
//!
//! ## Usage Example
//!
//! ```rust,no_run,ignore
//! use dsb::core::{SandboxService, SandboxConfig, SandboxState};
//! use dsb::docker::DockerManager;
//! use dsb::core::StateStore;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let docker = DockerManager::new()?;
//!     let state = Arc::new(StateStore::new()) as Arc<dyn dsb::core::store_trait::StateStoreTrait + Send + Sync>;
//!     let service = SandboxService::new(Arc::new(docker), state);
//!
//!     // Create a sandbox
//!     let config = SandboxConfig {
//!         image: "nginx:latest".to_string(),
//!         ..Default::default()
//!     };
//!
//!     let sandbox = service.create_sandbox(config, None).await?;
//!     assert_eq!(sandbox.state, SandboxState::Running);
//!
//!     // Execute a command
//!     let output = service.exec_sandbox(&sandbox.id, vec![
//!         "ls".to_string(), "-la".to_string()
//!     ]).await?;
//!
//!     // Stop and cleanup
//!     service.stop_sandbox(&sandbox.id).await?;
//!     service.delete_sandbox(&sandbox.id).await?;
//!
//!     Ok(())
//! }
//! ```

use crate::core::manager::SandboxManager;
use crate::core::store_trait::StateStoreTrait;
use crate::core::types::SandboxState;
use std::sync::Arc;

/// Filter criteria for listing sandboxes with pagination.
///
/// Used with [`SandboxService::list_sandboxes_filtered`] to apply
/// advanced filtering and pagination to sandbox queries.
#[derive(Debug, Clone, Default)]
pub struct ListSandboxesFilter {
    /// Include deleted sandboxes in the results.
    pub include_deleted: bool,
    /// Filter by sandbox state (e.g., `Running`, `Stopped`).
    pub state: Option<SandboxState>,
    /// Filter by image name (partial match).
    pub image: Option<String>,
    /// Only include sandboxes created after this timestamp.
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Only include sandboxes created before this timestamp.
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
    /// Page number for pagination (1-based).
    pub page: Option<usize>,
    /// Number of items per page.
    pub per_page: Option<usize>,
}

/// Request parameters for executing a tool via HTTP inside a sandbox.
///
/// Used with [`SandboxService::exec_tool_http`] to run tool scripts
/// through the sandbox's internal HTTP tool proxy.
#[derive(Debug, Clone)]
pub struct ExecToolHttpRequest {
    /// Interpreter to use (e.g., "python", "node", "sh").
    pub interpreter: String,
    /// Path to the tool script (e.g., "web_tools", "databend_tools").
    pub script_path: String,
    /// Specific action to perform (e.g., "scrape", "execute", "navigate").
    pub action: String,
    /// JSON arguments to pass to the action.
    pub args: serde_json::Value,
    /// Optional timeout in seconds (uses config defaults if not specified).
    pub timeout: Option<u64>,
    /// Optional environment variables to set for the tool execution.
    pub environment: Option<std::collections::HashMap<String, String>>,
}

/// Core service for managing sandbox lifecycle.
///
/// Coordinates between the backend container manager (Docker/K8s) and the state store,
/// handling creation, deletion, execution, and monitoring of sandboxes.
#[derive(Clone)]
pub struct SandboxService {
    /// Backend sandbox manager for container operations (Docker, Kubernetes, etc.)
    pub backend: Arc<dyn SandboxManager>,

    /// State store for tracking sandbox metadata (can be in-memory or PostgreSQL)
    state: Arc<dyn StateStoreTrait + Send + Sync>,

    /// Optional activity service for tracking sandbox operations
    /// Only available when using PostgreSQL backend
    activity_service: Option<Arc<crate::core::activities::ActivityService>>,

    /// Default inactivity timeout in minutes
    pub default_inactivity_timeout: u64,
    /// Whether cleanup runs in dry-run mode
    pub cleanup_dry_run: bool,
    /// Interval for state monitoring in seconds
    pub state_monitor_interval: u64,
    /// Number of days to retain deleted sandboxes
    pub deleted_sandbox_retention_days: u64,

    /// Default sandbox image (for frontend config)
    pub default_sandbox_image: String,

    /// Whether authentication is required (for frontend display)
    pub authentication_required: bool,

    /// Maximum file size for sandbox file uploads/downloads in bytes (default: 10MB)
    pub max_file_size_bytes: u64,

    /// Tool execution timeout configuration
    pub tool_timeouts: crate::config::ToolTimeoutConfig,

    /// Default resource limits for sandbox creation
    /// Applied when creating sandboxes without explicit resource limits
    pub default_resource_limits: crate::config::DefaultResourceLimits,

    /// Maximum number of browser tabs per sandbox (default: 20)
    ///
    /// When a sandbox opens more tabs than this limit, the oldest tabs
    /// are automatically closed (FIFO eviction). This prevents resource
    /// exhaustion from unbounded tab growth while allowing VNC users to
    /// see multiple visited pages in the browser tab strip.
    pub max_browser_tabs: u32,
}

mod constructor;
mod create;
mod query;
mod lifecycle;
mod exec;
mod files;
mod activity;
mod cleanup;
mod tasks;

#[cfg(test)]
mod tests;
