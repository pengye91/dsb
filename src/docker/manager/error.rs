// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! DockerManager-specific error types.

/// Errors that can occur during Docker manager operations.
///
/// This enum provides strongly-typed errors for all Docker manager methods,
/// replacing the previous `Box<dyn std::error::Error + Send + Sync>` approach.
#[derive(Debug, thiserror::Error)]
pub enum DockerManagerError {
    /// Docker API error
    #[error("Docker API error: {0}")]
    Api(String),

    /// Container not found
    #[error("Container not found: {0}")]
    ContainerNotFound(String),

    /// Image not found
    #[error("Image not found: {0}")]
    ImageNotFound(String),

    /// Exec operation failed
    #[error("Exec failed: {0}")]
    ExecFailed(String),

    /// Volume operation failed
    #[error("Volume error: {0}")]
    Volume(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Bollard client error
    #[error("Bollard error: {0}")]
    Bollard(#[from] bollard::errors::Error),

    /// Tool proxy error
    #[error("Tool proxy error: {message}")]
    ToolProxy {
        /// Error message from the tool proxy
        message: String,
        /// Operation that failed
        operation: String,
    },

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// HTTP client error
    #[error("HTTP error: {0}")]
    Http(String),

    /// Operation timed out
    #[error("Timeout: {0}")]
    Timeout(String),
}
