// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Configuration Module
//!
//! This module provides centralized configuration management for the DSB application.
//!
//! ## Features
//!
//! - **Multiple configuration sources**: Defaults, .env files, YAML files, environment variables, CLI arguments
//! - **Hierarchical priority**: CLI args > Environment vars > Config files > Defaults
//! - **Type-safe**: Uses serde for compile-time type checking
//! - **Validation**: Fail-fast validation at startup
//! - **Test support**: Separate configuration for tests
//!
//! ## Quick Start
//!
//! ### Basic Usage
//!
//! ```rust,no_run,ignore
//! use dsb::config;
//!
//! // Load configuration (automatically checks .env, YAML, env vars)
//! let config = config::load()?;
//! # Ok::<(), config::Error>(())
//! ```
//!
//! ### With CLI Arguments
//!
//! ```rust,no_run,ignore
//! use dsb::config;
//! use std::collections::HashMap;
//!
//! let mut cli_args = HashMap::new();
//! cli_args.insert("server.port".to_string(), "9000".to_string());
//!
//! let config = config::load_with_cli_args(Some(cli_args))?;
//! # Ok::<(), config::Error>(())
//! ```
//!
//! ### Test Configuration
//!
//! ```rust,no_run,ignore
//! use dsb::config;
//!
//! // Loads test-specific config (.env.test, dsb.test.yaml)
//! let config = config::load_for_tests()?;
//! # Ok::<(), config::Error>(())
//! ```
//!
//! ## Configuration Sources (Priority Order)
//!
//! 1. **Default values** (from code)
//! 2. **.env file** (if found in current or parent directory)
//! 3. **YAML file** (dsb.yaml or dsb.yml, if found)
//! 4. **Environment variables** (DSB_ prefix)
//! 5. **CLI arguments** (highest priority)
//!
//! ## Environment Variables
//!
//! All environment variables use the `DSB_` prefix with double underscore (`__`) for nested keys:
//!
//! ```bash
//! # Server configuration
//! export DSB_SERVER__PORT=8080
//! export DSB_SERVER__HOST=0.0.0.0
//! export DSB_SERVER__API_KEY=your-api-key
//!
//! # Database configuration
//! export DSB_DATABASE__URL=postgresql://user:pass@localhost:5432/db
//! export DSB_DATABASE__HOST=localhost
//! export DSB_DATABASE__PORT=5432
//!
//! # Docker configuration
//! export DSB_DOCKER__REGISTRY=docker.io
//! export DSB_DOCKER__HOST=unix:///var/run/docker.sock
//!
//! # Logging configuration
//! export DSB_LOGGING__LEVEL=info
//! ```
//!
//! ## Configuration File Format
//!
//! ```bash
//! export DSB_DOCKER__REGISTRY=docker.io
//! # Or via .env file
//! DSB_DOCKER__REGISTRY=docker.io
//! ```
//!
//! ## Configuration Priority
//!
//! 1. Command line arguments (highest)
//! 2. Environment variables
//! 3. .env file
//! 4. dsb.yaml config file
//! 5. Default values (lowest)
//!
//! ## Default Configuration
//!
//! ```yaml
//! server:
//!   port: 8080
//!   host: "0.0.0.0"
//! docker:
//!   registry: docker.io
//!   host: unix:///var/run/docker.sock
//!   default_image: docker.io/dsb/sandbox:latest
//!   test_image: python:3.12
//! ```
//!
//! ### .env File
//!
//! ```bash
//! DSB_SERVER__PORT=8080
//! DSB_SERVER__HOST=0.0.0.0
//! DSB_DATABASE__HOST=localhost
//! DSB_DATABASE__PORT=5432
//! DSB_DOCKER__REGISTRY=docker.io
//! DSB_LOGGING__LEVEL=info
//! ```
//!
//! ## Configuration Structure
//!
//! The configuration is organized into logical sections:
//!
//! - **Server**: HTTP server settings (port, host, API keys)
//! - **Database**: PostgreSQL connection settings
//! - **Docker**: Docker daemon and image registry settings
//! - **Sandbox**: Sandbox default behavior (timeouts, cleanup)
//! - **SSH**: SSH gateway settings
//! - **Logging**: Log level and format
//!
//! ## Examples
//!
//! ### Override Server Port
//!
//! ```rust,no_run,ignore
//! use dsb::config;
//! use std::collections::HashMap;
//!
//! let mut cli_args = HashMap::new();
//! cli_args.insert("server.port".to_string(), "9000".to_string());
//!
//! let config = config::load_with_cli_args(Some(cli_args))?;
//! assert_eq!(config.server.port, 9000);
//! # Ok::<(), config::Error>(())
//! ```
//!
//! ### Access Configuration Values
//!
//! ```rust,no_run,ignore
//! use dsb::config;
//!
//! let config = config::load()?;
//!
//! // Access nested configuration
//! let port = config.server.port;
//! let registry = config.docker.registry;
//! let db_url = config.database.url.as_deref();
//! # Ok::<(), config::Error>(())
//! ```
//!
//! ## Migration Guide
//!
//! If you're upgrading from hardcoded values or environment variables, here's how to migrate:
//!
//! ### Old Environment Variables
//!
//! ```bash
//! # Old (no longer works)
//! export DATABASE_URL=postgresql://...
//! export DB_HOST=localhost
//! export DOCKER_HOST=unix:///var/run/docker.sock
//! ```
//!
//! ### New Environment Variables
//!
//! ```bash
//! # New (use DSB_ prefix and __ separator)
//! export DSB_DATABASE__URL=postgresql://...
//! export DSB_DATABASE__HOST=localhost
//! export DSB_DOCKER__HOST=unix:///var/run/docker.sock
//! ```
//!
//! ## Error Handling
//!
//! Configuration loading returns `Result<Config, Error>`:
//!
//! ```rust,no_run,ignore
//! use dsb::config;
//!
//! match config::load() {
//!     Ok(config) => {
//!         // Application starts with validated configuration
//!     }
//!     Err(e) => {
//!         eprintln!("Configuration error: {}", e);
//!         std::process::1);
//!     }
//! }
//! ```

// Re-export error type at module root for convenience
pub use error::ConfigError as Error;

// Public API functions
pub use loader::{
    load, load_for_tests, load_with_cli_args, load_with_cli_args_and_files, load_with_files,
};

// Re-export types
pub use types::{
    BackendType, Config, DatabaseConfig, DefaultResourceLimits, DefaultUlimit, DockerConfig,
    GpuConfig, KubernetesConfig, KubernetesResourceDefaults, LoggingConfig, SandboxConfig,
    ServerConfig, SshConfig, StaticServerConfig, ToolTimeoutConfig,
};

// Private submodules
mod error;
mod loader;
mod types;
mod validator;
