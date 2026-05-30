// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! MCP prompts implementation
//!
//! This module provides prompt template definitions and handlers for MCP protocol.
//! Prompts allow servers to expose reusable prompt templates to clients.

pub mod handlers;
pub mod templates;

pub use handlers::{handle_prompts_get, handle_prompts_list};
pub use templates::get_prompts_list;
