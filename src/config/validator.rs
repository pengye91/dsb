// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Configuration Validation
//!
//! Validates configuration values before application startup.

use super::error::ConfigError;
use super::types::{default_docker_registry, Config};
use tracing::{debug, warn};

/// Validates the complete configuration
///
/// # Errors
///
/// Returns `Err(ConfigError::ValidationError)` if validation fails
///
/// # Example
///
/// ```rust,no_run,ignore
/// use dsb::config::{Config, validator};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = Config::default();
/// validator::validate(&config)?;
/// # Ok(())
/// # }
/// ```
pub fn validate(config: &Config) -> Result<(), ConfigError> {
    debug!("Validating configuration...");

    // Validate server config
    validate_server(config)?;

    // Validate database config (only if URL or password is set)
    validate_database(config)?;

    // Validate docker config
    validate_docker(config)?;

    // Validate SSH config
    validate_ssh(config)?;

    // Validate sandbox config
    validate_sandbox(config)?;

    // Validate logging config
    validate_logging(config)?;

    // Validate static server config
    validate_static_server(config)?;

    debug!("✓ Configuration validation passed");

    Ok(())
}

/// Validates server configuration
fn validate_server(config: &Config) -> Result<(), ConfigError> {
    let port = config.server.port;
    if port == 0 {
        return Err(ConfigError::InvalidValue {
            path: "server.port".to_string(),
            value: port.to_string(),
            reason: "port must be between 1 and 65535".to_string(),
        });
    }

    if config.server.port < 1024 {
        warn!(
            "server.port {} is in privileged range (<1024), requires root",
            port
        );
    }

    Ok(())
}

/// Validates database configuration
fn validate_database(config: &Config) -> Result<(), ConfigError> {
    // Only validate if database is configured
    let has_db_config = config.database.url.is_some() || config.database.password.is_some();

    if !has_db_config {
        debug!("No database configuration, will use in-memory storage");
        return Ok(());
    }

    // If URL is not set, build from components
    if config.database.url.is_none() {
        if config.database.password.is_none() {
            return Err(ConfigError::MissingValue {
                path: "database.password".to_string(),
            });
        }

        // Validate individual components
        if config.database.host.is_empty() {
            return Err(ConfigError::InvalidValue {
                path: "database.host".to_string(),
                value: config.database.host.clone(),
                reason: "host cannot be empty".to_string(),
            });
        }

        if config.database.name.is_empty() {
            return Err(ConfigError::InvalidValue {
                path: "database.name".to_string(),
                value: config.database.name.clone(),
                reason: "name cannot be empty".to_string(),
            });
        }

        if config.database.user.is_empty() {
            return Err(ConfigError::InvalidValue {
                path: "database.user".to_string(),
                value: config.database.user.clone(),
                reason: "user cannot be empty".to_string(),
            });
        }
    }

    debug!("✓ Database configuration validated");
    Ok(())
}

/// Validates Docker configuration
fn validate_docker(config: &Config) -> Result<(), ConfigError> {
    // Check if registry is accessible (basic format check)
    if config.docker.registry.is_empty() {
        return Err(ConfigError::InvalidValue {
            path: "docker.registry".to_string(),
            value: config.docker.registry.clone(),
            reason: "registry cannot be empty".to_string(),
        });
    }

    // Warn if using default registry
    if config.docker.registry == default_docker_registry() {
        debug!("Using default registry: {}", config.docker.registry);
    }

    Ok(())
}

/// Validates SSH configuration
fn validate_ssh(config: &Config) -> Result<(), ConfigError> {
    let port = config.ssh.port;
    if port == 0 {
        return Err(ConfigError::InvalidValue {
            path: "ssh.port".to_string(),
            value: port.to_string(),
            reason: "port must be between 1 and 65535".to_string(),
        });
    }

    // Validate API URL format
    if !config.ssh.api_url.starts_with("http://") && !config.ssh.api_url.starts_with("https://") {
        return Err(ConfigError::InvalidValue {
            path: "ssh.api_url".to_string(),
            value: config.ssh.api_url.clone(),
            reason: "must start with http:// or https://".to_string(),
        });
    }

    Ok(())
}

/// Validates sandbox configuration
fn validate_sandbox(config: &Config) -> Result<(), ConfigError> {
    // Validate inactivity timeout
    if config.sandbox.default_inactivity_timeout == 0 {
        return Err(ConfigError::InvalidValue {
            path: "sandbox.default_inactivity_timeout".to_string(),
            value: config.sandbox.default_inactivity_timeout.to_string(),
            reason: "must be greater than 0".to_string(),
        });
    }

    // Validate state monitor interval
    if config.sandbox.state_monitor_interval == 0 {
        return Err(ConfigError::InvalidValue {
            path: "sandbox.state_monitor_interval".to_string(),
            value: config.sandbox.state_monitor_interval.to_string(),
            reason: "must be greater than 0".to_string(),
        });
    }

    Ok(())
}

/// Validates logging configuration
fn validate_logging(config: &Config) -> Result<(), ConfigError> {
    let valid_levels = ["trace", "debug", "info", "warn", "error"];
    if !valid_levels.contains(&config.logging.level.as_str()) {
        return Err(ConfigError::InvalidValue {
            path: "logging.level".to_string(),
            value: config.logging.level.clone(),
            reason: format!("must be one of: {}", valid_levels.join(", ")),
        });
    }

    Ok(())
}

/// Validates static file server configuration
fn validate_static_server(config: &Config) -> Result<(), ConfigError> {
    // Validate default cache_control
    if !is_valid_cache_control(&config.static_server.cache_control) {
        return Err(ConfigError::InvalidValue {
            path: "static_server.cache_control".to_string(),
            value: config.static_server.cache_control.clone(),
            reason: "invalid cache-control directives".to_string(),
        });
    }

    // Validate each cache_control_by_type entry
    for (mime_type, cache_control) in &config.static_server.cache_control_by_type {
        // Validate MIME type format (basic check for structure)
        // Must contain a slash (e.g., "text/html" or "image/*")
        if !mime_type.contains('/') {
            return Err(ConfigError::InvalidValue {
                path: format!("static_server.cache_control_by_type.{}", mime_type),
                value: mime_type.clone(),
                reason: "invalid MIME type format (must contain '/')".to_string(),
            });
        }

        // Validate the cache control value for this type
        if !is_valid_cache_control(cache_control) {
            return Err(ConfigError::InvalidValue {
                path: format!("static_server.cache_control_by_type.{}", mime_type),
                value: cache_control.clone(),
                reason: "invalid cache-control directives".to_string(),
            });
        }
    }

    debug!("✓ Static server configuration validated");
    Ok(())
}

/// Validates a cache-control header value
///
/// # Arguments
///
/// * `value` - The cache-control header value to validate
///
/// # Returns
///
/// `true` if the value contains valid cache-control directives, `false` otherwise
///
/// # Supported Directives
///
/// - `public`, `private`
/// - `no-cache`, `no-store`, `no-transform`
/// - `must-revalidate`, `proxy-revalidate`
/// - `max-age=<seconds>`, `s-maxage=<seconds>`
fn is_valid_cache_control(value: &str) -> bool {
    if value.trim().is_empty() {
        return false;
    }

    // Split by comma and validate each directive
    for directive in value.split(',') {
        let directive = directive.trim();

        // Check for known directives with optional values
        if directive == "public"
            || directive == "private"
            || directive == "no-cache"
            || directive == "no-store"
            || directive == "no-transform"
            || directive == "must-revalidate"
            || directive == "proxy-revalidate"
        {
            continue;
        }

        // Check for max-age=<seconds>
        if let Some(rest) = directive.strip_prefix("max-age=") {
            if rest.parse::<u64>().is_ok() {
                continue;
            }
            return false;
        }

        // Check for s-maxage=<seconds>
        if let Some(rest) = directive.strip_prefix("s-maxage=") {
            if rest.parse::<u64>().is_ok() {
                continue;
            }
            return false;
        }

        // Unknown directive
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::Config;

    #[test]
    fn test_validate_default_config() {
        let config = Config::default();
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_server_port_valid() {
        let mut config = Config::default();
        config.server.port = 8080;
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_server_port_zero() {
        let mut config = Config::default();
        config.server.port = 0;
        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("server.port"));
        assert!(err_msg.contains("0"));
    }

    #[test]
    fn test_validate_server_port_in_privileged_range() {
        let mut config = Config::default();
        config.server.port = 80; // Privileged port (< 1024)

        // Should still validate, but with a warning
        let result = validate(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_database_with_url() {
        let mut config = Config::default();
        config.database.url = Some("postgresql://user:pass@localhost:5432/db".to_string());
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_database_with_components() {
        let mut config = Config::default();
        config.database.url = None;
        config.database.password = Some("password".to_string());
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_docker_registry_valid() {
        let mut config = Config::default();
        config.docker.registry = "registry.example.com".to_string();
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_docker_registry_empty() {
        let mut config = Config::default();
        config.docker.registry = "".to_string();

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("docker"));
        assert!(err_msg.contains("registry"));
    }

    #[test]
    fn test_validate_sandbox_timeouts() {
        let mut config = Config::default();

        // Valid timeout values
        config.sandbox.default_inactivity_timeout = 60;
        assert!(validate(&config).is_ok());

        config.sandbox.state_monitor_interval = 30;
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_sandbox_zero_timeout() {
        let mut config = Config::default();
        config.sandbox.default_inactivity_timeout = 0; // Disabled

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("inactivity") || err_msg.contains("timeout"));
    }

    #[test]
    fn test_validate_sandbox_zero_monitor_interval() {
        let mut config = Config::default();
        config.sandbox.state_monitor_interval = 0;

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("monitor"));
    }

    #[test]
    fn test_validate_ssh_port_valid() {
        let mut config = Config::default();
        config.ssh.port = 2222;
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_ssh_port_zero() {
        let mut config = Config::default();
        config.ssh.port = 0;

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("ssh"));
        assert!(err_msg.contains("port"));
    }

    #[test]
    fn test_validate_ssh_api_url_invalid_format() {
        let mut config = Config::default();
        config.ssh.api_url = "not-a-url".to_string();

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("ssh"));
        assert!(err_msg.contains("api_url"));
    }

    #[test]
    fn test_validate_ssh_api_url_missing_http() {
        let mut config = Config::default();
        config.ssh.api_url = "localhost:8080".to_string();

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("ssh"));
    }

    #[test]
    fn test_validate_ssh_api_url_valid_formats() {
        let mut config = Config::default();

        // Test valid URL formats
        config.ssh.api_url = "http://localhost:8080".to_string();
        assert!(validate(&config).is_ok());

        config.ssh.api_url = "https://api.example.com".to_string();
        assert!(validate(&config).is_ok());

        config.ssh.api_url = "http://192.168.1.1:9000".to_string();
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_logging_level_valid() {
        let mut config = Config::default();

        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        for level in valid_levels {
            config.logging.level = level.to_string();
            assert!(validate(&config).is_ok());
        }
    }

    #[test]
    fn test_validate_logging_level_invalid() {
        let mut config = Config::default();
        config.logging.level = "invalid".to_string();

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("log") || err_msg.contains("level"));
    }

    #[test]
    fn test_validate_all_sections_together() {
        let config = Config::default();
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_complete_valid_config() {
        let mut config = Config::default();

        // Set all fields to valid values
        config.server.port = 9000;
        config.server.api_key = Some("test-api-key".to_string());

        config.database.url = Some("postgresql://user:pass@localhost:5432/db".to_string());

        config.docker.registry = "registry.example.com".to_string();
        config.docker.host = Some("unix:///var/run/docker.sock".to_string());

        config.sandbox.default_inactivity_timeout = 45;
        config.sandbox.cleanup_dry_run = true;
        config.sandbox.state_monitor_interval = 90;

        config.ssh.port = 2223;
        config.ssh.api_url = "http://api.example.com".to_string();
        config.ssh.api_key = Some("ssh-key".to_string());

        config.logging.level = "debug".to_string();

        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_multiple_errors() {
        let mut config = Config::default();

        // Set multiple invalid values
        config.server.port = 0;
        config.database.password = None;
        config.docker.registry = "".to_string();
        config.ssh.port = 0;

        // Should return an error (first one encountered)
        let result = validate(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_database_url_with_no_password() {
        let mut config = Config::default();

        // URL contains password, so it should be valid
        config.database.url = Some("postgresql://user:pass@localhost:5432/db".to_string());
        config.database.password = None; // Not required when URL is set

        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_sandbox_cleanup_dry_run() {
        let mut config = Config::default();

        // Both true and false should be valid
        config.sandbox.cleanup_dry_run = true;
        assert!(validate(&config).is_ok());

        config.sandbox.cleanup_dry_run = false;
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_is_valid_cache_control_valid_directives() {
        // Test individual valid directives
        assert!(is_valid_cache_control("public"));
        assert!(is_valid_cache_control("private"));
        assert!(is_valid_cache_control("no-cache"));
        assert!(is_valid_cache_control("no-store"));
        assert!(is_valid_cache_control("no-transform"));
        assert!(is_valid_cache_control("must-revalidate"));
        assert!(is_valid_cache_control("proxy-revalidate"));
    }

    #[test]
    fn test_is_valid_cache_control_valid_with_max_age() {
        // Test max-age with valid values
        assert!(is_valid_cache_control("max-age=0"));
        assert!(is_valid_cache_control("max-age=3600"));
        assert!(is_valid_cache_control("max-age=86400"));
        assert!(is_valid_cache_control("public, max-age=3600"));
        assert!(is_valid_cache_control(
            "private, max-age=1800, must-revalidate"
        ));
    }

    #[test]
    fn test_is_valid_cache_control_valid_with_s_maxage() {
        // Test s-maxage with valid values
        assert!(is_valid_cache_control("s-maxage=3600"));
        assert!(is_valid_cache_control("public, s-maxage=7200"));
        assert!(is_valid_cache_control("s-maxage=86400, must-revalidate"));
    }

    #[test]
    fn test_is_valid_cache_control_valid_combinations() {
        // Test common combinations
        assert!(is_valid_cache_control("public, max-age=3600"));
        assert!(is_valid_cache_control(
            "private, must-revalidate, max-age=1800"
        ));
        assert!(is_valid_cache_control("no-cache"));
        assert!(is_valid_cache_control("no-store"));
        assert!(is_valid_cache_control(
            "public, max-age=3600, must-revalidate"
        ));
        assert!(is_valid_cache_control(
            "private, max-age=0, must-revalidate"
        ));
    }

    #[test]
    fn test_is_valid_cache_control_invalid_directives() {
        // Test invalid directives
        assert!(!is_valid_cache_control("invalid-directive"));
        assert!(!is_valid_cache_control("public, invalid"));
        assert!(!is_valid_cache_control("max-age=abc")); // Not a number
        assert!(!is_valid_cache_control("s-maxage=xyz")); // Not a number
        assert!(!is_valid_cache_control("")); // Empty
        assert!(!is_valid_cache_control("   ")); // Whitespace only
    }

    #[test]
    fn test_is_valid_cache_control_invalid_max_age_negative() {
        // Test negative max-age (not a valid u64)
        assert!(!is_valid_cache_control("max-age=-1"));
    }

    #[test]
    fn test_validate_static_server_default_config() {
        let config = Config::default();
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_static_server_valid_cache_control() {
        let mut config = Config::default();

        // Test valid cache control values
        config.static_server.cache_control = "public, max-age=3600".to_string();
        assert!(validate(&config).is_ok());

        config.static_server.cache_control = "no-cache".to_string();
        assert!(validate(&config).is_ok());

        config.static_server.cache_control = "private, must-revalidate, max-age=1800".to_string();
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_static_server_invalid_cache_control() {
        let mut config = Config::default();

        // Test invalid cache control value
        config.static_server.cache_control = "invalid-directive".to_string();

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("static_server"));
        assert!(err_msg.contains("cache_control"));
    }

    #[test]
    fn test_validate_static_server_valid_cache_control_by_type() {
        let mut config = Config::default();
        use std::collections::HashMap;

        // Test valid cache control by type
        let mut cache_by_type = HashMap::new();
        cache_by_type.insert("text/html".to_string(), "no-cache".to_string());
        cache_by_type.insert("image/*".to_string(), "public, max-age=86400".to_string());
        cache_by_type.insert("application/json".to_string(), "no-cache".to_string());
        cache_by_type.insert(
            "application/javascript".to_string(),
            "public, max-age=1800, must-revalidate".to_string(),
        );

        config.static_server.cache_control_by_type = cache_by_type;

        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_static_server_invalid_cache_control_by_type() {
        let mut config = Config::default();
        use std::collections::HashMap;

        // Test invalid cache control value in by_type
        let mut cache_by_type = HashMap::new();
        cache_by_type.insert("text/html".to_string(), "invalid-directive".to_string());

        config.static_server.cache_control_by_type = cache_by_type;

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("static_server"));
        assert!(err_msg.contains("cache_control_by_type"));
    }

    #[test]
    fn test_validate_static_server_invalid_mime_type_format() {
        let mut config = Config::default();
        use std::collections::HashMap;

        // Test invalid MIME type format (no slash)
        let mut cache_by_type = HashMap::new();
        cache_by_type.insert("invalid_mime_type".to_string(), "no-cache".to_string());

        config.static_server.cache_control_by_type = cache_by_type;

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(
            err_msg.contains("invalid MIME type format") || err_msg.contains("must contain '/'")
        );
    }

    #[test]
    fn test_validate_static_server_wildcard_mime_types() {
        let mut config = Config::default();
        use std::collections::HashMap;

        // Test wildcard MIME types are accepted
        let mut cache_by_type = HashMap::new();
        cache_by_type.insert("image/*".to_string(), "public, max-age=86400".to_string());
        cache_by_type.insert("font/*".to_string(), "public, max-age=86400".to_string());
        cache_by_type.insert("video/*".to_string(), "public, max-age=3600".to_string());

        config.static_server.cache_control_by_type = cache_by_type;

        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_validate_database_empty_host() {
        let mut config = Config::default();

        // No URL, have password but empty host
        config.database.url = None;
        config.database.password = Some("password".to_string());
        config.database.host = "".to_string();
        config.database.name = "testdb".to_string();
        config.database.user = "testuser".to_string();

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("database"));
        assert!(err_msg.contains("host"));
    }

    #[test]
    fn test_validate_database_empty_name() {
        let mut config = Config::default();

        // No URL, have password but empty name
        config.database.url = None;
        config.database.password = Some("password".to_string());
        config.database.host = "localhost".to_string();
        config.database.name = "".to_string();
        config.database.user = "testuser".to_string();

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("database"));
        assert!(err_msg.contains("name"));
    }

    #[test]
    fn test_validate_database_empty_user() {
        let mut config = Config::default();

        // No URL, have password but empty user
        config.database.url = None;
        config.database.password = Some("password".to_string());
        config.database.host = "localhost".to_string();
        config.database.name = "testdb".to_string();
        config.database.user = "".to_string();

        let result = validate(&config);
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("database"));
        assert!(err_msg.contains("user"));
    }
}
