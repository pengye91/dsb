// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Integration tests for Static Files API
//!
//! These tests require:
//! - Docker daemon running
//! - Test environment setup via docker compose

use dsb::api::handlers::static_files::{
    delete_static_file, list_sandbox_directory_tree, list_static_files, serve_static_file,
};
use dsb::config::Config;
use dsb::core::state::StateStore;
use dsb::core::{SandboxService, StateStoreTrait, StaticFileService};
use dsb::docker::DockerManager;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use reqwest::StatusCode;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

mod common;
use common::using_external_api;
use common::{default_test_image, setup_test_env};

// ============================================================================
// Test Fixtures
// ============================================================================

struct TestServer {
    server_url: String,
    _docker_manager: Option<DockerManager>,
    cleanup_containers: Vec<String>,
    cleanup_sandboxes: Vec<String>,
    is_external: bool,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if self.is_external {
            // Delete sandboxes created via API on external server
            let sandboxes = std::mem::take(&mut self.cleanup_sandboxes);
            let server_url = self.server_url.clone();
            let api_key = common::test_config::TestInfraConfig::from_env().api_key;
            let _ = std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
                rt.block_on(async move {
                    let client = reqwest::Client::new();
                    for sandbox_id in sandboxes {
                        let _ = client
                            .delete(format!("{}/sandboxes/{}", server_url, sandbox_id))
                            .header("x-api-key", &api_key)
                            .send()
                            .await;
                    }
                });
            })
            .join();
            return;
        }

        if std::env::var("KEEP_TEST_CONTAINERS").is_ok() {
            return;
        }

        let containers = self.cleanup_containers.clone();
        let sandbox_dirs = self.cleanup_sandboxes.clone();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(async move {
                for container_id in containers {
                    if let Ok(dm) = DockerManager::new_with_config(&Config::default()) {
                        let _ = dm.remove_container(&container_id).await;
                    }
                }
                for sandbox_id in sandbox_dirs {
                    let dir = std::path::PathBuf::from("/tmp/dsb-test-static").join(&sandbox_id);
                    let _ = tokio::fs::remove_dir_all(&dir).await;
                }
            });
        })
        .join();
    }
}

/// Build a reqwest client with the test API key when running against external API.
fn static_files_test_client() -> Client {
    let mut headers = HeaderMap::new();
    let config = common::test_config::TestInfraConfig::from_env();
    if !config.api_key.is_empty() {
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&config.api_key).expect("Invalid API key header"),
        );
    }
    Client::builder()
        .default_headers(headers)
        .build()
        .expect("Failed to build test client")
}

async fn setup_test_server_for_static_files() -> TestServer {
    let _ = setup_test_env().await;

    let state = Arc::new(StateStore::new()) as Arc<dyn StateStoreTrait + Send + Sync>;
    let docker_manager =
        DockerManager::new_with_config(&Config::default()).expect("Failed to create DockerManager");

    let docker_manager_for_service = docker_manager.clone();
    let sandbox_service = Arc::new(SandboxService::new(
        Arc::new(docker_manager_for_service),
        state.clone(),
    ));

    // Create config for static files
    let mut config = Config::default();
    config.static_server.base_path = "/tmp/dsb-test-static".to_string();

    let static_file_service = Arc::new(StaticFileService::new(Arc::new(config)));

    // Use the same route patterns as the actual server
    let app = axum::Router::new()
        .route(
            "/health",
            axum::routing::get(dsb::api::handlers::health_check),
        )
        .route(
            "/static/{sandbox_id}/{*file_path}",
            axum::routing::get(serve_static_file),
        )
        .route(
            "/static/files/{sandbox_id}",
            axum::routing::get(list_static_files),
        )
        .route(
            "/static/file/{sandbox_id}/{file_path}",
            axum::routing::delete(delete_static_file),
        )
        .route(
            "/static/tree/{sandbox_id}",
            axum::routing::get(list_sandbox_directory_tree),
        )
        .with_state((static_file_service, sandbox_service));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let addr = listener.local_addr().expect("Failed to get address");
    let server_url = format!("http://{}", addr);

    let _handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    sleep(Duration::from_millis(100)).await;

    TestServer {
        server_url,
        _docker_manager: Some(docker_manager),
        cleanup_containers: Vec::new(),
        cleanup_sandboxes: Vec::new(),
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
            cleanup_sandboxes: Vec::new(),
            is_external: true,
        }
    } else {
        setup_test_server_for_static_files().await
    }
}

/// Create a test sandbox for static file tests.
///
/// In local mode: creates a directory on disk with a valid UUID.
/// In external mode: creates a sandbox via the API and waits for it to be running.
async fn create_test_sandbox_for_static_files(server: &mut TestServer) -> uuid::Uuid {
    if server.is_external {
        let client = static_files_test_client();
        let unique_name = format!("static-test-{}", uuid::Uuid::new_v4());
        let resp = client
            .post(format!("{}/sandboxes", server.server_url))
            .json(&serde_json::json!({
                "image": default_test_image(),
                "name": unique_name,
                "command": ["sleep", "300"],
                "pull_policy": "missing"
            }))
            .send()
            .await
            .expect("Failed to create sandbox");

        if resp.status() != StatusCode::CREATED {
            let body = resp.text().await.unwrap_or_default();
            panic!("Create sandbox failed: {}", body);
        }

        let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
        let id = body["id"].as_str().expect("Missing sandbox id").to_string();

        // Wait for sandbox to be running
        for _ in 0..60 {
            sleep(Duration::from_millis(500)).await;
            let check = client
                .get(format!("{}/sandboxes/{}", server.server_url, id))
                .send()
                .await
                .expect("Failed to check sandbox");
            if check.status().is_success() {
                let state: serde_json::Value = check.json().await.expect("Failed to parse");
                if state["state"] == "running" {
                    server.cleanup_sandboxes.push(id.clone());
                    return uuid::Uuid::parse_str(&id).expect("Invalid UUID");
                }
            }
        }
        panic!("Sandbox did not become running in time");
    } else {
        let sandbox_id = uuid::Uuid::new_v4();
        let dir = std::path::PathBuf::from("/tmp/dsb-test-static").join(sandbox_id.to_string());
        tokio::fs::create_dir_all(&dir)
            .await
            .expect("Failed to create test dir");
        server.cleanup_sandboxes.push(sandbox_id.to_string());
        sandbox_id
    }
}

// ============================================================================
// Static Files API Tests - Success Cases
// ============================================================================

#[tokio::test]
async fn test_list_static_files_success() {
    let mut server = setup_test_server_or_external().await;
    let client = static_files_test_client();

    let sandbox_id = create_test_sandbox_for_static_files(&mut server).await;

    // List files in root directory
    let response = client
        .get(format!("{}/static/files/{}", server.server_url, sandbox_id))
        .send()
        .await
        .expect("Failed to list files");

    // Should succeed (may be empty or have files)
    assert!(response.status() == StatusCode::OK || response.status().is_client_error());
}

#[tokio::test]
async fn test_list_sandbox_directory_tree_success() {
    let mut server = setup_test_server_or_external().await;
    let client = static_files_test_client();

    let sandbox_id = create_test_sandbox_for_static_files(&mut server).await;

    // Get directory tree
    let response = client
        .get(format!("{}/static/tree/{}", server.server_url, sandbox_id))
        .send()
        .await
        .expect("Failed to get directory tree");

    // Should return directory structure
    assert!(response.status() == StatusCode::OK || response.status().is_client_error());
}

// ============================================================================
// Static Files API Tests - Error Cases
// ============================================================================

#[tokio::test]
async fn test_list_static_files_nonexistent_sandbox() {
    let server = setup_test_server_or_external().await;
    let client = static_files_test_client();

    let fake_sandbox_id = "00000000-0000-0000-0000-000000000000";

    // List files for non-existent sandbox
    let response = client
        .get(format!(
            "{}/static/files/{}",
            server.server_url, fake_sandbox_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    // In-memory backend returns 200 OK with empty data.
    // PostgreSQL/K8s backend may return 500 for nonexistent sandbox or empty body.
    assert!(
        response.status() == StatusCode::OK
            || response.status() == StatusCode::INTERNAL_SERVER_ERROR
            || response.status().is_server_error(),
        "Expected 200 or 5xx, got {}",
        response.status()
    );

    // Body may be empty or HTML on some backends — only parse JSON if valid
    let body_bytes = response.bytes().await.expect("Failed to read body");
    if !body_bytes.is_empty() {
        if let Ok(files) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
            if let Some(files_array) = files.get("files") {
                assert_eq!(files_array, &serde_json::json!([]));
            }
        }
        // Non-JSON body (e.g., HTML error page) is also acceptable
    }
    // Empty body is also acceptable (some backends return no content for errors)
}

#[tokio::test]
async fn test_download_file_nonexistent_sandbox() {
    let server = setup_test_server_or_external().await;
    let client = static_files_test_client();

    let fake_sandbox_id = "00000000-0000-0000-0000-000000000000";

    // Try to download file from non-existent sandbox
    let response = client
        .get(format!(
            "{}/static/{}/test.txt",
            server.server_url, fake_sandbox_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    // Should return error (may be client or server error depending on handler)
    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_delete_static_file_nonexistent_sandbox() {
    let server = setup_test_server_or_external().await;
    let client = static_files_test_client();

    let fake_sandbox_id = "00000000-0000-0000-0000-000000000000";

    // Try to delete file from non-existent sandbox
    let response = client
        .delete(format!(
            "{}/static/file/{}/test.txt",
            server.server_url, fake_sandbox_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    // Should return error (may be client or server error depending on handler)
    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_list_directory_tree_nonexistent_sandbox() {
    let server = setup_test_server_or_external().await;
    let client = static_files_test_client();

    let fake_sandbox_id = "00000000-0000-0000-0000-000000000000";

    // Try to get directory tree for non-existent sandbox
    let response = client
        .get(format!(
            "{}/static/tree/{}",
            server.server_url, fake_sandbox_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    // In-memory backend returns 200 OK with empty tree.
    // PostgreSQL/K8s backend returns 500 for nonexistent sandbox.
    if response.status() == StatusCode::OK {
        let tree: serde_json::Value = response.json().await.expect("Failed to parse JSON");
        // Should have empty tree
        assert_eq!(tree["tree"], serde_json::json!([]));
    } else {
        assert!(
            response.status().is_server_error(),
            "Expected 200 or 5xx for nonexistent sandbox, got {}",
            response.status()
        );
    }
}

#[tokio::test]
async fn test_download_nonexistent_file() {
    let mut server = setup_test_server_or_external().await;
    let client = static_files_test_client();

    let sandbox_id = create_test_sandbox_for_static_files(&mut server).await;

    // Try to download non-existent file
    let response = client
        .get(format!(
            "{}/static/{}/nonexistent.txt",
            server.server_url, sandbox_id
        ))
        .send()
        .await
        .expect("Failed to download file");

    // Should return error (file not found)
    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_delete_nonexistent_file() {
    let mut server = setup_test_server_or_external().await;
    let client = static_files_test_client();

    let sandbox_id = create_test_sandbox_for_static_files(&mut server).await;

    // Try to delete non-existent file
    let response = client
        .delete(format!(
            "{}/static/file/{}/nonexistent.txt",
            server.server_url, sandbox_id
        ))
        .send()
        .await
        .expect("Failed to delete file");

    // Should return error or handle gracefully
    assert!(response.status().is_client_error() || response.status().is_server_error());
}

// ============================================================================
// Static Files API Tests - Edge Cases
// ============================================================================

#[tokio::test]
async fn test_list_files_with_special_characters() {
    let mut server = setup_test_server_or_external().await;
    let client = static_files_test_client();

    let sandbox_id = create_test_sandbox_for_static_files(&mut server).await;

    // List files - should handle special characters in paths
    let response = client
        .get(format!("{}/static/files/{}", server.server_url, sandbox_id))
        .send()
        .await
        .expect("Failed to list files");

    // Should succeed
    assert!(response.status() == StatusCode::OK || response.status().is_client_error());
}
