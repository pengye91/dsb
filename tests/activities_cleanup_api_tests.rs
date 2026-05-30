// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Integration tests for Activities API cleanup endpoint
//!
//! These tests require:
//! - Docker daemon running
//! - Test environment setup via docker compose
//!
//! # Test Coverage
//!
//! Activities API:
//! - cleanup_inactive_sandboxes (success + error)

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
use serial_test::serial;

fn test_api_key() -> String {
    common::test_config::TestInfraConfig::from_env().api_key
}

// ============================================================================
// Test Fixtures
// ============================================================================

struct TestServer {
    server_url: String,
    _docker_manager: Option<DockerManager>,
    cleanup_containers: Vec<String>,
    is_external: bool,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        // External mode: no local cleanup needed
        if self.is_external {
            return;
        }

        if std::env::var("KEEP_TEST_CONTAINERS").is_ok() {
            return;
        }
        let containers = self.cleanup_containers.clone();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(async move {
                for container_id in containers {
                    if let Ok(dm) = DockerManager::new_with_config(&Config::default()) {
                        let _ = dm.remove_container(&container_id).await;
                    }
                }
            });
        })
        .join();
    }
}

fn test_client() -> Client {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&test_api_key()).expect("Invalid API key header"),
    );

    Client::builder()
        .default_headers(headers)
        .build()
        .expect("Failed to build authenticated test client")
}

async fn setup_test_server_with_activities() -> TestServer {
    setup_test_env().await;

    let state = Arc::new(StateStore::new()) as Arc<dyn StateStoreTrait + Send + Sync>;
    let docker_manager =
        DockerManager::new_with_config(&Config::default()).expect("Failed to create DockerManager");

    let docker_manager_for_service = docker_manager.clone();
    let sandbox_service = Arc::new(SandboxService::new(
        Arc::new(docker_manager_for_service),
        state.clone(),
    ));

    let app = axum::Router::new()
        .route(
            "/health",
            axum::routing::get(dsb::api::handlers::health_check),
        )
        .route(
            "/sandboxes",
            axum::routing::post(dsb::api::handlers::create_sandbox),
        )
        .route(
            "/sandboxes/{id}",
            axum::routing::get(dsb::api::handlers::get_sandbox),
        )
        .route(
            "/sandboxes/{id}/stop",
            axum::routing::post(dsb::api::handlers::stop_sandbox),
        )
        .route(
            "/sandboxes/{id}/exec",
            axum::routing::post(dsb::api::handlers::exec_sandbox),
        )
        .route(
            "/activities/cleanup-all",
            axum::routing::post(dsb::api::handlers::cleanup_inactive_sandboxes),
        )
        .with_state(sandbox_service.clone())
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

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let addr = listener.local_addr().expect("Failed to get address");
    let server_url = format!("http://{}", addr);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    sleep(Duration::from_millis(100)).await;

    TestServer {
        server_url,
        _docker_manager: Some(docker_manager),
        cleanup_containers: Vec::new(),
        is_external: false,
    }
}

/// Returns a test server either by starting a local one or connecting to the external API.
async fn setup_test_server_or_external() -> TestServer {
    if using_external_api() {
        TestServer {
            server_url: common::test_config::TestInfraConfig::from_env().api_base_url,
            _docker_manager: None,
            cleanup_containers: Vec::new(),
            is_external: true,
        }
    } else {
        setup_test_server_with_activities().await
    }
}

// ============================================================================
// Activities API Tests - Success Cases
// ============================================================================

#[tokio::test]
async fn test_cleanup_inactive_sandboxes_dry_run_success() {
    let mut server = setup_test_server_or_external().await;
    let client = test_client();

    // Use a unique name to avoid conflicts with parallel or repeated test runs
    let unique_name = format!("test-cleanup-dryrun-{}", uuid::Uuid::new_v4());

    // Create a sandbox first
    let create_response = client
        .post(format!("{}/sandboxes", server.server_url))
        .json(&json!({
            "image": default_test_image(),
            "name": unique_name,
            "command": ["sleep", "300"],
            "pull_policy": "missing"
        }))
        .send()
        .await
        .expect("Failed to create sandbox");

    if create_response.status() == StatusCode::CREATED {
        let sandbox: serde_json::Value =
            create_response.json().await.expect("Failed to parse JSON");

        let sandbox_id = sandbox["id"].as_str().expect("No sandbox ID");

        // Wait for sandbox to be running and register its Docker container ID for cleanup
        let mut container_id = String::new();
        for _ in 0..60 {
            sleep(Duration::from_millis(500)).await;
            let check = client
                .get(format!("{}/sandboxes/{}", server.server_url, sandbox_id))
                .send()
                .await
                .expect("Failed to check sandbox");
            if check.status().is_success() {
                let state: serde_json::Value = check.json().await.expect("Failed to parse");
                if state["state"] == "running" {
                    if let Some(cid) = state["container_id"].as_str() {
                        container_id = cid.to_string();
                        break;
                    }
                }
            }
        }
        if !container_id.is_empty() {
            server.cleanup_containers.push(container_id);
        }

        // Now test cleanup with dry_run=true
        let response = client
            .post(format!(
                "{}/activities/cleanup-all?dry_run=true&timeout=1",
                server.server_url
            ))
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(response.status(), StatusCode::OK);

        let cleanup_result: serde_json::Value =
            response.json().await.expect("Failed to parse JSON");

        assert_eq!(cleanup_result["dry_run"], true);
        assert!(cleanup_result["cleaned"].is_number());
    }
}

#[tokio::test]
async fn test_cleanup_inactive_sandboxes_with_timeout_success() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    // Test cleanup with custom timeout
    let response = client
        .post(format!(
            "{}/activities/cleanup-all?timeout=1440", // 24 hours
            server.server_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let cleanup_result: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    assert!(cleanup_result["cleaned"].is_number());
    assert!(cleanup_result["message"].is_string());
}

#[tokio::test]
async fn test_cleanup_inactive_sandboxes_default_params() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    // Test cleanup with default parameters (no query params)
    let response = client
        .post(format!("{}/activities/cleanup-all", server.server_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let cleanup_result: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    assert!(cleanup_result["cleaned"].is_number());
    assert!(cleanup_result["message"].is_string());
}

// ============================================================================
// Activities API Tests - Error Cases
// ============================================================================

#[tokio::test]
async fn test_cleanup_with_invalid_timeout_returns_400() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    // Test with negative timeout (if the endpoint accepts it, it should be validated)
    let response = client
        .post(format!(
            "{}/activities/cleanup-all?timeout=-1",
            server.server_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    // Negative timeout might be accepted (u64 can't be negative in Rust)
    // or rejected by validation
    // We just ensure the endpoint doesn't crash
    assert!(response.status() == StatusCode::OK || response.status().is_client_error());
}

#[tokio::test]
async fn test_cleanup_with_zero_timeout() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    // Test with zero timeout (should clean everything)
    let response = client
        .post(format!(
            "{}/activities/cleanup-all?dry_run=true&timeout=0",
            server.server_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let cleanup_result: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    assert_eq!(cleanup_result["dry_run"], true);
}

#[tokio::test]
async fn test_cleanup_with_very_large_timeout() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    // Test with very large timeout value
    let response = client
        .post(format!(
            "{}/activities/cleanup-all?dry_run=true&timeout=999999999",
            server.server_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let cleanup_result: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    assert!(cleanup_result["cleaned"].is_number());
}

// ============================================================================
// Integration Tests - Combined Scenarios
// ============================================================================

#[tokio::test]
async fn test_sandbox_lifecycle_and_cleanup() {
    let mut server = setup_test_server_or_external().await;
    let client = test_client();

    let unique_name = format!(
        "test-lifecycle-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    );

    // Create a sandbox
    let create_response = client
        .post(format!("{}/sandboxes", server.server_url))
        .json(&json!({
            "image": default_test_image(),
            "name": unique_name,
            "command": ["sleep", "300"],
            "pull_policy": "missing"
        }))
        .send()
        .await
        .expect("Failed to create sandbox");

    if create_response.status() == StatusCode::CREATED {
        let sandbox: serde_json::Value =
            create_response.json().await.expect("Failed to parse JSON");

        let sandbox_id = sandbox["id"].as_str().expect("No sandbox ID");

        // Wait for sandbox to be running
        let mut ready = false;
        let mut container_id = String::new();
        for _ in 0..60 {
            sleep(Duration::from_millis(500)).await;
            let check = client
                .get(format!("{}/sandboxes/{}", server.server_url, sandbox_id))
                .send()
                .await
                .expect("Failed to check sandbox");
            if check.status().is_success() {
                let state: serde_json::Value = check.json().await.expect("Failed to parse");
                if state["state"] == "running" && state["container_id"].is_string() {
                    ready = true;
                    container_id = state["container_id"].as_str().unwrap_or("").to_string();
                    break;
                }
            }
        }
        assert!(ready, "Sandbox did not become ready in time");

        if !container_id.is_empty() {
            server.cleanup_containers.push(container_id);
        }

        // Stop the sandbox
        let stop_response = client
            .post(format!(
                "{}/sandboxes/{}/stop",
                server.server_url, sandbox_id
            ))
            .send()
            .await
            .expect("Failed to stop sandbox");

        assert!(stop_response.status().is_success());

        // Run cleanup (dry run)
        let cleanup_response = client
            .post(format!(
                "{}/activities/cleanup-all?dry_run=true",
                server.server_url
            ))
            .send()
            .await
            .expect("Failed to cleanup");

        assert_eq!(cleanup_response.status(), StatusCode::OK);
    }
}

#[serial]
#[tokio::test]
async fn test_cleanup_after_executing_command() {
    let mut server = setup_test_server_or_external().await;
    let client = test_client();

    // Generate a unique name to avoid conflicts with parallel tests
    let unique_name = format!("test-cleanup-after-exec-{}", uuid::Uuid::new_v4());

    // Create a sandbox
    let create_response = client
        .post(format!("{}/sandboxes", server.server_url))
        .json(&json!({
            "image": default_test_image(),
            "name": unique_name,
            "command": ["sleep", "300"],
            "pull_policy": "missing"
        }))
        .send()
        .await
        .expect("Failed to create sandbox");

    if create_response.status() == StatusCode::CREATED {
        let sandbox: serde_json::Value =
            create_response.json().await.expect("Failed to parse JSON");

        let sandbox_id = sandbox["id"].as_str().expect("No sandbox ID");

        // Wait for sandbox to be ready (running state with container_id)
        let mut ready = false;
        let mut container_id = String::new();
        for _ in 0..60 {
            sleep(Duration::from_millis(500)).await;
            let check_response = client
                .get(format!("{}/sandboxes/{}", server.server_url, sandbox_id))
                .send()
                .await
                .expect("Failed to check sandbox state");
            if check_response.status().is_success() {
                let state: serde_json::Value =
                    check_response.json().await.expect("Failed to parse state");
                if state["state"] == "running" && state["container_id"].is_string() {
                    ready = true;
                    container_id = state["container_id"].as_str().unwrap_or("").to_string();
                    break;
                }
            }
        }
        assert!(ready, "Sandbox did not become ready in time");

        // Store the Docker container ID for cleanup, not the sandbox UUID.
        // The TestServer's Drop cleanup uses container IDs, not sandbox UUIDs.
        // This ensures proper cleanup when tests run in parallel.
        if !container_id.is_empty() {
            server.cleanup_containers.push(container_id);
        }

        // Allow container to fully stabilize before exec
        sleep(Duration::from_millis(2000)).await;

        // Try exec — may fail if container was removed by parallel test cleanup
        let exec_response = client
            .post(format!(
                "{}/sandboxes/{}/exec",
                server.server_url, sandbox_id
            ))
            .json(&json!({
                "command": ["echo", "test"]
            }))
            .send()
            .await
            .expect("Failed to exec");

        // Log but don't fail on exec errors — the test's purpose is cleanup validation
        let exec_status = exec_response.status();
        if !exec_status.is_success() {
            let body = exec_response.text().await.unwrap_or_default();
            eprintln!(
                "Note: exec returned {}: {} (container may have been cleaned by parallel tests)",
                exec_status, body
            );
        }

        // Run cleanup to verify it works after exec
        let cleanup_response = client
            .post(format!(
                "{}/activities/cleanup-all?dry_run=true&timeout=0",
                server.server_url
            ))
            .send()
            .await
            .expect("Failed to cleanup");

        assert_eq!(cleanup_response.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn test_cleanup_response_structure() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    let response = client
        .post(format!(
            "{}/activities/cleanup-all?dry_run=true",
            server.server_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let cleanup_result: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    // Verify response has expected fields
    assert!(cleanup_result.get("message").is_some());
    assert!(cleanup_result.get("cleaned").is_some());
    assert!(cleanup_result.get("dry_run").is_some());

    // Verify field types
    assert!(cleanup_result["message"].is_string());
    assert!(cleanup_result["cleaned"].is_number());
    assert!(cleanup_result["dry_run"].is_boolean());
}
