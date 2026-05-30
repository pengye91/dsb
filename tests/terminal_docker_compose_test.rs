// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Terminal Docker-Compose Integration Tests
//!
//! This module provides integration tests for the web terminal that run against
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
//! cargo test --test terminal_docker_compose_test -- --test-threads=1
//!
//! # Or via make
//! make test-compose-terminal
//! ```
//!
//! ## Test Coverage
//!
//! - WebSocket connection to terminal
//! - API key authentication
//! - Bidirectional terminal I/O
//! - Command execution and output
//! - Error scenarios
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

/// Create a test sandbox with a unique name
async fn create_test_sandbox(
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
            // Don't override command - use default supervisord for proper PTY/terminal support
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
async fn test_terminal_websocket_connection() {
    info!("Testing terminal WebSocket connection in docker-compose environment");

    let client = reqwest::Client::new();

    // Create a sandbox with unique name
    let sandbox_id = create_test_sandbox(&client, "test_terminal_websocket_connection")
        .await
        .expect("Failed to create sandbox");
    info!("Created sandbox: {}", sandbox_id);

    // Wait for sandbox to be running
    wait_for_sandbox_running(&client, &sandbox_id, 60)
        .await
        .expect("Sandbox did not start");

    // Connect to terminal WebSocket
    let ws_url = format!(
        "{}/terminal/{}?api_key={}",
        api_url().replace("http://", "ws://"),
        sandbox_id,
        api_key()
    );

    info!("Connecting to terminal WebSocket: {}", ws_url);

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to terminal WebSocket");

    info!("✓ WebSocket connection established");

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send a command
    let command = json!({
        "type": "input",
        "data": "echo 'Hello from terminal test'\n"
    });

    ws_sender
        .send(Message::Text(command.to_string().into()))
        .await
        .expect("Failed to send command");

    info!("✓ Sent command to terminal");

    // Receive output - may need to read multiple messages to find command output
    let timeout = Duration::from_secs(10);
    let mut found_output = false;
    let deadline = tokio::time::Instant::now() + timeout;

    while tokio::time::Instant::now() < deadline {
        let output =
            tokio::time::timeout(deadline - tokio::time::Instant::now(), ws_receiver.next()).await;

        match output {
            Ok(Some(Ok(message))) => match message {
                Message::Text(text) => {
                    info!("✓ Received terminal output: {}", text);
                    if text.contains("Hello from terminal test") || text.contains("echo") {
                        found_output = true;
                        break;
                    }
                }
                Message::Binary(data) => {
                    let text = String::from_utf8_lossy(&data);
                    info!("✓ Received terminal output: {}", text);
                    if text.contains("Hello from terminal test") || text.contains("echo") {
                        found_output = true;
                        break;
                    }
                }
                Message::Close(_) => {
                    panic!("WebSocket closed unexpectedly");
                }
                _ => {}
            },
            Ok(None) => {
                panic!("WebSocket closed");
            }
            Ok(Some(Err(e))) => {
                panic!("WebSocket error: {:?}", e);
            }
            Err(_) => {
                panic!("Timeout waiting for terminal output");
            }
        }
    }

    assert!(
        found_output,
        "Expected command output containing 'Hello from terminal test' or 'echo'"
    );

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

    info!("✓ Terminal WebSocket connection test passed");
}

#[tokio::test]
async fn test_terminal_command_execution() {
    info!("Testing terminal command execution");

    let client = reqwest::Client::new();

    // Create a sandbox with unique name
    let sandbox_id = create_test_sandbox(&client, "test_terminal_command_execution")
        .await
        .expect("Failed to create sandbox");
    info!("Created sandbox: {}", sandbox_id);

    // Wait for sandbox to be running
    wait_for_sandbox_running(&client, &sandbox_id, 60)
        .await
        .expect("Sandbox did not start");

    // Connect to terminal
    let ws_url = format!(
        "{}/terminal/{}?api_key={}",
        api_url().replace("http://", "ws://"),
        sandbox_id,
        api_key()
    );

    let (ws_stream, _) = connect_async(&ws_url).await.expect("Failed to connect");

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Wait for welcome
    let _ = tokio::time::timeout(Duration::from_secs(5), ws_receiver.next()).await;

    // Send multiple commands
    let commands = vec!["pwd\n", "ls /tmp\n", "whoami\n"];

    for cmd in commands {
        let command = json!({
            "type": "input",
            "data": cmd
        });

        ws_sender
            .send(Message::Text(command.to_string().into()))
            .await
            .expect("Failed to send command");

        info!("✓ Sent command: {}", cmd.trim());

        // Wait for output
        let timeout = Duration::from_secs(5);
        let output = tokio::time::timeout(timeout, ws_receiver.next()).await;

        match output {
            Ok(Some(Ok(Message::Text(text)))) => {
                info!("✓ Received output for '{}': {}", cmd.trim(), text);
            }
            Ok(Some(Ok(Message::Binary(data)))) => {
                let text = String::from_utf8_lossy(&data);
                info!("✓ Received output for '{}': {}", cmd.trim(), text);
            }
            _ => {
                info!("No output received for '{}'", cmd.trim());
            }
        }

        sleep(Duration::from_millis(500)).await;
    }

    // Close connection
    ws_sender.send(Message::Close(None)).await.ok();

    // Cleanup
    println!("Cleaning up sandbox: {}", sandbox_id);
    let delete_result = delete_sandbox(&client, &sandbox_id).await;
    match &delete_result {
        Ok(_) => println!("✓ Sandbox deleted successfully"),
        Err(e) => println!("✗ Failed to delete sandbox: {}", e),
    }
    delete_result.expect("Failed to delete sandbox");

    info!("✓ Terminal command execution test passed");
}

#[tokio::test]
async fn test_terminal_auth_failure() {
    info!("Testing terminal authentication failure");

    // Try to connect without API key (should fail or be rejected)
    let ws_url = format!(
        "{}/terminal/{}",
        api_url().replace("http://", "ws://"),
        "invalid-sandbox-id"
    );

    let result = connect_async(&ws_url).await;

    match result {
        Err(e) => {
            info!("✓ Connection rejected as expected: {:?}", e);
        }
        Ok((ws_stream, _)) => {
            // If connection succeeds, we should receive an error message
            let (_, mut ws_receiver) = ws_stream.split();

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
                        info!("✓ Connection closed as expected");
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
    }
}
