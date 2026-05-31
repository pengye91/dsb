// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Command-Line Interface
//!
//! This module provides the CLI for interacting with sandboxes.
//!
//! ## Commands
//!
//! - `create` - Create a new sandbox
//! - `list` - List all sandboxes
//! - `info` - Get sandbox details
//! - `exec` - Execute a command in a sandbox
//! - `ssh` - SSH into a sandbox (interactive shell)
//! - `stop` - Stop a running sandbox
//! - `delete` - Delete a sandbox
//! - `restore` - Restore a deleted sandbox
//! - `stats` - Get sandbox resource statistics
//! - `cleanup` - Force cleanup sandbox resources
//! - `health` - Check server health
//! - `config` - Get server configuration
//! - `upload` - Upload a file to a sandbox
//! - `download` - Download a file from a sandbox
//! - `tools` - Execute a tool/script in a sandbox
//! - `web` - Web fetch and search commands
//! - `images` - Docker image management (list, inspect, pull, delete)
//! - `static` - Static file management (list, tree, get, delete, download)
//! - `session-tokens` - Session token management (create, validate)
//! - `ssh-sessions` - SSH session management (list, show, terminate, stats)
//! - `activities` - Activity tracking commands
//! - `api-key` - API key management commands
//! - `server` - Start the API server

/// CLI command definitions and argument parsing.
pub mod commands;
/// Output formatting utilities for CLI display.
pub mod display;
/// Helper utilities for CLI operations.
pub mod utils;

pub use commands::{run_cli, Cli};
