// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Test Docker Setup
//!
//! Provides lightweight Docker client for tests that don't need PostgreSQL.
//!
//! This is a simpler alternative to TestDatabase for tests that only need
//! Docker container management (like SSH gateway tests and authorization tests).

use bollard::Docker;
use dsb::config::Config;
use dsb::docker::DockerManager;
use std::sync::Arc;

/// A lightweight Docker test helper for tests that don't need PostgreSQL
///
/// # Examples
///
/// ```no_run
/// use tests::common::docker_test_setup::TestDocker;
///
/// #[tokio::test]
/// async fn test_with_docker() {
///     let docker = TestDocker::new().expect("Failed to create TestDocker");
///     // Use docker.docker_client for container operations
///     // Use docker.config for configuration
/// }
/// ```
#[allow(dead_code)]
pub struct TestDocker {
    /// Docker client (wrapped in Arc for cloning)
    pub docker: Arc<Docker>,
    /// Configuration (useful for creating containers)
    #[allow(dead_code)]
    pub config: Config,
}

#[allow(dead_code)]
impl TestDocker {
    /// Creates a new TestDocker instance using default configuration
    ///
    /// This uses Config::default() which provides sensible defaults for testing,
    /// eliminating the need for .env.test files.
    ///
    /// # Errors
    ///
    /// Returns an error if Docker daemon is not running or not accessible.
    pub fn new() -> Result<Self, String> {
        let config = Config::default();
        let docker_manager = DockerManager::new_with_config(&config)
            .map_err(|e| format!("Failed to create Docker manager: {}", e))?;
        let docker = docker_manager.docker_client();

        Ok(TestDocker { docker, config })
    }

    /// Returns a reference to the Docker client
    pub fn docker_client(&self) -> Arc<Docker> {
        Arc::clone(&self.docker)
    }

    /// Returns a reference to the configuration
    pub fn config(&self) -> &Config {
        &self.config
    }
}
