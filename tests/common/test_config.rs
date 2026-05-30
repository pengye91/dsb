// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Test configuration utilities
//!
//! Provides helper functions for tests to load and use configuration
//! consistently with the application's configuration system.
//!
//! ## Infrastructure Dependency Injection
//!
//! Tests can run against different deployments (Docker Compose, EKS, local dev)
//! by setting environment variables. The [`TestInfraConfig`] struct centralizes
//! all infrastructure settings.
//!
//! ### Quick Start
//!
//! ```bash
//! # Default: run against local docker-compose stack
//! cargo test --test api_key_integration_tests -- --test-threads=1
//!
//! # Against EKS with port-forwards
//! export DSB_TEST_API_URL=http://127.0.0.1:28080
//! export DSB_TEST_DATABASE_URL=postgresql://postgres:pass@127.0.0.1:15433/dsb
//! export DSB_TEST_API_KEY=test-admin-key-for-testing-only
//! cargo test --test api_key_integration_tests -- --test-threads=1
//! ```
//!
//! ### Environment Variables
//!
//! | Variable | Fallback | Default | Description |
//! |----------|----------|---------|-------------|
//! | `DSB_TEST_API_URL` | `DSB_API_URL` | `http://127.0.0.1:18080` | Full API base URL |
//! | `DSB_TEST_DATABASE_URL` | `DATABASE_URL` | built from `DSB_DATABASE__*` | Full DB connection URL |
//! | `DSB_TEST_API_KEY` | `DSB_API_KEY` | `test-admin-key-for-testing-only` | Admin API key |
//! | `DSB_TEST_SANDBOX_IMAGE` | `DSB_SANDBOX_IMAGE` | `dsb/sandbox:latest` | Sandbox Docker image |
//! | `DSB_TEST_DOCKER_SOCKET` | `DOCKER_HOST` | auto-detected | Docker socket path |
//! | `DSB_TEST_BACKEND` | — | `docker` | Backend type hint |
//! | `DSB_TEST_SSH_API_URL` | `DSB_SSH__API_URL` | `http://127.0.0.1:2222` | SSH gateway URL |
//!
//! When running inside a Docker container (`/.dockerenv` exists) with
//! `DOCKER_COMPOSE_TEST=true`, defaults switch to the docker-compose internal
//! network (`dsb-server-test:8080`, `postgres-test:5432`).

use dsb::config;

// ============================================================================
// TestInfraConfig — dependency injection for test infrastructure
// ============================================================================

/// Centralized infrastructure configuration for integration tests.
///
/// All test fixtures should read from this config rather than hardcoding
/// hosts, ports, or credentials. Values are loaded from environment variables
/// with sensible defaults so `cargo test` works out of the box.
#[derive(Debug, Clone)]
pub struct TestInfraConfig {
    /// Full API base URL, e.g. `http://127.0.0.1:18080`
    pub api_base_url: String,
    /// Full database connection URL
    pub database_url: String,
    /// Admin API key for authenticated requests
    pub api_key: String,
    /// Sandbox Docker image to use for test sandboxes
    pub sandbox_image: String,
    /// Docker socket path (for tests that interact with Docker directly)
    pub docker_socket: String,
    /// Backend type: `docker` or `kubernetes`
    pub backend: String,
    /// SSH gateway API URL
    pub ssh_api_url: String,
    /// Whether tests are running inside a Docker container
    #[allow(dead_code)]
    pub inside_docker: bool,
    /// Whether the docker-compose test environment is active
    #[allow(dead_code)]
    pub docker_compose_test: bool,
}

impl TestInfraConfig {
    /// Load infrastructure configuration from environment variables.
    ///
    /// Reads variables in priority order (explicit → fallback → default).
    /// When running inside a Docker container with `DOCKER_COMPOSE_TEST=true`,
    /// defaults switch to the docker-compose internal network.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// use tests::common::test_config::TestInfraConfig;
    ///
    /// let config = TestInfraConfig::from_env();
    /// println!("Testing against: {}", config.api_base_url);
    /// ```
    pub fn from_env() -> Self {
        let inside_docker = std::path::Path::new("/.dockerenv").exists();
        let docker_compose_test =
            inside_docker && std::env::var("DOCKER_COMPOSE_TEST").is_ok();

        // API base URL
        let api_base_url = if docker_compose_test {
            "http://dsb-server-test:8080".to_string()
        } else {
            Self::env_with_fallback("DSB_TEST_API_URL", "DSB_API_URL")
                .unwrap_or_else(|| "http://127.0.0.1:18080".to_string())
        };

        // Database URL
        let database_url = if docker_compose_test {
            "postgresql://postgres_test:postgres_test_password@postgres-test:5432/dsb_test"
                .to_string()
        } else {
            let url = Self::env_with_fallback("DSB_TEST_DATABASE_URL", "DATABASE_URL")
                .unwrap_or_else(Self::build_default_database_url);
            // Rewrite docker-compose internal hostnames for host-side testing
            Self::rewrite_docker_hostname(&url)
        };

        // API key
        let api_key = Self::env_with_fallback("DSB_TEST_API_KEY", "DSB_API_KEY")
            .unwrap_or_else(|| "test-admin-key-for-testing-only".to_string());

        // Sandbox image
        let sandbox_image =
            Self::env_with_fallback("DSB_TEST_SANDBOX_IMAGE", "DSB_SANDBOX_IMAGE")
                .unwrap_or_else(|| "dsb/sandbox:latest".to_string());

        // Docker socket
        let docker_socket =
            Self::env_with_fallback("DSB_TEST_DOCKER_SOCKET", "DOCKER_HOST")
                .unwrap_or_else(Self::detect_docker_socket);

        // Backend type
        let backend = std::env::var("DSB_TEST_BACKEND").unwrap_or_else(|_| "docker".to_string());

        // SSH API URL
        let ssh_api_url =
            Self::env_with_fallback("DSB_TEST_SSH_API_URL", "DSB_SSH__API_URL")
                .unwrap_or_else(|| "http://127.0.0.1:2222".to_string());

        Self {
            api_base_url,
            database_url,
            api_key,
            sandbox_image,
            docker_socket,
            backend,
            ssh_api_url,
            inside_docker,
            docker_compose_test,
        }
    }

    /// Extract host and port from the API base URL.
    ///
    /// Returns `(host, port)` for tests that need them separately.
    /// Panics if the URL is malformed (tests should crash early on bad config).
    pub fn api_host_port(&self) -> (String, u16) {
        let url = self.api_base_url.parse::<url::Url>().unwrap_or_else(|_| {
            panic!("Invalid DSB_TEST_API_URL: {}", self.api_base_url)
        });
        let host = url.host_str()
            .unwrap_or("127.0.0.1")
            .to_string();
        let port = url.port().unwrap_or_else(|| match url.scheme() {
            "https" => 443,
            _ => 80,
        });
        (host, port)
    }

    /// Return the database URL with a different database name.
    ///
    /// Parses the configured `database_url`, replaces the path segment
    /// (database name), and returns the new URL. Useful for tests that
    /// need isolated databases with the same host/port/credentials.
    ///
    /// # Panics
    ///
    /// Panics if `database_url` is not a valid PostgreSQL URL.
    pub fn database_url_with_name(&self, name: &str) -> String {
        let mut url = self.database_url.parse::<url::Url>().unwrap_or_else(|_| {
            panic!("Invalid database URL: {}", self.database_url)
        });
        url.set_path(&format!("/{}", name));
        url.to_string()
    }

    /// Helper: read env var with fallback.
    fn env_with_fallback(primary: &str, fallback: &str) -> Option<String> {
        std::env::var(primary)
            .ok()
            .or_else(|| std::env::var(fallback).ok())
            .filter(|s| !s.is_empty())
    }

    /// Rewrite docker-compose internal hostnames to localhost equivalents.
    fn rewrite_docker_hostname(url: &str) -> String {
        if url.contains("postgres-test:5432") {
            url.replace("postgres-test:5432", "127.0.0.1:15432")
        } else if url.contains("postgres-test") {
            url.replace("postgres-test", "127.0.0.1:15432")
        } else {
            url.to_string()
        }
    }

    /// Build default database URL from DSB_DATABASE__* components.
    fn build_default_database_url() -> String {
        let cfg = load_test_config();

        if let Some(url) = &cfg.database.url {
            return Self::rewrite_docker_hostname(url);
        }

        let password = cfg
            .database
            .password
            .as_deref()
            .unwrap_or("postgres_test_password");

        let host = std::env::var("DSB_DATABASE__HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

        let port: u16 = std::env::var("DSB_DATABASE__PORT")
            .unwrap_or_else(|_| "15432".to_string())
            .parse()
            .unwrap_or(15432);

        let user = if cfg.database.user == "postgres" {
            "postgres_test"
        } else {
            &cfg.database.user
        };

        let db = if cfg.database.name == "dsb" {
            "dsb_test"
        } else {
            &cfg.database.name
        };

        format!("postgresql://{}:{}@{}:{}/{}", user, password, host, port, db)
    }

    /// Auto-detect Docker socket path.
    fn detect_docker_socket() -> String {
        if let Ok(home) = std::env::var("HOME") {
            let docker_desktop = std::path::Path::new(&home).join(".docker/run/docker.sock");
            if docker_desktop.exists() {
                return format!("unix://{}", docker_desktop.display());
            }
        }
        "unix:///var/run/docker.sock".to_string()
    }
}

// ============================================================================
// Legacy helpers (kept for backward compatibility)
// ============================================================================

/// Load test configuration.
///
/// This function loads configuration specifically for tests using the config system.
/// It will load from (in order of priority):
///
/// 1. `dsb.test.yaml` - Test-specific config file
/// 2. `.env.test` - Test environment variables
/// 3. Environment variables with `DSB_` prefix
/// 4. Default test values
///
/// # Returns
///
/// Configuration object properly loaded for testing
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::test_config::load_test_config;
///
/// let config = load_test_config();
/// let db_url = &config.database.url;
/// let docker_host = &config.docker.host;
/// ```
pub fn load_test_config() -> config::Config {
    // Set environment variable to tell config loader to use test files
    std::env::set_var("DSB_CONFIG_FILE", "dsb.test.yaml");
    std::env::set_var("DSB_ENV_FILE", ".env.test");

    // Load config using the standard config loader
    let cfg = config::load().unwrap_or_default();

    // Clean up
    std::env::remove_var("DSB_CONFIG_FILE");
    std::env::remove_var("DSB_ENV_FILE");

    cfg
}

/// Get the test Docker socket path from configuration.
///
/// This is a convenience helper for tests that need the Docker socket path
/// as a string. New tests should use [`TestInfraConfig`] directly.
///
/// # Returns
///
/// Docker socket path from test configuration
#[allow(dead_code)]
pub fn get_test_docker_socket() -> String {
    TestInfraConfig::from_env().docker_socket
}

/// Get test database URL from configuration.
///
/// This is a convenience helper for tests that need a database URL string.
/// New tests should use [`TestInfraConfig`] directly.
///
/// # Returns
///
/// Database connection URL string
pub fn get_test_database_url() -> String {
    TestInfraConfig::from_env().database_url
}

/// Get test API base URL from configuration.
///
/// This is a convenience helper for tests that need the API URL as a string.
/// New tests should use [`TestInfraConfig`] directly.
///
/// # Returns
///
/// API base URL string
pub fn get_test_api_url() -> String {
    TestInfraConfig::from_env().api_base_url
}

/// Get test SSH API URL from configuration.
///
/// # Returns
///
/// SSH gateway API URL string
pub fn get_test_ssh_api_url() -> String {
    TestInfraConfig::from_env().ssh_api_url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_test_config() {
        let cfg = load_test_config();
        // Should always have database config
        assert!(!cfg.database.host.is_empty());
        assert!(!if cfg.database.name == "dsb" {
            "dsb_test"
        } else {
            &cfg.database.name
        }
        .is_empty());
    }

    #[test]
    fn test_get_test_database_url() {
        let url = get_test_database_url();
        assert!(url.contains("postgresql://"));
        assert!(url.contains("dsb"));
    }

    #[test]
    fn test_get_test_api_url() {
        let url = get_test_api_url();
        assert!(url.contains("http://"));
    }

    #[test]
    fn test_get_test_ssh_api_url() {
        let url = get_test_ssh_api_url();
        assert!(url.contains("http://"));
    }

    #[test]
    fn test_infra_config_defaults() {
        let config = TestInfraConfig::from_env();
        assert!(config.api_base_url.starts_with("http://"));
        assert!(config.database_url.starts_with("postgresql://"));
        assert!(!config.api_key.is_empty());
        assert!(!config.sandbox_image.is_empty());
    }

    #[test]
    fn test_infra_config_api_host_port() {
        let mut config = TestInfraConfig::from_env();
        config.api_base_url = "http://example.com:9090".to_string();
        let (host, port) = config.api_host_port();
        assert_eq!(host, "example.com");
        assert_eq!(port, 9090);
    }

    #[test]
    fn test_infra_config_api_host_port_defaults() {
        let mut config = TestInfraConfig::from_env();

        // http without explicit port → 80
        config.api_base_url = "http://example.com".to_string();
        let (_, port) = config.api_host_port();
        assert_eq!(port, 80);

        // https without explicit port → 443
        config.api_base_url = "https://example.com".to_string();
        let (_, port) = config.api_host_port();
        assert_eq!(port, 443);
    }

    #[test]
    fn test_infra_config_database_url_with_name() {
        let mut config = TestInfraConfig::from_env();
        config.database_url = "postgresql://user:pass@localhost:5432/old_db".to_string();
        let new_url = config.database_url_with_name("new_db");
        assert!(new_url.contains("/new_db"));
        assert!(!new_url.contains("/old_db"));
        assert!(new_url.starts_with("postgresql://user:pass@localhost:5432/"));
    }
}
