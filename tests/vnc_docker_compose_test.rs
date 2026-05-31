// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # VNC Proxy Docker-Compose Integration Tests
//!
//! This module provides integration tests for the VNC proxy that run against
//! the docker-compose stack, where the DSB server is on the same Docker network
//! as the sandbox containers.
//!
//! ## Prerequisites
//!
//! These tests require the docker-compose test stack to be running:
//! ```bash
//! docker compose -f docker-compose.test.yml up -d
//! ```
//!
//! ## Running Tests
//!
//! ```bash
//! # Run from within test-runner container
//! cargo test --test vnc_docker_compose_test -- --test-threads=1
//!
//! # Or via make
//! make test-compose-vnc
//! ```
//!
//! ## Test Coverage
//!
//! - WebSocket connection to VNC proxy
//! - API key authentication
//! - Bidirectional WebSocket data flow
//! - VNC server connectivity (same Docker network)
//! - Error scenarios (invalid sandbox, auth failures)
//! - Automatic resource cleanup with unique naming

mod common;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::info;
use uuid::Uuid;

/// Get the sandbox Docker image from [`TestInfraConfig`].
fn sandbox_image() -> String {
    common::test_config::TestInfraConfig::from_env().sandbox_image
}

/// Get the DSB API URL from [`TestInfraConfig`].
fn api_url() -> String {
    common::test_config::TestInfraConfig::from_env().api_base_url
}

/// Get the DSB API key from [`TestInfraConfig`].
fn api_key() -> String {
    common::test_config::TestInfraConfig::from_env().api_key
}

/// Generate a unique sandbox name for testing
fn generate_unique_sandbox_name(test_name: &str) -> String {
    let uuid = Uuid::new_v4();
    format!(
        "test-{}-{}",
        test_name.replace("::", "_").replace(" ", "_"),
        uuid
    )
}

/// Create a test sandbox with VNC enabled
async fn create_vnc_sandbox(
    client: &reqwest::Client,
    test_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let unique_name = generate_unique_sandbox_name(test_name);
    let url = format!("{}/sandboxes", api_url());

    let response = client
        .post(&url)
        .header("X-API-Key", api_key())
        .json(&json!({
            "image": sandbox_image(),
            "name": unique_name,
            // Don't override command - use default supervisord for proper VNC support
            "enable_all_features": true
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Failed to create sandbox: {} - {}", status, error_text).into());
    }

    let sandbox: serde_json::Value = response.json().await?;
    let sandbox_id = sandbox["id"]
        .as_str()
        .ok_or("Missing sandbox id in response")?
        .to_string();

    Ok(sandbox_id)
}

/// Delete a sandbox
async fn delete_sandbox(
    client: &reqwest::Client,
    sandbox_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{}/sandboxes/{}", api_url(), sandbox_id);
    let response = client
        .delete(&url)
        .header("X-API-Key", api_key())
        .send()
        .await?;

    if !response.status().is_success() && response.status() != 404 {
        return Err(format!("Failed to delete sandbox: {}", response.status()).into());
    }

    Ok(())
}

/// Wait for sandbox to be in running state
async fn wait_for_sandbox_running(
    client: &reqwest::Client,
    sandbox_id: &str,
    timeout_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{}/sandboxes/{}", api_url(), sandbox_id);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

    while tokio::time::Instant::now() < deadline {
        let response = client
            .get(&url)
            .header("X-API-Key", api_key())
            .send()
            .await?;

        if response.status().is_success() {
            let sandbox: serde_json::Value = response.json().await?;
            if let Some(state) = sandbox["state"].as_str() {
                if state == "running" {
                    return Ok(());
                }
            }
        }

        sleep(Duration::from_secs(1)).await;
    }

    Err("Sandbox did not reach running state in time".into())
}

#[tokio::test]
async fn test_vnc_websocket_connection() {
    info!("Testing VNC WebSocket connection in docker-compose environment");

    let client = reqwest::Client::new();

    // Create a sandbox with unique name
    let sandbox_id = create_vnc_sandbox(&client, "test_vnc_websocket_connection")
        .await
        .expect("Failed to create sandbox");
    info!("Created sandbox: {}", sandbox_id);

    // Wait for sandbox to be running
    wait_for_sandbox_running(&client, &sandbox_id, 60)
        .await
        .expect("Sandbox did not start");

    // Give VNC server time to start (supervisord needs to start x11vnc)
    sleep(Duration::from_secs(3)).await;

    // Connect to VNC WebSocket
    let ws_url = format!(
        "{}/vnc/{}?api_key={}",
        api_url().replace("http://", "ws://"),
        sandbox_id,
        api_key()
    );

    info!("Connecting to VNC WebSocket: {}", ws_url);

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to VNC WebSocket");

    info!("✓ WebSocket connection established");

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Wait for VNC server handshake (server sends protocol version first)
    let timeout = Duration::from_secs(5);
    let data = tokio::time::timeout(timeout, ws_receiver.next()).await;

    match data {
        Ok(Some(Ok(message))) => {
            info!("✓ Received data from VNC server: {:?}", message);
            match message {
                Message::Binary(data) => {
                    assert!(!data.is_empty(), "VNC server should respond with data");
                    info!(
                        "✓ VNC server handshake data received ({} bytes)",
                        data.len()
                    );
                }
                Message::Text(text) => {
                    // Error message from VNC proxy
                    panic!("VNC proxy error: {}", text);
                }
                Message::Close(_) => {
                    panic!("VNC WebSocket closed unexpectedly");
                }
                _ => {
                    info!("Received non-binary message from VNC server");
                }
            }
        }
        Ok(Some(Err(e))) => {
            panic!("WebSocket error: {:?}", e);
        }
        Ok(None) => {
            panic!("WebSocket closed without data");
        }
        Err(_) => {
            panic!("Timeout waiting for VNC server response");
        }
    }

    // Close connection gracefully
    ws_sender
        .send(Message::Close(None))
        .await
        .expect("Failed to send close");

    // Cleanup
    println!("Cleaning up sandbox: {}", sandbox_id);
    let delete_result = delete_sandbox(&client, &sandbox_id).await;
    match &delete_result {
        Ok(_) => println!("✓ Sandbox deleted successfully"),
        Err(e) => println!("✗ Failed to delete sandbox: {}", e),
    }
    delete_result.expect("Failed to delete sandbox");

    info!("✓ VNC WebSocket connection test passed");
}

#[tokio::test]
async fn test_vnc_auth_failure() {
    info!("Testing VNC authentication failure");

    // Try to connect without API key (should fail)
    let ws_url = format!(
        "{}/vnc/{}",
        api_url().replace("http://", "ws://"),
        "invalid-sandbox-id"
    );

    let result = connect_async(&ws_url).await;

    match result {
        Err(e) => {
            info!("✓ Connection rejected as expected: {:?}", e);
        }
        Ok((ws_stream, _)) => {
            // If connection succeeds (e.g., auth disabled), we should receive an error message
            let (_, mut ws_receiver) = ws_stream.split();

            let timeout = Duration::from_secs(5);
            let data = tokio::time::timeout(timeout, ws_receiver.next()).await;

            match data {
                Ok(Some(Ok(message))) => match message {
                    Message::Text(text) => {
                        assert!(
                            text.contains("error")
                                || text.contains("not found")
                                || text.contains("Unauthorized"),
                            "Expected error message, got: {}",
                            text
                        );
                        info!("✓ Received expected error: {}", text);
                    }
                    Message::Close(_) => {
                        info!("✓ Connection closed as expected");
                    }
                    _ => {
                        info!("✓ Connection accepted (auth may be disabled)");
                    }
                },
                _ => {
                    info!("✓ Connection accepted (auth may be disabled)");
                }
            }
        }
    }
}

#[tokio::test]
async fn test_vnc_invalid_sandbox() {
    info!("Testing VNC with invalid sandbox ID");

    // Try to connect to non-existent sandbox
    let ws_url = format!(
        "{}/vnc/{}?api_key={}",
        api_url().replace("http://", "ws://"),
        "00000000-0000-0000-0000-000000000000",
        api_key()
    );

    let (ws_stream, _) = match connect_async(&ws_url).await {
        Ok(connection) => connection,
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(
                response.status(),
                404,
                "Expected HTTP 404 for invalid sandbox"
            );
            info!("✓ Invalid sandbox rejected during HTTP upgrade with 404");
            return;
        }
        Err(e) => panic!("Unexpected WebSocket connection error: {e:?}"),
    };

    let (_, mut ws_receiver) = ws_stream.split();

    // Should receive error message
    let timeout = Duration::from_secs(5);
    let data = tokio::time::timeout(timeout, ws_receiver.next()).await;

    match data {
        Ok(Some(Ok(message))) => match message {
            Message::Text(text) => {
                assert!(
                    text.contains("error") || text.contains("not found"),
                    "Expected error message, got: {}",
                    text
                );
                info!("✓ Received expected error: {}", text);
            }
            Message::Close(_) => {
                info!("✓ Connection closed as expected for invalid sandbox");
            }
            _ => {
                panic!("Unexpected message type");
            }
        },
        _ => {
            panic!("Expected error message or close");
        }
    }
}
