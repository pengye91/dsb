// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! DSB - Distributed Sandboxes CLI
//!
//! Command-line interface for managing Docker sandboxes.
//!
//! # Usage
//!
//! ```bash
//! dsb <COMMAND> [OPTIONS]
//! ```
//!
//! # Commands
//!
//! - `create` - Create a new sandbox
//! - `list` - List all sandboxes
//! - `info` - Get sandbox details
//! - `exec` - Execute a command in a sandbox
//! - `stop` - Stop a running sandbox
//! - `delete` - Delete a sandbox
//! - `stats` - Get sandbox resource statistics
//! - `cleanup` - Force cleanup sandbox resources
//! - `activities` - Activity tracking commands
//! - `server` - Start the API server
//!
//! # Examples
//!
//! ```bash
//! # Create a sandbox with port mapping
//! dsb create -i nginx:latest -n web-server -p 8080:80
//!
//! # List all sandboxes
//! dsb list
//!
//! # Get details
//! dsb info <sandbox-id>
//!
//! # Execute command
//! dsb exec <sandbox-id> -- ls -la /
//!
//! # Stop sandbox
//! dsb stop <sandbox-id>
//!
//! # Delete sandbox
//! dsb delete <sandbox-id>
//!
//! # Start API server
//! dsb server --port 8080
//!
//! # Activity tracking
//! dsb activities list
//! dsb activities cleanup-all
//! ```
//!
//! # SSH Session Management
//!
//! SSH session management is available via the API when PostgreSQL is enabled.
//! The SSH gateway service (separate from DSB) uses these endpoints to track
//! and manage SSH connections to sandboxes. See the [SSH Session Management
//! documentation](../../docs/ssh/SSH_SESSIONS.md) for details.
//!
//! # Environment Variables
//!
//! - `DSB_API_URL` - API server URL (default: `http://localhost:8080`)
//! - `DSB_API_KEY` - API key for authentication
//! - `DSB_ADMIN_API_KEY` - Admin API key for admin-only operations
//! - `DSB_SEARXNG_API_URL` - SearXNG API URL for `dsb web search`
//! - `DATABASE_URL` - PostgreSQL connection URL (enables persistent storage and SSH sessions)

use dsb::cli::run_cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load configuration
    let config = dsb::config::load()?;

    // Initialize logging with configuration
    dsb::logging::init_logging(&config)?;

    // Run CLI dispatcher
    run_cli().await
}
