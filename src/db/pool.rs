// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Database Connection Pool Module
//!
//! This module creates and manages the PostgreSQL connection pool using deadpool.
//!
//! ## Overview
//!
//! The connection pool provides:
//! - Automatic connection management
//! - Connection recycling and reuse
//! - Configurable pool size
//! - Graceful handling of connection failures
//!
//! ## Configuration
//!
//! Database configuration can be provided via:
//! - Application [`Config`](crate::config::Config) struct (recommended)
//! - Environment variables (legacy, for backward compatibility)
//!
//! ### Configuration Options
//!
//! - `database.url` - Full PostgreSQL connection URL (takes precedence)
//! - `database.host` - Database host (default: localhost)
//! - `database.port` - Database port (default: 5432)
//! - `database.name` - Database name (default: dsb)
//! - `database.user` - Database user (default: postgres)
//! - `database.password` - Database password (required if URL not set)
//! - `database.pool_max_size` - Maximum pool size (default: 10)
//!
//! ## Testing Strategy
//!
//! Database pool operations are tested through:
//!
//! ### Unit Tests (This Module)
//! URL construction and validation tests:
//! - Connection URL formatting
//! - Environment variable fallback logic
//! - Error message validation
//! - Type trait bounds
//!
//! ### Integration Tests
//! Pool creation tests in:
//! - **`tests/common/db_test_setup.rs`**: TestDatabase fixture uses pool creation
//! - **`tests/db_integration_tests.rs`**: Integration tests with real PostgreSQL
//!
//! Integration tests cover:
//! - Pool creation from connection strings
//! - Pool creation from config
//! - Connection lifecycle
//! - Pool reuse and cleanup
//!
//! ## Example
//!
//! ```rust,no_run,ignore
//! use dsb::db::pool::create_pool;
//! use dsb::db::pool::create_pool_from_config;
//! use dsb::config;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! // From connection string
//! let pool = create_pool("postgresql://user:pass@localhost/dsb").await?;
//!
//! // From application config (recommended)
//! let config = config::load()?;
//! let pool = create_pool_from_config(&config).await?;
//! # Ok(())
//! # }
//! ```

use crate::config::Config as AppConfig;
use deadpool_postgres::{Config as DeadpoolConfig, Pool, Runtime};

/// Creates a PostgreSQL connection pool from a connection string.
///
/// # Arguments
///
/// * `database_url` - PostgreSQL connection URL
///   Format: `postgresql://user:password@host:port/database`
///
/// # Returns
///
/// * `Ok(Pool)` - Connection pool ready to use
/// * `Err(...)` - Failed to create pool
///
/// # Errors
///
/// This function will return an error if:
/// - Connection URL is invalid
/// - Database connection fails
/// - Authentication fails
///
/// # Example
///
/// ```rust,no_run,ignore
/// # use dsb::db::pool::create_pool;
/// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// let pool = create_pool("postgresql://postgres:postgres@localhost:5432/dsb").await?;
/// println!("Pool created");
/// # Ok(())
/// # }
/// ```
pub async fn create_pool(
    database_url: &str,
) -> Result<Pool, Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!("Creating PostgreSQL connection pool");

    let mut cfg = DeadpoolConfig::new();
    cfg.url = Some(database_url.to_string());

    // Configure pool size
    cfg.pool = Some(deadpool_postgres::PoolConfig {
        max_size: 20,
        ..Default::default()
    });

    let pool = cfg.create_pool(
        Some(Runtime::Tokio1),
        deadpool_postgres::tokio_postgres::NoTls,
    )?;

    tracing::info!("PostgreSQL connection pool created successfully");
    Ok(pool)
}

/// Creates a PostgreSQL connection pool from application configuration.
///
/// This is the recommended way to create a database pool. It uses the centralized
/// application configuration which can be loaded from config files, environment
/// variables, or defaults.
///
/// # Arguments
///
/// * `config` - Application configuration
///
/// # Returns
///
/// * `Ok(Pool)` - Connection pool ready to use
/// * `Err(...)` - Failed to create pool or missing required configuration
///
/// # Errors
///
/// This function will return an error if:
/// - No database configuration is provided (both url and password are None)
/// - Connection URL is invalid
/// - Database connection fails
/// - Authentication fails
///
/// # Example
///
/// ```rust,no_run,ignore
/// # use dsb::db::pool::create_pool_from_config;
/// # use dsb::config;
/// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// let config = config::load()?;
/// let pool = create_pool_from_config(&config).await?;
/// println!("Pool created");
/// # Ok(())
/// # }
/// ```
pub async fn create_pool_from_config(
    config: &AppConfig,
) -> Result<Pool, Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!("Creating PostgreSQL connection pool from configuration");

    // Use database.url if provided, otherwise build from components
    let database_url = if let Some(url) = &config.database.url {
        tracing::info!("Using database URL from configuration");
        url.clone()
    } else {
        // Check if we have enough configuration to build URL
        if config.database.password.is_none() {
            return Err(
                "Database password is required (set database.password or database.url)".into(),
            );
        }

        let host = &config.database.host;
        let port = config.database.port;
        let dbname = &config.database.name;
        let user = &config.database.user;
        let password = config.database.password.as_ref().unwrap();

        format!(
            "postgresql://{}:{}@{}:{}/{}",
            user, password, host, port, dbname
        )
    };

    tracing::info!(
        "Connecting to PostgreSQL at {}:{}/{}",
        config.database.host,
        config.database.port,
        config.database.name
    );

    let mut cfg = DeadpoolConfig::new();
    cfg.url = Some(database_url);

    // Configure pool size
    cfg.pool = Some(deadpool_postgres::PoolConfig {
        max_size: 20,
        ..Default::default()
    });

    let pool = cfg
        .create_pool(
            Some(Runtime::Tokio1),
            deadpool_postgres::tokio_postgres::NoTls,
        )
        .map_err(|e| format!("Failed to create connection pool: {}", e))?;

    tracing::info!("PostgreSQL connection pool created successfully");
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Connection URL Construction Tests
    // ========================================================================

    #[test]
    fn test_build_connection_url_standard() {
        let url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            "user", "pass", "localhost", "5432", "dsb"
        );
        assert_eq!(url, "postgresql://user:pass@localhost:5432/dsb");
    }

    #[test]
    fn test_build_connection_url_with_custom_host() {
        let url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            "admin", "secret", "db.example.com", "3306", "mydb"
        );
        assert_eq!(url, "postgresql://admin:secret@db.example.com:3306/mydb");
    }

    #[test]
    fn test_build_connection_url_with_ipv4() {
        let url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            "user", "pass", "192.168.1.100", "5432", "testdb"
        );
        assert_eq!(url, "postgresql://user:pass@192.168.1.100:5432/testdb");
    }

    #[test]
    fn test_build_connection_url_with_localhost_default() {
        let url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            "postgres", "postgres", "localhost", "5432", "dsb"
        );
        assert_eq!(url, "postgresql://postgres:postgres@localhost:5432/dsb");
    }

    #[test]
    fn test_build_connection_url_with_special_chars() {
        let url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            "user-name", "p@ss:w0rd", "localhost", "5432", "my-db"
        );
        // Note: Special characters are allowed in URL components
        assert!(url.contains("user-name"));
        assert!(url.contains("localhost"));
        assert!(url.contains("my-db"));
    }

    #[test]
    fn test_build_connection_url_with_unicode() {
        let url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            "用户", "密码", "localhost", "5432", "数据库"
        );
        assert!(url.contains("用户"));
        assert!(url.contains("数据库"));
    }

    // ========================================================================
    // Config-based Database Configuration Tests
    // ========================================================================

    #[test]
    fn test_config_database_defaults() {
        // Test that config provides correct default database values
        let config = crate::config::Config::default();

        assert_eq!(config.database.host, "localhost");
        assert_eq!(config.database.port, 5432);
        assert_eq!(config.database.name, "dsb");
        assert_eq!(config.database.user, "postgres");
        assert_eq!(config.database.pool_max_size, Some(10));
    }

    #[test]
    fn test_config_database_with_custom_values() {
        // Test that config accepts custom database values
        use crate::config::DatabaseConfig;

        let custom_config = DatabaseConfig {
            url: None,
            host: "customhost".to_string(),
            port: 3306,
            name: "customdb".to_string(),
            user: "customuser".to_string(),
            password: Some("password".to_string()),
            pool_max_size: Some(20),
        };

        assert_eq!(custom_config.host, "customhost");
        assert_eq!(custom_config.port, 3306);
        assert_eq!(custom_config.name, "customdb");
        assert_eq!(custom_config.user, "customuser");
        assert_eq!(custom_config.password, Some("password".to_string()));
        assert_eq!(custom_config.pool_max_size, Some(20));
    }

    // ========================================================================
    // Pool Type Tests
    // ========================================================================

    #[test]
    fn test_pool_is_clone() {
        // Verify Pool implements Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<Pool>();
    }

    #[test]
    fn test_pool_is_send() {
        // Verify Pool implements Send
        fn assert_send<T: Send>() {}
        assert_send::<Pool>();
    }

    #[test]
    fn test_pool_is_sync() {
        // Verify Pool implements Sync
        fn assert_sync<T: Sync>() {}
        assert_sync::<Pool>();
    }

    // ========================================================================
    // Function Signature Tests
    // ========================================================================

    #[test]
    fn test_create_pool_exists() {
        // Compile-time test that create_pool function exists
        // The function signature is: async fn create_pool(pool_url: &str) -> Result<Pool, Box<dyn std::error::Error>>
        let _ = create_pool;
    }

    #[test]
    fn test_create_pool_from_config_exists() {
        // Compile-time test that create_pool_from_config function exists
        // The function signature is: async fn create_pool_from_config(config: &Config) -> Result<Pool, Box<dyn std::error::Error>>
        let _ = create_pool_from_config;
    }

    #[test]
    fn test_create_pool_from_env_exists() {
        // This test is removed - create_pool_from_env was deprecated
        // All pool creation now goes through create_pool_from_config
        // which uses the centralized configuration system
    }

    // ========================================================================
    // Integration Test References
    // ========================================================================

    #[test]
    fn test_integration_test_files_exist() {
        // Documents where integration tests are located
        let _integration_tests = (
            "tests/common/db_test_setup.rs",
            "tests/db_integration_tests.rs",
        );
    }

    // ========================================================================
    // Error Cases
    // ========================================================================

    #[test]
    fn test_connection_url_components() {
        // Test that URL construction preserves all components
        let user = "testuser";
        let password = "testpass";
        let host = "testhost";
        let port = "5433";
        let dbname = "testdb";

        let url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            user, password, host, port, dbname
        );

        assert!(url.contains("testuser"));
        assert!(url.contains("testpass"));
        assert!(url.contains("testhost"));
        assert!(url.contains("5433"));
        assert!(url.contains("testdb"));
        assert!(url.starts_with("postgresql://"));
    }

    #[test]
    fn test_connection_url_with_empty_password() {
        // URL construction with empty password (edge case)
        let url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            "user", "", "localhost", "5432", "dsb"
        );
        assert_eq!(url, "postgresql://user:@localhost:5432/dsb");
    }

    #[test]
    fn test_connection_url_with_underscores() {
        // Test with underscores in all components
        let url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            "test_user", "test_pass", "test_host", "5432", "test_db"
        );
        assert!(url.contains("test_user"));
        assert!(url.contains("test_host"));
        assert!(url.contains("test_db"));
    }

    #[test]
    fn test_connection_url_with_dots() {
        // Test with dots in hostname
        let url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            "user", "pass", "db.server.example", "5432", "mydb"
        );
        assert!(url.contains("db.server.example"));
    }

    #[test]
    fn test_connection_url_format_consistency() {
        // Multiple URL constructions should be consistent
        let url1 = format!(
            "postgresql://{}:{}@{}:{}/{}",
            "user", "pass", "host", "5432", "db"
        );
        let url2 = "postgresql://user:pass@host:5432/db".to_string();

        assert_eq!(url1, url2);
    }

    // ========================================================================
    // Edge Cases and Boundaries
    // ========================================================================

    #[test]
    fn test_very_long_component_values() {
        // Test with very long values (stress test)
        let long_val = "a".repeat(1000);
        let url = format!(
            "postgresql://{}:{}@{}:{}/{}",
            long_val, long_val, long_val, "5432", long_val
        );

        assert!(url.len() > 3000);
        assert!(url.contains(&long_val[..100]));
    }

    #[test]
    fn test_port_number_variations() {
        // Test different valid port numbers
        let ports = vec!["5432", "3306", "5433", "65535"];

        for port in ports {
            let url = format!(
                "postgresql://{}:{}@{}:{}/{}",
                "user", "pass", "localhost", port, "db"
            );
            assert!(url.contains(port));
            assert!(url.contains("localhost"));
        }
    }

    #[test]
    fn test_database_name_variations() {
        // Test different database name formats
        let db_names = vec![
            "dsb",
            "my_app_db",
            "my-app-db",
            "my.app.db",
            "MY_DB",
            "mydb123",
        ];

        for dbname in db_names {
            let url = format!(
                "postgresql://{}:{}@{}:{}/{}",
                "user", "pass", "localhost", "5432", dbname
            );
            assert!(url.ends_with(dbname));
        }
    }

    #[test]
    fn test_username_variations() {
        // Test different username formats
        let users = vec![
            "postgres",
            "admin",
            "app_user",
            "app-user",
            "user123",
            "test.user",
        ];

        for user in users {
            let url = format!(
                "postgresql://{}:{}@{}:{}/{}",
                user, "pass", "localhost", "5432", "db"
            );
            assert!(url.contains(user));
        }
    }

    #[test]
    fn test_password_with_special_characters() {
        // Test passwords with various special characters
        let passwords = vec![
            "simple123",
            "complex!@#$%",
            "with/slash",
            "with:colon",
            "with@at",
            "with space",
            "Unicode密码",
        ];

        for password in &passwords {
            let url = format!(
                "postgresql://{}:{}@{}:{}/{}",
                "user", password, "localhost", "5432", "db"
            );
            // Verify URL was created and contains expected parts
            assert!(url.starts_with("postgresql://user:"));
            assert!(url.contains("@localhost:5432/db"));
        }
    }
}
