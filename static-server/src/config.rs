// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Static Server Configuration Extensions
//!
//! This module defines configuration extensions specific to the static file server.
//!
//! ⚠️ **STATUS**: Placeholder for future implementation.

/// Authentication mode for the static file server
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthMode {
    /// No authentication required - public access
    #[default]
    Public,

    /// Require authentication for all requests
    Authenticated,

    /// Mixed mode - some files public, some require auth
    Mixed,
}

/// Static server configuration extensions
///
/// This structure will hold configuration specific to the standalone static server,
/// separate from the main DSB configuration.
///
/// ⚠️ **STATUS**: Placeholder for future implementation.
#[derive(Debug, Clone)]
pub struct StaticServerConfig {
    /// Port to listen on (default: 8081)
    pub port: u16,

    /// DSB API URL for authentication validation
    pub dsb_api_url: Option<String>,

    /// API key for DSB API authentication
    pub api_key: Option<String>,

    /// Authentication mode
    pub auth_mode: AuthMode,
}

impl Default for StaticServerConfig {
    fn default() -> Self {
        Self {
            port: 8081,
            dsb_api_url: None,
            api_key: None,
            auth_mode: AuthMode::Public,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_mode_default() {
        assert_eq!(AuthMode::default(), AuthMode::Public);
    }

    #[test]
    fn test_static_server_config_default() {
        let config = StaticServerConfig::default();
        assert_eq!(config.port, 8081);
        assert_eq!(config.dsb_api_url, None);
        assert_eq!(config.api_key, None);
        assert_eq!(config.auth_mode, AuthMode::Public);
    }

    #[test]
    fn test_static_server_config_clone() {
        let config = StaticServerConfig {
            port: 9000,
            dsb_api_url: Some("http://localhost:8080".to_string()),
            api_key: Some("secret".to_string()),
            auth_mode: AuthMode::Authenticated,
        };

        let cloned = config.clone();
        assert_eq!(cloned.port, 9000);
        assert_eq!(
            cloned.dsb_api_url,
            Some("http://localhost:8080".to_string())
        );
        assert_eq!(cloned.api_key, Some("secret".to_string()));
        assert_eq!(cloned.auth_mode, AuthMode::Authenticated);
    }
}
