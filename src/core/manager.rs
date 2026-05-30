// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! This module defines the `SandboxManager` trait and related types.
//!
//! The `SandboxManager` is responsible for managing the lifecycle of sandboxes and images.
//! It provides an abstraction over different backends like Docker, Podman, or others.

use crate::core::types::{ImageDetails, ImageSummary};
use crate::core::types::{ContainerStats, KubernetesInfo, SandboxConfig, SandboxInfo};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

/// Errors that can occur during [`SandboxManager`] operations.
///
/// These errors abstract over backend-specific failures (Docker, Kubernetes, etc.)
/// to provide a unified error type for the core business logic layer.
#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    /// Error returned when the underlying API (e.g. Docker API) returns an error.
    #[error("API error: {0}")]
    Api(String),
    /// Error returned when a requested resource (sandbox or image) is not found.
    #[error("Not found: {0}")]
    NotFound(String),
    /// Error returned when an operation fails to complete.
    #[error("Operation failed: {0}")]
    OperationFailed(String),
    /// Error returned when an operation is not supported by the current backend.
    #[error("Not supported in this backend: {0}")]
    NotSupported(String),
    /// Error returned when an operation times out (e.g., waiting for pod readiness).
    #[error("Timeout: {0}")]
    Timeout(String),
    /// Error returned when there is a conflict (e.g., resource already exists, 409).
    #[error("Conflict: {0}")]
    Conflict(String),
    /// Standard IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// A result type for [`SandboxManager`] operations.
pub type ManagerResult<T> = Result<T, ManagerError>;

/// Result of running a command inside a sandbox.
///
/// Contains both the command output and its exit code, enabling callers to
/// distinguish between successful and failed command executions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecCommandResult {
    /// Combined stdout and stderr emitted by the command.
    pub output: String,
    /// Process exit status reported by the container backend.
    pub exit_code: i32,
}

/// A single frame from a terminal session.
///
/// Terminal sessions stream data as a sequence of frames, allowing interactive
/// bidirectional communication with a sandbox container.
#[derive(Debug, Clone)]
pub enum TerminalFrame {
    /// Output data from the terminal (stdout or stderr).
    Data(Vec<u8>),
    /// Terminal has closed.
    Closed,
}

/// Trait for interacting with a terminal session in a sandbox.
///
/// Implementations abstract over Docker exec and Kubernetes exec protocols,
/// providing a unified interface for interactive terminal sessions.
#[async_trait::async_trait]
pub trait TerminalStream: Send {
    /// Read the next frame from the terminal.
    ///
    /// Returns `None` when the terminal session has ended.
    async fn read_frame(&mut self) -> Result<Option<TerminalFrame>, ManagerError>;
    /// Write data to the terminal's stdin.
    async fn write(&mut self, data: &[u8]) -> Result<(), ManagerError>;
    /// Resize the terminal.
    ///
    /// # Arguments
    ///
    /// * `rows` - Number of rows (height)
    /// * `cols` - Number of columns (width)
    async fn resize(&mut self, rows: u16, cols: u16) -> Result<(), ManagerError>;
}

/// Trait defining the operations for managing sandboxes and images.
///
/// This trait is implemented by backends to provide a consistent interface for
/// sandbox lifecycle management.
///
/// # Implementors
///
/// - [`DockerManager`](crate::docker::DockerManager) - Docker daemon backend
/// - `K8sSandboxManager` - Kubernetes backend (with `kubernetes` feature)
#[async_trait]
pub trait SandboxManager: Send + Sync {
    /// Creates a new sandbox with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `sandbox_id` - Optional UUID to assign to the sandbox; if None, the backend generates one
    /// * `config` - Sandbox configuration (image, ports, volumes, resource limits, etc.)
    ///
    /// # Returns
    ///
    /// The backend-specific container/pod identifier (e.g., Docker container ID).
    async fn create(
        &self,
        sandbox_id: Option<&Uuid>,
        config: &SandboxConfig,
    ) -> ManagerResult<String>;

    /// Starts a sandbox with the given ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier returned by [`create`](Self::create)
    async fn start(&self, id: &str) -> ManagerResult<()>;

    /// Stops a running sandbox.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    async fn stop(&self, id: &str) -> ManagerResult<()>;

    /// Deletes a sandbox and its associated resources.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    async fn delete(&self, id: &str) -> ManagerResult<()>;

    /// Executes a command within a running sandbox and returns the combined output.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    /// * `cmd` - Command and arguments as a list of strings (e.g., `["ls", "-la"]`)
    ///
    /// # Returns
    ///
    /// Combined stdout and stderr output from the command.
    async fn exec(&self, id: &str, cmd: Vec<String>) -> ManagerResult<String>;

    /// Retrieves resource usage statistics for a sandbox.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    ///
    /// # Returns
    ///
    /// Real-time CPU, memory, network I/O, and disk I/O statistics.
    async fn stats(&self, id: &str) -> ManagerResult<ContainerStats>;

    /// Checks if a sandbox is currently running.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    ///
    /// # Returns
    ///
    /// `true` if the container/pod is in a running state, `false` otherwise.
    async fn is_running(&self, id: &str) -> ManagerResult<bool>;

    /// Gets the exit information for a sandbox.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    ///
    /// # Returns
    ///
    /// A tuple of (exit_code, oom_killed) where:
    /// - `exit_code` is the process exit code (-1 if unknown)
    /// - `oom_killed` is true if the container was killed by the OOM killer
    async fn get_exit_info(&self, id: &str) -> ManagerResult<(i64, bool)>;

    /// Gets the working directory of a sandbox.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    ///
    /// # Returns
    ///
    /// The absolute path of the container's working directory.
    async fn get_workdir(&self, id: &str) -> ManagerResult<String>;

    /// Lists all sandboxes/containers managed by this backend.
    ///
    /// # Arguments
    ///
    /// * `all` - If true, include stopped/exited containers; if false, only running ones
    /// * `filters` - Optional backend-specific filters (e.g., label filters for Docker)
    ///
    /// # Returns
    ///
    /// A vector of [`SandboxInfo`] summaries for each container/pod.
    async fn list(
        &self,
        all: bool,
        filters: Option<HashMap<String, Vec<String>>>,
    ) -> ManagerResult<Vec<SandboxInfo>>;

    // Volume operations
    /// Removes a volume by name.
    ///
    /// # Arguments
    ///
    /// * `name` - The volume name to remove
    async fn remove_volume(&self, name: &str) -> ManagerResult<()>;

    // Image operations
    /// Gets detailed information about a specific image, including labels.
    ///
    /// # Arguments
    ///
    /// * `image` - The image reference (e.g., "nginx:latest")
    ///
    /// # Returns
    ///
    /// Image details including labels, size, and creation time.
    async fn get_image_features(&self, image: &str) -> ManagerResult<ImageDetails>;

    /// Lists all available images in the backend.
    ///
    /// # Returns
    ///
    /// A vector of image summaries.
    async fn list_images(&self) -> ManagerResult<Vec<ImageSummary>>;

    /// Pulls an image from a remote registry.
    ///
    /// # Arguments
    ///
    /// * `image` - The image reference to pull (e.g., "docker.io/nginx:latest")
    async fn pull_image(&self, image: &str) -> ManagerResult<()>;

    /// Pulls an image with a progress callback.
    ///
    /// The callback receives the current status string, current bytes, and total bytes if available.
    ///
    /// # Arguments
    ///
    /// * `image` - The image reference to pull
    /// * `callback` - A closure called with `(status, current_bytes, total_bytes)` on each progress event
    async fn pull_image_with_progress(
        &self,
        image: &str,
        callback: Box<dyn FnMut(String, Option<u64>, Option<u64>) + Send + 'static>,
    ) -> ManagerResult<()>;

    /// Deletes an image from the backend.
    ///
    /// # Arguments
    ///
    /// * `id` - The image ID or reference to delete
    async fn delete_image(&self, id: &str) -> ManagerResult<()>;

    /// Checks if an image exists locally.
    ///
    /// # Arguments
    ///
    /// * `image` - The image reference to check
    ///
    /// # Returns
    ///
    /// `true` if the image is available locally, `false` otherwise.
    async fn image_exists(&self, image: &str) -> ManagerResult<bool>;

    // HTTP operations
    /// Executes an HTTP request within a running sandbox and returns the JSON response.
    ///
    /// Used to communicate with the tool_proxy service running inside the sandbox.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    /// * `path` - The HTTP path (e.g., "/health", "/exec")
    /// * `method` - The HTTP method (e.g., "GET", "POST")
    /// * `body` - Optional JSON request body
    /// * `timeout_secs` - Optional request timeout in seconds
    ///
    /// # Returns
    ///
    /// The JSON response body from the sandbox's HTTP endpoint.
    async fn exec_http(
        &self,
        id: &str,
        path: &str,
        method: &str,
        body: Option<Value>,
        timeout_secs: Option<u64>,
    ) -> ManagerResult<Value>;

    /// Executes a command with stdin input within a running sandbox.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    /// * `cmd` - Command and arguments as a list of strings
    /// * `stdin` - Optional input to write to the command's stdin
    /// * `timeout_secs` - Optional execution timeout in seconds
    ///
    /// # Returns
    ///
    /// Combined stdout and stderr output from the command.
    async fn exec_with_stdin(
        &self,
        id: &str,
        cmd: Vec<String>,
        stdin: Option<String>,
        timeout_secs: Option<u64>,
    ) -> ManagerResult<String>;

    /// Executes a command with stdin input within a running sandbox and returns
    /// the combined output together with the command exit code.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    /// * `cmd` - Command and arguments as a list of strings
    /// * `stdin` - Optional input to write to the command's stdin
    /// * `timeout_secs` - Optional execution timeout in seconds
    ///
    /// # Returns
    ///
    /// An [`ExecCommandResult`] containing both the output and the exit code.
    async fn exec_with_stdin_result(
        &self,
        id: &str,
        cmd: Vec<String>,
        stdin: Option<String>,
        timeout_secs: Option<u64>,
    ) -> ManagerResult<ExecCommandResult> {
        let output = self.exec_with_stdin(id, cmd, stdin, timeout_secs).await?;
        Ok(ExecCommandResult {
            output,
            exit_code: 0,
        })
    }

    /// Uploads a tar archive to a container at the specified path.
    ///
    /// Uses Docker's `PUT /containers/{id}/archive` API for efficient file transfer
    /// without shell command length limits.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    /// * `path` - The destination path inside the container where the archive is extracted
    /// * `tar_data` - The tar archive as a byte vector
    async fn upload_archive(&self, id: &str, path: &str, tar_data: Vec<u8>) -> ManagerResult<()>;

    /// Returns the network address (host:port) for accessing a sandbox on a specific port.
    ///
    /// Docker: returns container IP:port.
    /// Kubernetes: returns Service DNS name:port.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    /// * `port` - The port number to access
    async fn get_sandbox_address(&self, id: &str, port: u16) -> ManagerResult<String> {
        let _ = (id, port);
        Err(ManagerError::NotSupported(
            "get_sandbox_address not implemented".to_string(),
        ))
    }

    /// Gets Kubernetes-specific status information for a sandbox.
    ///
    /// Returns node_name, pod_ip, service_name, and message from the K8s CRD status.
    /// Returns `None` if not running on Kubernetes or if the sandbox doesn't exist.
    ///
    /// This method is only meaningful for the Kubernetes backend.
    async fn get_sandbox_k8s_status(
        &self,
        sandbox_id: &Uuid,
    ) -> ManagerResult<Option<KubernetesInfo>> {
        let _ = sandbox_id;
        Ok(None) // Default: not supported on non-K8s backends
    }

    /// Opens an interactive terminal session with a sandbox.
    ///
    /// Returns a boxed stream that abstracts over Docker exec and K8s exec protocols.
    ///
    /// # Arguments
    ///
    /// * `id` - The container/pod identifier
    /// * `shell` - Optional shell command to use (e.g., "/bin/bash"); defaults to container's default shell
    async fn exec_terminal(
        &self,
        id: &str,
        shell: Option<String>,
    ) -> ManagerResult<Box<dyn TerminalStream + Send>> {
        let _ = (id, shell);
        Err(ManagerError::NotSupported(
            "exec_terminal not implemented".to_string(),
        ))
    }
}
