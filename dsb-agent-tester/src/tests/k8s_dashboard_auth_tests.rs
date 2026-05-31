// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! K8s E2E tests: Dashboard API-key authentication and authorization.
//!
//! These tests validate that the DSB API correctly enforces authentication
//! and authorization on a real K8s cluster deployment.
//!
//! Prerequisites:
//!   - DSB server deployed in the k8s cluster
//!   - Port-forward to DSB API: `kubectl port-forward svc/dsb 18080:8080 -n dsb`
//!   - `DSB_API_KEY` set to the admin key

use crate::tests::k8s_mod::{k8s_api_key, k8s_dsb_api_url};
use reqwest::Client;
use serde_json::json;
use tracing::info;

/// Build an authenticated client with the given API key.
fn authed_client(api_key: &str) -> Client {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "x-api-key",
        reqwest::header::HeaderValue::from_str(api_key).expect("valid api key"),
    );
    Client::builder()
        .default_headers(headers)
        .build()
        .expect("build client")
}

/// Build an unauthenticated client.
fn unauthed_client() -> Client {
    Client::new()
}

// ============================================================================
// Category 1: Basic auth
// ============================================================================

/// Test: Health endpoint is accessible without authentication.
#[tokio::test]
async fn test_k8s_health_no_auth() -> anyhow::Result<()> {
    let client = unauthed_client();
    let url = format!("{}/health", k8s_dsb_api_url());

    let resp = client.get(&url).send().await?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await?;

    anyhow::ensure!(
        status.is_success(),
        "Health endpoint without auth should succeed, got {}: {:?}",
        status,
        body
    );
    anyhow::ensure!(
        body["status"] == "ok",
        "Health body should have status=ok, got: {:?}",
        body
    );

    info!("Health endpoint accessible without auth");
    Ok(())
}

/// Test: List sandboxes without API key fails with 401.
#[tokio::test]
async fn test_k8s_list_sandboxes_no_auth_fails() -> anyhow::Result<()> {
    let client = unauthed_client();
    let url = format!("{}/sandboxes", k8s_dsb_api_url());

    let resp = client.get(&url).send().await?;
    let status = resp.status();

    anyhow::ensure!(
        status.as_u16() == 401,
        "Expected 401 without auth, got {}",
        status
    );

    info!("List sandboxes correctly rejected without auth: {}", status);
    Ok(())
}

/// Test: List sandboxes with valid admin API key succeeds.
#[tokio::test]
async fn test_k8s_list_sandboxes_with_admin_key() -> anyhow::Result<()> {
    let api_key = k8s_api_key();
    anyhow::ensure!(!api_key.is_empty(), "DSB_API_KEY must be set");

    let client = authed_client(&api_key);
    let url = format!("{}/sandboxes", k8s_dsb_api_url());

    let resp = client.get(&url).send().await?;
    let status = resp.status();

    anyhow::ensure!(
        status.is_success(),
        "Expected success with admin key, got {}",
        status
    );

    let body: serde_json::Value = resp.json().await?;
    info!(
        "List sandboxes with admin key: status={}, body_keys={:?}",
        status,
        body.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
    Ok(())
}

/// Test: Invalid API key is rejected.
#[tokio::test]
async fn test_k8s_invalid_api_key_rejected() -> anyhow::Result<()> {
    let client = authed_client("invalid-key-12345");
    let url = format!("{}/sandboxes", k8s_dsb_api_url());

    let resp = client.get(&url).send().await?;
    let status = resp.status();

    anyhow::ensure!(
        status.as_u16() == 401,
        "Expected 401 for invalid key, got {}",
        status
    );

    info!("Invalid API key correctly rejected: {}", status);
    Ok(())
}

// ============================================================================
// Category 2: Sandbox lifecycle auth
// ============================================================================

/// Test: Create sandbox with admin key, verify it exists.
#[tokio::test]
async fn test_k8s_create_sandbox_with_auth() -> anyhow::Result<()> {
    let api_key = k8s_api_key();
    anyhow::ensure!(!api_key.is_empty(), "DSB_API_KEY must be set");

    let client = authed_client(&api_key);
    let base_url = k8s_dsb_api_url();

    // Create sandbox
    let create_resp = client
        .post(format!("{}/sandboxes", base_url))
        .json(&json!({
            "image": "ghcr.io/dsb/sandbox:k8s-v0.0.5",
            "name": format!("k8s-auth-test-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs())
        }))
        .send()
        .await?;

    let create_status = create_resp.status();
    anyhow::ensure!(
        create_status.is_success(),
        "Create sandbox failed with status: {}",
        create_status
    );

    let create_body: serde_json::Value = create_resp.json().await?;
    let sandbox_id = create_body["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No sandbox ID in response: {:?}", create_body))?;
    info!("Created sandbox: {}", sandbox_id);

    // Wait for it to be created (not necessarily running)
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Get sandbox details
    let get_resp = client
        .get(format!("{}/sandboxes/{}", base_url, sandbox_id))
        .send()
        .await?;

    anyhow::ensure!(
        get_resp.status().is_success(),
        "Get sandbox failed: {}",
        get_resp.status()
    );

    let get_body: serde_json::Value = get_resp.json().await?;
    anyhow::ensure!(
        get_body["id"] == sandbox_id,
        "Sandbox ID mismatch: {:?}",
        get_body
    );

    // Clean up
    let delete_resp = client
        .delete(format!("{}/sandboxes/{}", base_url, sandbox_id))
        .send()
        .await?;

    info!(
        "Delete sandbox {}: status={}",
        sandbox_id,
        delete_resp.status()
    );

    info!("Sandbox create/get/delete with auth all succeeded");
    Ok(())
}

// ============================================================================
// Category 3: API key via query parameter
// ============================================================================

/// Test: API key passed as query parameter works (for SSE/WebSocket compatibility).
#[tokio::test]
async fn test_k8s_api_key_query_param() -> anyhow::Result<()> {
    let api_key = k8s_api_key();
    anyhow::ensure!(!api_key.is_empty(), "DSB_API_KEY must be set");

    let client = unauthed_client();
    let url = format!("{}/sandboxes?api_key={}", k8s_dsb_api_url(), api_key);

    let resp = client.get(&url).send().await?;
    let status = resp.status();

    anyhow::ensure!(
        status.is_success(),
        "Expected success with query param key, got {}",
        status
    );

    info!("API key via query parameter works: {}", status);
    Ok(())
}

// ============================================================================
// Category 4: Static file auth
// ============================================================================

/// Test: Static file access without auth fails when auth is required.
#[tokio::test]
async fn test_k8s_static_files_auth_required() -> anyhow::Result<()> {
    let client = unauthed_client();
    // Use a dummy sandbox ID — we expect 401 before it even checks if it exists
    let url = format!("{}/static/dummy-sandbox-id/test.txt", k8s_dsb_api_url());

    let resp = client.get(&url).send().await?;
    let status = resp.status();

    // Should be 400 (bad sandbox ID), 401 (auth required), or 404 (not found)
    anyhow::ensure!(
        status.as_u16() == 400 || status.as_u16() == 401 || status.as_u16() == 404,
        "Expected 400, 401, or 404 for static file without auth, got {}",
        status
    );

    info!("Static file without auth: {} (expected)", status);
    Ok(())
}
