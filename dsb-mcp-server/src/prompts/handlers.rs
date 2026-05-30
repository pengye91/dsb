// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! MCP prompt handlers
//!
//! This module handles incoming MCP prompt requests and routes them to the appropriate implementation.

use crate::prompts::templates;
use serde_json::Value;

/// List available prompts
pub fn handle_prompts_list() -> Value {
    templates::get_prompts_list()
}

/// Get a specific prompt with arguments
pub async fn handle_prompts_get(name: &str, arguments: Value) -> Result<Value, String> {
    templates::get_prompt_messages(name, arguments).await
}
