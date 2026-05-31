// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! MCP tool implementations
//!
//! This module provides tool implementations for MCP protocol via the rmcp
//! `#[tool_router]` and `#[tool]` macros in `dsb_service.rs`.

pub mod browser;
pub mod exec;
pub mod sandbox;
pub mod web;
