// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Configuration Error Types
//!
//! Custom error types for configuration loading and validation.

use thiserror::Error;

/// Configuration error type
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Failed to load configuration file
    #[error("Failed to load configuration from '{path}': {source}")]
    FileLoadError {
        /// Path to the configuration file
        path: String,
        /// Underlying error
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Configuration validation failed
    #[error("Configuration validation failed: {message}")]
    ValidationError {
        /// Validation error message
        message: String,
    },

    /// Missing required configuration value
    #[error("Missing required configuration: {path}")]
    MissingValue {
        /// Configuration key path
        path: String,
    },

    /// Invalid configuration value
    #[error("Invalid value for '{path}': {value} - {reason}")]
    InvalidValue {
        /// Configuration key path
        path: String,
        /// Invalid value
        value: String,
        /// Reason the value is invalid
        reason: String,
    },

    /// Failed to parse configuration
    #[error("Failed to parse configuration: {source}")]
    ParseError {
        /// Underlying parse error
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}
