// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
// TODO: Deprecate once SandboxManager is fully rolled out
//! Docker API Trait Abstraction
//!
//! This module defines a trait for Docker operations, enabling mocking and testing
//! without requiring a running Docker daemon.

use async_trait::async_trait;

use crate::core::types::{ContainerStats, SandboxConfig, VolumeMount};

use crate::core::errors::ErrorCode;

/// Errors that can occur during Docker operations.
///
/// Each error variant maps to a specific `ErrorCode` for consistent error handling
/// across the API. Use the `error_code()` method to get the corresponding code.
#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    /// Docker API error
    #[error("Docker API error: {0}")]
    Api(String),

    /// Tool proxy error with specific error code
    ///
    /// This variant is used when the tool proxy returns an error with a specific
    /// error code that needs to be preserved through the error handling chain.
    #[error("Tool proxy error: {message}")]
    ToolProxy {
        /// Error message from the tool proxy
        message: String,
        /// Error code from the tool proxy (e.g., TOOL_VALIDATION_ERROR)
        code: ErrorCode,
    },

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
}

impl Clone for DockerError {
    fn clone(&self) -> Self {
        match self {
            Self::Api(msg) => Self::Api(msg.clone()),
            Self::ToolProxy { message, code } => Self::ToolProxy {
                message: message.clone(),
                code: *code,
            },
            Self::ContainerNotFound(id) => Self::ContainerNotFound(id.clone()),
            Self::ImageNotFound(id) => Self::ImageNotFound(id.clone()),
            Self::ExecFailed(msg) => Self::ExecFailed(msg.clone()),
            Self::Volume(msg) => Self::Volume(msg.clone()),
            // For Io errors, we can't clone the io::Error, so convert to Api
            Self::Io(io_err) => Self::Api(io_err.to_string()),
        }
    }
}

impl DockerError {
    /// Get the error code for this error
    ///
    /// Returns the unified `ErrorCode` that corresponds to this error variant.
    /// This enables consistent error handling across Rust backend, Python SDK, and sandbox.
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::Api(_) => ErrorCode::ServiceUnavailable,
            Self::ToolProxy { code, .. } => *code,
            Self::ContainerNotFound(_) => ErrorCode::BackendContainerNotFound,
            Self::ImageNotFound(_) => ErrorCode::BackendImagePullFailed,
            Self::ExecFailed(_) => ErrorCode::BackendExecFailed,
            Self::Volume(_) => ErrorCode::BackendVolumeError,
            Self::Io(_) => ErrorCode::InternalError,
        }
    }
}

/// Result type for Docker operations.
pub type DockerResult<T> = Result<T, DockerError>;

/// Abstraction for Docker operations.
///
/// This trait allows mocking Docker functionality for testing without
/// requiring a running Docker daemon. All methods return `DockerResult<T>`
/// for consistent error handling.
///
/// # Design
///
/// - Uses `async_trait` for async method support
/// - Returns `DockerResult<T>` for error handling
/// - Takes references to avoid ownership issues
/// - Supports all common Docker operations used by DSB
///
/// # Example
///
/// ```rust,ignore
/// use dsb::docker::DockerTrait;
/// use dsb::core::types::SandboxConfig;
///
/// async fn create_sandbox<D: DockerTrait>(
///     docker: &D,
///     config: &SandboxConfig
/// ) -> Result<String, Box<dyn std::error::Error>> {
///     let container_id = docker.create_container(config).await?;
///     docker.start_container(&container_id).await?;
///     Ok(container_id)
/// }
/// ```
#[async_trait]
pub trait DockerTrait: Send + Sync {
    /// Creates a new Docker container.
    ///
    /// # Arguments
    ///
    /// * `config` - Sandbox configuration
    /// * `sandbox_id` - Optional sandbox UUID for static file server setup
    ///
    /// # Returns
    ///
    /// Container ID string on success
    async fn create_container(
        &self,
        config: &SandboxConfig,
        sandbox_id: Option<&uuid::Uuid>,
    ) -> DockerResult<String>;

    /// Starts a created container.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Container identifier
    async fn start_container(&self, container_id: &str) -> DockerResult<()>;

    /// Stops a running container.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Container identifier
    async fn stop_container(&self, container_id: &str) -> DockerResult<()>;

    /// Removes a container.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Container identifier
    async fn remove_container(&self, container_id: &str) -> DockerResult<()>;

    /// Pulls a Docker image.
    ///
    /// # Arguments
    ///
    /// * `image` - Image name (e.g., "nginx:latest")
    async fn pull_image(&self, image: &str) -> DockerResult<()>;

    /// Pulls a Docker image with progress callback.
    ///
    /// # Arguments
    ///
    /// * `image` - Image name (e.g., "nginx:latest")
    /// * `callback` - Callback function receiving status, current bytes, and total bytes
    async fn pull_image_with_progress<F>(&self, image: &str, callback: F) -> DockerResult<()>
    where
        F: FnMut(String, Option<u64>, Option<u64>) + Send;

    /// Checks if an image exists locally.
    ///
    /// # Arguments
    ///
    /// * `image` - Image name
    ///
    /// # Returns
    ///
    /// true if image exists locally, false otherwise
    async fn image_exists(&self, image: &str) -> DockerResult<bool>;

    /// Executes a command in a running container.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Container identifier
    /// * `command` - Command and arguments to execute
    ///
    /// # Returns
    ///
    /// Command output as string
    async fn exec_container(
        &self,
        container_id: &str,
        command: Vec<String>,
    ) -> DockerResult<String>;

    /// Gets container resource usage statistics.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Container identifier
    ///
    /// # Returns
    ///
    /// Container statistics
    async fn get_container_stats(&self, container_id: &str) -> DockerResult<ContainerStats>;

    /// Removes a Docker volume.
    ///
    /// # Arguments
    ///
    /// * `volume_name` - Volume name
    async fn remove_volume(&self, volume_name: &str) -> DockerResult<()>;

    /// Checks if a container is running.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Container identifier
    ///
    /// # Returns
    ///
    /// true if container is running, false otherwise
    async fn is_container_running(&self, container_id: &str) -> DockerResult<bool>;

    /// Creates a volume from a volume mount configuration.
    ///
    /// # Arguments
    ///
    /// * `volume_mount` - Volume mount configuration
    /// * `sandbox_id` - Sandbox identifier for naming
    async fn create_volume(
        &self,
        volume_mount: &VolumeMount,
        sandbox_id: &str,
    ) -> DockerResult<String>;

    /// Removes volumes associated with a sandbox.
    ///
    /// # Arguments
    ///
    /// * `volume_mounts` - List of volume mounts to remove
    /// * `sandbox_id` - Sandbox identifier
    async fn remove_volumes(
        &self,
        volume_mounts: &[VolumeMount],
        sandbox_id: &str,
    ) -> DockerResult<()>;

    /// Lists all local Docker images.
    ///
    /// # Returns
    ///
    /// Vector of image summaries with ID, tags, size, and creation time
    async fn list_images(&self) -> DockerResult<Vec<crate::core::types::ImageSummary>>;

    /// Inspects a Docker image to get detailed information.
    ///
    /// # Arguments
    ///
    /// * `id` - Image ID or tag (e.g., "nginx:latest" or "sha256:abc123")
    ///
    /// # Returns
    ///
    /// Detailed image information including detected features
    async fn inspect_image(&self, id: &str) -> DockerResult<crate::core::types::ImageDetails>;

    /// Removes a Docker image from local storage.
    ///
    /// # Arguments
    ///
    /// * `id` - Image ID or tag to remove
    async fn remove_image(&self, id: &str) -> DockerResult<()>;
}
