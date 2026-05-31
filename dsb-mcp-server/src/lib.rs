// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! DSB MCP Server
//!
//! This crate provides a Model Context Protocol (MCP) server that exposes
//! DSB capabilities as MCP tools for LLMs.

#![warn(missing_docs)]

pub mod dsb_client;
pub mod dsb_service;
pub mod services;
pub mod session;
pub mod settings;
// pub mod prompts;  // Disabled - prompts not used
// pub mod resources;  // Disabled - resources not used
pub mod server;
pub mod tools;

pub use server::MCPServer;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exists() {
        // Test that the module compiles and exports work
        let _ = std::marker::PhantomData::<MCPServer>;
    }

    #[test]
    fn test_server_type_exists() {
        // Verify MCPServer is exported
        let _ = std::marker::PhantomData::<MCPServer>;
    }
}
