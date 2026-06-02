// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Test setup and cleanup utilities
//!
//! This module provides utilities for setting up tests and cleaning up
//! resources from previous test runs.

use bollard::query_parameters::{ListContainersOptions, RemoveContainerOptions};
use bollard::Docker;

/// Clean up any test resources from previous runs
///
/// This function attempts to remove all Docker containers with a "test-" prefix
/// OR containers created from test images (python:3.12.11, alpine:latest used in tests)
/// that may have been left behind by crashed or interrupted test runs.
/// It's useful to call this at the beginning of test runs to ensure a clean state.
///
/// # Returns
///
/// * `Ok(count)` - Number of containers cleaned up
/// * `Err(error)` - Error if cleanup failed
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::test_setup::cleanup_previous_test_resources;
///
/// #[tokio::test]
/// async fn test_with_clean_state() {
///     // Clean up any previous test runs
///     cleanup_previous_test_resources().await.expect("Failed to cleanup previous resources");
///
///     // Proceed with test...
/// }
/// ```
pub async fn cleanup_previous_test_resources() -> Result<usize, Box<dyn std::error::Error>> {
    // Use the proper Docker socket path (handles macOS Docker Desktop)
    let docker_socket = crate::common::test_config::get_test_docker_socket();
    let docker = Docker::connect_with_local(&docker_socket, 120, bollard::API_DEFAULT_VERSION)?;

    // List all containers (including stopped ones)
    let options = Some(ListContainersOptions {
        all: true,
        ..Default::default()
    });

    let containers = docker.list_containers(options).await?;
    let mut cleaned_count = 0;

    for container in containers {
        let should_remove = should_remove_container(&container);

        if should_remove {
            // Get the container_id
            let container_id = container.id.as_deref().unwrap_or("");

            // Get container name for logging
            let container_name = container
                .names
                .as_ref()
                .and_then(|n| n.first())
                .map(|n| n.as_str())
                .unwrap_or("<no name>");

            tracing::info!(
                container_name = %container_name,
                container_id = %container_id,
                image = %container.image.as_deref().unwrap_or("unknown"),
                "Removing leftover test container"
            );

            // Force remove with volumes
            let remove_options = RemoveContainerOptions {
                force: true,
                v: true,
                ..Default::default()
            };

            match docker
                .remove_container(container_id, Some(remove_options))
                .await
            {
                Ok(_) => {
                    cleaned_count += 1;
                    tracing::debug!(
                        container_name = %container_name,
                        "Successfully removed leftover test container"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        container_name = %container_name,
                        error = %e,
                        "Failed to remove leftover test container"
                    );
                    // Continue with other containers
                }
            }
        }
    }

    if cleaned_count > 0 {
        tracing::info!(
            cleaned_count = cleaned_count,
            "Cleaned up {} leftover test containers",
            cleaned_count
        );
    }

    Ok(cleaned_count)
}

/// Determine if a container should be removed during cleanup
///
/// Only removes containers that:
/// 1. Are older than 120 seconds (to avoid removing containers from the current run)
/// 2. Have a name or image matching known test patterns
fn should_remove_container(container: &bollard::models::ContainerSummary) -> bool {
    // Safety guard: never remove containers created within the last 120 seconds.
    // Concurrent test binaries may start overlapping cleanup passes while sibling
    // tests are already creating containers. Any container younger than 2 minutes
    // belongs to the current test run and must be left alone.
    let created = container.created.unwrap_or(0);
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let age_secs = now_secs - created;
    if age_secs < 120 {
        return false;
    }

    // Check if container has a name starting with "/test-"
    if let Some(names) = &container.names {
        for name in names {
            if name.starts_with("/test-") {
                return true;
            }
        }
    }

    // Check if container is using a test image
    if let Some(image) = &container.image {
        let image_lower = image.to_lowercase();

        // Match Python test images (with or without registry prefix)
        if image_lower.contains("python:3.12.11") || image_lower.contains("library/python:3.12.11")
        {
            return true;
        }

        // Match Alpine test images used in integration tests
        if image_lower.contains("alpine:latest") {
            if let Some(names) = &container.names {
                for name in names {
                    if name.starts_with("/test-sandbox-") {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Count test resources currently present
///
/// This function counts all Docker containers with a "test-" prefix.
/// Useful for debugging and verifying cleanup.
///
/// # Returns
///
/// * `Ok(count)` - Number of test containers found
/// * `Err(error)` - Error if listing failed
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::test_setup::count_test_resources;
///
/// #[tokio::test]
/// async fn test_check_resources() {
///     let count = count_test_resources().await.expect("Failed to count resources");
///     println!("Found {} test containers", count);
/// }
/// ```
pub async fn count_test_resources() -> Result<usize, Box<dyn std::error::Error>> {
    // Use the proper Docker socket path (handles macOS Docker Desktop)
    let docker_socket = crate::common::test_config::get_test_docker_socket();
    let docker = Docker::connect_with_local(&docker_socket, 120, bollard::API_DEFAULT_VERSION)?;

    let options = Some(ListContainersOptions {
        all: true,
        ..Default::default()
    });

    let containers = docker.list_containers(options).await?;
    let mut count = 0;

    for container in containers {
        // Check if container has a name starting with "/test-"
        if let Some(names) = container.names {
            for name in names {
                if name.starts_with("/test-") {
                    count += 1;
                    // Only count once per container, even if it has multiple names
                    break;
                }
            }
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_count_test_resources() {
        if crate::common::using_external_api() {
            eprintln!("Skipping test_count_test_resources: requires local Docker socket");
            return;
        }
        // This test should always pass (just counts containers)
        let count = count_test_resources()
            .await
            .expect("Failed to count resources");
        tracing::info!("Found {} test containers", count);
    }

    #[tokio::test]
    async fn test_cleanup_previous_test_resources() {
        if crate::common::using_external_api() {
            eprintln!(
                "Skipping test_cleanup_previous_test_resources: requires local Docker socket"
            );
            return;
        }
        // This test cleans up any previous test containers
        let count = cleanup_previous_test_resources()
            .await
            .expect("Failed to cleanup");
        tracing::info!("Cleaned up {} test containers", count);
    }
}
