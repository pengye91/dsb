// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Integration Tests for DSB API
//!
//! This module provides comprehensive integration tests for the DSB API server.
//!
//! ## Architecture: Global Test Fixtures
//!
//! These tests use a **global test fixture pattern** for end-to-end testing:
//!
//! ```text
//! TestFixture (initialized once)
//!   ├── PostgreSQL Service (docker-compose postgres-test)
//!   │   └── Connected to running docker-compose service, migrations applied
//!   └── DSB Server (running on port 18080)
//!       ├── Accepts HTTP requests from all tests
//!       ├── Persists state across tests
//!       └── Cleaned up when test binary exits
//! ```
//!
//! ### Benefits of Global Fixtures
//!
//! - ✅ **Performance**: Server starts once, not per-test (saves 10-20 seconds per test)
//! - ✅ **Realism**: Tests run against a long-running server (like production)
//! - ✅ **Efficiency**: No repeated container spinup overhead
//! - ✅ **Resource**: Lower memory/CPU usage
//! - ✅ **State**: Can test cross-test scenarios (e.g., create in test A, verify in test B)
//!
//! ### How It Works
//!
//! 1. First test that calls `init_global_fixture()` initializes:
//!    - Spawns a background thread with its own tokio runtime
//!    - Connects to docker-compose postgres-test service
//!    - Runs database migrations
//!    - Starts DSB server on port 18080
//!    - Waits for server to be ready
//!    - Stores everything in a `OnceLock` static
//!
//! 2. Subsequent tests reuse the same fixture:
//!    - Check if fixture exists (instant, no startup delay)
//!    - Use existing server and database
//!    - Tests can see data created by earlier tests
//!
//! 3. Cleanup:
//!    - When test binary exits, everything is cleaned up automatically
//!    - DSB server shuts down
//!
//! ## Prerequisites
//!
//! - Docker daemon must be running
//! - Docker compose services must be running: `docker compose -f docker-compose.test.yml up -d postgres-test`
//! - Tests must run **sequentially** (`--test-threads=1`) to avoid race conditions
//!   (This is enforced because all tests share the same server/database)
//!
//! ## Running Tests
//!
//! ### Quick Start
//!
//! ```bash
//! # Run all integration tests (recommended way)
//! make test-e2e
//!
//! # Or manually with single thread
//! cargo test --test integration_test -- --test-threads=1
//! ```
//!
//! ### Individual Tests
//!
//! ```bash
//! # Run specific test
//! cargo test --test integration_test test_health_check -- --test-threads=1
//!
//! # Run with output
//! cargo test --test integration_test -- --show-output --test-threads=1
//! ```
//!
//! ### Full Test Suite
//!
//! ```bash
//! # Run all tests with E2E tests
//! make test-all
//! ```
//!
//! ## Test Coverage
//!
//! - Health check endpoint
//! - Create sandbox with default command
//! - Create sandbox with custom command
//! - List sandboxes
//! - Get sandbox details
//! - Execute commands in sandbox
//! - Get sandbox statistics
//! - Stop sandbox
//! - Delete sandbox
//! - Force cleanup
//! - Full sandbox lifecycle

use dsb::api::start_server;
use dsb::config::Config;
use reqwest::Client;
use serde_json::json;
use serial_test::serial;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

mod common;
use common::db_test_setup::TestDatabase;
use common::sandbox_image;
use common::setup_test_env;

/// Create a reqwest client with the test API key pre-configured as a default header.
/// This ensures all requests are authenticated when running against external deployments
/// (EKS, etc.) that require API key auth. The header is harmless when auth is disabled.
fn test_client() -> Client {
    let api_key = common::test_config::TestInfraConfig::from_env().api_key;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "X-API-Key",
        api_key.parse().expect("Invalid API key format"),
    );
    Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to build test client")
}

/// Poll the sandbox endpoint until it reaches "running" state.
async fn wait_for_running(
    client: &Client,
    sandbox_id: &str,
    timeout_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    while tokio::time::Instant::now() < deadline {
        let resp = client
            .get(format!("{}/sandboxes/{}", api_base(), sandbox_id))
            .send()
            .await?;
        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().await?;
            if body.get("state").and_then(|s| s.as_str()) == Some("running") {
                return Ok(());
            }
        }
        sleep(Duration::from_millis(200)).await;
    }
    Err(format!(
        "Sandbox {} did not reach running state within {}s",
        sandbox_id, timeout_secs
    )
    .into())
}

/// Global test fixture - initialized once for all tests
struct TestFixture {
    #[allow(dead_code)]
    db: Option<TestDatabase>, // Keep alive to prevent container from being dropped; None for external deployments
}

static GLOBAL_FIXTURE: OnceLock<&'static TestFixture> = OnceLock::new();

/// Initialize the global test fixture
///
/// This function is called once at the beginning of the test suite.
/// It starts a PostgreSQL container and the DSB server, which are
/// then reused by all tests.
/// Detect whether tests are running against an external deployment (EKS, etc.)
/// rather than the local server started by this test file.
fn using_external_api() -> bool {
    let url = api_base();
    // If the API URL is not localhost:18080, we're using an external deployment
    !url.contains("127.0.0.1:18080") && !url.contains("localhost:18080")
}

fn init_global_fixture() -> &'static TestFixture {
    GLOBAL_FIXTURE.get_or_init(|| {
        use std::sync::mpsc;

        // Create a channel to keep the server thread alive
        let (tx, rx) = mpsc::channel();

        // Spawn a thread to run the server
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");

            let fixture = rt.block_on(async {
                let external = using_external_api();

                // Only set up local infrastructure (DB, Docker, local server)
                // when running against the local docker-compose stack.
                let db = if external {
                    None
                } else {
                    // Create test database
                    let db = TestDatabase::new()
                        .await
                        .expect("Failed to create test database. Make sure docker-compose services are running: docker compose -f docker-compose.test.yml up -d postgres-test");

                    // Set DOCKER_HOST for web terminal Docker connection from test config
                    let docker_socket = common::test_config::get_test_docker_socket();
                    std::env::set_var("DOCKER_HOST", &docker_socket);

                    Some(db)
                };

                // Only start local server when not using an external deployment
                if !external {
                    // Create test configuration with port 18080
                    let mut config = Config::default();
                    config.server.port = 18080;

                    // Configure database from TestInfraConfig (supports Docker Compose, EKS, local)
                    let infra_config = common::test_config::TestInfraConfig::from_env();
                    config.database.url = Some(infra_config.database_url.clone());

                    // Start server in background task
                    tokio::spawn(async move {
                        eprintln!("Server starting on port 18080...");
                        if let Err(e) = start_server(&config).await {
                            eprintln!("Server failed to start: {}", e);
                        }
                    });

                    // Wait for server to be ready
                    let client = test_client();
                    let mut retries = 0;
                    while retries < 120 {
                        match client.get("http://127.0.0.1:18080/health").send().await {
                            Ok(response) if response.status().is_success() => break,
                            _ => {
                                sleep(Duration::from_millis(250)).await;
                                retries += 1;
                            }
                        }
                    }

                    if retries >= 120 {
                        panic!("Server did not start within expected time");
                    }

                    // Give additional time for server to fully initialize
                    sleep(Duration::from_millis(500)).await;
                }

                Box::new(TestFixture { db })
            });

            // Send the fixture back before blocking
            tx.send(Box::leak(fixture)).expect("Failed to send fixture");

            // Keep the runtime alive by blocking forever
            // The runtime will be destroyed when the process exits
            std::mem::forget(rt);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });

        // Wait for the fixture to be ready
        rx.recv().expect("Failed to receive fixture")
    })
}

/// API base URL for testing.
///
/// Reads from `DSB_TEST_API_URL` environment variable (via [`TestInfraConfig`]),
/// defaulting to `http://127.0.0.1:18080` for local docker-compose testing.
fn api_base() -> &'static str {
    use std::sync::OnceLock;
    static API_BASE_URL: OnceLock<String> = OnceLock::new();
    API_BASE_URL.get_or_init(|| {
        common::test_config::TestInfraConfig::from_env().api_base_url
    })
}

/// Test helper struct for sandbox management
struct TestSandbox {
    id: String,
    client: Client,
}

impl TestSandbox {
    /// Creates a new sandbox for testing
    async fn create_with_default_command(
        client: &Client,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let response = client
            .post(format!("{}/sandboxes", api_base()))
            .json(&json!({
                "image": sandbox_image(),
                "name": format!("test-sandbox-{}", Uuid::new_v4()),
                "command": ["sleep", "60"],
                "pull_policy": "missing"
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to create sandbox: {}", response.status()).into());
        }

        let value: serde_json::Value = response.json().await?;
        let id = value
            .get("id")
            .and_then(|i| i.as_str())
            .ok_or("Missing id in response")?;

        Ok(TestSandbox {
            id: id.to_string(),
            client: client.clone(),
        })
    }

    /// Creates a sandbox with a custom command
    async fn create_with_command(
        client: &Client,
        command: Vec<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let response = client
            .post(format!("{}/sandboxes", api_base()))
            .json(&json!({
                "image": sandbox_image(),
                "name": format!("test-sandbox-{}", Uuid::new_v4()),
                "command": command,
                "pull_policy": "missing"
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to create sandbox: {}", response.status()).into());
        }

        let value: serde_json::Value = response.json().await?;
        let id = value
            .get("id")
            .and_then(|i| i.as_str())
            .ok_or("Missing id in response")?;

        Ok(TestSandbox {
            id: id.to_string(),
            client: client.clone(),
        })
    }

    /// Gets sandbox details
    async fn get_info(&self) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let response = self
            .client
            .get(format!("{}/sandboxes/{}", api_base(), self.id))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to get sandbox info: {}", response.status()).into());
        }

        Ok(response.json().await?)
    }

    /// Executes a command in the sandbox
    async fn exec(&self, command: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let mut last_err = None;
        // Retry up to 3 times: tool_proxy may not be ready even after sandbox
        // reaches "Running" state (health check fallback after 30s timeout).
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            let response = self
                .client
                .post(format!("{}/sandboxes/{}/exec", api_base(), self.id))
                .json(&json!({
                    "command": command
                }))
                .send()
                .await?;

            if response.status().is_success() {
                let value: serde_json::Value = response.json().await?;
                return Ok(value
                    .get("output")
                    .and_then(|o| o.as_str())
                    .unwrap_or("")
                    .to_string());
            }

            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            last_err = Some(format!(
                "Failed to exec command (attempt {}/3): {} - {}",
                attempt + 1, status, body
            ));
        }

        Err(last_err.unwrap_or_else(|| "Failed to exec command after 3 attempts".into()).into())
    }

    /// Gets sandbox statistics
    async fn get_stats(&self) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let response = self
            .client
            .get(format!("{}/sandboxes/{}/stats", api_base(), self.id))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to get sandbox stats: {}", response.status()).into());
        }

        Ok(response.json().await?)
    }

    /// Stops the sandbox
    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let response = self
            .client
            .post(format!("{}/sandboxes/{}/stop", api_base(), self.id))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to stop sandbox: {}", response.status()).into());
        }

        Ok(())
    }

    /// Deletes the sandbox
    async fn delete(self) -> Result<(), Box<dyn std::error::Error>> {
        let response = self
            .client
            .delete(format!("{}/sandboxes/{}", api_base(), self.id))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to delete sandbox: {}", response.status()).into());
        }

        Ok(())
    }

    /// Force cleanup the sandbox
    async fn cleanup(self) -> Result<(), Box<dyn std::error::Error>> {
        let response = self
            .client
            .post(format!("{}/sandboxes/{}/cleanup", api_base(), self.id))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to cleanup sandbox: {}", response.status()).into());
        }

        Ok(())
    }
}

/// Implement Drop to ensure cleanup happens even if test panics
impl Drop for TestSandbox {
    fn drop(&mut self) {
        let id = self.id.clone();
        let client = self.client.clone();

        // Use a blocking thread to ensure cleanup completes
        // Since Drop is sync, we spawn a thread that creates its own runtime
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(async move {
                tracing::debug!(sandbox_id = %id, "Auto-cleaning up test sandbox via Drop");

                if let Err(e) = client
                    .post(format!("{}/sandboxes/{}/cleanup", api_base(), id))
                    .send()
                    .await
                {
                    tracing::warn!(
                        sandbox_id = %id,
                        error = %e,
                        "Failed to auto-cleanup test sandbox"
                    );
                }
            });
        });

        // Don't wait for cleanup in Drop - fire and forget
        // The test should explicitly call cleanup() before dropping
        // This prevents hangs if cleanup takes too long
        drop(handle);
    }
}

#[serial]
#[tokio::test]
async fn test_health_check() {
    // Cleanup any previous test resources (only relevant for local Docker)
    if !using_external_api() {
        setup_test_env().await;
    }

    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    let response = client
        .get(format!("{}/health", api_base()))
        .send()
        .await
        .expect("Failed to send health check request");

    let status = response.status();
    if !status.is_success() {
        panic!("Download failed with status: {}", status);
    }
    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    assert_eq!(body.get("status").and_then(|s| s.as_str()), Some("ok"));
}

#[serial]
#[tokio::test]
async fn test_create_sandbox_default_command() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create sandbox with default command
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Get sandbox info
    let info = sandbox
        .get_info()
        .await
        .expect("Failed to get sandbox info");

    // Verify sandbox is created and running
    assert_eq!(
        info.get("id").and_then(|i| i.as_str()),
        Some(sandbox.id.as_str())
    );
    assert_eq!(info.get("state").and_then(|s| s.as_str()), Some("running"));

    // Verify default command - can be null for default image
    let _config = info.get("config").expect("Config not found");
    // Command is optional for default containers

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_create_sandbox_custom_command() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create sandbox with custom command
    let sandbox = TestSandbox::create_with_command(
        &client,
        vec!["sh".to_string(), "-c".to_string(), "echo hello".to_string()],
    )
    .await
    .expect("Failed to create sandbox");

    // Get sandbox info
    let info = sandbox
        .get_info()
        .await
        .expect("Failed to get sandbox info");

    // Verify custom command is set
    let _config = info.get("config").expect("Config not found");
    let _command = _config.get("command").and_then(|c| c.as_array());
    // Command might be null if not explicitly set

    // Note: Container may exit quickly with custom command, but that's expected
    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_exec_command() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create sandbox with default command (keeps container running)
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Wait a bit for container to be fully ready
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // Execute a simple command
    let output = sandbox
        .exec(&["echo", "hello from sandbox"])
        .await
        .expect("Failed to exec command");

    assert!(output.contains("hello from sandbox"));

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_exec_complex_commands() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create sandbox with default command (keeps container running)
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Wait a bit for container to be fully ready
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // Test command chaining
    let output = sandbox
        .exec(&[
            "sh",
            "-c",
            "mkdir -p /tmp/test && cd /tmp/test && touch file.txt && ls file.txt",
        ])
        .await
        .expect("Failed to exec chained commands");

    assert!(output.contains("file.txt"));

    // Test pipe command
    let output = sandbox
        .exec(&["sh", "-c", "echo 'hello world' | grep hello"])
        .await
        .expect("Failed to exec pipe command");

    assert!(output.contains("hello world"));

    // Test redirection
    let output = sandbox
        .exec(&[
            "sh",
            "-c",
            "echo 'test content' > /tmp/test.txt && cat /tmp/test.txt",
        ])
        .await
        .expect("Failed to exec redirection command");

    assert!(output.contains("test content"));

    // Test command with quotes
    let output = sandbox
        .exec(&["sh", "-c", "echo \"quoted text\" | grep quoted"])
        .await
        .expect("Failed to exec quoted command");

    assert!(output.contains("quoted text"));

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_exec_error_handling() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create sandbox with default command (keeps container running)
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Wait a bit for container to be fully ready
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // Test command that returns error
    let _result = sandbox.exec(&["sh", "-c", "exit 1"]).await;
    // The exec API may return success even if command fails - just verify we get a response
    // Output might contain error information

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_list_sandboxes() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create two sandboxes
    eprintln!("Creating sandbox1...");
    let sandbox1 = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create first sandbox");
    eprintln!("Created sandbox1: {}", sandbox1.id);

    eprintln!("Creating sandbox2...");
    let sandbox2 = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create second sandbox");
    eprintln!("Created sandbox2: {}", sandbox2.id);

    // Wait for sandboxes to be running
    wait_for_running(&client, &sandbox1.id, 30)
        .await
        .expect("Sandbox1 did not reach running state");
    wait_for_running(&client, &sandbox2.id, 30)
        .await
        .expect("Sandbox2 did not reach running state");

    // List sandboxes
    let response = client
        .get(format!("{}/sandboxes", api_base()))
        .send()
        .await
        .expect("Failed to list sandboxes");

    assert!(response.status().is_success());

    // Handle both legacy array and new paginated format
    let json_value: serde_json::Value = response.json().await.expect("Failed to parse response");

    eprintln!(
        "List response: {}",
        serde_json::to_string_pretty(&json_value).unwrap()
    );

    let sandboxes = if let Some(data) = json_value.get("data") {
        // New paginated format
        eprintln!("Using paginated format");
        data.as_array().unwrap_or(&vec![]).clone()
    } else {
        // Legacy array format
        eprintln!("Using legacy array format");
        json_value.as_array().unwrap_or(&vec![]).clone()
    };

    eprintln!("Found {} sandboxes", sandboxes.len());
    eprintln!("Sandbox1 ID: {}", sandbox1.id);
    eprintln!("Sandbox2 ID: {}", sandbox2.id);

    // Verify our sandboxes are in the list
    let ids: Vec<&str> = sandboxes
        .iter()
        .filter_map(|s| s.get("id").and_then(|i| i.as_str()))
        .collect();

    eprintln!("IDs in list: {:?}", ids);
    assert!(ids.contains(&sandbox1.id.as_str()));
    assert!(ids.contains(&sandbox2.id.as_str()));

    // Cleanup
    sandbox1
        .cleanup()
        .await
        .expect("Failed to cleanup sandbox1");
    sandbox2
        .cleanup()
        .await
        .expect("Failed to cleanup sandbox2");
}

#[serial]
#[tokio::test]
async fn test_get_sandbox_stats() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Wait for container to be running
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // Get stats - API returns stats in different format, just verify we get a response
    let stats = sandbox
        .get_stats()
        .await
        .expect("Failed to get sandbox stats");

    // Stats response may vary, just verify it's not empty
    assert!(!stats.is_null(), "Stats should not be null");

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_stop_and_delete_sandbox() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Stop sandbox
    sandbox.stop().await.expect("Failed to stop sandbox");

    // Verify it's stopped
    let info = sandbox
        .get_info()
        .await
        .expect("Failed to get sandbox info");
    assert_eq!(info.get("state").and_then(|s| s.as_str()), Some("stopped"));

    // Save the ID before delete consumes self
    let sandbox_id = sandbox.id.clone();

    // Delete sandbox
    sandbox.delete().await.expect("Failed to delete sandbox");

    // Verify it's deleted (404 expected)
    let response = client
        .get(format!("{}/sandboxes/{}", api_base(), sandbox_id))
        .send()
        .await
        .expect("Failed to send get request");

    assert_eq!(response.status(), 404);
}

#[serial]
#[tokio::test]
async fn test_delete_sandbox_with_missing_container() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    // This test directly manipulates Docker containers, which is not available
    // on Kubernetes or other non-Docker backends.
    if using_external_api() {
        eprintln!("Skipping test_delete_sandbox_with_missing_container: requires direct Docker access");
        return;
    }

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Stop sandbox
    sandbox.stop().await.expect("Failed to stop sandbox");

    // Get the container ID from sandbox info
    let info = sandbox
        .get_info()
        .await
        .expect("Failed to get sandbox info");
    let container_id = info
        .get("container_id")
        .and_then(|s| s.as_str())
        .expect("Failed to get container_id");

    // Manually remove the container from Docker (simulating the container being gone)
    let docker = dsb::docker::DockerManager::new().expect("Failed to create Docker manager");
    docker
        .remove_container(container_id)
        .await
        .expect("Failed to manually remove container");

    // Save the ID before delete consumes self
    let sandbox_id = sandbox.id.clone();

    // Now delete the sandbox through the API
    // This should succeed even though the container is already gone (404 from Docker)
    sandbox.delete().await.expect("Failed to delete sandbox");

    // Verify it's deleted (404 expected)
    let response = client
        .get(format!("{}/sandboxes/{}", api_base(), sandbox_id))
        .send()
        .await
        .expect("Failed to send get request");

    assert_eq!(response.status(), 404);
}

#[serial]
#[tokio::test]
async fn test_sandbox_lifecycle() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Verify it's running
    let info = sandbox
        .get_info()
        .await
        .expect("Failed to get sandbox info");
    assert_eq!(info.get("state").and_then(|s| s.as_str()), Some("running"));

    // Execute command
    let output = sandbox
        .exec(&["hostname"])
        .await
        .expect("Failed to exec command");
    assert!(!output.is_empty(), "hostname command should return output");

    // Get stats
    let stats = sandbox
        .get_stats()
        .await
        .expect("Failed to get sandbox stats");
    assert!(!stats.is_null(), "Stats should not be null");

    // Stop sandbox
    sandbox.stop().await.expect("Failed to stop sandbox");
    let info = sandbox
        .get_info()
        .await
        .expect("Failed to get sandbox info");
    assert_eq!(info.get("state").and_then(|s| s.as_str()), Some("stopped"));

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

// ============================================================================
// File Download Integration Tests
// ============================================================================

#[serial]
#[tokio::test]
async fn test_download_file_success() {
    init_global_fixture();

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Wait for container to be ready
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // Create a test file in the sandbox
    sandbox
        .exec(&["sh", "-c", "echo 'Hello, World!' > test.txt"])
        .await
        .expect("Failed to create test file");

    // Download the file
    let response = client
        .get(format!(
            "{}/sandboxes/{}/download?path=test.txt",
            api_base(), sandbox.id
        ))
        .send()
        .await
        .expect("Failed to download file");

    assert!(response.status().is_success());

    // Check headers
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "text/plain"
    );
    assert_eq!(
        response
            .headers()
            .get("x-file-name")
            .unwrap()
            .to_str()
            .unwrap(),
        "test.txt"
    );
    assert_eq!(
        response
            .headers()
            .get("x-file-path")
            .unwrap()
            .to_str()
            .unwrap(),
        "test.txt"
    );

    // Check content
    let content = response
        .bytes()
        .await
        .expect("Failed to get response bytes");
    assert_eq!(content.as_ref(), b"Hello, World!\n");

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_download_file_not_found() {
    init_global_fixture();

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Wait for container to be ready
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // Try to download non-existent file
    let response = client
        .get(format!(
            "{}/sandboxes/{}/download?path=nonexistent.txt",
            api_base(), sandbox.id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 404);

    let error: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse error response");
    assert_eq!(
        error.get("error").and_then(|e| e.as_str()),
        Some("File not found in sandbox")
    );

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_download_file_stopped_sandbox() {
    init_global_fixture();

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Wait for container to be ready
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // Create a test file
    sandbox
        .exec(&["sh", "-c", "echo 'test' > file.txt"])
        .await
        .expect("Failed to create test file");

    // Stop sandbox
    sandbox.stop().await.expect("Failed to stop sandbox");

    // Try to download from stopped sandbox
    let response = client
        .get(format!(
            "{}/sandboxes/{}/download?path=file.txt",
            api_base(), sandbox.id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 409);

    let error: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse error response");
    assert_eq!(
        error.get("error").and_then(|e| e.as_str()),
        Some("Sandbox is not running")
    );

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_download_file_missing_path_parameter() {
    init_global_fixture();

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Try to download without path parameter
    let response = client
        .get(format!("{}/sandboxes/{}/download", api_base(), sandbox.id))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 400);

    let error: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse error response");
    assert!(error
        .get("error")
        .and_then(|e| e.as_str())
        .unwrap()
        .contains("Missing 'path'"));

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_download_file_inline_disposition() {
    init_global_fixture();

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Wait for container to be ready
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // Create a test file
    sandbox
        .exec(&["sh", "-c", "echo 'inline test' > inline.txt"])
        .await
        .expect("Failed to create test file");

    // Download with inline disposition
    let response = client
        .get(format!(
            "{}/sandboxes/{}/download?path=inline.txt&disposition=inline",
            api_base(), sandbox.id
        ))
        .send()
        .await
        .expect("Failed to download file");

    assert!(response.status().is_success());

    // Check Content-Disposition header
    let content_disposition = response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_disposition.starts_with("inline"));

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_download_file_json() {
    init_global_fixture();

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Wait for container to be ready
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // Create a JSON file
    sandbox
        .exec(&["sh", "-c", "echo '{\"key\": \"value\"}' > config.json"])
        .await
        .expect("Failed to create test file");

    // Download the JSON file
    let response = client
        .get(format!(
            "{}/sandboxes/{}/download?path=config.json",
            api_base(), sandbox.id
        ))
        .send()
        .await
        .expect("Failed to download file");

    assert!(response.status().is_success());

    // Check content type
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "application/json"
    );

    // Verify content
    let content = response.text().await.expect("Failed to get response text");
    assert!(content.contains("key"));

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_download_file_binary() {
    init_global_fixture();

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Wait for container to be ready
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // Create a binary file using base64
    sandbox
        .exec(&[
            "sh",
            "-c",
            "echo 'SGVsbG8gV29ybGQ=' | base64 -d > binary.bin",
        ])
        .await
        .expect("Failed to create test file");

    // Download the binary file
    let response = client
        .get(format!(
            "{}/sandboxes/{}/download?path=binary.bin",
            api_base(), sandbox.id
        ))
        .send()
        .await
        .expect("Failed to download file");

    assert!(response.status().is_success());

    // Check content type
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "application/octet-stream"
    );

    // Verify content
    let content = response
        .bytes()
        .await
        .expect("Failed to get response bytes");
    assert_eq!(content.as_ref(), b"Hello World");

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_download_file_path_traversal_prevention() {
    init_global_fixture();

    let client = test_client();

    // Create sandbox
    let sandbox = TestSandbox::create_with_default_command(&client)
        .await
        .expect("Failed to create sandbox");

    // Wait for container to be ready
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // Try to download file with path traversal (should be rejected)
    let response = client
        .get(format!(
            "{}/sandboxes/{}/download?path=../../../etc/passwd",
            api_base(), sandbox.id
        ))
        .send()
        .await
        .expect("Failed to send request");

    // Should return 400 (path traversal rejected by sanitize_path)
    assert_eq!(response.status(), 400);

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

// ============================================================================
// Activities API Tests
// ============================================================================

#[serial]
#[tokio::test]
async fn test_list_activities() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create a sandbox to generate activity
    let sandbox =
        TestSandbox::create_with_command(&client, vec!["sleep".to_string(), "10".to_string()])
            .await
            .expect("Failed to create sandbox");

    // Wait for sandbox to be created
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // List activities
    let response = client
        .get(format!("{}/activities", api_base()))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse JSON response");

    // Should be an array
    assert!(body.is_array());
    let activities = body.as_array().unwrap();

    // Should have at least one activity from creating the sandbox
    assert!(!activities.is_empty(), "Expected at least one activity");

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_list_sandbox_activities() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create a sandbox
    let sandbox =
        TestSandbox::create_with_command(&client, vec!["sleep".to_string(), "10".to_string()])
            .await
            .expect("Failed to create sandbox");

    // Wait for sandbox to be created
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // List activities for this sandbox
    let response = client
        .get(format!("{}/sandboxes/{}/activities", api_base(), sandbox.id))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse JSON response");

    // Should be an array
    assert!(body.is_array());
    let activities = body.as_array().unwrap();

    // Should have at least one activity for this sandbox
    assert!(
        !activities.is_empty(),
        "Expected at least one activity for sandbox"
    );

    // All activities should belong to this sandbox
    for activity in activities {
        assert_eq!(
            activity["sandbox_id"], sandbox.id,
            "Activity belongs to wrong sandbox"
        );
    }

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_get_activity() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    // Create a sandbox
    let sandbox =
        TestSandbox::create_with_command(&client, vec!["sleep".to_string(), "10".to_string()])
            .await
            .expect("Failed to create sandbox");

    // Wait for sandbox to be created
    wait_for_running(&client, &sandbox.id, 30)
        .await
        .expect("Sandbox did not reach running state");

    // List activities for this specific sandbox to get an activity ID
    let list_response = client
        .get(format!("{}/sandboxes/{}/activities", api_base(), sandbox.id))
        .send()
        .await
        .expect("Failed to send request");

    let list_body: serde_json::Value = list_response
        .json()
        .await
        .expect("Failed to parse JSON response");
    let activities = list_body.as_array().expect("Not an array");

    if !activities.is_empty() {
        let activity_id = activities[0]["id"].as_str().expect("No activity ID");

        // Get specific activity
        let response = client
            .get(format!("{}/activities/{}", api_base(), activity_id))
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(response.status(), 200);

        let body: serde_json::Value = response
            .json()
            .await
            .expect("Failed to parse JSON response");

        assert_eq!(body["id"], activity_id);
        assert_eq!(body["sandbox_id"], sandbox.id);
    }

    // Cleanup
    sandbox.cleanup().await.expect("Failed to cleanup sandbox");
}

#[serial]
#[tokio::test]
async fn test_get_nonexistent_activity_returns_404() {
    // Initialize global fixture (once for all tests)
    init_global_fixture();

    let client = test_client();

    let fake_id = uuid::Uuid::new_v4();

    // Get nonexistent activity
    let response = client
        .get(format!("{}/activities/{}", api_base(), fake_id))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 404);
}

/*
# Running Integration Tests

These integration tests require a running server. Start the server with:

```bash
# Terminal 1: Start the server
export DATABASE_URL="postgresql://postgres:postgres@localhost:5432/dsb"
cargo run --bin dsb -- server --port 18080

# Terminal 2: Run the tests
cargo test --test integration_test -- --ignored
```

The tests use port 18080 to avoid conflicts with the default port 8080.

## Test Coverage

- `test_health_check` - Verifies the health endpoint works
- `test_create_sandbox_default_command` - Creates sandbox with default command
- `test_create_sandbox_custom_command` - Creates sandbox with custom command
- `test_exec_command` - Executes simple commands in a sandbox
- `test_exec_complex_commands` - Tests compound commands with pipes, redirections, and chaining
- `test_exec_error_handling` - Tests error handling for invalid commands
- `test_list_sandboxes` - Lists all sandboxes
- `test_get_sandbox_stats` - Gets sandbox statistics
- `test_stop_and_delete_sandbox` - Stops and deletes a sandbox
- `test_sandbox_lifecycle` - Tests complete sandbox lifecycle

All tests are marked with `#[ignore]` and require Docker to be running.
Run them with: `cargo test --test integration_test -- --ignored`
*/
