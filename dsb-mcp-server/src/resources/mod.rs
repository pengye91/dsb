// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! MCP resources implementation
//!
//! This module provides resource definitions and handlers for MCP protocol.
//! Resources allow servers to expose data and context to clients.

pub mod handlers;

pub use handlers::{handle_resources_list, handle_resources_read};
