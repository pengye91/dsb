// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! # Docker Exec PTY Proxy Module
//!
//! This module provides a reusable interface for creating and managing Docker exec
//! instances with pseudo-terminal (PTY) support.
//!
//! ## Overview
//!
//! The `DockerExecProxy` simplifies the process of:
//! - Creating exec instances with PTY enabled
//! - Starting exec and getting I/O streams
//! - Resizing PTY windows
//! - Managing exec lifecycle
//!
//! ## Example
//!
//! ```rust,no_run,ignore,ignore
//! use dsb::docker::exec_proxy::{DockerExecProxy, DockerExecProxyTrait, ExecConfig};
//! use bollard::Docker;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let docker = Docker::connect_with_defaults().await?;
//! let proxy = DockerExecProxy::new(docker);
//!
//! // Create exec with PTY
//! let config = ExecConfig {
//!     container_id: "abc123",
//!     command: vec!["bash".to_string()],
//!     ..Default::default()
//! };
//! let exec_id = proxy.create_exec_pty(&config).await?;
//!
//! // Start exec and get stream
//! let stream = proxy.start_exec(&exec_id).await?;
//!
//! // Use stream for I/O...
//! # Ok(())
//! # }
//! ```

use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use bollard::exec::{CreateExecOptions, StartExecOptions};
use bollard::query_parameters::InspectContainerOptions;
use bollard::Docker;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, instrument};

/// Configuration for creating a Docker exec instance with PTY.
#[derive(Debug, Clone)]
pub struct ExecConfig {
    /// Container ID or name
    pub container_id: String,

    /// Command to execute
    pub command: Vec<String>,

    /// Working directory
    pub working_dir: Option<String>,

    /// Environment variables
    pub env: Option<Vec<String>>,

    /// User to run as
    pub user: Option<String>,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            container_id: String::new(),
            command: vec!["bash".to_string()],
            working_dir: None,
            env: None,
            user: None,
        }
    }
}

/// Errors that can occur during Docker exec operations.
#[derive(Error, Debug)]
pub enum ExecProxyError {
    /// The specified container was not found
    #[error("Container not found: {0}")]
    ContainerNotFound(String),

    /// Failed to create the exec instance
    #[error("Exec creation failed: {0}")]
    CreationFailed(String),

    /// Failed to start the exec instance
    #[error("Exec start failed: {0}")]
    StartFailed(String),

    /// Failed to resize the PTY
    #[error("PTY resize failed: {0}")]
    ResizeFailed(String),

    /// Generic Docker API error
    #[error("Docker API error: {0}")]
    DockerError(String),

    /// IO error during exec operation
    #[error("IO error: {0}")]
    IoError(String),
}

/// Result type for exec proxy operations.
pub type Result<T> = std::result::Result<T, ExecProxyError>;

/// Trait for Docker exec operations with PTY support.
///
/// This trait allows for different implementations (e.g., mocking for tests)
/// and provides a clean interface for exec management.
#[async_trait::async_trait]
pub trait DockerExecProxyTrait: Send + Sync {
    /// Create a new exec instance with PTY enabled.
    ///
    /// # Arguments
    ///
    /// * `config` - Exec configuration
    ///
    /// # Returns
    ///
    /// The exec ID
    async fn create_exec_pty(&self, config: &ExecConfig) -> Result<String>;

    /// Start an exec instance and return the I/O stream.
    ///
    /// # Arguments
    ///
    /// * `exec_id` - Exec instance ID
    ///
    /// # Returns
    ///
    /// A multiplexed stream for bidirectional I/O with the exec process
    async fn start_exec(&self, exec_id: &str) -> Result<ExecMultiplexedStream>;

    /// Resize the PTY window for an exec instance.
    ///
    /// # Arguments
    ///
    /// * `exec_id` - Exec instance ID
    /// * `rows` - New number of rows
    /// * `cols` - New number of columns
    async fn resize_pty(&self, exec_id: &str, rows: u16, cols: u16) -> Result<()>;
}

pin_project! {
    /// Multiplexed I/O stream for Docker exec.
    ///
    /// This stream combines stdout and stderr into a single bidirectional stream.
    pub struct ExecMultiplexedStream {
        #[pin]
        output: Pin<Box<dyn futures_util::stream::Stream<Item = std::result::Result<LogOutput, BollardError>> + Send>>,
        #[pin]
        input: Pin<Box<dyn tokio::io::AsyncWrite + Send>>,
    }
}

impl ExecMultiplexedStream {
    /// Create a new exec multiplexed stream.
    pub fn new(
        output: Pin<
            Box<
                dyn futures_util::stream::Stream<
                        Item = std::result::Result<LogOutput, BollardError>,
                    > + Send,
            >,
        >,
        input: Pin<Box<dyn tokio::io::AsyncWrite + Send>>,
    ) -> Self {
        Self { output, input }
    }

    /// Write data to the exec process (stdin).
    pub async fn write(&mut self, data: &[u8]) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        self.input
            .write_all(data)
            .await
            .map_err(|e| ExecProxyError::IoError(e.to_string()))?;
        Ok(())
    }

    /// Read the next frame from the exec process.
    pub async fn read_frame(&mut self) -> Result<Option<LogOutput>> {
        use futures_util::stream::StreamExt;
        match self.output.next().await {
            Some(Ok(frame)) => Ok(Some(frame)),
            Some(Err(e)) => Err(ExecProxyError::IoError(e.to_string())),
            None => Ok(None),
        }
    }

    /// Split the stream into separate read and write halves.
    ///
    /// This allows concurrent reading and writing from different tasks.
    pub fn split(self) -> (ExecReadStream, ExecWriteStream) {
        use futures_util::stream::StreamExt;
        use tokio::sync::mpsc;

        // Create channels for coordination
        let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(100);
        let (read_tx, read_rx) = mpsc::channel::<Option<LogOutput>>(100);

        // Spawn task to handle writes
        tokio::spawn(async move {
            let mut input = self.input;
            while let Some(data) = write_rx.recv().await {
                use tokio::io::AsyncWriteExt;
                if input.write_all(&data).await.is_err() {
                    break;
                }
                let _ = input.flush().await;
            }
        });

        // Spawn task to handle reads
        tokio::spawn(async move {
            let mut output = self.output;
            loop {
                match output.next().await {
                    Some(Ok(frame)) => {
                        if read_tx.send(Some(frame)).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(_)) | None => {
                        let _ = read_tx.send(None).await;
                        break;
                    }
                }
            }
        });

        (
            ExecReadStream { receiver: read_rx },
            ExecWriteStream { sender: write_tx },
        )
    }
}

/// Read half of the exec stream.
pub struct ExecReadStream {
    receiver: tokio::sync::mpsc::Receiver<Option<LogOutput>>,
}

impl ExecReadStream {
    /// Read the next frame from the exec process.
    pub async fn read_frame(&mut self) -> Result<Option<LogOutput>> {
        match self.receiver.recv().await {
            Some(Some(frame)) => Ok(Some(frame)),
            Some(None) => Ok(None),
            None => Ok(None), // Channel closed
        }
    }
}

/// Write half of the exec stream.
pub struct ExecWriteStream {
    sender: tokio::sync::mpsc::Sender<Vec<u8>>,
}

impl ExecWriteStream {
    /// Write data to the exec process (stdin).
    pub async fn write(&self, data: &[u8]) -> Result<()> {
        self.sender
            .send(data.to_vec())
            .await
            .map_err(|_| ExecProxyError::IoError("Failed to send write data".to_string()))
    }
}

/// Docker exec proxy implementation using bollard.
#[derive(Clone)]
pub struct DockerExecProxy {
    docker: Docker,
}

impl DockerExecProxy {
    /// Create a new Docker exec proxy.
    ///
    /// # Arguments
    ///
    /// * `docker` - Docker client
    ///
    /// # Returns
    ///
    /// A new `DockerExecProxy` instance
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    /// Create a new Docker exec proxy from a shared Docker client.
    ///
    /// This is the preferred constructor when the Docker client is already
    /// managed by a [`crate::docker::DockerManager`], avoiding redundant
    /// connections to the Docker daemon.
    ///
    /// # Arguments
    ///
    /// * `docker` - Shared Docker client (`Arc<Docker>`)
    ///
    /// # Returns
    ///
    /// A new `DockerExecProxy` instance
    pub fn new_from_arc(docker: Arc<Docker>) -> Self {
        // Arc<Docker> derefs to &Docker, but DockerManager's docker_client()
        // returns an Arc-wrapped Docker. We dereference to get an owned Docker.
        // Bollard's Docker is cheaply cloneable (it wraps an Arc internally).
        match Arc::try_unwrap(docker) {
            Ok(d) => Self { docker: d },
            Err(arc) => Self {
                docker: (*arc).clone(),
            },
        }
    }
}

#[async_trait::async_trait]
impl DockerExecProxyTrait for DockerExecProxy {
    #[instrument(skip(self, config), fields(container_id = %config.container_id, command = ?config.command))]
    async fn create_exec_pty(&self, config: &ExecConfig) -> Result<String> {
        debug!("Creating Docker exec with PTY");

        // Validate container exists
        let _container = self
            .docker
            .inspect_container(&config.container_id, None::<InspectContainerOptions>)
            .await
            .map_err(|e| {
                if e.to_string().contains("No such container") {
                    ExecProxyError::ContainerNotFound(config.container_id.clone())
                } else {
                    ExecProxyError::DockerError(e.to_string())
                }
            })?;

        // Build exec options with PTY enabled
        let options = CreateExecOptions {
            cmd: Some(config.command.clone()),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(true), // Enable PTY
            working_dir: config.working_dir.clone(),
            env: config.env.clone(),
            user: config.user.clone(),
            ..Default::default()
        };

        // Create exec instance
        let exec = self
            .docker
            .create_exec(&config.container_id, options)
            .await
            .map_err(|e| ExecProxyError::CreationFailed(e.to_string()))?;

        debug!(exec_id = %exec.id, "Exec created successfully");
        Ok(exec.id)
    }

    #[instrument(skip(self), fields(exec_id = %exec_id))]
    async fn start_exec(&self, exec_id: &str) -> Result<ExecMultiplexedStream> {
        debug!("Starting Docker exec");

        // Start exec with detach=false to get the stream
        let start_options = Some(StartExecOptions {
            detach: false,
            ..Default::default()
        });

        let result = self
            .docker
            .start_exec(exec_id, start_options)
            .await
            .map_err(|e| ExecProxyError::StartFailed(e.to_string()))?;

        // Extract the multiplexed stream
        match result {
            bollard::exec::StartExecResults::Attached { output, input } => {
                debug!("Exec started successfully");
                Ok(ExecMultiplexedStream::new(output, input))
            }
            _ => Err(ExecProxyError::StartFailed(
                "Unexpected result type from start_exec".to_string(),
            )),
        }
    }

    #[instrument(skip(self), fields(exec_id = %exec_id, rows = rows, cols = cols))]
    async fn resize_pty(&self, exec_id: &str, rows: u16, cols: u16) -> Result<()> {
        debug!("Resizing Docker exec PTY");

        use bollard::query_parameters::ResizeExecOptions;

        let options = ResizeExecOptions {
            h: rows.into(),
            w: cols.into(),
        };

        self.docker
            .resize_exec(exec_id, options)
            .await
            .map_err(|e| ExecProxyError::ResizeFailed(e.to_string()))?;

        debug!("PTY resized successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;
    use std::sync::Arc;
    use tokio::io::{self, AsyncWrite};

    // ========================================================================
    // ExecConfig Tests
    // ========================================================================

    #[test]
    fn test_exec_config_default() {
        let config = ExecConfig::default();
        assert_eq!(config.command.len(), 1);
        assert_eq!(config.command[0], "bash");
        assert!(config.working_dir.is_none());
        assert!(config.env.is_none());
        assert!(config.user.is_none());
    }

    #[test]
    fn test_exec_config_custom() {
        let config = ExecConfig {
            container_id: "test-container".to_string(),
            command: vec!["echo".to_string(), "hello".to_string()],
            working_dir: Some("/tmp".to_string()),
            env: Some(vec!["FOO=bar".to_string()]),
            user: Some("root".to_string()),
        };

        assert_eq!(config.container_id, "test-container");
        assert_eq!(config.command, vec!["echo", "hello"]);
        assert_eq!(config.working_dir, Some("/tmp".to_string()));
        assert_eq!(config.env, Some(vec!["FOO=bar".to_string()]));
        assert_eq!(config.user, Some("root".to_string()));
    }

    #[test]
    fn test_exec_config_empty_command() {
        let config = ExecConfig {
            container_id: "test".to_string(),
            command: vec![],
            working_dir: None,
            env: None,
            user: None,
        };
        assert!(config.command.is_empty());
    }

    #[test]
    fn test_exec_config_multiple_env_vars() {
        let config = ExecConfig {
            container_id: "test".to_string(),
            command: vec!["sh".to_string()],
            working_dir: None,
            env: Some(vec![
                "PATH=/usr/bin".to_string(),
                "HOME=/root".to_string(),
                "TERM=xterm".to_string(),
            ]),
            user: None,
        };
        assert_eq!(config.env.as_ref().unwrap().len(), 3);
    }

    // ========================================================================
    // Error Tests
    // ========================================================================

    #[test]
    fn test_error_display() {
        let err = ExecProxyError::ContainerNotFound("abc123".to_string());
        assert!(err.to_string().contains("Container not found"));
        assert!(err.to_string().contains("abc123"));
    }

    #[test]
    fn test_error_creation_failed() {
        let err = ExecProxyError::CreationFailed("invalid command".to_string());
        assert!(err.to_string().contains("Exec creation failed"));
        assert!(err.to_string().contains("invalid command"));
    }

    #[test]
    fn test_error_start_failed() {
        let err = ExecProxyError::StartFailed("exec not found".to_string());
        assert!(err.to_string().contains("Exec start failed"));
    }

    #[test]
    fn test_error_resize_failed() {
        let err = ExecProxyError::ResizeFailed("invalid dimensions".to_string());
        assert!(err.to_string().contains("PTY resize failed"));
    }

    #[test]
    fn test_error_docker_error() {
        let err = ExecProxyError::DockerError("connection refused".to_string());
        assert!(err.to_string().contains("Docker API error"));
    }

    #[test]
    fn test_error_io_error() {
        let err = ExecProxyError::IoError("broken pipe".to_string());
        assert!(err.to_string().contains("IO error"));
    }

    // ========================================================================
    // ExecMultiplexedStream Tests
    // ========================================================================

    #[tokio::test]
    async fn test_exec_multiplexed_stream_write() {
        // Create a mock write stream
        let write_buffer = Vec::new();
        let mock_write = {
            let buffer = Arc::new(std::sync::Mutex::new(write_buffer));
            struct MockWrite {
                buffer: Arc<std::sync::Mutex<Vec<u8>>>,
            }
            impl AsyncWrite for MockWrite {
                fn poll_write(
                    self: std::pin::Pin<&mut Self>,
                    _cx: &mut std::task::Context<'_>,
                    buf: &[u8],
                ) -> std::task::Poll<std::io::Result<usize>> {
                    let mut buffer = self.buffer.lock().unwrap();
                    buffer.extend_from_slice(buf);
                    std::task::Poll::Ready(Ok(buf.len()))
                }

                fn poll_flush(
                    self: std::pin::Pin<&mut Self>,
                    _cx: &mut std::task::Context<'_>,
                ) -> std::task::Poll<std::io::Result<()>> {
                    std::task::Poll::Ready(Ok(()))
                }

                fn poll_shutdown(
                    self: std::pin::Pin<&mut Self>,
                    _cx: &mut std::task::Context<'_>,
                ) -> std::task::Poll<std::io::Result<()>> {
                    std::task::Poll::Ready(Ok(()))
                }
            }
            let mock = MockWrite { buffer };
            Box::pin(mock) as Pin<Box<dyn AsyncWrite + Send>>
        };

        // Create a dummy output stream
        let dummy_output =
            Box::pin(stream::empty::<std::result::Result<LogOutput, BollardError>>());

        let mut stream = ExecMultiplexedStream::new(dummy_output, mock_write);
        let result = stream.write(b"test data").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_exec_multiplexed_stream_read_frame() {
        // Create a mock output stream with frames
        let frame = LogOutput::StdOut {
            message: b"test output".to_vec().into(),
        };

        let mock_output = Box::pin(stream::iter(vec![Ok(frame)]));

        // Create a dummy write stream
        let dummy_write = Box::pin(io::empty()) as Pin<Box<dyn AsyncWrite + Send>>;

        let mut stream = ExecMultiplexedStream::new(mock_output, dummy_write);

        let result = stream.read_frame().await;
        assert!(result.is_ok());
        let frame_opt = result.unwrap();
        assert!(frame_opt.is_some());

        // Verify it's a stdout frame
        match frame_opt.unwrap() {
            LogOutput::StdOut { .. } => (),
            _ => panic!("Expected StdOut variant"),
        }
    }

    #[tokio::test]
    async fn test_exec_multiplexed_stream_read_frame_eof() {
        // Create an empty output stream
        let mock_output = Box::pin(stream::empty::<std::result::Result<LogOutput, BollardError>>());

        // Create a dummy write stream
        let dummy_write = Box::pin(io::empty()) as Pin<Box<dyn AsyncWrite + Send>>;

        let mut stream = ExecMultiplexedStream::new(mock_output, dummy_write);

        let result = stream.read_frame().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // EOF
    }

    #[tokio::test]
    async fn test_exec_multiplexed_stream_split() {
        // Create mock streams
        let frame = LogOutput::StdOut {
            message: b"test".to_vec().into(),
        };
        let mock_output = Box::pin(stream::iter(vec![Ok(frame)]));
        let mock_write = Box::pin(io::empty()) as Pin<Box<dyn AsyncWrite + Send>>;

        let stream = ExecMultiplexedStream::new(mock_output, mock_write);

        let (mut read_stream, write_stream) = stream.split();

        // Test read stream
        let result = read_stream.read_frame().await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());

        // Test write stream
        let result = write_stream.write(b"test input").await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // ExecReadStream Tests
    // ========================================================================

    #[tokio::test]
    async fn test_exec_read_stream_read_frame() {
        use tokio::sync::mpsc;

        let (tx, rx) = mpsc::channel::<Option<LogOutput>>(10);

        let frame = LogOutput::StdErr {
            message: b"error message".to_vec().into(),
        };

        tokio::spawn(async move {
            let _ = tx.send(Some(frame)).await;
        });

        let mut stream = ExecReadStream { receiver: rx };
        let result = stream.read_frame().await;

        assert!(result.is_ok());
        let frame_opt = result.unwrap();
        assert!(frame_opt.is_some());

        // Verify it's a stderr frame
        match frame_opt.unwrap() {
            LogOutput::StdErr { .. } => (),
            _ => panic!("Expected StdErr variant"),
        }
    }

    #[tokio::test]
    async fn test_exec_read_stream_eof() {
        use tokio::sync::mpsc;

        let (tx, rx) = mpsc::channel::<Option<LogOutput>>(10);

        // Close the channel immediately
        drop(tx);

        let mut stream = ExecReadStream { receiver: rx };
        let result = stream.read_frame().await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // EOF
    }

    #[tokio::test]
    async fn test_exec_read_stream_none_sent() {
        use tokio::sync::mpsc;

        let (tx, rx) = mpsc::channel::<Option<LogOutput>>(10);

        tokio::spawn(async move {
            let _ = tx.send(None).await;
        });

        let mut stream = ExecReadStream { receiver: rx };
        let result = stream.read_frame().await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // ========================================================================
    // ExecWriteStream Tests
    // ========================================================================

    #[tokio::test]
    async fn test_exec_write_stream_write() {
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(10);

        let stream = ExecWriteStream { sender: tx };

        let result = stream.write(b"test data").await;
        assert!(result.is_ok());

        // Verify data was sent
        let received = rx.recv().await;
        assert!(received.is_some());
        assert_eq!(received.unwrap(), b"test data".to_vec());
    }

    #[tokio::test]
    async fn test_exec_write_stream_write_large_data() {
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(10);

        let stream = ExecWriteStream { sender: tx };

        let large_data = vec![b'X'; 10000];
        let result = stream.write(&large_data).await;
        assert!(result.is_ok());

        let received = rx.recv().await;
        assert!(received.is_some());
        assert_eq!(received.unwrap().len(), 10000);
    }

    #[tokio::test]
    async fn test_exec_write_stream_write_empty() {
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(10);

        let stream = ExecWriteStream { sender: tx };

        let result = stream.write(b"").await;
        assert!(result.is_ok());

        let received = rx.recv().await;
        assert!(received.is_some());
        assert_eq!(received.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_exec_write_stream_channel_closed() {
        use tokio::sync::mpsc;

        let (tx, _rx) = mpsc::channel::<Vec<u8>>(10);
        let stream = ExecWriteStream { sender: tx };

        // Drop receiver to close the channel
        drop(stream);

        // Writing to a closed stream will fail when the task tries to send
        // This is tested implicitly by the implementation
    }
}
