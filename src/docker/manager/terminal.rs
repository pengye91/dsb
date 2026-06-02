// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! Docker terminal stream implementation for interactive exec sessions.

use crate::core::manager::ManagerError;
use bollard::Docker;
use std::sync::Arc;

/// Docker-specific implementation of the `TerminalStream` trait.
///
/// Wraps bollard's exec API to provide an interactive terminal session
/// with a container. Supports reading output frames, writing input,
/// and resizing the PTY.
pub struct DockerTerminalStream {
    pub(crate) docker: Arc<Docker>,
    pub(crate) exec_id: String,
    pub(crate) output: std::pin::Pin<
        Box<
            dyn futures_util::stream::Stream<
                    Item = std::result::Result<
                        bollard::container::LogOutput,
                        bollard::errors::Error,
                    >,
                > + Send,
        >,
    >,
    pub(crate) input: std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send>>,
}

#[async_trait::async_trait]
impl crate::core::manager::TerminalStream for DockerTerminalStream {
    async fn read_frame(
        &mut self,
    ) -> Result<Option<crate::core::manager::TerminalFrame>, ManagerError> {
        use futures_util::StreamExt;
        loop {
            match self.output.next().await {
                Some(Ok(log)) => {
                    use bollard::container::LogOutput;
                    let data = match log {
                        LogOutput::StdOut { message } | LogOutput::StdErr { message } => {
                            message.to_vec()
                        }
                        LogOutput::Console { message } => message.to_vec(),
                        LogOutput::StdIn { .. } => continue,
                    };
                    if data.is_empty() {
                        // Skip empty frames and try again
                        continue;
                    }
                    return Ok(Some(crate::core::manager::TerminalFrame::Data(data)));
                }
                Some(Err(e)) => {
                    return Err(ManagerError::OperationFailed(format!(
                        "Terminal read error: {}",
                        e
                    )))
                }
                None => return Ok(Some(crate::core::manager::TerminalFrame::Closed)),
            }
        }
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), ManagerError> {
        use tokio::io::AsyncWriteExt;
        self.input
            .write_all(data)
            .await
            .map_err(|e| ManagerError::OperationFailed(format!("Terminal write error: {}", e)))?;
        self.input
            .flush()
            .await
            .map_err(|e| ManagerError::OperationFailed(format!("Terminal flush error: {}", e)))?;
        Ok(())
    }

    async fn resize(&mut self, rows: u16, cols: u16) -> Result<(), ManagerError> {
        use bollard::query_parameters::ResizeExecOptions;

        let options = ResizeExecOptions {
            h: rows.into(),
            w: cols.into(),
        };

        self.docker
            .resize_exec(&self.exec_id, options)
            .await
            .map_err(|e| ManagerError::OperationFailed(format!("Terminal resize error: {}", e)))?;
        Ok(())
    }
}
