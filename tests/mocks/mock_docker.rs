// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Mock Docker Implementation
//!
//! This module provides a mock implementation of the DockerTrait for testing
//! without requiring a running Docker daemon.

use mockall::predicate::*;
use std::sync::Arc;
use tokio::sync::RwLock;

use dsb::core::types::{ContainerStats, SandboxConfig, VolumeMount};
use dsb::docker::{DockerError, DockerResult, DockerTrait};

/// Mock Docker container state for testing.
#[derive(Debug, Clone)]
pub struct MockContainer {
    pub id: String,
    pub image: String,
    pub state: MockContainerState,
    #[allow(dead_code)]
    pub config: SandboxConfig,
}

/// Mock container state.
#[derive(Debug, Clone, PartialEq)]
pub enum MockContainerState {
    Created,
    Running,
    Stopped,
    #[allow(dead_code)]
    Paused,
    Removed,
}

/// Mock Docker implementation for testing.
///
/// This mock keeps track of containers, images, and volumes in memory,
/// allowing comprehensive testing without Docker.
///
/// # Example
///
/// ```rust,ignore
/// use dsb::tests::mocks::MockDocker;
/// use dsb::docker::DockerTrait;
///
/// #[tokio::test]
/// async fn test_sandbox_creation() {
///     let mock = MockDocker::new();
///     let config = SandboxConfig::default();
///
///     // Expect create_container to be called
///     mock.expect_create_container()
///         .returning(|_| Ok("container-123".to_string()));
///
///     let id = mock.create_container(&config).await.unwrap();
///     assert_eq!(id, "container-123");
/// }
/// ```
#[derive(Clone)]
pub struct MockDocker {
    inner: Arc<RwLock<MockDockerInner>>,
}

#[derive(Debug)]
struct MockDockerInner {
    containers: Vec<MockContainer>,
    images: Vec<String>,
    volumes: Vec<String>,
    next_container_id: u64,
}

impl Default for MockDocker {
    fn default() -> Self {
        Self::new()
    }
}

impl MockDocker {
    /// Creates a new MockDocker instance.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MockDockerInner {
                containers: Vec::new(),
                images: Vec::new(),
                volumes: Vec::new(),
                next_container_id: 1,
            })),
        }
    }

    /// Adds an image to the mock registry (simulates a pulled image).
    pub async fn add_image(&self, image: &str) {
        let mut inner = self.inner.write().await;
        if !inner.images.contains(&image.to_string()) {
            inner.images.push(image.to_string());
        }
    }

    /// Gets all containers in the mock.
    pub async fn get_containers(&self) -> Vec<MockContainer> {
        let inner = self.inner.read().await;
        inner.containers.clone()
    }

    /// Gets a container by ID.
    pub async fn get_container(&self, id: &str) -> Option<MockContainer> {
        let inner = self.inner.read().await;
        inner.containers.iter().find(|c| c.id == id).cloned()
    }

    /// Generates a new mock container ID.
    /// Note: This must be called while holding the write lock
    fn generate_container_id(inner: &mut MockDockerInner) -> String {
        let id = format!("container-{}", inner.next_container_id);
        inner.next_container_id += 1;
        id
    }
}

#[async_trait::async_trait]
impl DockerTrait for MockDocker {
    async fn create_container(
        &self,
        config: &SandboxConfig,
        _sandbox_id: Option<&uuid::Uuid>,
    ) -> DockerResult<String> {
        let mut inner = self.inner.write().await;
        let id = Self::generate_container_id(&mut inner);

        inner.containers.push(MockContainer {
            id: id.clone(),
            image: config.image.clone(),
            state: MockContainerState::Created,
            config: config.clone(),
        });

        Ok(id)
    }

    async fn start_container(&self, container_id: &str) -> DockerResult<()> {
        let mut inner = self.inner.write().await;

        let container = inner
            .containers
            .iter_mut()
            .find(|c| c.id == container_id)
            .ok_or_else(|| DockerError::ContainerNotFound(container_id.to_string()))?;

        if container.state == MockContainerState::Removed {
            return Err(DockerError::ContainerNotFound(container_id.to_string()));
        }

        container.state = MockContainerState::Running;
        Ok(())
    }

    async fn stop_container(&self, container_id: &str) -> DockerResult<()> {
        let mut inner = self.inner.write().await;

        let container = inner
            .containers
            .iter_mut()
            .find(|c| c.id == container_id)
            .ok_or_else(|| DockerError::ContainerNotFound(container_id.to_string()))?;

        if container.state == MockContainerState::Removed {
            return Err(DockerError::ContainerNotFound(container_id.to_string()));
        }

        container.state = MockContainerState::Stopped;
        Ok(())
    }

    async fn remove_container(&self, container_id: &str) -> DockerResult<()> {
        let mut inner = self.inner.write().await;

        let container = inner
            .containers
            .iter_mut()
            .find(|c| c.id == container_id)
            .ok_or_else(|| DockerError::ContainerNotFound(container_id.to_string()))?;

        container.state = MockContainerState::Removed;
        Ok(())
    }

    async fn pull_image(&self, image: &str) -> DockerResult<()> {
        let mut inner = self.inner.write().await;
        if !inner.images.contains(&image.to_string()) {
            inner.images.push(image.to_string());
        }
        Ok(())
    }

    async fn image_exists(&self, image: &str) -> DockerResult<bool> {
        let inner = self.inner.read().await;
        Ok(inner.images.contains(&image.to_string()))
    }

    async fn exec_container(
        &self,
        container_id: &str,
        command: Vec<String>,
    ) -> DockerResult<String> {
        let inner = self.inner.read().await;

        let container = inner
            .containers
            .iter()
            .find(|c| c.id == container_id)
            .ok_or_else(|| DockerError::ContainerNotFound(container_id.to_string()))?;

        if container.state != MockContainerState::Running {
            return Err(DockerError::ExecFailed(
                "Container is not running".to_string(),
            ));
        }

        // Return mock output based on command
        let output = match command.first().map(|s| s.as_str()) {
            Some("ls") => "file1.txt\nfile2.txt\nfile3.txt".to_string(),
            Some("echo") => command[1..].join(" "),
            Some("pwd") => "/workspace".to_string(),
            Some("cat") => "mock file content".to_string(),
            Some("whoami") => "mockuser".to_string(),
            _ => format!("executed: {}", command.join(" ")),
        };

        Ok(output)
    }

    async fn get_container_stats(&self, container_id: &str) -> DockerResult<ContainerStats> {
        let inner = self.inner.read().await;

        inner
            .containers
            .iter()
            .find(|c| c.id == container_id)
            .ok_or_else(|| DockerError::ContainerNotFound(container_id.to_string()))?;

        // Return mock stats
        Ok(ContainerStats {
            cpu_percent: 50.0,
            memory_usage_mb: 512,
            memory_limit_mb: 1024,
            memory_percent: 50.0,
            network_rx_bytes: 1024,
            network_tx_bytes: 2048,
            block_read_bytes: 0,
            block_write_bytes: 0,
            timestamp: chrono::Utc::now(),
        })
    }

    async fn remove_volume(&self, volume_name: &str) -> DockerResult<()> {
        let mut inner = self.inner.write().await;
        inner.volumes.retain(|v| v != volume_name);
        Ok(())
    }

    async fn is_container_running(&self, container_id: &str) -> DockerResult<bool> {
        let inner = self.inner.read().await;

        let container = inner
            .containers
            .iter()
            .find(|c| c.id == container_id)
            .ok_or_else(|| DockerError::ContainerNotFound(container_id.to_string()))?;

        Ok(container.state == MockContainerState::Running)
    }

    async fn create_volume(
        &self,
        volume_mount: &VolumeMount,
        _sandbox_id: &str,
    ) -> DockerResult<String> {
        match volume_mount {
            VolumeMount::Named { name, .. } => {
                let mut inner = self.inner.write().await;
                if !inner.volumes.contains(name) {
                    inner.volumes.push(name.clone());
                }
                Ok(name.clone())
            }
            VolumeMount::Bind { .. } => Ok(String::new()),
        }
    }

    async fn remove_volumes(
        &self,
        volume_mounts: &[VolumeMount],
        _sandbox_id: &str,
    ) -> DockerResult<()> {
        for volume_mount in volume_mounts {
            if let VolumeMount::Named { name, .. } = volume_mount {
                self.remove_volume(name).await?;
            }
        }
        Ok(())
    }

    async fn pull_image_with_progress<F>(
        &self,
        image: &str,
        mut _progress_callback: F,
    ) -> DockerResult<()>
    where
        F: FnMut(String, Option<u64>, Option<u64>) + Send,
    {
        // Mock implementation - just mark the image as pulled
        let mut inner = self.inner.write().await;
        if !inner.images.contains(&image.to_string()) {
            inner.images.push(image.to_string());
        }
        Ok(())
    }

    async fn list_images(&self) -> DockerResult<Vec<dsb::api::handlers::ImageSummary>> {
        let inner = self.inner.read().await;
        Ok(inner
            .images
            .iter()
            .map(|img| dsb::api::handlers::ImageSummary {
                id: uuid::Uuid::new_v4().to_string(),
                repo_tags: vec![img.clone()],
                size: 1024 * 1024 * 100,
                created: chrono::Utc::now().timestamp(),
                labels: None,
            })
            .collect())
    }

    async fn inspect_image(
        &self,
        id: &str,
    ) -> DockerResult<dsb::api::handlers::images::ImageDetails> {
        let inner = self.inner.read().await;
        inner
            .images
            .iter()
            .find(|img| *img == id || id.contains(img.as_str()))
            .ok_or_else(|| DockerError::ImageNotFound(id.to_string()))?;

        // Return mock image details
        Ok(dsb::api::handlers::images::ImageDetails {
            id: uuid::Uuid::new_v4().to_string(),
            repo_tags: vec![id.to_string()],
            size: 1024 * 1024 * 100,
            virtual_size: 1024 * 1024 * 100,
            created: chrono::Utc::now().timestamp(),
            architecture: "amd64".to_string(),
            os: "linux".to_string(),
            labels: None,
            env: Some(vec![
                "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
            ]),
            features: vec![],
        })
    }

    async fn remove_image(&self, id: &str) -> DockerResult<()> {
        let mut inner = self.inner.write().await;
        let original_len = inner.images.len();
        inner.images.retain(|img| img != id && !id.contains(img));

        if inner.images.len() == original_len {
            return Err(DockerError::ImageNotFound(id.to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_docker_create_container() {
        let mock = MockDocker::new();

        let config = SandboxConfig {
            image: "nginx:latest".to_string(),
            name: Some("test-nginx".to_string()),
            ..Default::default()
        };

        let id = mock.create_container(&config, None).await.unwrap();
        assert!(id.starts_with("container-"));

        let containers = mock.get_containers().await;
        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].image, "nginx:latest");
    }

    #[tokio::test]
    async fn test_mock_docker_start_stop() {
        let mock = MockDocker::new();

        let config = SandboxConfig {
            image: "alpine:latest".to_string(),
            ..Default::default()
        };

        let id = mock.create_container(&config, None).await.unwrap();

        // Container should not be running initially
        assert!(!mock.is_container_running(&id).await.unwrap());

        // Start the container
        mock.start_container(&id).await.unwrap();
        assert!(mock.is_container_running(&id).await.unwrap());

        // Stop the container
        mock.stop_container(&id).await.unwrap();
        assert!(!mock.is_container_running(&id).await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_docker_exec() {
        let mock = MockDocker::new();

        let config = SandboxConfig {
            image: "alpine:latest".to_string(),
            ..Default::default()
        };

        let id = mock.create_container(&config, None).await.unwrap();
        mock.start_container(&id).await.unwrap();

        // Test various commands
        let output = mock
            .exec_container(&id, vec!["ls".to_string()])
            .await
            .unwrap();
        assert!(output.contains("file1.txt"));

        let output = mock
            .exec_container(&id, vec!["echo".to_string(), "hello".to_string()])
            .await
            .unwrap();
        assert_eq!(output, "hello");

        let output = mock
            .exec_container(&id, vec!["pwd".to_string()])
            .await
            .unwrap();
        assert_eq!(output, "/workspace");
    }

    #[tokio::test]
    async fn test_mock_docker_image_operations() {
        let mock = MockDocker::new();

        // Image doesn't exist initially
        assert!(!mock.image_exists("nginx:latest").await.unwrap());

        // Pull image
        mock.pull_image("nginx:latest").await.unwrap();

        // Now it exists
        assert!(mock.image_exists("nginx:latest").await.unwrap());

        // Add another image
        mock.add_image("alpine:latest").await;
        assert!(mock.image_exists("alpine:latest").await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_docker_remove_container() {
        let mock = MockDocker::new();

        let config = SandboxConfig {
            image: "nginx:latest".to_string(),
            ..Default::default()
        };

        let id = mock.create_container(&config, None).await.unwrap();
        mock.start_container(&id).await.unwrap();

        // Remove the container
        mock.remove_container(&id).await.unwrap();

        // Container should still exist but in removed state
        let container = mock.get_container(&id).await;
        assert!(container.is_some());
        assert_eq!(container.unwrap().state, MockContainerState::Removed);
    }

    #[tokio::test]
    async fn test_mock_docker_volume_operations() {
        let mock = MockDocker::new();

        let volume_mount = VolumeMount::Named {
            name: "test-volume".to_string(),
            container_path: "/data".to_string(),
            read_only: false,
        };

        let volume_name = mock
            .create_volume(&volume_mount, "sandbox-123")
            .await
            .unwrap();
        assert_eq!(volume_name, "test-volume");

        // Remove the volume
        mock.remove_volume(&volume_name).await.unwrap();
    }

    #[tokio::test]
    async fn test_mock_docker_exec_not_running() {
        let mock = MockDocker::new();

        let config = SandboxConfig {
            image: "alpine:latest".to_string(),
            ..Default::default()
        };

        let id = mock.create_container(&config, None).await.unwrap();

        // Don't start the container, try to exec
        let result = mock.exec_container(&id, vec!["ls".to_string()]).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DockerError::ExecFailed(_)));
    }

    #[tokio::test]
    async fn test_mock_docker_container_not_found() {
        let mock = MockDocker::new();

        let result = mock.start_container("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DockerError::ContainerNotFound(_)
        ));
    }
}
