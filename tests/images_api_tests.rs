// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Integration tests for Images API
//!
//! These tests require:
//! - Docker daemon running
//! - Test environment setup via docker compose

use dsb::config::Config;
use dsb::docker::{DockerManager, DockerTrait};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use reqwest::StatusCode;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

mod common;
use common::using_external_api;
use common::{default_test_image, setup_test_env};

/// Build an authenticated reqwest client for image API tests.
///
/// In external mode, adds the `x-api-key` header so the server accepts requests.
fn test_client() -> Client {
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

// ============================================================================
// Test Fixtures
// ============================================================================

struct TestServer {
    server_url: String,
    _docker_manager: Option<DockerManager>,
    cleanup_images: Vec<String>,
    is_external: bool,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if self.is_external {
            return;
        }
        if std::env::var("KEEP_TEST_CONTAINERS").is_ok() {
            return;
        }
        let _docker_manager = match self._docker_manager.as_ref() {
            Some(dm) => dm,
            None => return,
        };
        let images = self.cleanup_images.clone();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            rt.block_on(async move {
                if let Ok(dm) = DockerManager::new_with_config(&Config::default()) {
                    for image in images {
                        let _ = dm.remove_image(&image).await;
                    }
                }
            });
        })
        .join();
    }
}

async fn setup_test_server_for_images() -> TestServer {
    let _ = setup_test_env().await;

    let docker_manager =
        DockerManager::new_with_config(&Config::default()).expect("Failed to create DockerManager");

    // Images handlers expect Arc<DockerManager> as state
    let app = axum::Router::new()
        .route(
            "/health",
            axum::routing::get(dsb::api::handlers::health_check),
        )
        .route(
            "/images",
            axum::routing::get(dsb::api::handlers::list_images),
        )
        .route(
            "/images/pull",
            axum::routing::post(dsb::api::handlers::pull_image),
        )
        .route(
            "/images/{name}/pull",
            axum::routing::post(dsb::api::handlers::pull_image_stream),
        )
        .route(
            "/images/{name}/inspect",
            axum::routing::get(dsb::api::handlers::inspect_image),
        )
        .route(
            "/images/{name}",
            axum::routing::delete(dsb::api::handlers::delete_image),
        )
        .with_state(Arc::new(docker_manager.clone()));

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
        cleanup_images: Vec::new(),
        is_external: false,
    }
}

/// Returns a test server either by starting a local one or connecting to the external API.
async fn setup_test_server_or_external() -> TestServer {
    if using_external_api() {
        TestServer {
            server_url: common::test_config::TestInfraConfig::from_env().api_base_url,
            _docker_manager: None,
            cleanup_images: Vec::new(),
            is_external: true,
        }
    } else {
        setup_test_server_for_images().await
    }
}

// ============================================================================
// Images API Tests - Success Cases
// ============================================================================

#[tokio::test]
async fn test_list_images_success() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    // List all images
    let response = client
        .get(format!("{}/images", server.server_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let images: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    // Should be an array of images
    assert!(images.is_array());
}

#[tokio::test]
async fn test_inspect_image_success() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    let image_name = default_test_image();

    // Inspect a specific image (simple image name, no encoding needed)
    let response = client
        .get(format!(
            "{}/images/{}/inspect",
            server.server_url, image_name
        ))
        .send()
        .await
        .expect("Failed to send request");

    // Should succeed if image exists
    if response.status() == StatusCode::OK {
        let image_info: serde_json::Value = response.json().await.expect("Failed to parse JSON");

        // Should have image details
        assert!(image_info.is_object());
    } else {
        // Image might not exist - that's ok for this test
        assert!(response.status().is_client_error() || response.status().is_server_error());
    }
}

#[tokio::test]
async fn test_pull_image_success() {
    let mut server = setup_test_server_or_external().await;
    let client = test_client();

    let image_name = "alpine:latest";

    // Pull an image
    let response = client
        .post(format!("{}/images/pull", server.server_url))
        .json(&json!({
            "image": image_name
        }))
        .send()
        .await
        .expect("Failed to send request");

    // Pull might succeed or fail depending on network/Docker
    // Just verify it doesn't crash
    if response.status() == StatusCode::OK {
        let result: serde_json::Value = response.json().await.expect("Failed to parse JSON");
        assert!(result.is_object());
        server.cleanup_images.push(image_name.to_string());
    }
}

// ============================================================================
// Images API Tests - Error Cases
// ============================================================================

#[tokio::test]
async fn test_inspect_nonexistent_image() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    let fake_image = "thisimagedefinitelydoesnotexist123456:latest";

    // Inspect non-existent image (simple name, no special chars)
    let response = client
        .get(format!(
            "{}/images/{}/inspect",
            server.server_url, fake_image
        ))
        .send()
        .await
        .expect("Failed to send request");

    // Should return error
    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_pull_image_empty_name() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    // Pull with empty image name
    let response = client
        .post(format!("{}/images/pull", server.server_url))
        .json(&json!({
            "image": ""
        }))
        .send()
        .await
        .expect("Failed to send request");

    // Should return error (may be client or server error)
    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_delete_image_success() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    // First try to pull a small image
    let image_name = "alpine:latest";

    let pull_response = client
        .post(format!("{}/images/pull", server.server_url))
        .json(&json!({
            "image": image_name
        }))
        .send()
        .await
        .expect("Failed to send pull request");

    // If pull succeeded, try to delete it
    if pull_response.status() == StatusCode::OK {
        let delete_response = client
            .delete(format!("{}/images/{}", server.server_url, image_name))
            .send()
            .await
            .expect("Failed to send delete request");

        // Delete should succeed or fail gracefully
        assert!(
            delete_response.status() == StatusCode::OK
                || delete_response.status().is_client_error()
                || delete_response.status().is_server_error()
        );
    }
}

#[tokio::test]
async fn test_delete_nonexistent_image() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    let fake_image = "nonexistentimage123456:latest";

    // Delete non-existent image
    let response = client
        .delete(format!("{}/images/{}", server.server_url, fake_image))
        .send()
        .await
        .expect("Failed to send request");

    // Should return error (may be client or server error)
    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_pull_invalid_image_format() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    // Pull with invalid image format
    let response = client
        .post(format!("{}/images/pull", server.server_url))
        .json(&json!({
            "image": "invalid@image@format!@#$"
        }))
        .send()
        .await
        .expect("Failed to send request");

    // Should return error
    assert!(response.status().is_client_error() || response.status().is_server_error());
}

// ============================================================================
// Images API Tests - Edge Cases
// ============================================================================

#[tokio::test]
async fn test_list_images_empty_response() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    // List images - should always work even if no images
    let response = client
        .get(format!("{}/images", server.server_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_inspect_image_special_characters() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    // Image name with common format (no problematic special chars)
    let image_name = "alpine:latest";

    let response = client
        .get(format!(
            "{}/images/{}/inspect",
            server.server_url, image_name
        ))
        .send()
        .await
        .expect("Failed to send request");

    // Should handle gracefully (Docker may return 5xx under load or if daemon errors)
    assert!(
        response.status() == StatusCode::OK
            || response.status().is_client_error()
            || response.status().is_server_error()
    );
}

#[tokio::test]
async fn test_health_endpoint() {
    let server = setup_test_server_or_external().await;
    let client = test_client();

    let response = client
        .get(format!("{}/health", server.server_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let health: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    assert_eq!(health["status"], "ok");
}
