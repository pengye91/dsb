//! Mock-based Docker tests
//!
//! This file demonstrates using MockDocker to test Docker operations
//! without requiring a running Docker daemon.

use crate::core::types::{PortMapping, PortProtocol, SandboxConfig, VolumeMount};
use crate::docker::{DockerError, DockerTrait};
use crate::testing::mocks::MockDocker;

///////////////////////////////////////////////////////////////////////////////
// Helper Functions
///////////////////////////////////////////////////////////////////////////////

/// Helper function that creates a sandbox using DockerTrait
///
/// This demonstrates how higher-level code can be written to use
/// the trait abstraction instead of concrete DockerManager.
async fn create_sandbox<D: DockerTrait>(
    docker: &D,
    name: &str,
    image: &str,
) -> Result<String, crate::docker::DockerError> {
    let config = SandboxConfig {
        image: image.to_string(),
        name: Some(name.to_string()),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await?;
    docker.start_container(&container_id).await?;

    Ok(container_id)
}

/// Helper function that executes a command in a sandbox
async fn exec_in_sandbox<D: DockerTrait>(
    docker: &D,
    container_id: &str,
    command: Vec<String>,
) -> Result<String, crate::docker::DockerError> {
    let output = docker.exec_container(container_id, command).await?;
    Ok(output)
}

///////////////////////////////////////////////////////////////////////////////
// Mock Unit Tests
///////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn test_mock_docker_create_and_start() {
    let mock = MockDocker::new();

    // Add the image so it exists
    mock.add_image("nginx:latest").await;

    let container_id = create_sandbox(&mock, "test-nginx", "nginx:latest")
        .await
        .unwrap();

    // Verify container was created and is running
    let container = mock.get_container(&container_id).await.unwrap();
    assert_eq!(container.image, "nginx:latest");
    assert!(matches!(
        container.state,
        crate::testing::mocks::MockContainerState::Running
    ));
}

#[tokio::test]
async fn test_mock_docker_full_lifecycle() {
    let mock = MockDocker::new();
    mock.add_image("alpine:latest").await;

    // Create
    let container_id = create_sandbox(&mock, "test-alpine", "alpine:latest")
        .await
        .unwrap();

    // Verify it's running
    assert!(mock.is_container_running(&container_id).await.unwrap());

    // Stop
    mock.stop_container(&container_id).await.unwrap();
    let container = mock.get_container(&container_id).await.unwrap();
    assert!(matches!(
        container.state,
        crate::testing::mocks::MockContainerState::Stopped
    ));
    assert!(!mock.is_container_running(&container_id).await.unwrap());

    // Remove
    mock.remove_container(&container_id).await.unwrap();
    let container = mock.get_container(&container_id).await.unwrap();
    assert!(matches!(
        container.state,
        crate::testing::mocks::MockContainerState::Removed
    ));
}

#[tokio::test]
async fn test_mock_docker_remove_nonexistent_container() {
    let mock = MockDocker::new();

    // Try to remove a container that doesn't exist
    let nonexistent_id = "nonexistent-container-123";
    let result = mock.remove_container(nonexistent_id).await;

    // MockDocker returns ContainerNotFound for non-existent containers
    // But the actual DockerManager should handle this gracefully
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        DockerError::ContainerNotFound(_)
    ));
}

#[tokio::test]
async fn test_mock_docker_remove_already_removed_container() {
    let mock = MockDocker::new();
    mock.add_image("alpine:latest").await;

    // Create and remove a container
    let container_id = create_sandbox(&mock, "test-remove-again", "alpine:latest")
        .await
        .unwrap();
    mock.remove_container(&container_id).await.unwrap();

    // Try to remove it again - MockDocker allows removing an already-removed container
    // (it just sets the state to Removed again)
    let result = mock.remove_container(&container_id).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_mock_docker_exec_commands() {
    let mock = MockDocker::new();
    mock.add_image("alpine:latest").await;

    let container_id = create_sandbox(&mock, "test-exec", "alpine:latest")
        .await
        .unwrap();

    // Test various commands
    let output = exec_in_sandbox(&mock, &container_id, vec!["ls".to_string()])
        .await
        .unwrap();
    assert!(output.contains("file1.txt"));

    let output = exec_in_sandbox(
        &mock,
        &container_id,
        vec!["echo".to_string(), "hello world".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(output, "hello world");

    let output = exec_in_sandbox(&mock, &container_id, vec!["pwd".to_string()])
        .await
        .unwrap();
    assert_eq!(output, "/workspace");

    let output = exec_in_sandbox(&mock, &container_id, vec!["whoami".to_string()])
        .await
        .unwrap();
    assert_eq!(output, "mockuser");
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

    // Pull again should work
    mock.pull_image("nginx:latest").await.unwrap();
    assert!(mock.image_exists("nginx:latest").await.unwrap());
}

#[tokio::test]
async fn test_mock_docker_volume_operations() {
    let mock = MockDocker::new();

    // Create named volume
    let volume_mount = VolumeMount::Named {
        name: "test-data".to_string(),
        container_path: "/app/data".to_string(),
        read_only: false,
    };

    let volume_name = mock
        .create_volume(&volume_mount, "sandbox-123")
        .await
        .unwrap();

    assert_eq!(volume_name, "test-data");

    // Clean up
    mock.remove_volume(&volume_name).await.unwrap();
}

#[tokio::test]
async fn test_mock_docker_bind_mount() {
    let mock = MockDocker::new();
    mock.add_image("alpine:latest").await;

    let volume_mount = VolumeMount::Bind {
        host_path: "/tmp/data".to_string(),
        container_path: "/container/data".to_string(),
        read_only: true,
    };

    let volume_name = mock
        .create_volume(&volume_mount, "sandbox-456")
        .await
        .unwrap();

    // Bind mounts don't create volumes, so name should be empty
    assert!(volume_name.is_empty());
}

#[tokio::test]
async fn test_mock_docker_exec_fails_when_not_running() {
    let mock = MockDocker::new();
    mock.add_image("alpine:latest").await;

    let container_id = create_sandbox(&mock, "test-stopped", "alpine:latest")
        .await
        .unwrap();

    // Stop the container
    mock.stop_container(&container_id).await.unwrap();

    // Try to exec - should fail
    let result = exec_in_sandbox(&mock, &container_id, vec!["ls".to_string()]).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_mock_docker_container_not_found() {
    let mock = MockDocker::new();

    // Try operations on non-existent container
    let result = mock.start_container("nonexistent").await;
    assert!(result.is_err());

    let result = mock.stop_container("nonexistent").await;
    assert!(result.is_err());

    let result = mock.is_container_running("nonexistent").await;
    assert!(result.is_err());

    let result = mock
        .exec_container("nonexistent", vec!["ls".to_string()])
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mock_docker_multiple_containers() {
    let mock = MockDocker::new();
    mock.add_image("nginx:latest").await;
    mock.add_image("alpine:latest").await;

    // Create multiple containers
    let id1 = create_sandbox(&mock, "nginx-1", "nginx:latest")
        .await
        .unwrap();
    let id2 = create_sandbox(&mock, "alpine-1", "alpine:latest")
        .await
        .unwrap();
    let id3 = create_sandbox(&mock, "nginx-2", "nginx:latest")
        .await
        .unwrap();

    // Verify all are running
    let running1: Result<bool, _> = mock.is_container_running(&id1).await;
    assert!(running1.unwrap());
    let running2: Result<bool, _> = mock.is_container_running(&id2).await;
    assert!(running2.unwrap());
    let running3: Result<bool, _> = mock.is_container_running(&id3).await;
    assert!(running3.unwrap());

    // Verify we have 3 containers
    let containers = mock.get_containers().await;
    assert_eq!(containers.len(), 3);
}

#[tokio::test]
async fn test_mock_docker_complex_config() {
    use crate::core::types::{PullPolicy, ResourceLimits};
    use std::collections::HashMap;

    let mock = MockDocker::new();
    mock.add_image("redis:latest").await;

    let mut environment = HashMap::new();
    environment.insert("REDIS_MODE".to_string(), "master".to_string());

    let config = SandboxConfig {
        image: "redis:latest".to_string(),
        name: Some("my-redis".to_string()),
        environment,
        port_mappings: vec![PortMapping {
            host_port: 6379,
            container_port: 6379,
            protocol: PortProtocol::Tcp,
        }],
        resource_limits: ResourceLimits {
            memory_mb: Some(256),
            cpu_quota: Some(50000),
            cpu_period: Some(100000),
            cpu_shares: Some(512),
            pids_limit: Some(50),
            ulimits: Some(vec![]),
        },
        pull_policy: PullPolicy::Always,
        ..Default::default()
    };

    let container_id: Result<String, _> = mock.create_container(&config, None).await;
    let container_id = container_id.unwrap();

    // Verify the container was created with the correct config
    let container = mock.get_container(&container_id).await.unwrap();
    assert_eq!(container.image, "redis:latest");
    assert_eq!(container.config.name, Some("my-redis".to_string()));
    assert_eq!(container.config.environment.len(), 1);
    assert_eq!(container.config.port_mappings.len(), 1);
}
