// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie

use async_trait::async_trait;
use kube::api::{Api, AttachParams};
use kube::Client;
use k8s_openapi::api::core::v1::Pod;
use tracing::debug;

use crate::core::manager::{ExecCommandResult, ManagerError, ManagerResult, TerminalFrame, TerminalStream};

// ---------------------------------------------------------------------------
// RemoteExec helper for non-interactive command execution
// ---------------------------------------------------------------------------

/// Helper for executing commands in K8s pods without TTY.
///
/// Wraps the kube-rs exec API to provide a simple interface for running
/// commands and capturing their output and exit codes.
pub(super) struct RemoteExec {
    client: Client,
    namespace: String,
}
impl RemoteExec {
    /// Creates a new RemoteExec helper.
    pub(super) fn new(client: Client, namespace: String) -> Self {
        Self { client, namespace }
    }

    /// Executes a command in a pod and returns the output and exit code.
    ///
    /// Uses kube-rs exec API without TTY. Stdout and stderr are combined into
    /// a single output string. If stdin is provided, it is written to the
    /// process before reading output.
    ///
    /// # Arguments
    ///
    /// * `pod_name` - The name of the target pod.
    /// * `cmd` - The command to execute as a list of strings.
    /// * `stdin` - Optional input to write to the process stdin.
    /// * `timeout_secs` - Optional timeout in seconds for the command.
    pub(super) async fn exec_in_pod(
        &self,
        pod_name: &str,
        cmd: Vec<String>,
        stdin: Option<Vec<u8>>,
        timeout_secs: Option<u64>,
    ) -> ManagerResult<ExecCommandResult> {
        use tokio::io::AsyncReadExt;
        use tokio::io::AsyncWriteExt;

        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);

        // Verify pod exists
        let pod = pods.get(pod_name).await.map_err(|e| {
            if let kube::Error::Api(api_err) = &e {
                if api_err.code == 404 {
                    return ManagerError::NotFound(format!("Pod: {}", pod_name));
                }
            }
            ManagerError::Api(format!("Failed to get pod: {}", e))
        })?;

        let running = pod
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|phase| phase == "Running")
            .unwrap_or(false);

        if !running {
            return Err(ManagerError::OperationFailed(format!(
                "Pod {} is not running",
                pod_name
            )));
        }

        // Build AttachParams: no TTY, stdout + stderr, conditionally stdin
        let has_stdin = stdin.is_some();
        let ap = AttachParams::default()
            .stdin(has_stdin)
            .stdout(true)
            .stderr(true)
            .tty(false)
            .container("sandbox".to_string());

        let mut attached = pods
            .exec(pod_name, cmd, &ap)
            .await
            .map_err(|e| ManagerError::OperationFailed(format!("Exec failed: {}", e)))?;

        // Write stdin if provided
        if let Some(stdin_data) = stdin {
            if let Some(mut stdin_writer) = attached.stdin() {
                debug!(
                    pod_name = %pod_name,
                    stdin_len = stdin_data.len(),
                    "Writing stdin to pod exec"
                );
                stdin_writer.write_all(&stdin_data).await.map_err(|e| {
                    ManagerError::OperationFailed(format!("Failed to write stdin: {}", e))
                })?;
                stdin_writer.flush().await.map_err(|e| {
                    ManagerError::OperationFailed(format!("Failed to flush stdin: {}", e))
                })?;
                // Yield to give the tokio runtime time to transmit the flushed
                // WebSocket frames before we send the close frame. Without this,
                // the close frame can race ahead of data still in flight,
                // causing the remote tar process to see truncated stdin.
                tokio::task::yield_now().await;
                stdin_writer.shutdown().await.map_err(|e| {
                    ManagerError::OperationFailed(format!("Failed to shutdown stdin: {}", e))
                })?;
            }
        }

        // Read stdout and stderr concurrently
        let stdout_reader = attached.stdout();
        let stderr_reader = attached.stderr();

        let (stdout_output, stderr_output) = {
            let stdout_fut = async {
                if let Some(mut reader) = stdout_reader {
                    let mut buf = Vec::new();
                    let _ = reader.read_to_end(&mut buf).await;
                    String::from_utf8_lossy(&buf).to_string()
                } else {
                    String::new()
                }
            };
            let stderr_fut = async {
                if let Some(mut reader) = stderr_reader {
                    let mut buf = Vec::new();
                    let _ = reader.read_to_end(&mut buf).await;
                    String::from_utf8_lossy(&buf).to_string()
                } else {
                    String::new()
                }
            };
            tokio::join!(stdout_fut, stderr_fut)
        };

        let combined_output = format!("{}{}", stdout_output, stderr_output);

        // Wait for the process to complete, with optional timeout
        let join_result = if let Some(secs) = timeout_secs {
            match tokio::time::timeout(std::time::Duration::from_secs(secs), attached.join()).await
            {
                Ok(result) => result,
                Err(_) => {
                    return Err(ManagerError::Timeout(format!(
                        "Command timed out after {} seconds",
                        secs
                    )));
                }
            }
        } else {
            attached.join().await
        };

        debug!(
            pod_name = %pod_name,
            stdout_len = stdout_output.len(),
            stderr_len = stderr_output.len(),
            join_ok = join_result.is_ok(),
            "Exec completed"
        );

        // Determine exit code from the status channel
        let exit_code = match join_result {
            Ok(()) => {
                // Process completed successfully (exit code 0)
                0
            }
            Err(_) => {
                // Process completed with an error. The status object
                // from the K8s API contains the exit code, but since we
                // already consumed stdout/stderr, we report a non-zero code.
                1
            }
        };

        Ok(ExecCommandResult {
            output: combined_output,
            exit_code,
        })
    }
}

// ---------------------------------------------------------------------------
// K8sTerminalStream - interactive terminal session over K8s exec
// ---------------------------------------------------------------------------

/// Kubernetes-specific implementation of the `TerminalStream` trait.
///
/// Wraps kube-rs exec API with TTY to provide an interactive terminal
/// session with a pod. Supports reading output frames, writing input,
/// and resizing the PTY via the terminal size channel.
pub struct K8sTerminalStream {
    /// Writer for sending data to the process stdin.
pub(super) stdin: Box<dyn tokio::io::AsyncWrite + Send + Unpin>,
    /// Reader for receiving data from the process stdout.
pub(super) stdout: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    /// Channel for sending terminal resize events to the K8s exec process.
pub(super) terminal_size_tx: Option<futures::channel::mpsc::Sender<kube::api::TerminalSize>>,
}

#[async_trait]
impl TerminalStream for K8sTerminalStream {
    /// Reads the next frame from the terminal.
    ///
    /// Returns `TerminalFrame::Data` with bytes read from stdout,
    /// or `TerminalFrame::Closed` when the stream ends.
    async fn read_frame(&mut self) -> Result<Option<TerminalFrame>, ManagerError> {
        use tokio::io::AsyncReadExt;

        let mut buf = vec![0u8; 8192];
        match self.stdout.read(&mut buf).await {
            Ok(0) => Ok(Some(TerminalFrame::Closed)),
            Ok(n) => {
                buf.truncate(n);
                Ok(Some(TerminalFrame::Data(buf)))
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    Ok(Some(TerminalFrame::Closed))
                } else {
                    Err(ManagerError::OperationFailed(format!(
                        "Terminal read error: {}",
                        e
                    )))
                }
            }
        }
    }

    /// Writes data to the terminal's stdin.
    async fn write(&mut self, data: &[u8]) -> Result<(), ManagerError> {
        use tokio::io::AsyncWriteExt;

        self.stdin
            .write_all(data)
            .await
            .map_err(|e| ManagerError::OperationFailed(format!("Terminal write error: {}", e)))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| ManagerError::OperationFailed(format!("Terminal flush error: {}", e)))?;
        Ok(())
    }

    /// Resizes the terminal by sending a resize event through the terminal
    /// size channel.
    async fn resize(&mut self, rows: u16, cols: u16) -> Result<(), ManagerError> {
        if let Some(tx) = self.terminal_size_tx.as_mut() {
            use futures::SinkExt;

            tx.send(kube::api::TerminalSize {
                width: cols,
                height: rows,
            })
            .await
            .map_err(|e| ManagerError::OperationFailed(format!("Terminal resize error: {}", e)))?;
        }
        Ok(())
    }
}
