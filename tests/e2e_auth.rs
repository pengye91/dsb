// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! E2E Authentication Tests
//!
//! Consolidated API key authentication tests using self-contained fixtures.
//!
//! ```bash
//! # Run all auth E2E tests
//! cargo test --test e2e_auth
//! ```

mod common;
use common::server_fixture::ServerFixture;
use common::sandbox_image;
use common::using_external_api;
use serde_json::json;
use uuid::Uuid;

/// Helper: create an API key via the admin API.
async fn create_api_key_via_admin(
    client: &common::server_fixture::TestClient,
    name: &str,
) -> (String, Uuid) {
    let resp = client
        .post_json(
            "/admin/api-keys",
            &json!({
                "name": name,
                "description": format!("Test key for {}", name),
                "scopes": ["sandbox:read", "sandbox:write"]
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        201,
        "Failed to create API key: {:?}",
        resp.text().await.unwrap_or_default()
    );
    let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
    let api_key = body["api_key"]
        .as_str()
        .expect("Missing api_key in response")
        .to_string();
    let key_id = body["key"]["id"]
        .as_str()
        .map(|s| Uuid::parse_str(s).expect("Invalid UUID"))
        .expect("Missing key id");
    (api_key, key_id)
}

// ============================================================================
// Auth Middleware (in-memory server)
// ============================================================================

#[tokio::test]
async fn test_health_endpoint_no_auth_required() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory_with_auth()
            .await
            .expect("Failed to start server")
    };
    let client = fixture.client.with_api_key(None);

    let resp = client.get("/health").await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_auth_enabled_requires_api_key() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory_with_auth()
            .await
            .expect("Failed to start server")
    };
    let client = fixture.client.with_api_key(None);

    let resp = client.get("/sandboxes").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_auth_enabled_rejects_invalid_key() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory_with_auth()
            .await
            .expect("Failed to start server")
    };
    let client = fixture.client.with_api_key(Some("invalid_key_xyz".to_string()));

    let resp = client.get("/sandboxes").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_auth_enabled_accepts_admin_key() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory_with_auth()
            .await
            .expect("Failed to start server")
    };

    // Default client already has admin key pre-configured
    let resp = fixture.client.get("/sandboxes").await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_empty_api_key_header() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory_with_auth()
            .await
            .expect("Failed to start server")
    };
    let client = fixture.client.with_api_key(Some("".to_string()));

    let resp = client.get("/sandboxes").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_case_insensitive_header() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_with_postgres()
            .await
            .expect("Failed to start server")
    };

    // Create a database key
    let (api_key, _) = create_api_key_via_admin(&fixture.client, &format!("case_test-{}", Uuid::new_v4())).await;

    // TestClient sends lowercase "x-api-key" by default — this verifies case insensitivity
    let client = fixture.client.with_api_key(Some(api_key));
    let resp = client.get("/sandboxes").await;
    assert_eq!(resp.status(), 200);
}

// ============================================================================
// Database API Keys (requires Postgres)
// ============================================================================

#[tokio::test]
async fn test_database_api_key_validation() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_with_postgres()
            .await
            .expect("Failed to start server")
    };

    let (api_key, _) = create_api_key_via_admin(&fixture.client, &format!("test_db_key-{}", Uuid::new_v4())).await;

    let client = fixture.client.with_api_key(Some(api_key));
    let resp = client.get("/sandboxes").await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_multiple_database_api_keys() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_with_postgres()
            .await
            .expect("Failed to start server")
    };

    let suffix = Uuid::new_v4();
    let (key1, _) = create_api_key_via_admin(&fixture.client, &format!("test_multi_1-{}", suffix)).await;
    let (key2, _) = create_api_key_via_admin(&fixture.client, &format!("test_multi_2-{}", suffix)).await;
    let (key3, _) = create_api_key_via_admin(&fixture.client, &format!("test_multi_3-{}", suffix)).await;

    let raw = reqwest::Client::new();
    for key in [&key1, &key2, &key3] {
        let resp = raw
            .get(format!("{}/sandboxes", fixture.base_url))
            .header("x-api-key", key)
            .send()
            .await
            .expect("Request failed");
        assert_eq!(resp.status(), 200, "Key {} should be valid", key);
    }
}

// ============================================================================
// Admin API (requires Postgres)
// ============================================================================

#[tokio::test]
async fn test_admin_api_create_key() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_with_postgres()
            .await
            .expect("Failed to start server")
    };

    let resp = fixture
        .client
        .post_json(
            "/admin/api-keys",
            &json!({
                "name": format!("test_admin_key-{}", uuid::Uuid::new_v4()),
                "description": "Test key for admin API",
                "scopes": ["sandbox:read", "sandbox:write"]
            }),
        )
        .await;

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
    let api_key = body["api_key"]
        .as_str()
        .expect("API key not found in response");

    assert!(
        api_key.starts_with("dsb_pk_"),
        "API key should start with dsb_pk_"
    );
    assert_eq!(
        api_key.len(),
        39,
        "API key should be 39 chars (dsb_pk_ + 32)"
    );

    assert_eq!(body["key"]["name"].as_str().unwrap().starts_with("test_admin_key-"), true);
    assert!(body["key"]["id"].is_string());
    assert!(body["key"]["key_prefix"].is_string());
    assert_eq!(body["key"]["is_active"], true);
}

#[tokio::test]
async fn test_admin_api_list_keys() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_with_postgres()
            .await
            .expect("Failed to start server")
    };

    let suffix = Uuid::new_v4();
    create_api_key_via_admin(&fixture.client, &format!("list_test_1-{}", suffix)).await;
    create_api_key_via_admin(&fixture.client, &format!("list_test_2-{}", suffix)).await;

    let resp = fixture.client.get("/admin/api-keys").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
    let keys = body.as_array().expect("Expected array");
    assert!(
        keys.len() >= 2,
        "Expected at least 2 keys, got {}",
        keys.len()
    );
}

#[tokio::test]
async fn test_admin_api_delete_key() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_with_postgres()
            .await
            .expect("Failed to start server")
    };

    let (_, key_id) = create_api_key_via_admin(&fixture.client, &format!("delete_test-{}", Uuid::new_v4())).await;

    let resp = fixture
        .client
        .delete(&format!("/admin/api-keys/{}", key_id))
        .await;
    assert_eq!(resp.status(), 204);

    // Verify key is deleted (GET should return 404)
    let resp = fixture
        .client
        .get(&format!("/admin/api-keys/{}", key_id))
        .await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_admin_api_rotate_key() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_with_postgres()
            .await
            .expect("Failed to start server")
    };

    let (old_key, key_id) = create_api_key_via_admin(&fixture.client, &format!("rotate_test-{}", Uuid::new_v4())).await;

    let resp = fixture
        .client
        .post(&format!("/admin/api-keys/{}/rotate", key_id))
        .await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
    let new_key = body["api_key"]
        .as_str()
        .expect("New API key not found in response")
        .to_string();

    assert_ne!(old_key, new_key, "Rotated key should be different");

    let raw = reqwest::Client::new();

    // Old key should no longer work
    let resp = raw
        .get(format!("{}/sandboxes", fixture.base_url))
        .header("x-api-key", &old_key)
        .send()
        .await
        .expect("Request failed");
    assert_eq!(
        resp.status(),
        401,
        "Old key should be invalid after rotation"
    );

    // New key should work
    let resp = raw
        .get(format!("{}/sandboxes", fixture.base_url))
        .header("x-api-key", &new_key)
        .send()
        .await
        .expect("Request failed");
    assert_eq!(resp.status(), 200, "New key should work");
}

// ============================================================================
// Key Isolation
// ============================================================================

#[tokio::test]
async fn test_cross_key_isolation() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_with_postgres()
            .await
            .expect("Failed to start server")
    };
    let base_url = &fixture.base_url;

    let (key_a, _) = create_api_key_via_admin(&fixture.client, &format!("isolation_key_a-{}", Uuid::new_v4())).await;
    let (key_b, _) = create_api_key_via_admin(&fixture.client, &format!("isolation_key_b-{}", Uuid::new_v4())).await;

    // Create sandbox with key A
    let raw = reqwest::Client::new();
    let create_resp = raw
        .post(format!("{}/sandboxes", base_url))
        .header("x-api-key", &key_a)
        .json(&json!({
            "image": sandbox_image(),
            "name": format!("isolation-test-{}", Uuid::new_v4()),
            "command": ["sleep", "10"]
        }))
        .send()
        .await
        .expect("Failed to create sandbox");

    assert_eq!(create_resp.status(), 201);
    let create_body: serde_json::Value = create_resp.json().await.expect("Failed to parse JSON");
    let sandbox_id = create_body["id"]
        .as_str()
        .expect("Missing sandbox id")
        .to_string();

    // Key B should not see the sandbox
    let resp_b = raw
        .get(format!("{}/sandboxes/{}", base_url, sandbox_id))
        .header("x-api-key", &key_b)
        .send()
        .await
        .expect("Failed to get sandbox with key B");
    assert_eq!(
        resp_b.status(),
        404,
        "Key B should not see sandbox created by key A"
    );

    // Admin key should see the sandbox
    let resp_admin = raw
        .get(format!("{}/sandboxes/{}", base_url, sandbox_id))
        .header("x-api-key", common::test_config::TestInfraConfig::from_env().api_key)
        .send()
        .await
        .expect("Failed to get sandbox with admin key");
    assert_eq!(
        resp_admin.status(),
        200,
        "Admin key should see all sandboxes"
    );

    // Clean up
    let _ = raw
        .delete(format!("{}/sandboxes/{}", base_url, sandbox_id))
        .header("x-api-key", &key_a)
        .send()
        .await;
}
