// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Agent implementations for dsb-agent-tester
//!
//! This module contains specialized agents that connect to the MCP server
//! and provide different capabilities for testing.

pub mod monorail;

pub use monorail::MonorailAgent;
