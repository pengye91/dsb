// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! SandboxManager implementation for DockerManager.

use super::{DockerManagerError, DockerTerminalStream};
use crate::core::manager::{ManagerError, ManagerResult, SandboxManager};
use crate::core::types::SandboxInfo;
use crate::docker::docker_trait::DockerTrait;
use bollard::exec::{CreateExecOptions, StartExecOptions};

impl From<crate::docker::docker_trait::DockerError> for ManagerError {
    fn from(err: crate::docker::docker_trait::DockerError) -> Self {
        match err {
            crate::docker::docker_trait::DockerError::Api(s) => ManagerError::Api(s),
            crate::docker::docker_trait::DockerError::ContainerNotFound(s) => {
                ManagerError::NotFound(format!("Container: {}", s))
            }
            crate::docker::docker_trait::DockerError::ImageNotFound(s) => {
                ManagerError::NotFound(format!("Image: {}", s))
            }
            crate::docker::docker_trait::DockerError::ExecFailed(s) => {
                ManagerError::OperationFailed(format!("Exec: {}", s))
            }
            crate::docker::docker_trait::DockerError::Volume(s) => {
                ManagerError::OperationFailed(format!("Volume: {}", s))
            }
            crate::docker::docker_trait::DockerError::Io(e) => ManagerError::Io(e),
            crate::docker::docker_trait::DockerError::ToolProxy { message, .. } => {
                ManagerError::OperationFailed(format!("Tool proxy: {}", message))
            }
        }
    }
}

impl From<DockerManagerError> for ManagerError {
    fn from(err: DockerManagerError) -> Self {
        match err {
            DockerManagerError::Api(s) => ManagerError::Api(s),
            DockerManagerError::ContainerNotFound(s) => {
                ManagerError::NotFound(format!("Container: {}", s))
            }
            DockerManagerError::ImageNotFound(s) => ManagerError::NotFound(format!("Image: {}", s)),
            DockerManagerError::ExecFailed(s) => {
                ManagerError::OperationFailed(format!("Exec: {}", s))
            }
            DockerManagerError::Volume(s) => {
                ManagerError::OperationFailed(format!("Volume: {}", s))
            }
            DockerManagerError::Io(e) => ManagerError::Io(e),
            DockerManagerError::Bollard(e) => ManagerError::Api(e.to_string()),
            DockerManagerError::ToolProxy { message, operation } => {
                ManagerError::OperationFailed(format!("Tool proxy {}: {}", operation, message))
            }
            DockerManagerError::InvalidConfig(s) => ManagerError::Api(s),
            DockerManagerError::Http(s) => ManagerError::Api(s),
            DockerManagerError::Timeout(s) => ManagerError::OperationFailed(s),
        }
    }
}

#[async_trait::async_trait]
impl SandboxManager for crate::docker::DockerManager {
    async fn create(
        &self,
        sandbox_id: Option<&uuid::Uuid>,
        config: &crate::core::types::SandboxConfig,
    ) -> ManagerResult<String> {
        Ok(DockerTrait::create_container(self, config, sandbox_id).await?)
    }

    async fn start(&self, id: &str) -> ManagerResult<()> {
        Ok(DockerTrait::start_container(self, id).await?)
    }

    async fn stop(&self, id: &str) -> ManagerResult<()> {
        Ok(DockerTrait::stop_container(self, id).await?)
    }

    async fn delete(&self, id: &str) -> ManagerResult<()> {
        Ok(DockerTrait::remove_container(self, id).await?)
    }

    async fn exec(&self, id: &str, cmd: Vec<String>) -> ManagerResult<String> {
        Ok(self.exec_container(id, cmd, None).await?)
    }

    async fn stats(&self, id: &str) -> ManagerResult<crate::core::types::ContainerStats> {
        Ok(self.get_container_stats(id).await?)
    }

    async fn is_running(&self, id: &str) -> ManagerResult<bool> {
        Ok(self.is_container_running(id).await?)
    }

    async fn get_exit_info(&self, id: &str) -> ManagerResult<(i64, bool)> {
        Ok(self.get_container_exit_info(id).await?)
    }

    async fn get_workdir(&self, id: &str) -> ManagerResult<String> {
        Ok(self.get_container_workdir(id).await?)
    }

    async fn list(
        &self,
        all: bool,
        filters: Option<std::collections::HashMap<String, Vec<String>>>,
    ) -> ManagerResult<Vec<SandboxInfo>> {
        let containers = self.list_containers(all, filters).await?;
        Ok(containers.into_iter().map(SandboxInfo::from).collect())
    }

    async fn remove_volume(&self, name: &str) -> ManagerResult<()> {
        Ok(self.remove_volume(name).await?)
    }

    async fn get_image_features(
        &self,
        image: &str,
    ) -> ManagerResult<crate::core::types::ImageDetails> {
        Ok(self.inspect_image(image).await?)
    }

    async fn list_images(&self) -> ManagerResult<Vec<crate::core::types::ImageSummary>> {
        Ok(DockerTrait::list_images(self).await?)
    }

    async fn pull_image(&self, image: &str) -> ManagerResult<()> {
        Ok(DockerTrait::pull_image(self, image).await?)
    }

    async fn pull_image_with_progress(
        &self,
        image: &str,
        mut callback: Box<dyn FnMut(String, Option<u64>, Option<u64>) + Send + 'static>,
    ) -> ManagerResult<()> {
        Ok(
            DockerTrait::pull_image_with_progress(self, image, move |s, c, t| {
                callback(s, c, t);
            })
            .await?,
        )
    }

    async fn delete_image(&self, id: &str) -> ManagerResult<()> {
        Ok(DockerTrait::remove_image(self, id).await?)
    }

    async fn image_exists(&self, image: &str) -> ManagerResult<bool> {
        Ok(DockerTrait::image_exists(self, image).await?)
    }

    async fn exec_http(
        &self,
        id: &str,
        path: &str,
        method: &str,
        body: Option<serde_json::Value>,
        timeout_secs: Option<u64>,
    ) -> ManagerResult<serde_json::Value> {
        Ok(self
            .exec_container_http(id, path, method, body, timeout_secs)
            .await?)
    }

    async fn exec_with_stdin(
        &self,
        id: &str,
        cmd: Vec<String>,
        stdin: Option<String>,
        timeout_secs: Option<u64>,
    ) -> ManagerResult<String> {
        Ok(self
            .exec_container_with_stdin(id, cmd, stdin, timeout_secs)
            .await?)
    }

    async fn exec_with_stdin_result(
        &self,
        id: &str,
        cmd: Vec<String>,
        stdin: Option<String>,
        timeout_secs: Option<u64>,
    ) -> ManagerResult<crate::core::manager::ExecCommandResult> {
        Ok(self
            .exec_container_with_stdin_result(id, cmd, stdin, timeout_secs)
            .await?)
    }

    async fn upload_archive(&self, id: &str, path: &str, tar_data: Vec<u8>) -> ManagerResult<()> {
        #[allow(deprecated)]
        use bollard::container::UploadToContainerOptions;

        #[allow(deprecated)]
        let options = UploadToContainerOptions {
            path: path.to_string(),
            ..Default::default()
        };

        self.docker
            .upload_to_container(id, Some(options), bollard::body_full(tar_data.into()))
            .await
            .map_err(|e| ManagerError::OperationFailed(format!("Upload archive failed: {}", e)))
    }

    /// Returns the network address (host:port) for accessing a sandbox container.
    ///
    /// For Docker, this returns the short container ID (first 12 chars) concatenated
    /// with the port. Docker's embedded DNS resolves short container IDs on the
    /// same network, so `abc123def456:6080` is reachable from the DSB server container.
    async fn get_sandbox_address(&self, id: &str, port: u16) -> ManagerResult<String> {
        // Docker DNS recognizes short container IDs (first 12 characters)
        let short_id = if id.len() > 12 { &id[..12] } else { id };
        Ok(format!("{}:{}", short_id, port))
    }

    /// Opens an interactive terminal session with a Docker container.
    ///
    /// Creates an exec instance with TTY enabled and returns a `DockerTerminalStream`
    /// that implements the `TerminalStream` trait. Tries the specified shell first
    /// (defaults to "bash"), then falls back to "sh" if the primary shell is unavailable.
    async fn exec_terminal(
        &self,
        id: &str,
        shell: Option<String>,
    ) -> ManagerResult<Box<dyn crate::core::manager::TerminalStream + Send>> {
        let shell = shell.unwrap_or_else(|| "bash".to_string());
        let term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string());

        let options = CreateExecOptions {
            cmd: Some(vec![shell.clone()]),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(true),
            env: Some(vec![
                format!("TERM={}", term),
                "LANG=C.UTF-8".to_string(),
                "LC_ALL=C.UTF-8".to_string(),
            ]),
            ..Default::default()
        };

        let exec = self.docker.create_exec(id, options).await.map_err(|e| {
            let msg = e.to_string();
            if msg.contains("No such container") || msg.contains("not found") {
                ManagerError::NotFound(format!("Container: {}", id))
            } else {
                // If bash fails, try sh
                ManagerError::OperationFailed(format!(
                    "Failed to create exec with {}: {}",
                    shell, msg
                ))
            }
        })?;

        let start_options = Some(StartExecOptions {
            detach: false,
            ..Default::default()
        });

        let result = self
            .docker
            .start_exec(&exec.id, start_options)
            .await
            .map_err(|e| ManagerError::OperationFailed(format!("Failed to start exec: {}", e)))?;

        match result {
            bollard::exec::StartExecResults::Attached { output, input } => {
                tracing::debug!(exec_id = %exec.id, container_id = %id, "Terminal exec started");
                Ok(Box::new(DockerTerminalStream {
                    docker: self.docker.clone(),
                    exec_id: exec.id,
                    output,
                    input,
                }))
            }
            _ => Err(ManagerError::OperationFailed(
                "Unexpected result type from start_exec".to_string(),
            )),
        }
    }
}
