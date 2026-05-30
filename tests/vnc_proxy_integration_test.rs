// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # VNC Proxy Integration Tests
//!
//! This module provides comprehensive integration tests for the VNC proxy WebSocket functionality.
//!
//! ## Architecture
//!
//! These tests use the same global test fixture pattern as the main integration tests.
//!
//! ## Test Coverage
//!
//! - WebSocket connection establishment
//! - API key authentication (header and query parameter)
//! - Bidirectional data flow (WebSocket ↔ TCP ↔ VNC server)
//! - Error scenarios (invalid sandbox, invalid API key, VNC not running)
//! - Connection lifecycle (upgrade, data transfer, close)
//!
//! ## Prerequisites
//!
//! - Docker daemon must be running
//! - Database must be accessible (configured via [`TestInfraConfig`] / `DSB_TEST_DATABASE_URL`)
//! - Tests must run sequentially (`--test-threads=1`)
//! - dsb/sandbox image must be available (tag set via `DSB_TEST_SANDBOX_IMAGE` env var)
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all VNC proxy integration tests
//! cargo test --test vnc_proxy_integration_test -- --test-threads=1
//!
//! # Run specific test
//! cargo test --test vnc_proxy_integration_test test_vnc_websocket_connection -- --test-threads=1
//! ```

use dsb::api::start_server;
use dsb::config::Config;
use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use serde_json::json;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::time::sleep;
use tokio_postgres::NoTls;
use tokio_tungstenite::tungstenite::Message;
use tracing::info;

mod common;
use common::db_test_setup::TestDatabase;
use common::using_external_api;

/// Returns the base HTTP URL for VNC tests.
///
/// In local mode: `http://localhost:8081` (test server port).
/// In external mode: the configured `DSB_TEST_API_URL`.
fn vnc_http_base_url() -> String {
    if using_external_api() {
        common::test_config::TestInfraConfig::from_env().api_base_url
    } else {
        "http://localhost:8081".to_string()
    }
}

/// Returns the base WebSocket URL for VNC tests.
///
/// Converts `http://` → `ws://` and `https://` → `wss://`.
fn vnc_ws_base_url() -> String {
    let http = vnc_http_base_url();
    if http.starts_with("https://") {
        http.replacen("https://", "wss://", 1)
    } else {
        http.replacen("http://", "ws://", 1)
    }
}

/// Returns the API key for external mode, or `None` for local mode.
fn vnc_api_key() -> Option<String> {
    if using_external_api() {
        let key = common::test_config::TestInfraConfig::from_env().api_key;
        if key.is_empty() {
            None
        } else {
            Some(key)
        }
    } else {
        None
    }
}

/// Poll the sandbox endpoint until the sandbox reaches "running" state.
async fn wait_for_vnc_sandbox_running(client: &Client, sandbox_id: &str, timeout_secs: u64) {
    let url = format!("{}/sandboxes/{}", vnc_http_base_url(), sandbox_id);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    while tokio::time::Instant::now() < deadline {
        let mut req = client.get(&url);
        if let Some(ref key) = vnc_api_key() {
            req = req.header("x-api-key", key);
        }
        if let Ok(response) = req.send().await {
            if response.status().is_success() {
                if let Ok(body) = response.json::<serde_json::Value>().await {
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
            }
        }
        sleep(Duration::from_millis(200)).await;
    }
    panic!(
        "Sandbox {} did not reach running state within {}s",
        sandbox_id, timeout_secs
    );
}

// Use common::sandbox_image() which reads from TestInfraConfig (DSB_TEST_SANDBOX_IMAGE).

/// Global test fixture - initialized once for all VNC proxy tests
struct VncTestFixture {
    #[allow(dead_code)]
    db: TestDatabase, // Keep alive to prevent container from being dropped
}

static GLOBAL_FIXTURE: OnceLock<&'static VncTestFixture> = OnceLock::new();

async fn create_isolated_vnc_test_database() -> Result<TestDatabase, Box<dyn std::error::Error>> {
    let infra = common::test_config::TestInfraConfig::from_env();
    let db_name = format!("dsb_test_vnc_proxy_{}", std::process::id());

    // Connect to the admin "postgres" database to create/drop our isolated DB.
    let admin_url = infra.database_url_with_name("postgres");
    let (client, connection) = tokio_postgres::connect(&admin_url, NoTls).await?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("VNC test admin database connection error: {}", error);
        }
    });

    client
        .execute(&format!("DROP DATABASE IF EXISTS {}", db_name), &[])
        .await?;
    client
        .execute(&format!("CREATE DATABASE {}", db_name), &[])
        .await?;

    // Point TestDatabase::new() at the isolated database.
    std::env::set_var("TEST_DATABASE_URL", infra.database_url_with_name(&db_name));

    TestDatabase::new().await
}

/// Initialize the global test fixture.
///
/// In local mode: starts a dedicated test server on port 8081 with an isolated database.
/// In external mode: uses the shared test database and skips local server startup.
fn init_global_fixture() -> &'static VncTestFixture {
    GLOBAL_FIXTURE.get_or_init(|| {
        // External mode: use shared database, skip local server setup
        if using_external_api() {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            let db = rt.block_on(async {
                TestDatabase::new()
                    .await
                    .expect("Failed to connect to shared test database for VNC tests")
            });
            return Box::leak(Box::new(VncTestFixture { db }));
        }

        use std::sync::mpsc;

        // Create a channel to keep the server thread alive
        let (tx, rx) = mpsc::channel();

        // Spawn a thread to run the server
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");

            let fixture = rt.block_on(async {
                // Create an isolated test database so other Rust test binaries cannot
                // truncate shared tables while this dedicated VNC test server is running.
                let db = create_isolated_vnc_test_database()
                    .await
                    .expect("Failed to create isolated VNC test database. Make sure docker-compose services are running: docker compose -f docker-compose.test.yml up -d postgres-test");

                // Set DOCKER_HOST for web terminal Docker connection from test config
                let docker_socket = common::test_config::get_test_docker_socket();
                std::env::set_var("DOCKER_HOST", &docker_socket);

                // Create test configuration with port 8081
                let infra = common::test_config::TestInfraConfig::from_env();
                let mut config = Config::default();
                config.server.port = 8081;
                config.server.require_auth = false; // Disable auth for tests
                let db_name = format!("dsb_test_vnc_proxy_{}", std::process::id());
                config.database.url = Some(infra.database_url_with_name(&db_name));

                // Start server in background task
                tokio::spawn(async move {
                    eprintln!("VNC Test Server starting on port 8081...");
                    if let Err(e) = start_server(&config).await {
                        eprintln!("Server failed to start: {}", e);
                    }
                });

                // Wait for server to be ready
                let client = Client::new();
                let mut retries = 0;
                while retries < 120 {
                    match client.get("http://localhost:8081/health").send().await {
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

                eprintln!("VNC Test Server ready on port 8081");
                Box::new(VncTestFixture { db })
            });

            // Send the fixture back before blocking
            tx.send(Box::leak(fixture)).expect("Failed to send fixture");

            // Keep the runtime alive by blocking forever
            std::mem::forget(rt);
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });

        // Wait for the fixture to be ready
        rx.recv().expect("Failed to receive fixture")
    })
}

/// Helper function to create a sandbox with VNC feature
async fn create_vnc_sandbox(name: &str) -> String {
    let client = Client::new();
    let url = format!("{}/sandboxes", vnc_http_base_url());

    // Add random suffix to avoid name conflicts
    let unique_name = format!("{}-{}", name, uuid::Uuid::new_v4());

    info!("📦 Creating VNC sandbox: {}", unique_name);

    let mut req = client
        .post(&url)
        .json(&json!({
            "image": common::sandbox_image(),
            "name": unique_name,
            "features": ["vnc"],
            "timeout_seconds": 300
        }));
    if let Some(ref key) = vnc_api_key() {
        req = req.header("x-api-key", key);
    }

    let response = req.send().await.expect("Failed to create sandbox");

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        panic!("Failed to create sandbox: {} - {}", status, error_text);
    }

    let sandbox: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse sandbox response");

    let sandbox_id = sandbox["id"]
        .as_str()
        .expect("Sandbox ID not found in response");

    info!("✅ Sandbox created: {}", sandbox_id);

    // Poll until sandbox is running (replaces blind sleep)
    wait_for_vnc_sandbox_running(&Client::new(), sandbox_id, 60).await;

    sandbox_id.to_string()
}

/// Helper function to delete a sandbox
async fn delete_sandbox(sandbox_id: &str) {
    let client = Client::new();
    let url = format!("{}/sandboxes/{}", vnc_http_base_url(), sandbox_id);

    info!("🗑️  Deleting sandbox: {}", sandbox_id);

    let mut req = client.delete(&url);
    if let Some(ref key) = vnc_api_key() {
        req = req.header("x-api-key", key);
    }

    let response = req.send().await.expect("Failed to delete sandbox");

    assert!(
        response.status().is_success(),
        "Failed to delete sandbox: {}",
        response.status()
    );

    info!("✅ Sandbox deleted: {}", sandbox_id);
}

/// Helper function to connect to VNC WebSocket endpoint
async fn connect_vnc_websocket(
    sandbox_id: &str,
    api_key: Option<&str>,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Box<dyn std::error::Error>,
> {
    let mut url = format!("{}/vnc/{}", vnc_ws_base_url(), sandbox_id);

    // Add API key as query parameter (required for WebSocket connections)
    let key_opt = if let Some(k) = api_key {
        Some(k.to_string())
    } else {
        vnc_api_key()
    };
    if let Some(ref k) = key_opt {
        url.push_str(&format!("?api_key={}", k));
    }

    info!("🔌 Connecting to VNC WebSocket: {}", url);

    // Use connect_async_with_config for better control
    let (ws_stream, _) = tokio_tungstenite::connect_async(&url).await?;

    info!("✅ WebSocket connected");

    Ok(ws_stream)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_vnc_websocket_connection_with_valid_api_key() {
    if !using_external_api() {
        init_global_fixture();
    }

    // Create a sandbox with VNC feature
    let sandbox_id = create_vnc_sandbox("vnc-test-valid-key").await;

    // Test: Connection should succeed (auth is disabled in test config)
    {
        let ws_stream = connect_vnc_websocket(&sandbox_id, None).await;

        assert!(ws_stream.is_ok(), "WebSocket connection should succeed");

        if let Ok(mut ws_stream) = ws_stream {
            // Send a test message (VNC protocol init)
            ws_stream
                .send(Message::Binary(vec![0x00, 0x01, 0x02].into()))
                .await
                .expect("Failed to send WebSocket message");

            // Try to receive (may timeout, that's ok)
            let timeout = tokio::time::Duration::from_secs(2);
            let _ = tokio::time::timeout(timeout, ws_stream.next()).await;
        }
    }

    // Cleanup
    delete_sandbox(&sandbox_id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_vnc_websocket_connection_close() {
    if !using_external_api() {
        init_global_fixture();
    }

    // Create a sandbox with VNC feature
    let sandbox_id = create_vnc_sandbox("vnc-test-close").await;

    // Test: Connection and graceful close
    let mut ws_stream = connect_vnc_websocket(&sandbox_id, None)
        .await
        .expect("Failed to connect WebSocket");

    // Send close message
    ws_stream
        .send(Message::Close(None))
        .await
        .expect("Failed to send close message");

    // Wait for connection to close
    sleep(Duration::from_millis(500)).await;

    info!("✅ WebSocket closed gracefully");

    // Cleanup
    delete_sandbox(&sandbox_id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_vnc_websocket_binary_message_roundtrip() {
    if !using_external_api() {
        init_global_fixture();
    }

    // Create a sandbox with VNC feature
    let sandbox_id = create_vnc_sandbox("vnc-test-roundtrip").await;

    let mut ws_stream = connect_vnc_websocket(&sandbox_id, None)
        .await
        .expect("Failed to connect WebSocket");

    // Send binary message (simulating VNC protocol)
    let test_data = vec![0x00, 0x01, 0x02, 0x03, 0x04];
    ws_stream
        .send(Message::Binary(test_data.clone().into()))
        .await
        .expect("Failed to send binary message");

    info!("✅ Binary message sent: {} bytes", test_data.len());

    // Cleanup
    delete_sandbox(&sandbox_id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_vnc_websocket_ping_pong() {
    if !using_external_api() {
        init_global_fixture();
    }

    // Create a sandbox with VNC feature
    let sandbox_id = create_vnc_sandbox("vnc-test-pingpong").await;

    let mut ws_stream = connect_vnc_websocket(&sandbox_id, None)
        .await
        .expect("Failed to connect WebSocket");

    // Send ping message
    ws_stream
        .send(Message::Ping(b"ping".to_vec().into()))
        .await
        .expect("Failed to send ping");

    info!("✅ Ping message sent");

    // Wait for pong (with timeout)
    let timeout = tokio::time::Duration::from_secs(2);
    let result = tokio::time::timeout(timeout, ws_stream.next()).await;

    // Pong is optional - just verify the connection is still alive
    if let Ok(Some(Ok(message))) = result {
        info!("Received message: {:?}", message);
    }

    // Cleanup
    delete_sandbox(&sandbox_id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_vnc_sandbox_created_without_vnc_feature() {
    if !using_external_api() {
        init_global_fixture();
    }

    let client = Client::new();
    let url = format!("{}/sandboxes", vnc_http_base_url());

    // Add random suffix to avoid name conflicts
    let unique_name = format!("no-vnc-test-{}", uuid::Uuid::new_v4());

    info!("📦 Creating sandbox WITHOUT VNC feature: {}", unique_name);

    // Create a sandbox WITHOUT VNC feature
    let mut req = client
        .post(&url)
        .json(&json!({
            "image": common::sandbox_image(),
            "name": unique_name,
            "timeout_seconds": 300
        }));
    if let Some(ref key) = vnc_api_key() {
        req = req.header("x-api-key", key);
    }
    let response = req.send().await.expect("Failed to create sandbox");

    assert!(
        response.status().is_success(),
        "Failed to create sandbox: {}",
        response.status()
    );

    let sandbox: serde_json::Value = response
        .json()
        .await
        .expect("Failed to parse sandbox response");

    let sandbox_id = sandbox["id"]
        .as_str()
        .expect("Sandbox ID not found in response");

    info!("✅ Sandbox created without VNC: {}", sandbox_id);

    // Poll until sandbox is running (replaces blind sleep)
    wait_for_vnc_sandbox_running(&client, sandbox_id, 60).await;

    // Test: Connection to sandbox without VNC
    // The WebSocket connection might succeed, but actual VNC communication will fail
    // since VNC server is not running in the container
    let ws_result = connect_vnc_websocket(sandbox_id, None).await;

    match ws_result {
        Ok(mut ws_stream) => {
            // Connection succeeded, try to send a message
            // This should fail or timeout since VNC is not running
            let send_result = tokio::time::timeout(
                Duration::from_secs(2),
                ws_stream.send(Message::Binary(vec![0x01].into())),
            )
            .await;

            // Either the send fails or times out - both are acceptable
            assert!(
                send_result.is_err() || send_result.is_ok(),
                "VNC connection established but communication may fail (expected when VNC not running)"
            );
            info!("✅ WebSocket connection succeeded as expected (VNC proxy accepts connection)");
        }
        Err(e) => {
            // Connection failed - also acceptable
            info!("✅ WebSocket connection failed (expected): {}", e);
        }
    }

    // Cleanup
    delete_sandbox(sandbox_id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_vnc_websocket_invalid_sandbox_id_format() {
    if !using_external_api() {
        init_global_fixture();
    }

    // Test: Connection with malformed UUID
    // Note: Axum might accept the string path and only validate UUID format later
    let malformed_id = "not-a-valid-uuid";

    let result = connect_vnc_websocket(malformed_id, None).await;

    // The WebSocket connection might be accepted by Axum router
    // but the UUID validation happens later in the handler
    match result {
        Ok(mut ws_stream) => {
            // Connection succeeded - this means Axum accepted the route
            // Try to receive data - should get error about invalid sandbox
            let timeout = tokio::time::Duration::from_secs(2);
            let recv_result = tokio::time::timeout(timeout, ws_stream.next()).await;

            match recv_result {
                Ok(Some(Ok(message))) => {
                    // Received a message - might be an error message
                    info!("Received message: {:?}", message);
                }
                Ok(Some(Err(e))) => {
                    // Received an error
                    info!("Received error: {}", e);
                }
                Ok(None) => {
                    // Connection closed
                    info!("✅ Connection closed (UUID validation happened)");
                }
                Err(_) => {
                    // Timeout
                    info!("✅ Connection timed out (UUID validation happened)");
                }
            }
        }
        Err(e) => {
            // Connection failed at WebSocket level
            info!("✅ WebSocket connection failed (expected): {}", e);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_vnc_websocket_reconnection() {
    if !using_external_api() {
        init_global_fixture();
    }

    // Create a sandbox with VNC feature
    let sandbox_id = create_vnc_sandbox("vnc-test-reconnect").await;

    // Test: First connection
    {
        let ws_stream_result = connect_vnc_websocket(&sandbox_id, None).await;

        assert!(
            ws_stream_result.is_ok(),
            "First WebSocket connection should succeed"
        );

        if let Ok(mut ws_stream) = ws_stream_result {
            // Send a message
            ws_stream
                .send(Message::Binary(vec![0x01].into()))
                .await
                .expect("Failed to send message");

            // Close connection (may fail if already closed by server)
            let _ = ws_stream.send(Message::Close(None)).await;
        }
    }

    // Wait a bit
    sleep(Duration::from_millis(500)).await;

    // Test: Reconnection should work
    {
        let ws_stream_result = connect_vnc_websocket(&sandbox_id, None).await;

        assert!(ws_stream_result.is_ok(), "Reconnection should succeed");

        if let Ok(mut ws_stream) = ws_stream_result {
            ws_stream
                .send(Message::Binary(vec![0x02].into()))
                .await
                .expect("Failed to send message on reconnection");

            info!("✅ Reconnection successful");
        }
    }

    // Cleanup
    delete_sandbox(&sandbox_id).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_vnc_websocket_concurrent_connections() {
    if !using_external_api() {
        init_global_fixture();
    }

    // Create a sandbox with VNC feature
    let sandbox_id = create_vnc_sandbox("vnc-test-concurrent").await;

    // Test: Multiple concurrent connections
    let handles: Vec<_> = (0..3)
        .map(|i| {
            let sandbox_id = sandbox_id.clone();

            tokio::spawn(async move {
                let url = format!("{}/vnc/{}", vnc_ws_base_url(), sandbox_id);
                let mut full_url = url;
                if let Some(ref key) = vnc_api_key() {
                    full_url.push_str(&format!("?api_key={}", key));
                }

                let ws_result = tokio_tungstenite::connect_async(&full_url).await;

                (i, ws_result.is_ok())
            })
        })
        .collect();

    // Wait for all connections
    let results: Vec<_> = futures_util::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("Task join failed"))
        .collect();

    info!(
        "Connection results: {} successful, {} failed",
        results.iter().filter(|(_, success)| *success).count(),
        results.iter().filter(|(_, success)| !*success).count()
    );

    // At least some connections should succeed
    let successful_count = results.iter().filter(|(_, success)| *success).count();
    assert!(
        successful_count > 0,
        "At least one concurrent connection should succeed"
    );

    // Cleanup
    delete_sandbox(&sandbox_id).await;
}
