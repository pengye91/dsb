// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # DSB Static File Server - Library
//!
//! This library provides the core functionality for the DSB static file server.
//!
//! ⚠️ **STATUS**: Architecture backbone only - not yet functional.

#![warn(missing_docs)]

pub mod config;

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// Future exports:
// pub mod handlers;
// pub mod auth;
// pub mod serve;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_config_module_exists() {
        // Verify config module is accessible
        let _ = config::AuthMode::Public;
    }
}
