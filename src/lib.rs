// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # DSB - Distributed Sandboxes
//!
//! A fast, minimal Docker sandbox manager for ephemeral container environments.

#![warn(missing_docs)]
//!
//! ## Overview
//!
//! DSB (Distributed Sandboxes) provides a simple API and CLI for managing Docker containers
//! as isolated sandbox environments. It's designed for scenarios requiring:
//!
//! - **Ephemeral environments**: Quick spin-up and tear-down of test environments
//! - **API-driven management**: HTTP REST API for programmatic control
//! - **Resource isolation**: CPU, memory, and process limits per sandbox
//! - **Command execution**: Run arbitrary commands inside running containers
//!
//! ## Architecture
//!
//! ```text
//!                ┌─────────────┐     ┌─────────────┐
//!                │ CLI Client  │     │ HTTP Client │
//!                └──────┬──────┘     └──────┬──────┘
//!                       │                   │
//!                       └────────┬──────────┘
//!                                ▼
//!                        ┌───────────────┐
//!                        │  Axum API     │
//!                        │  Handlers     │
//!                        └───────┬───────┘
//!                                │
//!                                ▼
//!                        ┌───────────────┐
//!                        │ SandboxService│
//!                        └───────┬───────┘
//!                         │             │
//!                         ▼             ▼
//!                   ┌──────────┐  ┌──────────┐
//!                   │  Docker  │  │   State  │
//!                   │  Manager │  │   Store  │
//!                   └──────────┘  └──────────┘
//! ```
//!
//! ## Quick Start
//!
//! ### Using the Library
//!
//! ```rust,no_run,ignore
//! use dsb::core::{SandboxService, SandboxConfig};
//! use dsb::docker::DockerManager;
//! use dsb::core::StateStore;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//!     let docker = DockerManager::new()?;
//!     let state = Arc::new(StateStore::new()) as Arc<dyn dsb::core::store_trait::StateStoreTrait + Send + Sync>;
//!     let service = SandboxService::new(docker, state);
//!
//!     let config = SandboxConfig {
//!         image: "nginx:latest".to_string(),
//!         ..Default::default()
//!     };
//!
//!     let sandbox = service.create_sandbox(config, None).await?;
//!     println!("Created sandbox: {}", sandbox.id);
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Running the Server
//!
//! ```bash
//! # Start API server
//! cargo run --bin dsb -- server --port 8080
//!
//! # Create a sandbox via CLI
//! cargo run --bin dsb -- create -i nginx:latest -n my-nginx -p 8080:80
//!
//! # List sandboxes
//! cargo run --bin dsb -- list
//!
//! # Execute command
//! cargo run --bin dsb -- exec <id> -- ls -la
//! ```
//!
//! ## Features
//!
//! - **Fast container creation**: Optimized for speed with pre-pulled images
//! - **RESTful API**: Full CRUD operations over HTTP
//! - **CLI interface**: Command-line tool for manual operations
//! - **Resource limits**: Control CPU, memory, and process counts
//! - **Port mapping**: Forward host ports to container ports
//! - **Command execution**: Run commands in running containers
//!
//! ## Modules
//!
//! - [`config`] - Configuration management
//! - [`core`] - Core business logic and types
//! - [`docker`] - Docker container management
//! - [`api`] - HTTP API server and handlers
//! - [`cli`] - Command-line interface
//! - [`db`] - Database persistence
//! - [`logging`] - Logging initialization and configuration
//! - [`utils`] - Utility functions
//! - [`web_terminal`] - Web terminal support
//! - [`vnc_proxy`] - VNC proxy support
//! - `k8s` - Kubernetes backend (optional, requires "kubernetes" feature)

/// HTTP API server and request handlers.
///
/// Provides the REST API for sandbox management, authentication, and health checks.
pub mod api;
/// Command-line interface for DSB operations.
///
/// Supports creating, listing, stopping, and deleting sandboxes from the terminal.
pub mod cli;
/// Centralized configuration management.
///
/// Loads and merges settings from defaults, .env files, YAML config files,
/// environment variables, and CLI arguments with hierarchical priority.
pub mod config;
/// Core business logic and domain types.
///
/// Contains the [`SandboxService`](core::SandboxService), state store traits,
/// sandbox lifecycle types, and feature management.
pub mod core;
/// Database persistence layer.
///
/// Provides PostgreSQL-backed state storage for production deployments.
pub mod db;
/// Docker container management backend.
///
/// Implements the [`SandboxManager`](core::manager::SandboxManager) trait
/// using the Docker daemon via bollard.
pub mod docker;
/// Logging initialization and configuration.
///
/// Sets up structured logging with configurable format (pretty/JSON) and level.
pub mod logging;
/// Session token generation and validation for VNC authentication.
pub mod session_token;
/// Background task scheduling and management.
pub mod tasks;
/// Testing utilities and helpers.
pub mod testing;
/// Shared utility functions.
pub mod utils;
/// VNC proxy for browser-based remote desktop access.
pub mod vnc_proxy;
/// Web terminal support for interactive shell sessions in the browser.
pub mod web_terminal;

#[cfg(feature = "kubernetes")]
/// Kubernetes backend for managing sandbox pods.
///
/// Implements the [`SandboxManager`](core::manager::SandboxManager) trait
/// using the Kubernetes API via kube-rs. Enabled by the `kubernetes` feature.
pub mod k8s;
