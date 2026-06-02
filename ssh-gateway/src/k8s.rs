// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Kubernetes Exec Proxy Module
//!
//! This module provides Kubernetes pod exec with PTY support for the SSH gateway.
//! It handles bidirectional I/O streaming between the SSH gateway and pods.
//!
//! ## Overview
//!
//! The `K8sExecProxy` manages interactive shell sessions in Kubernetes pods:
//! - Creates exec instances with TTY via the Kubernetes API
//! - Handles terminal resize operations
//! - Provides bidirectional I/O streaming (stdin/stdout)
//! - Properly cleans up exec sessions on disconnect
//!
//! ## Architecture
//!
//! ```text
//! SSH Gateway → K8sExecProxy → Kubernetes API → Pod Shell
//!                      ↓
//!                 PTY I/O Stream
//!                      ↓
//!              (stdin/stdout)
//! ```

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::api::{Api, AttachParams};
use kube::Client;
use std::pin::Pin;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, instrument};

/// Byte stream for exec output.
pub type ByteStream =
    Pin<Box<dyn futures_util::Stream<Item = Result<Vec<u8>, std::io::Error>> + Send>>;

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

/// Kubernetes exec instance with PTY support.
pub struct K8sExecProxy {
    /// Kubernetes client
    client: Client,

    /// Namespace for sandbox pods
    namespace: String,

    /// Pod name (from sandbox.container_id)
    pod_name: String,

    /// PTY size (reserved for future resize support)
    #[allow(dead_code)]
    pty_size: PtySize,

    /// Exec output stream (stdout from pod)
    pub exec_output: Option<ByteStream>,

    /// Exec input stream (stdin to pod)
    pub exec_input: Option<Pin<Box<dyn tokio::io::AsyncWrite + Send>>>,

    /// Terminal resize channel
    terminal_size_tx: Option<futures::channel::mpsc::Sender<kube::api::TerminalSize>>,

    /// Custom exec command (for testing)
    pub exec_config: Option<Vec<String>>,
}

#[allow(dead_code)]
impl K8sExecProxy {
    /// Create a new K8s exec proxy using DSB's configuration system.
    ///
    /// # Arguments
    ///
    /// * `pod_name` - Kubernetes pod name
    /// * `namespace` - Kubernetes namespace for sandbox pods
    ///
    /// # Returns
    ///
    /// A new `K8sExecProxy` instance
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Kubernetes client creation fails
    pub async fn new(pod_name: String, namespace: String) -> Result<Self> {
        let client = Client::try_default().await.context(
            "Failed to create Kubernetes client. Ensure running in-cluster or kubeconfig is valid",
        )?;

        Ok(Self {
            client,
            namespace,
            pod_name,
            pty_size: PtySize::default_size(),
            exec_output: None,
            exec_input: None,
            terminal_size_tx: None,
            exec_config: None,
        })
    }

    /// Create a new K8s exec proxy with an explicit Kubernetes client.
    ///
    /// # Arguments
    ///
    /// * `pod_name` - Kubernetes pod name
    /// * `namespace` - Kubernetes namespace for sandbox pods
    /// * `client` - Pre-configured Kubernetes client
    ///
    /// # Returns
    ///
    /// A new `K8sExecProxy` instance
    pub fn with_client(pod_name: String, namespace: String, client: Client) -> Self {
        Self {
            client,
            namespace,
            pod_name,
            pty_size: PtySize::default_size(),
            exec_output: None,
            exec_input: None,
            terminal_size_tx: None,
            exec_config: None,
        }
    }

    /// Get the pod name.
    #[allow(dead_code)]
    pub fn get_pod_name(&self) -> &str {
        &self.pod_name
    }

    /// Create an exec instance with PTY.
    ///
    /// For Kubernetes, this verifies the pod exists and is running.
    /// The actual exec is started in `start_exec()`.
    ///
    /// # Returns
    ///
    /// A placeholder exec ID (pod name)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Pod not found
    /// - Pod not running
    /// - Kubernetes API communication fails
    #[instrument(skip(self), fields(pod_name = %self.pod_name))]
    pub async fn create_exec(&mut self) -> Result<String> {
        debug!("Verifying pod exists and is running");

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        let pod = pods.get(&self.pod_name).await.context(format!(
            "Failed to get pod {} in namespace {}",
            self.pod_name, self.namespace
        ))?;

        let running = pod
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|phase| phase == "Running")
            .unwrap_or(false);

        if !running {
            anyhow::bail!("Pod {} is not running", self.pod_name);
        }

        debug!("Pod {} verified as running", self.pod_name);

        // Return pod name as exec ID (Kubernetes doesn't have separate exec IDs)
        Ok(self.pod_name.clone())
    }

    /// Resize the PTY.
    ///
    /// This updates the terminal size for the running exec process.
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
    /// - Terminal resize channel not available
    #[instrument(skip(self), fields(rows, cols))]
    #[allow(dead_code)]
    pub async fn resize_pty(&mut self, rows: u16, cols: u16) -> Result<()> {
        let tx = self
            .terminal_size_tx
            .as_mut()
            .context("Cannot resize PTY: no terminal size channel")?;

        debug!("Resizing PTY to {}x{}", rows, cols);

        tx.send(kube::api::TerminalSize {
            width: cols,
            height: rows,
        })
        .await
        .context("Failed to send terminal resize")?;

        self.pty_size = PtySize::new(rows, cols);

        debug!("PTY resize sent successfully");
        Ok(())
    }

    /// Start the exec instance and store the I/O streams.
    ///
    /// This starts the exec instance with TTY and stores the output stream and input writer
    /// for bidirectional I/O.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - No exec instance created
    /// - Kubernetes API communication fails
    /// - Streams not available
    #[instrument(skip(self))]
    pub async fn start_exec(&mut self) -> Result<()> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        // Use custom exec_config if set, otherwise default to shell
        let shell_cmd = self
            .exec_config
            .clone()
            .map(|cmd| cmd.join(" "))
            .unwrap_or_else(|| "/bin/bash".to_string());

        debug!(
            "Starting exec in pod {} with shell: {}",
            self.pod_name, shell_cmd
        );

        // Use AttachParams with TTY enabled for interactive terminal
        // Note: stderr must be false when tty is true
        let ap = AttachParams::interactive_tty().container("sandbox".to_string());

        let mut attached = pods
            .exec(&self.pod_name, vec![shell_cmd], &ap)
            .await
            .context(format!(
                "Failed to exec into pod {} in namespace {}",
                self.pod_name, self.namespace
            ))?;

        let stdin_writer = attached
            .stdin()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin stream from pod exec"))?;
        let stdout_reader = attached
            .stdout()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout stream from pod exec"))?;
        let terminal_size_tx = attached
            .terminal_size()
            .ok_or_else(|| anyhow::anyhow!("Failed to get terminal resize channel"))?;

        // We need to keep the AttachedProcess alive for the streams to work.
        // Spawn a task that holds it and waits for completion.
        tokio::spawn(async move {
            let _ = attached.join().await;
        });

        // Wrap stdout reader in a stream that yields Vec<u8>
        let stdout_stream = async_stream::stream! {
            use tokio::io::AsyncReadExt;
            let mut reader = stdout_reader;
            let mut buf = vec![0u8; 8192];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => yield Ok(buf[..n].to_vec()),
                    Err(e) => {
                        yield Err(e);
                        break;
                    }
                }
            }
        };

        self.exec_input = Some(Box::pin(stdin_writer));
        self.exec_output = Some(Box::pin(stdout_stream));
        self.terminal_size_tx = Some(terminal_size_tx);

        debug!("Exec in pod {} started successfully", self.pod_name);
        Ok(())
    }

    /// Take the input stream, consuming the proxy.
    #[allow(dead_code)]
    pub fn take_input_stream(&mut self) -> Option<Pin<Box<dyn tokio::io::AsyncWrite + Send>>> {
        self.exec_input.take()
    }

    /// Take the output stream, consuming the proxy.
    #[allow(dead_code)]
    pub fn take_output_stream(&mut self) -> Option<ByteStream> {
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
            "Writing {} bytes to exec stdin for pod {} (first 32 bytes: {:?})",
            data.len(),
            self.pod_name,
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
    pub async fn read_output(&mut self) -> Option<Result<Vec<u8>, std::io::Error>> {
        let output = self.exec_output.as_mut()?;
        futures_util::pin_mut!(output);

        match output.next().await {
            Some(Ok(data)) => {
                debug!("Received {} bytes from stdout", data.len());
                Some(Ok(data))
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
}

impl Drop for K8sExecProxy {
    fn drop(&mut self) {
        debug!("K8sExecProxy dropped for pod {}", self.pod_name);
    }
}
