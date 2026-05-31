// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! MCP service implementations
//!
//! This module provides service-level implementations that combine the DSB client,
//! session manager, and settings into cohesive MCP tool handlers. Each service
//! implements `rmcp::ServerHandler` with `#[tool_router]` for declarative tool
//! registration.

pub mod browser;
pub mod sandbox;
pub mod system;
pub mod terminal;
pub mod value_retrieval;
pub mod web;
