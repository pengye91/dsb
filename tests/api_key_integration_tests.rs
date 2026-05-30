// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # API Key Authentication Integration Tests
//!
//! This module provides comprehensive integration tests for the API key authentication system.
//!
//! ## Architecture: Infrastructure-Agnostic Testing
//!
//! These tests read infrastructure configuration from [`TestInfraConfig`] (via environment
//! variables) so they can run against different deployments:
//!
//! ```text
//! Default (local docker-compose):
//!   ├── dsb-server-test (port 18080)
//!   ├── postgres-test (port 15432)
//!
//! EKS / External deployment:
//!   ├── kubectl port-forward svc/dsb 28080:8080
//!   ├── kubectl port-forward svc/postgres 15433:5432
//!   └── DSB_TEST_API_URL=http://127.0.0.1:28080
//! ```
//!
//! ## Test Scenarios
//!
//! - Health endpoint always accessible without auth
//! - Auth enabled requires valid API key
//! - Admin API CRUD operations
//! - Admin API protected by admin key only
//! - Database key validation
//! - Multiple concurrent API keys
//! - last_used_at timestamp updates
//!
//! ## Prerequisites
//!
//! - Docker daemon must be running (for local docker-compose)
//! - Target API server and database must be reachable
//! - Tests must run sequentially (`--test-threads=1`)
//!
//! ## Running Tests
//!
//! ```bash
//! # Default: against local docker-compose stack
//! docker compose -f docker-compose.test.yml up -d dsb-server-test postgres-test
//! cargo test --test api_key_integration_tests -- --test-threads=1
//!
//! # Against EKS with port-forwards
//! export DSB_TEST_API_URL=http://127.0.0.1:28080
//! export DSB_TEST_DATABASE_URL=postgresql://postgres:pass@127.0.0.1:15433/dsb
//! export DSB_TEST_API_KEY=test-admin-key-for-testing-only
//! cargo test --test api_key_integration_tests -- --test-threads=1
//! ```

use dsb::db::{ApiKeyStore, CreateApiKeyRequest, PostgresApiKeyStore};
use reqwest::Client;
use serde_json::json;
use uuid::Uuid;

mod common;
use common::TestDatabase;

/// Test fixture for API key authentication tests
///
/// Reads infrastructure configuration from [`TestInfraConfig`] so tests can
/// run against local docker-compose, EKS, or other deployments.
struct TestFixture {
    db: Option<TestDatabase>, // Database connection (initialized in tests)
    client: Client,
    admin_api_key: String,
    server_host: String,
    server_port: u16,
}

impl TestFixture {
    /// Initialize test fixture
    ///
    /// Reads infrastructure configuration from environment variables via
    /// [`TestInfraConfig`]. Defaults target the local docker-compose stack;
    /// set `DSB_TEST_API_URL` to test against EKS or other deployments.
    fn new() -> Self {
        let config = common::test_config::TestInfraConfig::from_env();
        let (server_host, server_port) = config.api_host_port();

        TestFixture {
            db: None, // Will be initialized in tests
            client: Client::new(),
            admin_api_key: config.api_key,
            server_host,
            server_port,
        }
    }

    /// Create an API key via direct database access (for testing)
    async fn create_api_key_direct(&self, name: &str) -> String {
        let pool = self.db.as_ref().unwrap().pool.clone();
        let store = PostgresApiKeyStore::new(pool);

        let req = CreateApiKeyRequest {
            name: name.to_string(),
            description: Some(format!("Test key for {}", name)),
            scopes: None,
            expires_in_days: None,
            created_by: Some("integration_test".to_string()),
        };

        let response = store
            .create_api_key(req)
            .await
            .expect("Failed to create API key");

        response.api_key
    }

    /// Cleanup API keys from previous test runs
    async fn cleanup_api_keys(&self) {
        // Use the existing database connection from the fixture
        if let Some(ref db) = self.db {
            let pool = db.pool.clone();
            if let Ok(client) = pool.get().await {
                // Delete keys from previous test runs
                let _ = client
                    .execute("DELETE FROM api_keys WHERE name LIKE ANY(ARRAY['test_%', 'list_%', 'delete_%', 'rotate_%', 'case_%', 'admin_%'])", &[])
                    .await;
            }
        }
    }

    /// Make a GET request to the health endpoint
    async fn get_health(&self) -> reqwest::Response {
        self.client
            .get(format!(
                "http://{}:{}/health",
                self.server_host, self.server_port
            ))
            .send()
            .await
            .expect("Failed to send request")
    }

    /// Make a GET request to list sandboxes
    async fn list_sandboxes(&self, api_key: Option<&str>) -> reqwest::Response {
        let mut request = self.client.get(format!(
            "http://{}:{}/sandboxes",
            self.server_host, self.server_port
        ));

        if let Some(key) = api_key {
            request = request.header("X-API-Key", key);
        }

        request.send().await.expect("Failed to send request")
    }

    /// Create an API key via admin API
    async fn create_api_key_admin(&self, name: &str) -> (String, serde_json::Value) {
        let response = self
            .client
            .post(format!(
                "http://{}:{}/admin/api-keys",
                self.server_host, self.server_port
            ))
            .header("X-API-Key", &self.admin_api_key)
            .json(&json!({
                "name": name,
                "description": format!("Test key for {}", name),
                "scopes": ["sandbox:read", "sandbox:write"]
            }))
            .send()
            .await
            .expect("Failed to send request");

        let status = response.status();
        let text = response.text().await.expect("Failed to read response body");

        assert_eq!(
            status, 201,
            "Expected 201 Created, got {} with body: {}",
            status, text
        );

        let body: serde_json::Value = serde_json::from_str(&text).expect("Failed to parse JSON");

        let api_key = body["api_key"]
            .as_str()
            .expect("API key not found in response")
            .to_string();

        (api_key, body)
    }

    /// List all API keys via admin API
    async fn list_api_keys_admin(&self) -> serde_json::Value {
        let response = self
            .client
            .get(format!(
                "http://{}:{}/admin/api-keys",
                self.server_host, self.server_port
            ))
            .header("X-API-Key", &self.admin_api_key)
            .send()
            .await
            .expect("Failed to send request");

        let status = response.status();
        let text = response.text().await.expect("Failed to read response body");

        assert_eq!(
            status, 200,
            "Expected 200 OK, got {} with body: {}",
            status, text
        );

        serde_json::from_str(&text).expect("Failed to parse JSON")
    }

    /// Delete an API key via admin API
    async fn delete_api_key_admin(&self, id: Uuid) {
        let response = self
            .client
            .delete(format!(
                "http://{}:{}/admin/api-keys/{}",
                self.server_host, self.server_port, id
            ))
            .header("X-API-Key", &self.admin_api_key)
            .send()
            .await
            .expect("Failed to send request");

        let status = response.status();
        assert_eq!(status, 204, "Expected 204 No Content, got {}", status);
    }

    /// Rotate an API key via admin API
    async fn rotate_api_key_admin(&self, id: Uuid) -> String {
        let response = self
            .client
            .post(format!(
                "http://{}:{}/admin/api-keys/{}/rotate",
                self.server_host, self.server_port, id
            ))
            .header("X-API-Key", &self.admin_api_key)
            .send()
            .await
            .expect("Failed to send request");

        let status = response.status();
        let text = response.text().await.expect("Failed to read response body");

        assert_eq!(
            status, 200,
            "Expected 200 OK, got {} with body: {}",
            status, text
        );

        let body: serde_json::Value = serde_json::from_str(&text).expect("Failed to parse JSON");

        body["api_key"]
            .as_str()
            .expect("API key not found in response")
            .to_string()
    }
}

// ============================================================================
// Test: Health Endpoint Always Accessible
// ============================================================================

#[tokio::test]
#[serial_test::serial]
async fn test_health_endpoint_no_auth_required() {
    let fixture = TestFixture::new();

    // Initialize database connection
    let _db = TestDatabase::new()
        .await
        .expect("Failed to create test database. Make sure docker-compose services are running");

    // Health endpoint should work without API key even when auth is enabled
    let response = fixture.get_health().await;
    assert_eq!(response.status(), 200);
}

// ============================================================================
// Test: Authentication Enabled (docker-compose default)
// ============================================================================

#[tokio::test]
#[serial_test::serial]
async fn test_auth_enabled_requires_api_key() {
    let fixture = TestFixture::new();

    // Initialize database connection
    let _db = TestDatabase::new()
        .await
        .expect("Failed to create test database");

    // Should fail without API key
    let response = fixture.list_sandboxes(None).await;
    assert_eq!(response.status(), 401);
}

#[tokio::test]
#[serial_test::serial]
async fn test_auth_enabled_rejects_invalid_key() {
    let fixture = TestFixture::new();

    // Initialize database connection
    let _db = TestDatabase::new()
        .await
        .expect("Failed to create test database");

    // Should fail with invalid key
    let response = fixture.list_sandboxes(Some("invalid_key_xyz")).await;
    assert_eq!(response.status(), 401);
}

#[tokio::test]
#[serial_test::serial]
async fn test_auth_enabled_accepts_admin_key() {
    let fixture = TestFixture::new();

    // Initialize database connection
    let _db = TestDatabase::new()
        .await
        .expect("Failed to create test database");

    // Admin API key should work
    let response = fixture.list_sandboxes(Some(&fixture.admin_api_key)).await;
    assert_eq!(response.status(), 200);
}

// ============================================================================
// Test: Database API Keys
// ============================================================================

#[tokio::test]
#[serial_test::serial]
async fn test_database_api_key_validation() {
    let mut fixture = TestFixture::new();

    // Initialize database connection
    fixture.db = Some(
        TestDatabase::new()
            .await
            .expect("Failed to create test database"),
    );

    // Cleanup any existing API keys from previous test runs
    fixture.cleanup_api_keys().await;

    // Create an API key directly in database
    let api_key = fixture.create_api_key_direct("test_key_1").await;

    // Should work with database key
    let response = fixture.list_sandboxes(Some(&api_key)).await;
    assert_eq!(response.status(), 200);
}

#[tokio::test]
#[serial_test::serial]
async fn test_multiple_database_api_keys() {
    let mut fixture = TestFixture::new();

    // Initialize database connection
    fixture.db = Some(
        TestDatabase::new()
            .await
            .expect("Failed to create test database"),
    );

    // Cleanup any existing API keys from previous test runs
    fixture.cleanup_api_keys().await;

    // Create multiple API keys with unique names
    let uuid1 = Uuid::new_v4().to_string()[..8].to_string();
    let uuid2 = Uuid::new_v4().to_string()[..8].to_string();
    let uuid3 = Uuid::new_v4().to_string()[..8].to_string();

    let key1 = fixture
        .create_api_key_direct(&format!("test_key_1_{}", uuid1))
        .await;
    let key2 = fixture
        .create_api_key_direct(&format!("test_key_2_{}", uuid2))
        .await;
    let key3 = fixture
        .create_api_key_direct(&format!("test_key_3_{}", uuid3))
        .await;

    // All keys should work
    let response1 = fixture.list_sandboxes(Some(&key1)).await;
    let response2 = fixture.list_sandboxes(Some(&key2)).await;
    let response3 = fixture.list_sandboxes(Some(&key3)).await;

    assert_eq!(response1.status(), 200);
    assert_eq!(response2.status(), 200);
    assert_eq!(response3.status(), 200);
}

// ============================================================================
// Test: Admin API Endpoints
// ============================================================================

#[tokio::test]
#[serial_test::serial]
async fn test_admin_api_create_key() {
    let mut fixture = TestFixture::new();

    // Initialize database connection
    fixture.db = Some(
        TestDatabase::new()
            .await
            .expect("Failed to create test database"),
    );

    // Cleanup any existing API keys from previous test runs
    fixture.cleanup_api_keys().await;

    let (api_key, body) = fixture.create_api_key_admin("test_admin_key").await;

    // Verify API key format
    assert!(
        api_key.starts_with("dsb_pk_"),
        "API key should start with dsb_pk_"
    );
    assert_eq!(
        api_key.len(),
        39,
        "API key should be 39 chars (dsb_pk_ + 32)"
    );

    // Verify response structure
    assert_eq!(body["key"]["name"], "test_admin_key");
    assert!(body["key"]["id"].is_string());
    assert!(body["key"]["key_prefix"].is_string());
    assert_eq!(body["key"]["is_active"], true);
}

#[tokio::test]
#[serial_test::serial]
async fn test_admin_api_list_keys() {
    let mut fixture = TestFixture::new();

    // Initialize database connection
    fixture.db = Some(
        TestDatabase::new()
            .await
            .expect("Failed to create test database"),
    );

    // Cleanup any existing API keys from previous test runs
    fixture.cleanup_api_keys().await;

    // Create two API keys
    fixture.create_api_key_admin("list_test_1").await;
    fixture.create_api_key_admin("list_test_2").await;

    // List all keys
    let body = fixture.list_api_keys_admin().await;

    // Should have at least 2 keys (plus any created by other tests)
    assert!(body.as_array().is_some_and(|arr| arr.len() >= 2));
}

#[tokio::test]
#[serial_test::serial]
async fn test_admin_api_delete_key() {
    let mut fixture = TestFixture::new();

    // Initialize database connection
    fixture.db = Some(
        TestDatabase::new()
            .await
            .expect("Failed to create test database"),
    );

    // Cleanup any existing API keys from previous test runs
    fixture.cleanup_api_keys().await;

    // Create an API key
    let (_api_key, body) = fixture.create_api_key_admin("delete_test").await;
    let key_id: Uuid =
        serde_json::from_value(body["key"]["id"].clone()).expect("Failed to parse key ID");

    // Delete the key
    fixture.delete_api_key_admin(key_id).await;

    // Verify key is deleted (should return 404)
    let response = fixture
        .client
        .get(format!(
            "http://{}:{}/admin/api-keys/{}",
            fixture.server_host, fixture.server_port, key_id
        ))
        .header("X-API-Key", &fixture.admin_api_key)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 404);
}

#[tokio::test]
#[serial_test::serial]
async fn test_admin_api_rotate_key() {
    let mut fixture = TestFixture::new();

    // Initialize database connection
    fixture.db = Some(
        TestDatabase::new()
            .await
            .expect("Failed to create test database"),
    );

    // Cleanup any existing API keys from previous test runs
    fixture.cleanup_api_keys().await;

    // Create an API key
    let (old_key, body) = fixture.create_api_key_admin("rotate_test").await;
    let key_id: Uuid =
        serde_json::from_value(body["key"]["id"].clone()).expect("Failed to parse key ID");

    // Rotate the key
    let new_key = fixture.rotate_api_key_admin(key_id).await;

    // Keys should be different
    assert_ne!(old_key, new_key, "Rotated key should be different");

    // Old key should no longer work
    let response = fixture.list_sandboxes(Some(&old_key)).await;
    assert_eq!(
        response.status(),
        401,
        "Old key should be invalid after rotation"
    );

    // New key should work
    let response = fixture.list_sandboxes(Some(&new_key)).await;
    assert_eq!(response.status(), 200, "New key should work");
}

// ============================================================================
// Test: Special Scenarios
// ============================================================================

#[tokio::test]
#[serial_test::serial]
async fn test_empty_api_key_header() {
    let fixture = TestFixture::new();

    // Initialize database connection
    let _db = TestDatabase::new()
        .await
        .expect("Failed to create test database");

    // Empty API key header should be rejected
    let response = fixture
        .client
        .get(format!(
            "http://{}:{}/sandboxes",
            fixture.server_host, fixture.server_port
        ))
        .header("X-API-Key", "")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 401);
}

#[tokio::test]
#[serial_test::serial]
async fn test_case_insensitive_header() {
    let mut fixture = TestFixture::new();

    // Initialize database connection
    fixture.db = Some(
        TestDatabase::new()
            .await
            .expect("Failed to create test database"),
    );

    // Cleanup any existing API keys from previous test runs
    fixture.cleanup_api_keys().await;

    // Create a test key with unique name to avoid duplicates
    let uuid = Uuid::new_v4().to_string()[..8].to_string();
    let api_key = fixture
        .create_api_key_direct(&format!("case_test_{}", uuid))
        .await;

    // Header should be case-insensitive (HTTP standard)
    let response = fixture
        .client
        .get(format!(
            "http://{}:{}/sandboxes",
            fixture.server_host, fixture.server_port
        ))
        .header("x-api-key", &api_key) // lowercase
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);
}
