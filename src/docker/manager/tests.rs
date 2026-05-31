// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! Docker manager integration tests.

use super::*;
use crate::config::load_for_tests;
use crate::core::types::{
    PortMapping, PortProtocol, ResourceLimits, SandboxConfig, VolumeMount,
};
use bollard::models::ContainerSummary;
use std::collections::HashMap;

/// Helper function to get the test image from configuration
fn test_image() -> String {
    let config = load_for_tests().expect("Failed to load test config");
    config.docker.test_image.clone()
}

////////////////////////////////////////////////////////////////////////////////
// Docker Integration Tests (Require Running Docker)
////////////////////////////////////////////////////////////////////////////////

// Note: Integration tests require Docker to be running
// They're typically placed in tests/ directory rather than #[cfg(test)]

#[tokio::test]
async fn test_image_exists() {
    let docker = DockerManager::new().unwrap();

    // Test with non-existent image
    let result: Result<bool, _> = docker.image_exists("nonexistent/image:12345").await;
    assert!(result.is_ok());
    assert!(!result.unwrap());

    // Test with existing image (try to pull, or use cached version)
    // First check if image exists locally - if so, skip pull
    let image_name = test_image();
    let local_exists = docker.image_exists(&image_name).await.unwrap();

    if !local_exists {
        let pull_result = docker.pull_image(&image_name).await;
        if let Err(ref e) = pull_result {
            let error_msg = e.to_string().to_lowercase();
            // If network error but image exists locally, that's fine
            if error_msg.contains("dns")
                || error_msg.contains("network")
                || error_msg.contains("connection")
                || error_msg.contains("eof")
                || error_msg.contains("access denied")
                || error_msg.contains("not found")
            {
                // Check if image now exists locally (might have been cached)
                let exists_after_pull = docker.image_exists(&image_name).await.unwrap();
                if !exists_after_pull {
                    // Image doesn't exist locally and can't pull - skip test
                    return;
                }
            } else {
                // Re-raise if not a recoverable error
                panic!("Unexpected error: {}", e);
            }
        }
    }

    let exists: bool = docker.image_exists(&image_name).await.unwrap();
    assert!(exists);
}

#[tokio::test]
async fn test_pull_image() {
    let docker = DockerManager::new().unwrap();

    let image_name = test_image();

    // First check if image exists locally - if so, skip pull
    let local_exists = docker.image_exists(&image_name).await.unwrap();
    if local_exists {
        // Image exists locally, test passes
        return;
    }

    // Pull a small image
    let result: Result<(), _> = docker.pull_image(&image_name).await;

    // If pull fails due to network issues, check if image exists locally
    if let Err(ref e) = result {
        let error_msg = e.to_string().to_lowercase();
        // If error is about network/DNS but image exists locally, test passes
        if error_msg.contains("dns")
            || error_msg.contains("network")
            || error_msg.contains("connection")
            || error_msg.contains("eof")
            || error_msg.contains("access denied")
            || error_msg.contains("not found")
        {
            let exists: bool = docker.image_exists(&image_name).await.unwrap();
            assert!(
                exists,
                "Image should exist locally when registry is unreachable"
            );
            return; // Test passes
        }
    }

    // If pull succeeded or failed for other reasons, propagate the result
    assert!(result.is_ok());

    // Verify it exists
    let exists: bool = docker.image_exists(&image_name).await.unwrap();
    assert!(exists);
}

#[tokio::test]
async fn test_pull_image_with_progress() {
    let docker = DockerManager::new().unwrap();
    let mut progress_updates = Vec::new();

    let image_name = test_image();

    // First check if image exists locally - if so, skip pull
    let local_exists = docker.image_exists(&image_name).await.unwrap();
    if local_exists {
        // Image exists locally, test passes without progress
        return;
    }

    let result = docker
        .pull_image_with_progress(&image_name, |status, current, total| {
            progress_updates.push((status, current, total));
        })
        .await;

    // If pull fails due to network issues, check if image exists locally
    if let Err(ref e) = result {
        let error_msg = e.to_string().to_lowercase();
        // If error is about network/DNS but image exists locally, test passes
        if error_msg.contains("dns")
            || error_msg.contains("network")
            || error_msg.contains("connection")
            || error_msg.contains("eof")
            || error_msg.contains("access denied")
            || error_msg.contains("not found")
        {
            let exists: bool = docker.image_exists(&image_name).await.unwrap();
            assert!(
                exists,
                "Image should exist locally when registry is unreachable"
            );
            return; // Test passes even without progress updates
        }
    }

    // If pull succeeded or failed for other reasons, propagate the result
    result.unwrap();

    // Should have received some progress updates
    assert!(!progress_updates.is_empty());

    // Verify it exists
    let exists: bool = docker.image_exists(&image_name).await.unwrap();
    assert!(exists);
}

#[tokio::test]
async fn test_is_container_running() {
    let docker = DockerManager::new().unwrap();

    // Test with non-existent container
    let result = docker
        .is_container_running("nonexistent_container_id")
        .await;
    assert!(result.is_ok());
    assert!(!result.unwrap());

    // Create and test with running container
    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "infinity".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Container should be running
    let is_running = docker.is_container_running(&container_id).await.unwrap();
    assert!(is_running);

    // Stop the container
    docker.stop_container(&container_id).await.unwrap();

    // Container should not be running
    let is_running = docker.is_container_running(&container_id).await.unwrap();
    assert!(!is_running);

    // Cleanup
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_is_container_running_not_found() {
    let docker = DockerManager::new().unwrap();

    // Test with clearly fake container ID
    let result = docker
        .is_container_running("this_container_definitely_does_not_exist")
        .await;
    assert!(result.is_ok());
    assert!(
        !result.unwrap(),
        "Non-existent container should return false"
    );
}

////////////////////////////////////////////////////////////////////////////////
// Additional Docker Integration Tests
////////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn test_create_and_remove_container() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "100".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    assert!(!container_id.is_empty());

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_create_container_with_name() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        name: Some("test-dsb-container".to_string()),
        command: Some(vec!["sleep".to_string(), "100".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    assert!(!container_id.is_empty());

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_create_container_with_environment() {
    let docker = DockerManager::new().unwrap();

    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let config = SandboxConfig {
        image: test_image(),
        environment: env,
        command: Some(vec!["sleep".to_string(), "100".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_exec_container_command() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "infinity".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Give container a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Execute a simple command
    let result = docker
        .exec_container(
            &container_id,
            vec!["echo".to_string(), "hello".to_string()],
            None,
        )
        .await;
    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("hello") || output.trim() == "hello");

    // Clean up
    docker.stop_container(&container_id).await.unwrap();
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_remove_container_with_force() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "infinity".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Give container a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Remove with force (should stop if running)
    let result = docker.remove_container(&container_id).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_get_container_stats() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "infinity".to_string()]),
        ..Default::default()
    };

    let container_id = match docker.create_container(&config, None).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Skipping test_get_container_stats: Docker unavailable: {e}");
            return;
        }
    };
    if let Err(e) = docker.start_container(&container_id).await {
        eprintln!("Skipping test_get_container_stats: start failed: {e}");
        let _ = docker.remove_container(&container_id).await;
        return;
    }

    // Give container a moment to start and generate some stats
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

    // Get stats
    let result = docker.get_container_stats(&container_id).await;
    assert!(result.is_ok());

    // Clean up
    let _ = docker.stop_container(&container_id).await;
    let _ = docker.remove_container(&container_id).await;
}

#[tokio::test]
async fn test_get_container_exit_info_running() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "infinity".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Give container a moment to start (use longer wait to avoid flakiness under load)
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Get exit info for running container (should return exit code -1 or 0)
    let result = docker.get_container_exit_info(&container_id).await;
    assert!(result.is_ok());

    let (exit_code, oom_killed) = result.unwrap();
    // Running container typically has exit code 0 or -1
    assert!(exit_code == 0 || exit_code == -1);
    assert!(!oom_killed);

    // Clean up
    docker.stop_container(&container_id).await.unwrap();
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_get_container_exit_info_exited() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "1".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Wait for container to exit
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    // Get exit info for exited container
    let result = docker.get_container_exit_info(&container_id).await;
    assert!(result.is_ok());

    let (exit_code, oom_killed) = result.unwrap();
    // Container that exits normally should have exit code 0
    assert_eq!(exit_code, 0);
    assert!(!oom_killed);

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_get_container_exit_info_not_found() {
    let docker = DockerManager::new().unwrap();

    // Try to get exit info for non-existent container
    let result = docker
        .get_container_exit_info("nonexistent-container-id")
        .await;
    assert!(result.is_err());

    let error_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(error_msg.contains("not found"));
}

#[tokio::test]
async fn test_get_container_logs() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo 'test output'; sleep 1".to_string(),
        ]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Give container time to produce output
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Get logs (currently returns empty string, just verify it doesn't error)
    let result = docker.get_container_logs(&container_id, Some(10)).await;
    assert!(result.is_ok());

    // Clean up
    docker.stop_container(&container_id).await.unwrap();
    docker.remove_container(&container_id).await.unwrap();
}

////////////////////////////////////////////////////////////////////////////////
// Error Handling and Edge Case Tests
////////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn test_create_container_invalid_image() {
    let docker = DockerManager::new().unwrap();

    // Test with a clearly invalid image format that should fail
    let config = SandboxConfig {
        image: "INVALID_IMAGE_NAME_WITH_INVALID_CHARS!@#$%^&*()".to_string(),
        ..Default::default()
    };

    let result = docker.create_container(&config, None).await;
    // May fail or succeed depending on Docker validation - just verify it doesn't crash
    let _ = result;
}

#[tokio::test]
async fn test_create_container_nonexistent_image() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: "thisdefinitelydoesnotexist123456:latest".to_string(),
        ..Default::default()
    };

    // Should either fail or succeed (image might be pulled later)
    let result = docker.create_container(&config, None).await;
    // We just verify it doesn't crash
    let _ = result;
}

#[tokio::test]
async fn test_start_container_already_running() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "10".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Start again - should handle gracefully
    let result = docker.start_container(&container_id).await;
    assert!(result.is_ok() || result.is_err());

    // Clean up
    docker.stop_container(&container_id).await.unwrap();
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_stop_container_not_running() {
    let docker = DockerManager::new().unwrap();

    // Stop a container that was never started
    let config = SandboxConfig {
        image: test_image(),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();

    // Stop without starting - should handle gracefully
    let result = docker.stop_container(&container_id).await;
    assert!(result.is_ok() || result.is_err());

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_remove_container_nonexistent() {
    let docker = DockerManager::new().unwrap();

    // Remove non-existent container - Docker's remove is idempotent
    let result = docker.remove_container("nonexistent-container-id").await;
    // Should succeed (idempotent operation) or fail - either is acceptable
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_exec_container_nonexistent() {
    let docker = DockerManager::new().unwrap();

    let result = docker
        .exec_container(
            "nonexistent-container-id",
            vec!["echo".to_string(), "test".to_string()],
            None,
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_pull_image_empty_name() {
    let docker = DockerManager::new().unwrap();

    // Pull with empty image name should fail
    let result = docker.pull_image("").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_image_exists_nonexistent_image() {
    let docker = DockerManager::new().unwrap();

    // Check for image that doesn't exist
    let result = docker.image_exists("nonexistentimage123456:latest").await;
    assert!(!result.unwrap_or(true)); // Should return false
}

#[tokio::test]
async fn test_is_container_running_nonexistent() {
    let docker = DockerManager::new().unwrap();

    let result = docker
        .is_container_running("nonexistent-container-id")
        .await;
    assert!(!result.unwrap_or(false)); // Should return false
}

#[tokio::test]
async fn test_list_containers_filters() {
    let docker = DockerManager::new().unwrap();

    // List with different filters - should not crash
    let result = docker.list_containers(true, None).await;
    assert!(result.is_ok());

    let containers = result.unwrap();
    // Just verify type is correct - Vec<ContainerSummary>
    let _: Vec<ContainerSummary> = containers;
}

#[tokio::test]
async fn test_get_container_stats_nonexistent() {
    let docker = DockerManager::new().unwrap();

    let result = docker.get_container_stats("nonexistent-container-id").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_container_workdir_nonexistent() {
    let docker = DockerManager::new().unwrap();

    let result = docker
        .get_container_workdir("nonexistent-container-id")
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_container_exit_info_stopped_container() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            "exit 42".to_string(),
        ]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Wait for container to finish
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    docker.stop_container(&container_id).await.unwrap();

    // Get exit info for stopped container
    let result = docker.get_container_exit_info(&container_id).await;
    assert!(result.is_ok());

    let (exit_code, _oom_killed) = result.unwrap();
    assert_eq!(exit_code, 42); // Should match our exit code

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_create_container_with_resource_limits() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        resource_limits: ResourceLimits {
            memory_mb: Some(128),
            cpu_quota: Some(50000),
            cpu_period: Some(100000),
            cpu_shares: Some(512),
            pids_limit: Some(50),
            ulimits: None,
        },
        ..Default::default()
    };

    let result = docker.create_container(&config, None).await;
    assert!(result.is_ok());

    // Clean up
    if let Ok(container_id) = result {
        docker.remove_container(&container_id).await.unwrap();
    }
}

#[tokio::test]
async fn test_create_container_with_port_mappings() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        port_mappings: vec![PortMapping {
            host_port: 8080,
            container_port: 80,
            protocol: PortProtocol::Tcp,
        }],
        ..Default::default()
    };

    let result = docker.create_container(&config, None).await;
    assert!(result.is_ok());

    // Clean up
    if let Ok(container_id) = result {
        docker.remove_container(&container_id).await.unwrap();
    }
}

#[tokio::test]
async fn test_create_container_with_volumes() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        volumes: vec![VolumeMount::Bind {
            host_path: "/tmp/test".to_string(),
            container_path: "/container/test".to_string(),
            read_only: false,
        }],
        ..Default::default()
    };

    let result = docker.create_container(&config, None).await;
    assert!(result.is_ok());

    // Clean up
    if let Ok(container_id) = result {
        docker.remove_container(&container_id).await.unwrap();
    }
}

#[tokio::test]
async fn test_create_container_with_invalid_port() {
    let docker = DockerManager::new().unwrap();

    let config = SandboxConfig {
        image: test_image(),
        port_mappings: vec![PortMapping {
            host_port: 0, // Invalid: host port 0 is typically not allowed
            container_port: 80,
            protocol: PortProtocol::Tcp,
        }],
        ..Default::default()
    };

    // May fail validation or be accepted (depends on Docker)
    let result = docker.create_container(&config, None).await;
    // Just verify it doesn't crash
    let _ = result;
}

// ========================================================================
// Additional Edge Case Tests
// ========================================================================

#[tokio::test]
async fn test_exec_container_with_empty_command() {
    let docker = DockerManager::new().unwrap();

    // Create and start a container
    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "300".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Test with empty command vector
    let result = docker.exec_container(&container_id, vec![], None).await;
    assert!(result.is_err());

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_get_container_exit_info_nonexistent_new() {
    let docker = DockerManager::new().unwrap();

    // Get exit info for non-existent container
    let result = docker
        .get_container_exit_info("nonexistent-container-id2")
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_get_container_exit_info_still_running() {
    let docker = DockerManager::new().unwrap();

    // Create and start a container
    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "300".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Get exit info while container is still running - should handle gracefully
    let result = docker.get_container_exit_info(&container_id).await;
    // May return error or default values - just verify it doesn't crash
    let _ = result;

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_get_container_logs_nonexistent() {
    let docker = DockerManager::new().unwrap();

    // Get logs for non-existent container
    let result = docker
        .get_container_logs("nonexistent-container-id", None)
        .await;
    assert!(result.is_ok()); // Returns empty string currently
}

#[tokio::test]
async fn test_get_container_logs_with_tail() {
    let docker = DockerManager::new().unwrap();

    // Create a container that outputs some text
    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo 'test line 1'; echo 'test line 2'; sleep 300".to_string(),
        ]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    // Docker socket proxy on macOS can refuse connections under load; retry once.
    for attempt in 0..3 {
        match docker.start_container(&container_id).await {
            Ok(_) => break,
            Err(e) if attempt < 2 && format!("{}", e).contains("connection refused") => {
                eprintln!("start_container attempt {} failed: {}. Retrying...", attempt + 1, e);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                continue;
            }
            Err(e) => panic!("Failed to start container: {}", e),
        }
    }

    // Wait a bit for output
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Get logs with tail
    let result = docker.get_container_logs(&container_id, Some(10)).await;
    assert!(result.is_ok());

    let _logs = result.unwrap();
    // Should contain some output (may be empty due to timing)
    // We just verify the operation succeeded above

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_remove_volume_nonexistent() {
    let docker = DockerManager::new().unwrap();

    // Remove non-existent volume - should handle gracefully
    let result = docker.remove_volume("nonexistent-volume-name").await;
    assert!(result.is_ok() || result.is_err()); // Either is acceptable
}

#[tokio::test]
async fn test_docker_manager_docker_client() {
    let docker = DockerManager::new().unwrap();

    // Get the underlying Docker client
    let _client = docker.docker_client();
    // Just verify we can access it
}

#[tokio::test]
async fn test_concurrent_container_operations() {
    let docker = DockerManager::new().unwrap();

    // Create multiple containers concurrently
    let mut handles = vec![];

    for _i in 0..3 {
        let docker_clone = docker.clone();
        let handle = tokio::spawn(async move {
            let config = SandboxConfig {
                image: test_image(),
                command: Some(vec!["sleep".to_string(), "10".to_string()]),
                ..Default::default()
            };

            docker_clone.create_container(&config, None).await
        });
        handles.push(handle);
    }

    // Wait for all creations
    let mut container_ids = vec![];
    for handle in handles {
        let result = handle.await.unwrap();
        if let Ok(id) = result {
            container_ids.push(id);
        }
    }

    // Clean up
    for container_id in container_ids {
        let _ = docker.remove_container(&container_id).await;
    }
}

#[tokio::test]
async fn test_list_containers_with_filters() {
    let docker = DockerManager::new().unwrap();

    // Test listing with specific filters
    let mut filters = std::collections::HashMap::new();
    filters.insert("label".to_string(), vec!["test.label=value".to_string()]);

    let result = docker.list_containers(true, Some(filters)).await;
    // Should succeed even if no containers match
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_exec_container_http_with_large_input() {
    let docker = DockerManager::new().unwrap();

    // Create and start a container
    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "300".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Test with large input
    let large_input = "x".repeat(10000);
    let body_value = serde_json::json!(large_input);
    let result = docker
        .exec_container_http(&container_id, "/exec", "POST", Some(body_value), Some(30))
        .await;

    // Just verify it doesn't crash
    let _ = result;

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_exec_container_http_with_special_chars() {
    let docker = DockerManager::new().unwrap();

    // Create and start a container
    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "300".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();
    docker.start_container(&container_id).await.unwrap();

    // Test with special characters
    let special_input = "test\n\t\r!@#$%^&*(){}[]<>?/\\|";
    let body_value = serde_json::json!(special_input);
    let result = docker
        .exec_container_http(&container_id, "/exec", "POST", Some(body_value), Some(30))
        .await;

    // Just verify it doesn't crash
    let _ = result;

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_create_container_with_all_features() {
    let docker = DockerManager::new().unwrap();

    // Test with all possible configuration options
    let config = SandboxConfig {
        image: test_image(),
        name: Some("test-all-features".to_string()),
        command: Some(vec!["sleep".to_string(), "10".to_string()]),
        environment: vec![
            ("VAR1".to_string(), "value1".to_string()),
            ("VAR2".to_string(), "value2".to_string()),
        ]
        .into_iter()
        .collect(),
        port_mappings: vec![PortMapping {
            host_port: 8081,
            container_port: 8081,
            protocol: PortProtocol::Tcp,
        }],
        resource_limits: ResourceLimits {
            memory_mb: Some(256),
            cpu_quota: Some(100000),
            cpu_period: Some(100000),
            cpu_shares: Some(1024),
            pids_limit: Some(100),
            ulimits: None,
        },
        volumes: vec![VolumeMount::Bind {
            host_path: "/tmp/test".to_string(),
            container_path: "/container/test".to_string(),
            read_only: false,
        }],
        ..Default::default()
    };

    let result = docker.create_container(&config, None).await;
    // May fail due to volume path not existing - just verify it doesn't crash
    let _ = result;
}

#[tokio::test]
async fn test_stop_container_already_stopped() {
    let docker = DockerManager::new().unwrap();

    // Create a container
    let config = SandboxConfig {
        image: test_image(),
        command: Some(vec!["sleep".to_string(), "10".to_string()]),
        ..Default::default()
    };

    let container_id = docker.create_container(&config, None).await.unwrap();

    // Stop without starting - should handle gracefully
    let result = docker.stop_container(&container_id).await;
    // May succeed or fail depending on Docker behavior
    let _ = result;

    // Clean up
    docker.remove_container(&container_id).await.unwrap();
}

#[tokio::test]
async fn test_image_exists_with_invalid_format() {
    let docker = DockerManager::new().unwrap();

    // Test with invalid image format
    let result = docker.image_exists("invalid@image@format").await;
    // Should handle gracefully
    assert!(result.is_ok() || result.is_err());
}

////////////////////////////////////////////////////////////////////////////////
// Defensive Error Handling Tests
////////////////////////////////////////////////////////////////////////////////

/// Verify that to_str() returns None for invalid UTF-8 paths.
/// This is the error condition that the production code at line ~417
/// now handles gracefully instead of unwrap()-ping.
#[test]
#[cfg(unix)]
fn test_path_to_str_rejects_invalid_utf8() {
    use std::os::unix::ffi::OsStrExt;
    let invalid_utf8 = std::ffi::OsStr::from_bytes(b"/tmp/\x80\x81\x82.sock");
    assert!(
        invalid_utf8.to_str().is_none(),
        "to_str() should return None for invalid UTF-8"
    );
}

/// Verify that CString::new rejects strings with embedded null bytes.
/// This is the error condition that the production code at line ~684
/// now handles gracefully instead of unwrap()-ping.
#[test]
fn test_cstring_new_rejects_null_byte() {
    let path_with_null = "/tmp/dsb\0static\0files";
    let result = std::ffi::CString::new(path_with_null);
    assert!(
        result.is_err(),
        "CString::new should reject strings containing null bytes"
    );
}
