// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! dsb-agent-tester
//!
//! E2E test agent for DSB that connects to dsb-mcp-server.
//! This crate provides tools for testing DSB capabilities through
//! a MCP client interface.

pub mod agents;
pub mod docker;
pub mod tests;

// Re-export DSBStack for convenience
pub use docker::DSBStack;
