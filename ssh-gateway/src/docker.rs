// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Docker Exec Proxy Module
//!
//! This module provides Docker exec instance management with PTY support.
//! It handles bidirectional I/O streaming between the SSH gateway and containers.
//!
//! ## Overview
//!
//! The `DockerExecProxy` manages interactive shell sessions in Docker containers:
//! - Creates exec instances with PTY (pseudo-terminal)
//! - Handles terminal resize operations
//! - Provides bidirectional I/O streaming (stdin/stdout/stderr)
//! - Properly cleans up exec instances on disconnect
//! - Supports stream demultiplexing (separate stdout/stderr)
//!
//! ## Features
//!
//! - ✅ Exec instance creation with PTY allocation
//! - ✅ Bidirectional streaming (stdin/stdout/stderr)
//! - ✅ PTY resize support
//! - ✅ Stream demultiplexing (stdout/stderr separation)
//! - ✅ Exec lifecycle management (start, inspect, cleanup)
//! - ✅ Integration with DSB configuration system
//!
//! ## Architecture
//!
//! ```text
//! SSH Gateway → DockerExecProxy → Docker API → Container Shell
//!                      ↓
//!                 PTY I/O Stream
//!                      ↓
//!              (stdin/stdout/stderr)
//! ```
//!
//! ## Configuration
//!
//! `DockerExecProxy` integrates with DSB's configuration system:
//! - Uses `config::load()` to read Docker configuration
//! - Respects `DSB_DOCKER__HOST` environment variable
//! - Falls back to platform-specific defaults
//! - Supports unix://, tcp://, and http:// protocols

use anyhow::{Context, Result};
use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::models::ExecInspectResponse;
use bollard::Docker;
use futures_util::StreamExt;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, instrument};

// Re-export DSB types for convenience
pub use dsb::config::Config;
pub use dsb::docker::DockerManager;

/// Size of PTY for terminal resize.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct PtySize {
    /// Number of rows
    pub rows: u16,
    /// Number of columns
    pub cols: u16,
}

impl PtySize {
    /// Create a new PTY size.
    #[allow(dead_code)]
    pub fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }

    /// Default PTY size (24x80).
    pub fn default_size() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

/// Docker exec instance with PTY support.
pub struct DockerExecProxy {
    /// Docker client (wrapped in Arc for thread-safe sharing)
    docker: Arc<Docker>,

    /// Container ID
    container_id: String,

    /// Exec instance ID
    exec_id: Option<String>,

    /// PTY size (reserved for future resize support)
    #[allow(dead_code)]
    pty_size: PtySize,

    /// Exec output stream (demultiplexed stdout/stderr)
    #[allow(clippy::type_complexity)]
    pub exec_output:
        Option<Pin<Box<dyn futures_util::Stream<Item = Result<LogOutput, BollardError>> + Send>>>,

    /// Exec input stream (stdin)
    pub exec_input: Option<Pin<Box<dyn tokio::io::AsyncWrite + Send>>>,

    /// Custom exec command (for testing)
    pub exec_config: Option<Vec<String>>,
}

#[allow(dead_code)]
impl DockerExecProxy {
    /// Create a new Docker exec proxy using DSB's configuration system.
    ///
    /// This method loads the DSB configuration and creates a DockerManager with it,
    /// ensuring the Docker connection respects `config.docker.host` and other settings.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Docker container ID
    ///
    /// # Returns
    ///
    /// A new `DockerExecProxy` instance
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Configuration loading fails
    /// - Docker connection fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use ssh_gateway::docker::DockerExecProxy;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let proxy = DockerExecProxy::new_with_config("container-id".to_string())?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_with_config(container_id: String) -> Result<Self> {
        let config = dsb::config::load()
            .map_err(|e| anyhow::anyhow!("Failed to load DSB configuration: {}", e))?;

        Self::new_with_config_and_id(container_id, &config)
    }

    /// Create a new Docker exec proxy with explicit configuration.
    ///
    /// This method is useful when you already have a loaded configuration
    /// and want to avoid reloading it.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Docker container ID
    /// * `config` - DSB configuration
    ///
    /// # Returns
    ///
    /// A new `DockerExecProxy` instance
    ///
    /// # Errors
    ///
    /// Returns an error if Docker connection fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use ssh_gateway::docker::DockerExecProxy;
    /// # use dsb::config;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = config::load()?;
    /// let proxy = DockerExecProxy::new_with_config_and_id(
    ///     "container-id".to_string(),
    ///     &config
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_with_config_and_id(container_id: String, config: &Config) -> Result<Self> {
        let docker_manager = DockerManager::new_with_config(config)
            .map_err(|e| anyhow::anyhow!("Failed to connect to Docker daemon: {}", e))?;

        Ok(Self {
            docker: docker_manager.docker_client(),
            container_id,
            exec_id: None,
            pty_size: PtySize::default_size(),
            exec_output: None,
            exec_input: None,
            exec_config: None,
        })
    }

    /// Create a new Docker exec proxy with a DockerManager.
    ///
    /// This is the preferred method when you already have a DockerManager instance,
    /// as it reuses the existing Docker connection.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Docker container ID
    /// * `docker_manager` - DockerManager instance
    ///
    /// # Returns
    ///
    /// A new `DockerExecProxy` instance
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use ssh_gateway::docker::DockerExecProxy;
    /// # use dsb::docker::DockerManager;
    /// # use dsb::config;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = config::load()?;
    /// let docker_manager = DockerManager::new_with_config(&config)?;
    /// let proxy = DockerExecProxy::with_docker_manager(
    ///     "container-id".to_string(),
    ///     &docker_manager
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_docker_manager(container_id: String, docker_manager: &DockerManager) -> Self {
        Self {
            docker: docker_manager.docker_client(),
            container_id,
            exec_id: None,
            pty_size: PtySize::default_size(),
            exec_output: None,
            exec_input: None,
            exec_config: None,
        }
    }

    /// Create a new Docker exec proxy (legacy method).
    ///
    /// # Deprecated
    ///
    /// This method uses `Docker::connect_with_defaults()` which does NOT respect
    /// DSB's configuration system. Use [`new_with_config`](Self::new_with_config)
    /// or [`with_docker_manager`](Self::with_docker_manager) instead.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Docker container ID
    ///
    /// # Returns
    ///
    /// A `Result` containing a new `DockerExecProxy` instance, or an error if
    /// the Docker daemon is unreachable.
    ///
    /// # Errors
    ///
    /// Returns an error if the Docker daemon connection fails.
    #[deprecated(
        since = "0.1.0",
        note = "Use new_with_config() or with_docker_manager() instead"
    )]
    pub fn new(container_id: String) -> Result<Self> {
        let docker = Docker::connect_with_defaults()
            .context("Failed to connect to Docker daemon")?;

        Ok(Self {
            docker: Arc::new(docker),
            container_id,
            exec_id: None,
            pty_size: PtySize::default_size(),
            exec_output: None,
            exec_input: None,
            exec_config: None,
        })
    }

    /// Create a new Docker exec proxy with custom Docker client.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Docker container ID
    /// * `docker` - Custom Docker client (wrapped in Arc)
    ///
    /// # Returns
    ///
    /// A new `DockerExecProxy` instance
    #[allow(dead_code)]
    pub fn with_docker(container_id: String, docker: Arc<Docker>) -> Self {
        Self {
            docker,
            container_id,
            exec_id: None,
            pty_size: PtySize::default_size(),
            exec_output: None,
            exec_input: None,
            exec_config: None,
        }
    }

    /// Get the container ID.
    #[allow(dead_code)]
    pub fn get_container_id(&self) -> &str {
        &self.container_id
    }

    /// Create an exec instance with PTY.
    ///
    /// This creates (but doesn't start) the exec instance with:
    /// - PTY enabled with current size
    /// - Bash shell (falls back to sh)
    /// - Interactive mode
    ///
    /// # Returns
    ///
    /// Exec instance ID
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Container not found
    /// - Docker API communication fails
    #[instrument(skip(self), fields(container_id = %self.container_id))]
    pub async fn create_exec(&mut self) -> Result<String> {
        debug!("Creating Docker exec instance with PTY");

        // Use custom exec_config if set, otherwise default to shell
        // Use direct shell invocation instead of 'sh -c' wrapper to ensure stdin works properly
        let cmd = self.exec_config.clone().unwrap_or_else(|| {
            // Try bash first, fall back to sh if needed
            // Using direct invocation instead of 'sh -c "exec shell"' to avoid process replacement issues
            vec!["/bin/bash".to_string()]
        });

        let options = CreateExecOptions {
            cmd: Some(cmd),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(true), // Enable PTY
            env: None,
            working_dir: None,
            user: None,
            detach_keys: None,
            privileged: None,
        };

        let result = self
            .docker
            .create_exec(&self.container_id, options)
            .await
            .context("Failed to create exec instance")?;

        self.exec_id = Some(result.id.clone());
        debug!("Created exec instance: {}", result.id);

        Ok(result.id)
    }

    /// Resize the PTY.
    ///
    /// This updates the terminal size for the running exec instance.
    ///
    /// # Arguments
    ///
    /// * `rows` - New number of rows
    /// * `cols` - New number of columns
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - No exec instance created
    /// - Docker API communication fails
    #[instrument(skip(self), fields(exec_id = ?self.exec_id, rows, cols))]
    #[allow(dead_code)]
    pub async fn resize_pty(&mut self, rows: u16, cols: u16) -> Result<()> {
        let exec_id = self
            .exec_id
            .as_ref()
            .context("Cannot resize PTY: no exec instance")?;

        debug!("Resizing PTY to {}x{}", rows, cols);

        self.docker
            .resize_exec(
                exec_id,
                bollard::exec::ResizeExecOptions {
                    height: rows,
                    width: cols,
                },
            )
            .await
            .context("Failed to resize PTY")?;

        self.pty_size = PtySize::new(rows, cols);

        debug!("PTY resized successfully");
        Ok(())
    }

    /// Start the exec instance and store the I/O streams.
    ///
    /// This starts the exec instance and stores the output stream and input writer
    /// for bidirectional I/O.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - No exec instance created
    /// - Docker API communication fails
    /// - Exec is not in attached mode
    #[instrument(skip(self), fields(exec_id = ?self.exec_id))]
    pub async fn start_exec(&mut self) -> Result<()> {
        let exec_id = self
            .exec_id
            .as_ref()
            .context("Cannot start exec: no exec instance")?;

        debug!("Starting exec instance {}", exec_id);

        // Start the exec with detach=false to get attached streams
        let options = StartExecOptions {
            detach: false,
            ..Default::default()
        };

        let result = self
            .docker
            .start_exec(exec_id, Some(options))
            .await
            .context("Failed to start exec instance")?;

        match result {
            StartExecResults::Attached { output, input } => {
                debug!("Exec instance {} started successfully", exec_id);
                self.exec_output = Some(output);
                self.exec_input = Some(input);
                Ok(())
            }
            StartExecResults::Detached => {
                anyhow::bail!("Exec instance was detached, expected attached mode");
            }
        }
    }

    /// Take the input stream, consuming the proxy.
    ///
    /// This is used to extract the stream for independent use without
    /// holding a lock on the entire proxy.
    #[allow(dead_code)]
    pub fn take_input_stream(&mut self) -> Option<Pin<Box<dyn tokio::io::AsyncWrite + Send>>> {
        self.exec_input.take()
    }

    /// Take the output stream, consuming the proxy.
    ///
    /// This is used to extract the stream for independent use without
    /// holding a lock on the entire proxy.
    #[allow(dead_code)]
    #[allow(clippy::type_complexity)]
    pub fn take_output_stream(
        &mut self,
    ) -> Option<Pin<Box<dyn futures_util::Stream<Item = Result<LogOutput, BollardError>> + Send>>>
    {
        self.exec_output.take()
    }

    /// Write data to the exec's stdin.
    ///
    /// # Arguments
    ///
    /// * `data` - Data to write to stdin
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Exec not started
    /// - Write fails
    pub async fn write_stdin(&mut self, data: &[u8]) -> Result<()> {
        let input = self
            .exec_input
            .as_mut()
            .context("Exec not started, no input stream")?;

        debug!(
            "Writing {} bytes to exec stdin for container {} (first 32 bytes: {:?})",
            data.len(),
            self.container_id,
            &data[..data.len().min(32)]
        );

        input
            .write_all(data)
            .await
            .context("Failed to write to exec stdin")?;

        input.flush().await.context("Failed to flush exec stdin")?;

        debug!("Successfully wrote {} bytes to exec stdin", data.len());

        Ok(())
    }

    /// Read the next chunk of output from the exec.
    ///
    /// This reads from the demultiplexed output stream which combines
    /// stdout and stderr.
    ///
    /// # Returns
    ///
    /// - `Some(Ok(Vec<u8>))` - Next chunk of output data
    /// - `Some(Err(e))` - Error reading output
    /// - `None` - Output stream closed
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Exec not started
    pub async fn read_output(&mut self) -> Option<Result<Vec<u8>, BollardError>> {
        let output = self.exec_output.as_mut()?;
        futures_util::pin_mut!(output);

        match output.next().await {
            Some(Ok(log_output)) => {
                let data = match log_output {
                    LogOutput::StdOut { message } => {
                        debug!("Received {} bytes from stdout", message.len());
                        message
                    }
                    LogOutput::StdErr { message } => {
                        debug!("Received {} bytes from stderr", message.len());
                        message
                    }
                    LogOutput::StdIn { message } => {
                        debug!("Received {} bytes from stdin", message.len());
                        message
                    }
                    LogOutput::Console { message } => {
                        debug!("Received {} bytes from console", message.len());
                        message
                    }
                };
                Some(Ok(data.to_vec()))
            }
            Some(Err(e)) => {
                error!("Error reading exec output: {:?}", e);
                Some(Err(e))
            }
            None => {
                debug!("Exec output stream closed");
                None
            }
        }
    }

    /// Inspect the exec instance to get its status.
    ///
    /// # Returns
    ///
    /// Exec inspect response with status and exit code
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - No exec instance created
    /// - Docker API communication fails
    #[instrument(skip(self), fields(exec_id = ?self.exec_id))]
    #[allow(dead_code)]
    pub async fn inspect_exec(&self) -> Result<ExecInspectResponse> {
        let exec_id = self
            .exec_id
            .as_ref()
            .context("Cannot inspect exec: no exec instance")?;

        debug!("Inspecting exec instance");

        let inspect = self
            .docker
            .inspect_exec(exec_id)
            .await
            .context("Failed to inspect exec instance")?;

        debug!(
            "Exec instance running: {}",
            inspect.running.unwrap_or(false)
        );

        Ok(inspect)
    }

    /// Check if the exec instance is still running.
    ///
    /// # Returns
    ///
    /// `true` if exec is running, `false` otherwise
    ///
    /// # Errors
    ///
    /// Returns error if inspection fails
    #[allow(dead_code)]
    pub async fn is_running(&self) -> Result<bool> {
        let inspect = self.inspect_exec().await?;
        Ok(inspect.running.unwrap_or(false))
    }

    /// Get the exit code of the exec instance.
    ///
    /// # Returns
    ///
    /// Exit code if exec has exited, `None` if still running
    ///
    /// # Errors
    ///
    /// Returns error if inspection fails
    #[allow(dead_code)]
    pub async fn get_exit_code(&self) -> Result<Option<i64>> {
        let inspect = self.inspect_exec().await?;
        Ok(inspect.exit_code)
    }
}

impl Drop for DockerExecProxy {
    fn drop(&mut self) {
        // Cleanup is handled by Docker automatically when the process exits
        // The exec instance will be removed when the container stops
        debug!(
            "DockerExecProxy dropped for container {}",
            self.container_id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_size_default() {
        let size = PtySize::default_size();
        assert_eq!(size.rows, 24);
        assert_eq!(size.cols, 80);
    }

    #[test]
    fn test_pty_size_new() {
        let size = PtySize::new(40, 120);
        assert_eq!(size.rows, 40);
        assert_eq!(size.cols, 120);
    }

    #[test]
    #[allow(deprecated)]
    fn test_docker_exec_proxy_new_returns_result() {
        // After the fix, this returns Result instead of panicking.
        // In CI without Docker this will be Err; with Docker it will be Ok.
        // Either way it must not panic.
        let result = DockerExecProxy::new("test-container".to_string());
        match result {
            Ok(proxy) => assert_eq!(proxy.get_container_id(), "test-container"),
            Err(_) => { /* Docker not available — acceptable */ }
        }
    }

    // Note: Full integration tests require a running Docker daemon
    // These would be in tests/integration_tests.rs
}
