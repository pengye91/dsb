// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Logging Initialization Module
//!
//! Configures tracing subscribers with support for:
//! - Development (pretty) vs Production (JSON) formats
//! - File and stdout output
//! - Environment-based log filtering
//! - Request ID propagation

use tracing_subscriber::EnvFilter;
use tracing_subscriber::{fmt, prelude::*};

/// Initializes tracing subscriber based on configuration
///
/// # Arguments
///
/// * `config` - Application configuration containing logging settings
///
/// # Returns
///
/// * `Ok(())` - Logging initialized successfully
/// * `Err(...)` - Failed to initialize logging
///
/// # Example
///
/// ```rust,no_run,ignore
/// # use dsb::config::Config;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = Config::load()?;
/// dsb::logging::init_logging(&config)?;
/// # Ok(())
/// # }
/// ```
pub fn init_logging(
    config: &crate::config::Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let log_config = &config.logging;

    // Parse log level and filters
    let env_filter = if let Some(filters) = &log_config.filters {
        EnvFilter::try_new(filters)?
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_config.level))
    };

    // Build subscriber based on format configuration
    if log_config.format == "json" {
        // Production: JSON format
        let subscriber = tracing_subscriber::registry().with(env_filter).with(
            fmt::layer()
                .json()
                .with_file(true)
                .with_line_number(true)
                .with_target(true),
        );
        tracing::subscriber::set_global_default(subscriber)?;
    } else {
        // Development: Pretty format
        let subscriber = tracing_subscriber::registry().with(env_filter).with(
            fmt::layer()
                .pretty()
                .with_ansi(log_config.ansi)
                .with_file(true)
                .with_line_number(true)
                .with_target(false),
        );
        tracing::subscriber::set_global_default(subscriber)?;
    }

    Ok(())
}
