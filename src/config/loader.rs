// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Configuration Loader
//!
//! Loads configuration from multiple sources with proper priority.
//!
//! ## Loading Priority (Low to High)
//!
//! 1. Default values (from struct defaults)
//! 2. Configuration file (.env or YAML)
//! 3. Environment variables
//! 4. Command-line arguments (highest priority)
//!
//! ## Example
//!
//! ```rust,no_run,ignore
//! use dsb::config::loader;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = loader::load()?;
//! # Ok(())
//! # }
//! ```

use super::error::ConfigError;
use super::types::Config;
use super::validator;
use std::path::PathBuf;
use tracing::{debug, warn};

/// Loads configuration for production use
///
/// This function loads configuration from multiple sources in order:
/// 1. Default values
/// 2. .env file (if found)
/// 3. YAML file (dsb.yaml or dsb.yml, if found)
/// 4. Environment variables (DSB_*)
/// 5. CLI arguments (if provided)
///
/// # Errors
///
/// Returns `Err` if configuration loading or validation fails
///
/// # Example
///
/// ```rust,no_run,ignore
/// use dsb::config::loader;
///
/// let config = loader::load()?;
/// # Ok::<(), dsb::config::error::ConfigError>(())
/// ```
pub fn load() -> Result<Config, ConfigError> {
    load_with_cli_args(None)
}

/// Loads configuration with custom file paths
///
/// This function allows specifying custom paths for .env and YAML configuration files.
///
/// # Arguments
///
/// * `env_file` - Optional path to .env file
/// * `config_file` - Optional path to YAML configuration file
///
/// # Errors
///
/// Returns `Err` if configuration loading or validation fails
///
/// # Example
///
/// ```rust,no_run,ignore
/// use dsb::config::loader;
///
/// // Load with custom files
/// let config = loader::load_with_files(Some(".env.test"), Some("dsb.test.yaml"))?;
/// # Ok::<(), dsb::config::error::ConfigError>(())
/// ```
pub fn load_with_files(
    env_file: Option<&str>,
    config_file: Option<&str>,
) -> Result<Config, ConfigError> {
    load_with_cli_args_and_files(None, env_file, config_file)
}

/// Loads configuration with custom file paths and CLI arguments
///
/// This function allows specifying custom paths for .env and YAML configuration files,
/// along with optional CLI argument overrides.
///
/// # Arguments
///
/// * `cli_args` - Optional HashMap of CLI argument overrides
/// * `env_file` - Optional path to .env file
/// * `config_file` - Optional path to YAML configuration file
///
/// # Errors
///
/// Returns `Err` if configuration loading or validation fails
///
/// # Example
///
/// ```rust,no_run,ignore
/// use dsb::config::loader;
/// use std::collections::HashMap;
///
/// let mut cli_args = HashMap::new();
/// cli_args.insert("server.port".to_string(), "9000".to_string());
///
/// let config = loader::load_with_cli_args_and_files(
///     Some(cli_args),
///     Some(".env.test"),
///     Some("dsb.test.yaml")
/// )?;
/// # Ok::<(), dsb::config::error::ConfigError>(())
/// ```
pub fn load_with_cli_args_and_files(
    cli_args: Option<std::collections::HashMap<String, String>>,
    env_file: Option<&str>,
    config_file: Option<&str>,
) -> Result<Config, ConfigError> {
    debug!("Loading configuration with custom files...");
    debug!("env_file: {:?}", env_file);
    debug!("config_file: {:?}", config_file);

    let mut builder = config::Config::builder();

    // 1. Load .env file if specified
    if let Some(env_path_str) = env_file {
        let env_path = PathBuf::from(env_path_str);
        if env_path.exists() {
            // Convert to absolute path to ensure dotenvy loads it correctly
            let env_path_absolute =
                env_path
                    .canonicalize()
                    .map_err(|e| ConfigError::FileLoadError {
                        path: env_path.display().to_string(),
                        source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
                    })?;
            let env_path_display = env_path_absolute.display().to_string();
            debug!("Loading .env file from: {}", env_path_display);
            dotenvy::from_path(&env_path_absolute).map_err(|e| ConfigError::FileLoadError {
                path: env_path_display.clone(),
                source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
            })?;
        } else {
            warn!(
                "Specified .env file not found, skipping: {}",
                env_path.display()
            );
        }
    } else {
        // Fall back to default .env file finding
        if let Some(env_path) = find_env_file() {
            let env_path_display = env_path.display().to_string();
            debug!("Loading .env file from: {}", env_path_display);
            dotenvy::from_path(&env_path).map_err(|e| ConfigError::FileLoadError {
                path: env_path_display.clone(),
                source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
            })?;
        } else {
            debug!("No .env file found, skipping");
        }
    }

    // 2. Load YAML file if specified
    if let Some(config_path_str) = config_file {
        let config_path = PathBuf::from(config_path_str);
        if config_path.exists() {
            debug!("Loading YAML config from: {}", config_path.display());
            builder = builder.add_source(
                config::File::from(config_path)
                    .format(config::FileFormat::Yaml)
                    .required(false),
            );
        } else {
            warn!(
                "Specified YAML config file not found, skipping: {}",
                config_path.display()
            );
        }
    } else {
        // Fall back to default config file finding
        if let Some(yaml_path) = find_config_file() {
            debug!("Loading YAML config from: {}", yaml_path.display());
            builder = builder.add_source(
                config::File::from(yaml_path)
                    .format(config::FileFormat::Yaml)
                    .required(false),
            );
        } else {
            debug!("No YAML config file found, skipping");
        }
    }

    // 3. Add environment variables (DSB_*)
    builder = builder.add_source(
        config::Environment::with_prefix("DSB")
            .prefix_separator("_")
            .separator("__") // Use double underscore for nested keys
            .try_parsing(true),
    );

    // Build the configuration
    let settings = builder.build().map_err(|e| ConfigError::ParseError {
        source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
    })?;

    // Deserialize into our Config struct
    let mut config: Config = settings
        .try_deserialize()
        .map_err(|e| ConfigError::ParseError {
            source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
        })?;

    // Support DSB_API_KEY as a convenience environment variable
    // This allows users to set a single env var instead of DSB_SERVER__API_KEY
    // Only set if api_key is not already configured
    if config.server.api_key.is_none() {
        if let Ok(dsb_api_key) = std::env::var("DSB_API_KEY") {
            debug!("Using DSB_API_KEY environment variable");
            config.server.api_key = Some(dsb_api_key);
        }
    }

    // Merge proxy: YAML / DSB_DOCKER__* `docker.proxy_env` plus process HTTP_PROXY / HTTPS_PROXY / …
    // Process env wins on duplicate keys (same as Docker backend expectation).
    config.docker.proxy_env = merge_docker_proxy_env(config.docker.proxy_env);

    // 4. Merge CLI arguments (highest priority)
    if let Some(cli_args) = cli_args {
        config = merge_cli_args(config, cli_args);
    }

    // 5. Validate configuration
    validator::validate(&config)?;

    // 6. Log configuration summary (without secrets)
    log_config_summary(&config);

    Ok(config)
}

/// Loads configuration with optional CLI arguments
///
/// CLI arguments take highest priority and override all other sources.
///
/// # Arguments
///
/// * `cli_args` - Optional HashMap of CLI argument overrides
///
/// # Errors
///
/// Returns `Err` if configuration loading or validation fails
pub fn load_with_cli_args(
    cli_args: Option<std::collections::HashMap<String, String>>,
) -> Result<Config, ConfigError> {
    debug!("Loading configuration...");

    let mut builder = config::Config::builder();

    // 1. Load .env file if found (this sets environment variables)
    if let Some(env_path) = find_env_file() {
        let env_path_display = env_path.display().to_string();
        debug!("Loading .env file from: {}", env_path_display);
        dotenvy::from_path(&env_path).map_err(|e| ConfigError::FileLoadError {
            path: env_path_display.clone(),
            source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
        })?;
    } else {
        debug!("No .env file found, skipping");
    }

    // 2. Try to load YAML file
    if let Some(yaml_path) = find_config_file() {
        debug!("Loading YAML config from: {}", yaml_path.display());
        // Try to load YAML, but fall back gracefully if it's invalid
        let file = config::File::from(yaml_path)
            .format(config::FileFormat::Yaml)
            .required(false);
        builder = builder.add_source(file);
    } else {
        debug!("No YAML config file found, skipping");
    }

    // 3. Add environment variables (DSB_*)
    builder = builder.add_source(
        config::Environment::with_prefix("DSB")
            .prefix_separator("_")
            .separator("__") // Use double underscore for nested keys
            .try_parsing(true),
    );

    // Build the configuration (may fail if YAML is invalid, catch and retry without YAML)
    let settings = match builder.build() {
        Ok(settings) => settings,
        Err(e) => {
            // If building failed and we had a YAML file, try again without it
            if find_config_file().is_some() {
                warn!(
                    "Failed to load YAML config, falling back to defaults: {}",
                    e
                );
                let mut builder_retry = config::Config::builder();
                // Add environment variables again
                builder_retry = builder_retry.add_source(
                    config::Environment::with_prefix("DSB")
                        .prefix_separator("_")
                        .separator("__")
                        .try_parsing(true),
                );
                builder_retry
                    .build()
                    .map_err(|e2| ConfigError::ParseError {
                        source: Box::new(e2) as Box<dyn std::error::Error + Send + Sync>,
                    })?
            } else {
                return Err(ConfigError::ParseError {
                    source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
                });
            }
        }
    };

    // Deserialize into our Config struct (uses Default impl for missing values)
    let mut config: Config = settings
        .try_deserialize()
        .map_err(|e| ConfigError::ParseError {
            source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
        })?;

    // Support DSB_API_KEY as a convenience environment variable
    // This allows users to set a single env var instead of DSB_SERVER__API_KEY
    // Only set if api_key is not already configured
    if config.server.api_key.is_none() {
        if let Ok(dsb_api_key) = std::env::var("DSB_API_KEY") {
            debug!("Using DSB_API_KEY environment variable");
            config.server.api_key = Some(dsb_api_key);
        }
    }

    // Merge proxy: YAML / DSB_DOCKER__* `docker.proxy_env` plus process HTTP_PROXY / HTTPS_PROXY / …
    config.docker.proxy_env = merge_docker_proxy_env(config.docker.proxy_env);

    // 4. Merge CLI arguments (highest priority)
    if let Some(cli_args) = cli_args {
        config = merge_cli_args(config, cli_args);
    }

    // 5. Validate configuration
    validator::validate(&config)?;

    // 6. Log configuration summary (without secrets)
    log_config_summary(&config);

    Ok(config)
}

/// Loads configuration for tests
///
/// This function prioritizes test-specific configuration files:
/// 1. Default values
/// 2. .env.test file (if found)
/// 3. dsb.test.yaml file (if found)
/// 4. Environment variables (DSB_*)
///
/// # Errors
///
/// Returns `Err` if configuration loading or validation fails
///
/// # Example
///
/// ```rust,no_run,ignore
/// use dsb::config::loader;
///
/// let config = load_for_tests()?;
/// # Ok::<(), dsb::config::error::ConfigError>(())
/// ```
pub fn load_for_tests() -> Result<Config, ConfigError> {
    debug!("Loading test configuration...");

    let mut builder = config::Config::builder();

    // 1. Load .env.test file if found
    if let Some(env_path) = find_test_env_file() {
        let env_path_display = env_path.display().to_string();
        debug!("Loading test .env file from: {}", env_path_display);
        dotenvy::from_path(&env_path).map_err(|e| ConfigError::FileLoadError {
            path: env_path_display.clone(),
            source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
        })?;
    } else {
        debug!("No .env.test file found, skipping");
    }

    // 2. Try to load test YAML file
    if let Some(yaml_path) = find_test_config_file() {
        debug!("Loading test YAML config from: {}", yaml_path.display());
        builder = builder.add_source(
            config::File::from(yaml_path)
                .format(config::FileFormat::Yaml)
                .required(false),
        );
    } else {
        debug!("No test YAML config file found, skipping");
    }

    // 3. Add environment variables
    builder = builder.add_source(
        config::Environment::with_prefix("DSB")
            .prefix_separator("_")
            .separator("__")
            .try_parsing(true),
    );

    // Build and deserialize
    let settings = builder.build().map_err(|e| ConfigError::ParseError {
        source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
    })?;

    let config: Config = settings
        .try_deserialize()
        .map_err(|e| ConfigError::ParseError {
            source: Box::new(e) as Box<dyn std::error::Error + Send + Sync>,
        })?;

    // 4. Validate
    validator::validate(&config)?;

    // 5. Log summary
    log_config_summary(&config);

    Ok(config)
}

/// Merges CLI arguments into configuration
///
/// CLI arguments use the same nested key format as environment variables
/// but with higher priority.
///
/// # Arguments
///
/// * `config` - Base configuration
/// * `cli_args` - HashMap of CLI argument overrides (e.g., "server.port" -> "8080")
///
/// # Returns
///
/// Configuration with CLI arguments merged in
pub fn merge_cli_args(
    mut config: Config,
    cli_args: std::collections::HashMap<String, String>,
) -> Config {
    debug!("Merging CLI arguments...");

    // Apply CLI overrides
    for (key, value) in cli_args {
        match key.as_str() {
            "server.port" => {
                if let Ok(port) = value.parse::<u16>() {
                    config.server.port = port;
                    debug!("CLI override: server.port = {}", port);
                }
            }
            "server.host" => {
                config.server.host = value.clone();
                debug!("CLI override: server.host = {}", value);
            }
            "server.api_key" => {
                config.server.api_key = Some(value.clone());
                debug!("CLI override: server.api_key = ***");
            }
            "database.url" => {
                config.database.url = Some(value.clone());
                debug!("CLI override: database.url = ***");
            }
            "docker.registry" => {
                config.docker.registry = value.clone();
                debug!("CLI override: docker.registry = {}", value);
            }
            "docker.host" => {
                config.docker.host = Some(value.clone());
                debug!("CLI override: docker.host = {}", value);
            }
            "log.level" | "logging.level" => {
                config.logging.level = value.clone();
                debug!("CLI override: logging.level = {}", value);
            }
            "ssh.port" => {
                if let Ok(port) = value.parse::<u16>() {
                    config.ssh.port = port;
                    debug!("CLI override: ssh.port = {}", port);
                }
            }
            "ssh.api_url" => {
                config.ssh.api_url = value.clone();
                debug!("CLI override: ssh.api_url = {}", value);
            }
            "sandbox.default_inactivity_timeout" => {
                if let Ok(timeout) = value.parse::<u64>() {
                    config.sandbox.default_inactivity_timeout = timeout;
                    debug!(
                        "CLI override: sandbox.default_inactivity_timeout = {}",
                        timeout
                    );
                }
            }
            "sandbox.cleanup_dry_run" => {
                if let Ok(dry_run) = value.parse::<bool>() {
                    config.sandbox.cleanup_dry_run = dry_run;
                    debug!("CLI override: sandbox.cleanup_dry_run = {}", dry_run);
                }
            }
            _ => {
                warn!("Unknown CLI argument: {}", key);
            }
        }
    }

    config
}

/// Finds .env file in current directory or parent directories
///
/// Search order:
/// 1. ./.env
/// 2. ../.env
/// 3. ../../.env (up to 3 levels)
fn find_env_file() -> Option<std::path::PathBuf> {
    find_file(".env", 3)
}

/// Finds YAML config file in current directory or parent directories
///
/// Search order:
/// 1. ./dsb.yaml
/// 2. ./dsb.yml
/// 3. ../dsb.yaml
/// 4. ../dsb.yml
/// 5. ../../dsb.yaml (up to 3 levels)
fn find_config_file() -> Option<std::path::PathBuf> {
    find_file("dsb.yaml", 3).or_else(|| find_file("dsb.yml", 3))
}

/// Finds test .env file
///
/// Search order:
/// 1. ./.env.test
/// 2. ../.env.test
/// 3. ../../.env.test (up to 3 levels)
fn find_test_env_file() -> Option<std::path::PathBuf> {
    find_file(".env.test", 3)
}

/// Finds test YAML config file
///
/// Search order:
/// 1. ./dsb.test.yaml
/// 2. ./dsb.test.yml
/// 3. ../dsb.test.yaml
/// 4. ../../dsb.test.yaml (up to 3 levels)
fn find_test_config_file() -> Option<std::path::PathBuf> {
    find_file("dsb.test.yaml", 3).or_else(|| find_file("dsb.test.yml", 3))
}

/// Generic file finder that searches up the directory tree
///
/// # Arguments
///
/// * `filename` - Name of file to find
/// * `max_levels` - Maximum number of parent directories to search
fn find_file(filename: &str, max_levels: usize) -> Option<std::path::PathBuf> {
    let current_dir = std::env::current_dir().ok()?;

    for level in 0..=max_levels {
        let path = if level == 0 {
            current_dir.join(filename)
        } else {
            let parent = current_dir.ancestors().nth(level)?;
            parent.join(filename)
        };

        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Reads HTTP proxy environment variables from the process environment
///
/// These are set by docker-compose from the deployment `.env` file and should
/// be forwarded to sandbox containers so they can reach external services.
/// Only non-empty values are included.
fn read_proxy_env() -> std::collections::HashMap<String, String> {
    const PROXY_ENV_VARS: &[&str] = &[
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "http_proxy",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "NO_PROXY",
        "no_proxy",
        "AWS_DEFAULT_REGION",
    ];

    let mut proxy_env = std::collections::HashMap::new();
    for var_name in PROXY_ENV_VARS {
        if let Ok(value) = std::env::var(var_name) {
            if !value.is_empty() {
                debug!("Inherited proxy env: {} ({} chars)", var_name, value.len());
                proxy_env.insert(var_name.to_string(), value);
            }
        }
    }
    proxy_env
}

/// Merges `docker.proxy_env` from config files / `DSB_DOCKER__*` with process proxy variables.
///
/// Process environment wins on key collisions so Kubernetes ConfigMap-injected `HTTP_PROXY`
/// overrides static YAML when both are present.
fn merge_docker_proxy_env(
    mut from_config: std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, String> {
    for (k, v) in read_proxy_env() {
        from_config.insert(k, v);
    }
    from_config
}

/// Logs configuration summary without exposing secrets
fn log_config_summary(config: &Config) {
    debug!("Configuration loaded:");
    debug!("  server:");
    debug!("    host: {}", config.server.host);
    debug!("    port: {}", config.server.port);
    debug!(
        "    api_key: {}",
        if config.server.api_key.is_some() {
            "***"
        } else {
            "(none)"
        }
    );
    debug!(
        "    admin_api_key: {}",
        if config.server.admin_api_key.is_some() {
            "***"
        } else {
            "(none)"
        }
    );
    debug!("    require_auth: {}", config.server.require_auth);

    debug!("  database:");
    if config.database.url.is_some() {
        debug!("    url: ***");
    } else {
        debug!("    host: {}", config.database.host);
        debug!("    port: {}", config.database.port);
        debug!("    name: {}", config.database.name);
        debug!("    user: {}", config.database.user);
        debug!(
            "    password: {}",
            if config.database.password.is_some() {
                "***"
            } else {
                "(none)"
            }
        );
    }

    debug!("  docker:");
    debug!("    registry: {}", config.docker.registry);
    debug!(
        "    host: {}",
        config.docker.host.as_deref().unwrap_or("(auto-detect)")
    );
    debug!("    default_image: {}", config.docker.default_image);
    debug!("    test_image: {}", config.docker.test_image);

    debug!("  sandbox:");
    debug!(
        "    default_inactivity_timeout: {} minutes",
        config.sandbox.default_inactivity_timeout
    );
    debug!("    cleanup_dry_run: {}", config.sandbox.cleanup_dry_run);
    debug!(
        "    state_monitor_interval: {} seconds",
        config.sandbox.state_monitor_interval
    );

    debug!("  ssh:");
    debug!("    port: {}", config.ssh.port);
    debug!("    api_url: {}", config.ssh.api_url);
    debug!(
        "    api_key: {}",
        if config.ssh.api_key.is_some() {
            "***"
        } else {
            "(none)"
        }
    );
    debug!(
        "    host_key_path: {}",
        config.ssh.host_key_path.as_deref().unwrap_or("(ephemeral)")
    );

    debug!("  logging:");
    debug!("    level: {}", config.logging.level);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::Config;
    use serial_test::serial;
    use std::collections::HashMap;
    use std::env;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_find_env_file() {
        // This test assumes no .env file exists in test directory
        let _result = find_env_file();
        // We can't assert None because tests might run from project root
        // Just verify it doesn't panic
    }

    #[test]
    fn test_find_config_file() {
        // Same as above - just verify no panic
        let _result = find_config_file();
    }

    #[test]
    fn test_find_test_env_file() {
        let _result = find_test_env_file();
        // Just verify no panic
    }

    #[test]
    fn test_find_test_config_file() {
        let _result = find_test_config_file();
        // Just verify no panic
    }

    #[test]
    fn test_merge_cli_args() {
        let config = Config::default();
        let mut cli_args = std::collections::HashMap::new();
        cli_args.insert("server.port".to_string(), "9000".to_string());
        cli_args.insert("log.level".to_string(), "debug".to_string());

        let merged = merge_cli_args(config, cli_args);

        assert_eq!(merged.server.port, 9000);
        assert_eq!(merged.logging.level, "debug");
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.docker.registry, "docker.io");
    }

    #[test]
    #[serial]
    fn test_load_with_cli_override() {
        // Save DSB env vars so they can be restored after the test.
        let saved_vars: Vec<(String, String)> = env::vars()
            .filter(|(k, _)| k.starts_with("DSB_"))
            .collect();

        // Clean up any DSB environment variables first
        for (key, _) in env::vars() {
            if key.starts_with("DSB_") {
                env::remove_var(&key);
            }
        }

        let mut cli_args = HashMap::new();
        cli_args.insert("server.port".to_string(), "9000".to_string());

        let result = load_with_cli_args(Some(cli_args));

        // Restore saved DSB environment variables
        for (key, value) in &saved_vars {
            env::set_var(key, value);
        }

        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.server.port, 9000);
    }

    #[test]
    #[serial]
    fn test_load_with_multiple_cli_overrides() {
        // Save DSB env vars so they can be restored after the test.
        let saved_vars: Vec<(String, String)> = env::vars()
            .filter(|(k, _)| k.starts_with("DSB_"))
            .collect();

        // Clean up any DSB environment variables first
        for (key, _) in env::vars() {
            if key.starts_with("DSB_") {
                env::remove_var(&key);
            }
        }

        let mut cli_args = HashMap::new();
        cli_args.insert("server.port".to_string(), "8081".to_string());
        cli_args.insert("log.level".to_string(), "debug".to_string());
        cli_args.insert("ssh.port".to_string(), "2223".to_string());

        let result = load_with_cli_args(Some(cli_args));

        // Restore saved DSB environment variables
        for (key, value) in &saved_vars {
            env::set_var(key, value);
        }

        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.server.port, 8081);
        assert_eq!(config.logging.level, "debug");
        assert_eq!(config.ssh.port, 2223);
    }

    #[test]
    #[serial]
    fn test_load_from_yaml_file() {
        // Save DSB env vars so they can be restored after the test.
        let saved_vars: Vec<(String, String)> = env::vars()
            .filter(|(k, _)| k.starts_with("DSB_"))
            .collect();

        // Clean up any DSB environment variables first
        for (key, _) in env::vars() {
            if key.starts_with("DSB_") {
                env::remove_var(&key);
            }
        }

        let temp_dir = TempDir::new().unwrap();
        let yaml_path = temp_dir.path().join("dsb.yaml");

        let yaml_content = r#"
server:
  port: 9000
  host: "127.0.0.1"

database:
  host: "dbhost"
  port: 5433
  name: "testdb"

docker:
  registry: "registry.example.com"

logging:
  level: "debug"
"#;

        let mut file = fs::File::create(&yaml_path).unwrap();
        file.write_all(yaml_content.as_bytes()).unwrap();
        file.flush().unwrap();

        // Change to temp directory so the config file is found
        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        let result = load();
        env::set_current_dir(original_dir).unwrap();

        // Restore saved DSB environment variables
        for (key, value) in &saved_vars {
            env::set_var(key, value);
        }

        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.database.host, "dbhost");
        assert_eq!(config.database.port, 5433);
        assert_eq!(config.database.name, "testdb");
        assert_eq!(config.docker.registry, "registry.example.com");
        assert_eq!(config.logging.level, "debug");
    }

    #[test]
    #[serial]
    fn test_load_from_env_file() {
        // Save DSB env vars so they can be restored after the test.
        let saved_vars: Vec<(String, String)> = env::vars()
            .filter(|(k, _)| k.starts_with("DSB_"))
            .collect();

        // Clean up any DSB environment variables first
        for (key, _) in env::vars() {
            if key.starts_with("DSB_") {
                env::remove_var(&key);
            }
        }

        // Temporarily hide any .env file in project root
        let project_env = std::path::PathBuf::from(".env");
        let backup_path = std::path::PathBuf::from(format!(".env.backup.{}", std::process::id()));
        let had_env_file = project_env.exists();
        if had_env_file {
            fs::rename(&project_env, &backup_path).unwrap();
        }

        let temp_dir = TempDir::new().unwrap();
        let env_path = temp_dir.path().join(".env");

        let env_content = r#"
DSB_SERVER__PORT=9000
DSB_SERVER__HOST=127.0.0.1
DSB_DOCKER__REGISTRY=registry.example.com
DSB_LOGGING__LEVEL=debug
"#;

        let mut file = fs::File::create(&env_path).unwrap();
        file.write_all(env_content.as_bytes()).unwrap();
        file.flush().unwrap();

        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        let result = load();

        env::set_current_dir(original_dir).unwrap();

        // Restore .env file if it existed
        if had_env_file {
            fs::rename(&backup_path, &project_env).unwrap();
        }

        // Restore saved DSB environment variables
        for (key, value) in &saved_vars {
            env::set_var(key, value);
        }

        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.docker.registry, "registry.example.com");
        assert_eq!(config.logging.level, "debug");
    }

    #[test]
    #[serial]
    fn test_load_from_environment_variables() {
        // Set environment variables
        env::set_var("DSB_SERVER__PORT", "9000");
        env::set_var("DSB_DOCKER__REGISTRY", "registry.example.com");
        env::set_var("DSB_LOGGING__LEVEL", "debug");

        let result = load();

        // Clean up env vars immediately after getting result
        env::remove_var("DSB_SERVER__PORT");
        env::remove_var("DSB_DOCKER__REGISTRY");
        env::remove_var("DSB_LOGGING__LEVEL");

        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.docker.registry, "registry.example.com");
        assert_eq!(config.logging.level, "debug");
    }

    #[test]
    #[serial]
    fn test_load_priority_env_over_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let yaml_path = temp_dir.path().join("dsb.yaml");
        let env_path = temp_dir.path().join(".env");

        // YAML sets port to 8081
        let yaml_content = r#"
server:
  port: 8081
"#;

        // .env sets port to 9000 (should override YAML)
        let env_content = r#"
DSB_SERVER__PORT=9000
"#;

        let mut yaml_file = fs::File::create(&yaml_path).unwrap();
        yaml_file.write_all(yaml_content.as_bytes()).unwrap();
        yaml_file.flush().unwrap();

        let mut env_file = fs::File::create(&env_path).unwrap();
        env_file.write_all(env_content.as_bytes()).unwrap();
        env_file.flush().unwrap();

        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        let result = load();
        env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        let config = result.unwrap();
        // Environment variable should override YAML
        assert_eq!(config.server.port, 9000);
    }

    #[test]
    #[serial]
    fn test_load_priority_cli_over_all() {
        let temp_dir = TempDir::new().unwrap();
        let yaml_path = temp_dir.path().join("dsb.yaml");
        let env_path = temp_dir.path().join(".env");

        // YAML sets port to 8081
        let yaml_content = r#"
server:
  port: 8081
"#;

        // .env sets port to 9000
        let env_content = r#"
DSB_SERVER__PORT=9000
"#;

        let mut yaml_file = fs::File::create(&yaml_path).unwrap();
        yaml_file.write_all(yaml_content.as_bytes()).unwrap();
        yaml_file.flush().unwrap();

        let mut env_file = fs::File::create(&env_path).unwrap();
        env_file.write_all(env_content.as_bytes()).unwrap();
        env_file.flush().unwrap();

        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        // CLI sets port to 9999 (should override everything)
        let mut cli_args = HashMap::new();
        cli_args.insert("server.port".to_string(), "9999".to_string());

        let result = load_with_cli_args(Some(cli_args));
        env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        let config = result.unwrap();
        // CLI should override both YAML and .env
        assert_eq!(config.server.port, 9999);
    }

    #[test]
    #[serial]
    fn test_load_test_config() {
        // Save DSB environment variables so they can be restored after the test.
        // Without restoration, other tests (e.g. database integration tests) that
        // rely on DSB_DATABASE__* vars set by docker-compose will fail because
        // load_for_tests() will fall back to defaults.
        let saved_vars: Vec<(String, String)> = env::vars()
            .filter(|(k, _)| k.starts_with("DSB_"))
            .collect();

        // Clean up any DSB environment variables first
        for (key, _) in env::vars() {
            if key.starts_with("DSB_") {
                env::remove_var(&key);
            }
        }

        let temp_dir = TempDir::new().unwrap();
        let test_yaml_path = temp_dir.path().join("dsb.test.yaml");

        let yaml_content = r#"
server:
  port: 8081

logging:
  level: "debug"
"#;

        let mut file = fs::File::create(&test_yaml_path).unwrap();
        file.write_all(yaml_content.as_bytes()).unwrap();
        file.flush().unwrap();

        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        let result = load_for_tests();
        env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.server.port, 8081);
        assert_eq!(config.logging.level, "debug");

        // Restore saved DSB environment variables
        for (key, value) in &saved_vars {
            env::set_var(key, value);
        }
    }

    #[test]
    #[serial]
    fn test_nested_environment_variables() {
        // Test double underscore separator for nested keys
        env::set_var("DSB_DOCKER__REGISTRY", "custom-registry.com");
        env::set_var("DSB_DATABASE__HOST", "dbhost");
        env::set_var("DSB_DATABASE__PORT", "5433");
        env::set_var("DSB_SSH__PORT", "2223");

        let result = load();

        // Clean up immediately after getting result
        env::remove_var("DSB_DOCKER__REGISTRY");
        env::remove_var("DSB_DATABASE__HOST");
        env::remove_var("DSB_DATABASE__PORT");
        env::remove_var("DSB_SSH__PORT");

        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.docker.registry, "custom-registry.com");
        assert_eq!(config.database.host, "dbhost");
        assert_eq!(config.database.port, 5433);
        assert_eq!(config.ssh.port, 2223);
    }

    #[test]
    #[serial]
    fn test_load_with_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let yaml_path = temp_dir.path().join("dsb.yaml");

        // Invalid YAML
        let yaml_content = r#"
server:
  port: [invalid
"#;

        let mut file = fs::File::create(&yaml_path).unwrap();
        file.write_all(yaml_content.as_bytes()).unwrap();
        file.flush().unwrap();

        let original_dir = env::current_dir().unwrap();
        env::set_current_dir(temp_dir.path()).unwrap();

        let result = load();
        env::set_current_dir(original_dir).unwrap();

        // Should still work with defaults since YAML is not required
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_config_validation_on_load() {
        let mut cli_args = HashMap::new();
        // Set invalid port
        cli_args.insert("server.port".to_string(), "0".to_string());

        let result = load_with_cli_args(Some(cli_args));
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("port") || err_msg.contains("validation"));
    }

    #[test]
    fn test_merge_cli_args_port() {
        let config = Config::default();
        let mut cli_args = HashMap::new();
        cli_args.insert("server.port".to_string(), "9999".to_string());

        let merged = merge_cli_args(config, cli_args);
        assert_eq!(merged.server.port, 9999);
    }

    #[test]
    fn test_merge_cli_args_multiple() {
        let config = Config::default();
        let mut cli_args = HashMap::new();
        cli_args.insert("server.port".to_string(), "9999".to_string());
        cli_args.insert("log.level".to_string(), "trace".to_string());
        cli_args.insert("ssh.port".to_string(), "2224".to_string());

        let merged = merge_cli_args(config, cli_args);
        assert_eq!(merged.server.port, 9999);
        assert_eq!(merged.logging.level, "trace");
        assert_eq!(merged.ssh.port, 2224);
    }

    #[test]
    #[serial]
    fn test_database_url_from_config() {
        let mut cli_args = HashMap::new();
        cli_args.insert(
            "database.url".to_string(),
            "postgresql://testuser:testpass@localhost:5432/testdb".to_string(),
        );

        let result = load_with_cli_args(Some(cli_args));
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(
            config.database.url,
            Some("postgresql://testuser:testpass@localhost:5432/testdb".to_string())
        );
    }

    #[test]
    #[serial]
    fn test_docker_host_from_config() {
        let mut cli_args = HashMap::new();
        cli_args.insert(
            "docker.host".to_string(),
            "unix:///custom/docker.sock".to_string(),
        );

        let result = load_with_cli_args(Some(cli_args));
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(
            config.docker.host,
            Some("unix:///custom/docker.sock".to_string())
        );
    }

    #[test]
    #[serial]
    fn test_api_key_from_config() {
        let mut cli_args = HashMap::new();
        cli_args.insert("server.api_key".to_string(), "secret-key".to_string());

        let result = load_with_cli_args(Some(cli_args));
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.server.api_key, Some("secret-key".to_string()));
    }

    #[test]
    #[serial]
    fn test_sandbox_config_from_config() {
        let mut cli_args = HashMap::new();
        cli_args.insert(
            "sandbox.default_inactivity_timeout".to_string(),
            "45".to_string(),
        );
        cli_args.insert("sandbox.cleanup_dry_run".to_string(), "true".to_string());

        let result = load_with_cli_args(Some(cli_args));
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.sandbox.default_inactivity_timeout, 45);
        assert!(config.sandbox.cleanup_dry_run);
    }
}
