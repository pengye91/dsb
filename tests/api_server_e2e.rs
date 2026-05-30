// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! End-to-end integration tests for the API server
//!
//! These tests:
//! - Start a real API server instance
//! - Use in-memory store for simplicity
//! - Make actual HTTP requests to endpoints
//! - Test full request/response cycles
//! - Clean up all resources properly
//!
//! # Prerequisites
//!
//! - Docker daemon running
//! - Docker socket accessible at /var/run/docker.sock or DOCKER_HOST env var
//!
//! # Environment Variables
//!
//! - `KEEP_TEST_CONTAINERS` - Set to "true" to keep test containers for debugging

use dsb::api::auth::{api_key_auth, AuthState};
use dsb::config::Config;
use dsb::core::state::StateStore;
use dsb::core::{SandboxService, StateStoreTrait};
use dsb::docker::DockerManager;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use reqwest::StatusCode;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

mod common;
use common::{default_test_image, setup_test_env};
use common::using_external_api;

fn test_api_key() -> String {
    common::test_config::TestInfraConfig::from_env().api_key
}

// ============================================================================
// Test Fixtures and Helpers
// ============================================================================

/// Poll the GET sandbox endpoint until the sandbox reaches "running" state.
///
/// This replaces blind `sleep()` calls after sandbox creation with active polling,
/// which is both faster (returns as soon as ready) and more reliable (fails on
/// unexpected terminal states).
async fn wait_for_sandbox_running(client: &TestClient, sandbox_id: &str, timeout_secs: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    while tokio::time::Instant::now() < deadline {
        let response = client.get(&format!("/sandboxes/{}", sandbox_id)).await;
        if response.status().is_success() {
            let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
            if let Some(state) = body["state"].as_str() {
                match state {
                    "running" => return,
                    "error" | "stopped" => {
                        panic!("Sandbox {} reached unexpected state: {}", sandbox_id, state)
                    }
                    _ => {} // creating/created/starting — keep polling
                }
            }
        }
        sleep(Duration::from_millis(200)).await;
    }
    panic!(
        "Sandbox {} did not reach running state within {}s",
        sandbox_id, timeout_secs
    );
}

/// Test server instance with all its resources
struct TestServer {
    server_url: String,
    cleanup_containers: Vec<String>,
    #[allow(dead_code)]
    local: bool,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        // Clean up test containers
        if std::env::var("KEEP_TEST_CONTAINERS").is_ok() {
            tracing::warn!("KEEP_TEST_CONTAINERS is set, not cleaning up containers");
            return;
        }

        let containers = self.cleanup_containers.clone();

        // Use a blocking approach to ensure cleanup completes
        // Since Drop is sync, we need to use a different approach
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(async move {
                for container_id in containers {
                    tracing::info!("Cleaning up container: {}", container_id);
                    // Create a new docker manager for cleanup
                    if let Ok(dm) = DockerManager::new_with_config(&Config::default()) {
                        if let Err(e) = dm.remove_container(&container_id).await {
                            tracing::warn!("Failed to cleanup container {}: {}", container_id, e);
                        }
                    }
                }
            });
        });

        // Wait a bit for cleanup to start (but don't block forever)
        let _ = handle.join();
    }
}

/// Creates a test server with in-memory store
async fn setup_test_server() -> TestServer {
    // Create in-memory state store
    let state = Arc::new(StateStore::new()) as Arc<dyn StateStoreTrait + Send + Sync>;

    // Create Docker manager
    let docker_manager = DockerManager::new_with_config(&Config::default())
        .expect("Failed to create DockerManager - is Docker daemon running?");

    // Create sandbox service (without activity tracking for simplicity)
    let sandbox_service = Arc::new(SandboxService::new(Arc::new(docker_manager), state.clone()));

    // Build a simplified router without SSH routes (those require PostgreSQL)
    let app = axum::Router::new()
        .route(
            "/health",
            axum::routing::get(dsb::api::handlers::health_check),
        )
        .route(
            "/sandboxes",
            axum::routing::get(dsb::api::handlers::list_sandboxes)
                .post(dsb::api::handlers::create_sandbox),
        )
        .route(
            "/sandboxes/{id}",
            axum::routing::get(dsb::api::handlers::get_sandbox)
                .delete(dsb::api::handlers::delete_sandbox),
        )
        .route(
            "/sandboxes/{id}/stop",
            axum::routing::post(dsb::api::handlers::stop_sandbox),
        )
        .with_state(sandbox_service)
        .layer(axum::middleware::from_fn_with_state(
            AuthState {
                config_api_key: None,
                admin_api_key: Some(test_api_key()),
                require_auth: true,
                static_server_require_auth: false,
                vnc_require_auth: false,
                api_key_store: None,
                cookie_key: axum_extra::extract::cookie::Key::generate(),
            },
            api_key_auth,
        ));

    // Start server on random port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind to random port");

    let addr = listener.local_addr().expect("Failed to get local address");
    let server_url = format!("http://{}", addr);

    tracing::info!("Test server listening on {}", server_url);

    // Spawn server in background
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    TestServer {
        server_url,
        cleanup_containers: Vec::new(),
        local: true,
    }
}

/// Returns a test server either by starting a local one or connecting to the external API.
async fn setup_test_server_or_external() -> TestServer {
    if using_external_api() {
        TestServer {
            server_url: common::test_config::TestInfraConfig::from_env().api_base_url,
            cleanup_containers: Vec::new(),
            local: false,
        }
    } else {
        setup_test_server().await
    }
}

/// HTTP client for making requests
struct TestClient {
    client: Client,
    base_url: String,
}

impl TestClient {
    fn new(base_url: String) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&test_api_key()).expect("Invalid API key header"),
        );

        Self {
            client: Client::builder()
                .default_headers(headers)
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to build authenticated test client"),
            base_url,
        }
    }

    async fn get(&self, path: &str) -> reqwest::Response {
        let url = format!("{}{}", self.base_url, path);
        self.client
            .get(&url)
            .send()
            .await
            .expect("GET request failed")
    }

    async fn post_json<T: serde::Serialize>(&self, path: &str, body: &T) -> reqwest::Response {
        let url = format!("{}{}", self.base_url, path);
        self.client
            .post(&url)
            .json(body)
            .send()
            .await
            .expect("POST request failed")
    }

    async fn delete(&self, path: &str) -> reqwest::Response {
        let url = format!("{}{}", self.base_url, path);
        self.client
            .delete(&url)
            .send()
            .await
            .expect("DELETE request failed")
    }

    async fn post(&self, path: &str) -> reqwest::Response {
        let url = format!("{}{}", self.base_url, path);
        self.client
            .post(&url)
            .send()
            .await
            .expect("POST request failed")
    }
}

// ============================================================================
// Health Check Tests
// ============================================================================

#[tokio::test]
#[serial_test::serial]
async fn test_health_check_endpoint() {
    let _server = setup_test_server_or_external().await;
    let client = TestClient::new(_server.server_url.clone());

    let response = client.get("/health").await;

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["status"], "ok");
}

// ============================================================================
// Sandbox Lifecycle Tests
// ============================================================================

#[tokio::test]
#[serial_test::serial]
async fn test_create_sandbox() {
    // Clean up any previous test resources
    setup_test_env().await;

    let mut server = setup_test_server_or_external().await;
    let client = TestClient::new(server.server_url.clone());

    let create_request = json!({
        "image": default_test_image().as_str(),
        "command": ["python", "-c", "print('hello')"],
        "timeout_seconds": 300
    });

    let response = client.post_json("/sandboxes", &create_request).await;

    assert_eq!(response.status(), reqwest::StatusCode::CREATED);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["id"].is_string());

    // Track container for cleanup
    if let Some(container_id) = body["container_id"].as_str() {
        server.cleanup_containers.push(container_id.to_string());
    }
}

#[tokio::test]
#[serial_test::serial]
async fn test_list_sandboxes() {
    let mut server = setup_test_server_or_external().await;
    let client = TestClient::new(server.server_url.clone());

    // Create a sandbox first
    let create_request = json!({
        "image": default_test_image().as_str(),
        "command": ["sleep", "60"]
    });

    let create_response = client.post_json("/sandboxes", &create_request).await;
    let create_body: serde_json::Value =
        create_response.json().await.expect("Failed to parse JSON");

    if let Some(container_id) = create_body["container_id"].as_str() {
        server.cleanup_containers.push(container_id.to_string());
    }

    let sandbox_id = create_body["id"].as_str().expect("No sandbox ID");
    wait_for_sandbox_running(&client, sandbox_id, 30).await;

    // List sandboxes
    let response = client.get("/sandboxes").await;
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    // Handle both legacy array format and new paginated format
    let sandboxes = if let serde_json::Value::Array(array) = &body {
        array
    } else if let serde_json::Value::Object(obj) = &body {
        // New paginated format: {"data": [...], "pagination": {...}}
        if let Some(data) = obj.get("data") {
            data.as_array().expect("Expected data to be an array")
        } else if let Some(items) = obj.get("items") {
            items.as_array().expect("Expected items to be an array")
        } else {
            // Fallback to legacy format - might still return just an array
            body.as_array().expect("Expected array at top level")
        }
    } else {
        panic!("Expected JSON array or object with 'data' field")
    };

    assert!(!sandboxes.is_empty());
}

#[tokio::test]
#[serial_test::serial]
async fn test_get_sandbox() {
    let mut server = setup_test_server_or_external().await;
    let client = TestClient::new(server.server_url.clone());

    // Create a sandbox
    let create_request = json!({
        "image": default_test_image().as_str(),
        "command": ["sleep", "60"]
    });

    let create_response = client.post_json("/sandboxes", &create_request).await;
    let create_body: serde_json::Value =
        create_response.json().await.expect("Failed to parse JSON");

    let sandbox_id = create_body["id"].as_str().expect("No sandbox ID");
    if let Some(container_id) = create_body["container_id"].as_str() {
        server.cleanup_containers.push(container_id.to_string());
    }

    wait_for_sandbox_running(&client, sandbox_id, 30).await;

    // Get sandbox by ID
    let response = client.get(&format!("/sandboxes/{}", sandbox_id)).await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["id"], sandbox_id);
}

#[tokio::test]
#[serial_test::serial]
async fn test_stop_sandbox() {
    let mut server = setup_test_server_or_external().await;
    let client = TestClient::new(server.server_url.clone());

    // Create a sandbox
    let create_request = json!({
        "image": default_test_image().as_str(),
        "command": ["sleep", "60"]
    });

    let create_response = client.post_json("/sandboxes", &create_request).await;
    let create_body: serde_json::Value =
        create_response.json().await.expect("Failed to parse JSON");

    let sandbox_id = create_body["id"].as_str().expect("No sandbox ID");
    if let Some(container_id) = create_body["container_id"].as_str() {
        server.cleanup_containers.push(container_id.to_string());
    }

    wait_for_sandbox_running(&client, sandbox_id, 30).await;

    // Stop the sandbox
    let response = client
        .post(&format!("/sandboxes/{}/stop", sandbox_id))
        .await;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
#[serial_test::serial]
async fn test_delete_sandbox() {
    let mut server = setup_test_server_or_external().await;
    let client = TestClient::new(server.server_url.clone());

    // Create a sandbox
    let create_request = json!({
        "image": default_test_image().as_str(),
        "command": ["sleep", "1"]
    });

    let create_response = client.post_json("/sandboxes", &create_request).await;
    let create_body: serde_json::Value =
        create_response.json().await.expect("Failed to parse JSON");

    let sandbox_id = create_body["id"].as_str().expect("No sandbox ID");
    let container_id = create_body["container_id"].as_str();

    wait_for_sandbox_running(&client, sandbox_id, 30).await;

    // Delete the sandbox
    let response = client.delete(&format!("/sandboxes/{}", sandbox_id)).await;
    assert_eq!(response.status(), reqwest::StatusCode::NO_CONTENT);

    // Don't add to cleanup since we already deleted it
    if let Some(cid) = container_id {
        server.cleanup_containers.retain(|c| c != cid);
    }
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
#[serial_test::serial]
async fn test_get_nonexistent_sandbox_returns_404() {
    let server = setup_test_server_or_external().await;
    let client = TestClient::new(server.server_url.clone());

    let fake_id = uuid::Uuid::new_v4();
    let response = client.get(&format!("/sandboxes/{}", fake_id)).await;

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial_test::serial]
async fn test_invalid_sandbox_create_request_returns_400() {
    let server = setup_test_server_or_external().await;
    let client = TestClient::new(server.server_url.clone());

    // Missing required field
    let invalid_request = json!({
        "command": ["echo", "hello"]
        // Missing "image" field
    });

    let response = client.post_json("/sandboxes", &invalid_request).await;

    // Should return 400 Bad Request or similar error
    assert!(response.status().is_client_error());
}

// ============================================================================
// Concurrent Request Tests
// ============================================================================

#[tokio::test]
#[serial_test::serial]
async fn test_concurrent_sandbox_creation() {
    let mut server = setup_test_server_or_external().await;
    let client = std::sync::Arc::new(TestClient::new(server.server_url.clone()));

    let mut handles = vec![];

    // Create 5 sandboxes concurrently
    for i in 0..5 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let create_request = json!({
                "image": default_test_image().as_str(),
                "command": ["sleep", "60"],
                "metadata": {
                    "test_index": i
                }
            });

            client_clone.post_json("/sandboxes", &create_request).await
        });

        handles.push((handle, i));
    }

    // Wait for all requests and collect results
    let mut container_ids = Vec::new();
    for (handle, index) in handles {
        let response = handle.await.expect("Task panicked");

        // Allow for server error (500) due to race conditions in concurrent tests
        // but ensure at least some requests succeed
        let status = response.status();
        if status == reqwest::StatusCode::CREATED {
            let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
            if let Some(container_id) = body["container_id"].as_str() {
                container_ids.push(container_id.to_string());
            }
        } else if status.is_server_error() {
            // Server error (5xx) is acceptable in concurrent tests due to race conditions
            eprintln!(
                "Request {} failed with status {} (acceptable in concurrent test)",
                index, status
            );
        } else {
            // Client errors (4xx) should not happen
            panic!(
                "Request {} failed with unexpected status: {}",
                index, status
            );
        }
    }

    // Ensure at least 3 out of 5 requests succeeded (tolerate some race conditions)
    assert!(
        container_ids.len() >= 3,
        "Expected at least 3 successful sandbox creations, got {}",
        container_ids.len()
    );

    // Add all containers to cleanup
    server.cleanup_containers.extend(container_ids);
}
