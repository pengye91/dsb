// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Common test utilities
//!
//! Shared infrastructure for integration tests.

pub mod db_test_setup;
pub mod docker_test_setup;
pub mod resource_registry;
pub mod server_fixture;
pub mod test_config;
pub mod test_panic_hook;
pub mod test_setup;
pub mod testcontainers_postgres;

// Re-export commonly used items
#[allow(unused_imports)]
pub use db_test_setup::TestDatabase;
#[allow(unused_imports)]
pub use resource_registry::{CleanupResult, ResourceRegistry, ResourceType};
#[allow(unused_imports)]
pub use test_panic_hook::{
    clear_panic_cleanup, install_test_panic_hook, log_cleanup_failures,
    register_async_panic_cleanup, register_panic_cleanup, register_registry_cleanup,
    register_registry_cleanup_with_verification, setup_panic_hook_with_verification,
};

/// Guard ensuring cleanup runs at most once per test binary.
///
/// Parallel tests in the same binary all call `setup_test_env()`. Without
/// this guard, each call races to remove containers, potentially deleting
/// ones just created by sibling tests running concurrently.
static CLEANUP_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Global test setup function - cleans up resources from previous test runs
///
/// This should be called at the beginning of test suites to ensure
/// a clean state by removing any containers/resources from previous runs.
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::setup_test_env;
///
/// #[tokio::test]
/// async fn my_test() {
///     setup_test_env().await;
///     // ... test code ...
/// }
/// ```
#[allow(dead_code)]
pub async fn setup_test_env() {
    // Run cleanup at most once per test binary. Without this guard, multiple
    // concurrent tests calling setup_test_env() in the same binary can race
    // to remove each other's freshly-created containers.
    if CLEANUP_DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    // Attempt to cleanup previous test resources
    match test_setup::cleanup_previous_test_resources().await {
        Ok(count) if count > 0 => {
            tracing::info!(
                "Cleaned up {} leftover test containers from previous run",
                count
            );
        }
        Ok(_) => {
            tracing::debug!("No leftover test containers to clean up");
        }
        Err(e) => {
            tracing::warn!(
                "Failed to cleanup previous test resources: {}. Continuing anyway...",
                e
            );
        }
    }
}

use dsb::config::load_for_tests;

/// Loads configuration for tests from environment variables.
///
/// This function reads the DSB configuration from environment variables
/// (typically set via .env.test or .env files). It's intended to be used
/// in tests to access configuration values like Docker image names,
/// server ports, and timeout values.
///
/// # Panics
///
/// Panics if configuration cannot be loaded from environment.
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::test_config;
///
/// let config = test_config();
/// let image = &config.docker.test_image;
/// let port = config.server.port;
/// ```
pub fn test_config() -> dsb::config::Config {
    load_for_tests()
        .expect("Failed to load test config from environment. Ensure .env.test is sourced.")
}

/// Detect whether tests should target an external API (EKS, remote dev).
///
/// Returns `true` when `DSB_TEST_API_URL` is set to a non-localhost URL
/// (anything other than the default `http://127.0.0.1:18080`). When
/// `true`, tests that start a local server or need direct Docker access
/// should skip or redirect to the external endpoint.
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::using_external_api;
///
/// if using_external_api() {
///     eprintln!("Skipping: requires local Docker");
///     return;
/// }
/// ```
#[allow(dead_code)]
pub fn using_external_api() -> bool {
    let api_url = test_config::TestInfraConfig::from_env().api_base_url;
    !api_url.starts_with("http://127.0.0.1:18080") && !api_url.starts_with("http://localhost:18080")
}

/// Gets the default test image from configuration.
///
/// Returns the fully-qualified Docker image name configured for testing,
/// including the registry prefix if configured.
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::default_test_image;
///
/// let image = default_test_image();
/// // Returns: "docker.io/python:3.12"
/// ```
#[allow(dead_code)]
pub fn default_test_image() -> String {
    let config = test_config();
    config.docker.test_image.clone()
}

/// Constructs a full Docker image name with the configured registry.
///
/// If a Docker registry is configured in the test configuration, this function
/// prefixes the image name with the registry. Otherwise, returns the image name
/// as-is.
///
/// # Arguments
///
/// * `image` - Base image name (e.g., "python:3.12" or "alpine:latest")
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::image_with_registry;
///
/// // With registry configured as "docker.io"
/// let full_image = image_with_registry("python:3.12");
/// // Returns: "docker.io/python:3.12"
///
/// let simple_image = image_with_registry("alpine:latest");
/// // Returns: "docker.io/alpine:latest"
/// ```
#[allow(dead_code)]
pub fn image_with_registry(image: &str) -> String {
    let config = test_config();
    if config.docker.registry.is_empty() {
        image.to_string()
    } else {
        format!("{}/{}", config.docker.registry, image)
    }
}

/// Returns the sandbox Docker image name for testing.
///
/// Reads from the `DSB_TEST_SANDBOX_IMAGE` environment variable, falling
/// back to `DSB_SANDBOX_IMAGE`, then to `dsb/sandbox:latest`. This aligns
/// with [`TestInfraConfig`] so tests can override the image consistently.
#[allow(dead_code)]
pub fn sandbox_image() -> String {
    std::env::var("DSB_TEST_SANDBOX_IMAGE")
        .ok()
        .or_else(|| std::env::var("DSB_SANDBOX_IMAGE").ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "dsb/sandbox:latest".to_string())
}
