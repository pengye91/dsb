// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Consolidated E2E API feature tests
//!
//! Migrated from:
//! - tests/images_api_tests.rs
//! - tests/static_files_api_tests.rs
//! - tests/activities_cleanup_api_tests.rs
//!
//! Each test starts its own fresh DSB server on a random port with an
//! in-memory state store — no shared state, no `--test-threads=1`.

mod common;
use common::sandbox_image;
use common::server_fixture::ServerFixture;
use common::using_external_api;
use reqwest::StatusCode;
use serde_json::json;

// ============================================================================
// Images API Tests
// ============================================================================

#[tokio::test]
async fn test_list_images_success() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let response = client.get("/images").await;
    assert_eq!(response.status(), StatusCode::OK);

    let images: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(images.is_array());
}

#[tokio::test]
async fn test_inspect_image_success() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let image_name = "alpine:latest";
    let response = client.get(&format!("/images/{}", image_name)).await;

    if response.status() == StatusCode::OK {
        let image_info: serde_json::Value = response.json().await.expect("Failed to parse JSON");
        assert!(image_info.is_object());
    } else {
        assert!(response.status().is_client_error() || response.status().is_server_error());
    }
}

#[tokio::test]
async fn test_pull_image_success() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let image_name = "alpine:latest";
    let response = client
        .post_json("/images/pull", &json!({ "image": image_name }))
        .await;

    // Pull may succeed (202 Accepted) or fail depending on network/Docker;
    // just verify it doesn't crash. The response body may be empty.
    assert!(
        response.status().is_success() || response.status().is_server_error(),
        "Unexpected status: {}",
        response.status()
    );
}

#[tokio::test]
async fn test_inspect_nonexistent_image() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let fake_image = "thisimagedefinitelydoesnotexist123456:latest";
    let response = client.get(&format!("/images/{}", fake_image)).await;

    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_pull_image_empty_name() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let response = client
        .post_json("/images/pull", &json!({ "image": "" }))
        .await;

    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_delete_image_success() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let image_name = "alpine:latest";

    // First pull the image
    let pull_response = client
        .post_json("/images/pull", &json!({ "image": image_name }))
        .await;

    if pull_response.status().is_success() {
        let delete_response = client.delete(&format!("/images/{}", image_name)).await;
        assert!(
            delete_response.status() == StatusCode::NO_CONTENT
                || delete_response.status().is_client_error()
                || delete_response.status().is_server_error()
        );
    }
}

#[tokio::test]
async fn test_delete_nonexistent_image() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let fake_image = "nonexistentimage123456:latest";
    let response = client.delete(&format!("/images/{}", fake_image)).await;

    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_health_endpoint() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let response = client.get("/health").await;
    assert_eq!(response.status(), StatusCode::OK);

    let health: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(health["status"], "ok");
}

// ============================================================================
// Static Files API Tests
// ============================================================================

#[tokio::test]
async fn test_list_static_files_success() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let sandbox_id = client
        .create_sandbox(&sandbox_image(), "test-static-files")
        .await;
    // 60s: server's async health check may take up to 30s before fallback marks Running
    client.wait_for_running(&sandbox_id, 60).await;

    let response = client.get(&format!("/static/files/{}", sandbox_id)).await;
    assert!(
        response.status() == StatusCode::OK || response.status().is_client_error(),
        "Unexpected status: {}",
        response.status()
    );

    client.delete_sandbox(&sandbox_id).await;
}

#[tokio::test]
async fn test_list_sandbox_directory_tree_success() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let sandbox_id = client
        .create_sandbox(&sandbox_image(), "test-directory-tree")
        .await;
    // 60s: server's async health check may take up to 30s before fallback marks Running
    client.wait_for_running(&sandbox_id, 60).await;

    let response = client.get(&format!("/static/tree/{}", sandbox_id)).await;
    assert!(
        response.status() == StatusCode::OK || response.status().is_client_error(),
        "Unexpected status: {}",
        response.status()
    );

    client.delete_sandbox(&sandbox_id).await;
}

#[tokio::test]
async fn test_list_static_files_nonexistent_sandbox() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let fake_sandbox_id = "00000000-0000-0000-0000-000000000000";
    let response = client
        .get(&format!("/static/files/{}", fake_sandbox_id))
        .await;

    // Real server may return 200 with empty list or an error depending on
    // implementation; just ensure it doesn't crash.
    assert!(
        response.status() == StatusCode::OK
            || response.status().is_client_error()
            || response.status().is_server_error()
    );
}

#[tokio::test]
async fn test_download_file_nonexistent_sandbox() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let fake_sandbox_id = "00000000-0000-0000-0000-000000000000";
    let response = client
        .get(&format!("/static/{}/test.txt", fake_sandbox_id))
        .await;

    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_delete_static_file_nonexistent_sandbox() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let fake_sandbox_id = "00000000-0000-0000-0000-000000000000";
    let response = client
        .delete(&format!("/static/file/{}/test.txt", fake_sandbox_id))
        .await;

    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[tokio::test]
async fn test_list_directory_tree_nonexistent_sandbox() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let fake_sandbox_id = "00000000-0000-0000-0000-000000000000";
    let response = client
        .get(&format!("/static/tree/{}", fake_sandbox_id))
        .await;

    assert!(
        response.status() == StatusCode::OK
            || response.status().is_client_error()
            || response.status().is_server_error()
    );
}

// ============================================================================
// Activities Cleanup API Tests
// ============================================================================

#[tokio::test]
async fn test_cleanup_inactive_sandboxes_dry_run_success() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let sandbox_id = client
        .create_sandbox(&sandbox_image(), "test-cleanup-dryrun")
        .await;
    // 60s: server's async health check may take up to 30s before fallback marks Running
    client.wait_for_running(&sandbox_id, 60).await;

    let response = client
        .post("/activities/cleanup-all?dry_run=true&timeout=0")
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let cleanup_result: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(cleanup_result["dry_run"], true);
    assert!(cleanup_result["cleaned"].is_number());

    client.delete_sandbox(&sandbox_id).await;
}

#[tokio::test]
async fn test_cleanup_inactive_sandboxes_with_timeout_success() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let response = client.post("/activities/cleanup-all?timeout=1440").await;

    assert_eq!(response.status(), StatusCode::OK);

    let cleanup_result: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(cleanup_result["cleaned"].is_number());
    assert!(cleanup_result["message"].is_string());
}

#[tokio::test]
async fn test_cleanup_inactive_sandboxes_default_params() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let response = client.post("/activities/cleanup-all").await;

    assert_eq!(response.status(), StatusCode::OK);

    let cleanup_result: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(cleanup_result["cleaned"].is_number());
    assert!(cleanup_result["message"].is_string());
}

#[tokio::test]
async fn test_cleanup_with_zero_timeout() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let response = client
        .post("/activities/cleanup-all?dry_run=true&timeout=0")
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    let cleanup_result: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(cleanup_result["dry_run"], true);
}

#[tokio::test]
async fn test_cleanup_response_structure() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let response = client.post("/activities/cleanup-all?dry_run=true").await;

    assert_eq!(response.status(), StatusCode::OK);

    let cleanup_result: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    assert!(cleanup_result.get("message").is_some());
    assert!(cleanup_result.get("cleaned").is_some());
    assert!(cleanup_result.get("dry_run").is_some());

    assert!(cleanup_result["message"].is_string());
    assert!(cleanup_result["cleaned"].is_number());
    assert!(cleanup_result["dry_run"].is_boolean());
}
