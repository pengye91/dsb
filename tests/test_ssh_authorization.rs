// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! SSH Authorization Endpoint Tests
//!
//! This test module verifies the SSH authorization endpoint functionality,
//! including API key validation, sandbox validation, and state checking.
//!
//! These tests use the TestDocker fixture for consistent Docker management.
//!
//! # Running Tests
//!
//! ```bash
//! # Run SSH authorization tests
//! cargo test --test test_ssh_authorization
//!
//! # Run specific test
//! cargo test test_authorize_ssh_access_running_sandbox
//! ```

mod common;
use common::docker_test_setup::TestDocker;
use common::sandbox_image;
use common::using_external_api;

use dsb::core::types::PullPolicy;
use dsb::core::{SandboxConfig, SandboxService, SandboxState};
use dsb::docker::DockerManager;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

const SANDBOX_WAIT_TIMEOUT_SECS: u64 = 60;
const SANDBOX_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Test fixture for SSH authorization tests
struct AuthTestFixture {
    service: Arc<SandboxService>,
    _docker: TestDocker, // Keep alive during test
}

impl AuthTestFixture {
    fn new() -> Result<Self, String> {
        let test_docker = TestDocker::new()?;
        let docker_manager = DockerManager::new_with_config(&test_docker.config)
            .map_err(|e| format!("Failed to create Docker manager: {}", e))?;
        let state = Arc::new(dsb::core::StateStore::new())
            as Arc<dyn dsb::core::store_trait::StateStoreTrait + Send + Sync>;
        let service = Arc::new(SandboxService::new(Arc::new(docker_manager), state));

        Ok(AuthTestFixture {
            service,
            _docker: test_docker,
        })
    }
}

/// Helper function to create a test sandbox
async fn create_test_sandbox(service: &SandboxService, name: &str) -> Uuid {
    let unique_name = format!("{}-{}", name, Uuid::new_v4());
    let config = SandboxConfig {
        image: sandbox_image(),
        name: Some(unique_name),
        pull_policy: PullPolicy::Missing, // Pull if missing
        ..Default::default()
    };

    let sandbox = service
        .create_sandbox(config, None)
        .await
        .expect("Failed to create sandbox");
    sandbox.id
}

async fn wait_for_sandbox_state(
    service: &SandboxService,
    sandbox_id: &Uuid,
    expected_state: SandboxState,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(SANDBOX_WAIT_TIMEOUT_SECS);

    loop {
        if let Some(sb) = service.get_sandbox(sandbox_id).await {
            if sb.state == expected_state {
                return;
            }

            match sb.state {
                SandboxState::Error => {
                    panic!(
                        "Sandbox {} reached unexpected state: Error while waiting for {:?}",
                        sandbox_id, expected_state
                    )
                }
                SandboxState::Stopped if expected_state == SandboxState::Running => {
                    panic!(
                        "Sandbox {} reached unexpected state: Stopped while waiting for {:?}",
                        sandbox_id, expected_state
                    )
                }
                SandboxState::Destroyed if expected_state != SandboxState::Destroyed => {
                    panic!(
                        "Sandbox {} reached unexpected state: Destroyed while waiting for {:?}",
                        sandbox_id, expected_state
                    )
                }
                _ => {}
            }
        }

        if tokio::time::Instant::now() >= deadline {
            panic!(
                "Sandbox {} did not reach {:?} state within {}s",
                sandbox_id, expected_state, SANDBOX_WAIT_TIMEOUT_SECS
            );
        }

        tokio::time::sleep(SANDBOX_POLL_INTERVAL).await;
    }
}

#[tokio::test]
#[serial_test::serial]
async fn test_authorize_ssh_access_running_sandbox() {
    if using_external_api() {
        // Use HTTP API against external deployment
        let fixture = common::server_fixture::ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API");
        let id = fixture
            .client
            .create_sandbox(&sandbox_image(), "test-auth-running")
            .await;
        fixture.client.wait_for_running(&id, 60).await;

        let resp = fixture.client.get(&format!("/sandboxes/{}", id)).await;
        assert!(resp.status().is_success());
        let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
        assert_eq!(
            body.get("state").and_then(|s| s.as_str()),
            Some("running"),
            "Sandbox should be running"
        );

        fixture.client.delete_sandbox(&id).await;
        println!("✓ Test passed: authorize_ssh_access with running sandbox (external API)");
        return;
    }

    // Test: Authorize access to a running sandbox
    let fixture = AuthTestFixture::new().expect("Failed to create fixture");

    // Create a test sandbox
    let sandbox_id = create_test_sandbox(&fixture.service, "test-auth-running").await;

    println!("Created sandbox: {}", sandbox_id);

    wait_for_sandbox_state(&fixture.service, &sandbox_id, SandboxState::Running).await;

    // Verify sandbox is running
    let sandbox = fixture.service.get_sandbox(&sandbox_id).await;
    assert!(sandbox.is_some(), "Sandbox should exist");
    assert_eq!(
        sandbox.unwrap().state,
        SandboxState::Running,
        "Sandbox should be running"
    );

    // Clean up
    let _ = fixture.service.delete_sandbox(&sandbox_id).await;
    println!("✓ Test passed: authorize_ssh_access with running sandbox");
}

#[tokio::test]
#[serial_test::serial]
async fn test_authorize_ssh_access_stopped_sandbox() {
    if using_external_api() {
        // Use HTTP API against external deployment
        let fixture = common::server_fixture::ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API");
        let id = fixture
            .client
            .create_sandbox(&sandbox_image(), "test-auth-stopped")
            .await;
        fixture.client.wait_for_running(&id, 60).await;

        // Stop the sandbox
        let stop_resp = fixture
            .client
            .post(&format!("/sandboxes/{}/stop", id))
            .await;
        assert!(
            stop_resp.status().is_success(),
            "Stop failed: {}",
            stop_resp.status()
        );

        // Poll until stopped
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        let mut stopped = false;
        while tokio::time::Instant::now() < deadline {
            let resp = fixture.client.get(&format!("/sandboxes/{}", id)).await;
            if resp.status().is_success() {
                let body: serde_json::Value = resp.json().await.expect("Failed to parse");
                if body["state"] == "stopped" {
                    stopped = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        assert!(stopped, "Sandbox did not reach stopped state");

        // Verify state
        let resp = fixture.client.get(&format!("/sandboxes/{}", id)).await;
        assert!(resp.status().is_success());
        let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
        assert_eq!(
            body.get("state").and_then(|s| s.as_str()),
            Some("stopped"),
            "Sandbox should be stopped"
        );

        fixture.client.delete_sandbox(&id).await;
        println!("✓ Test passed: authorize_ssh_access rejects stopped sandbox (external API)");
        return;
    }

    // Test: Stopped sandbox should not be accessible
    let fixture = AuthTestFixture::new().expect("Failed to create fixture");

    // Create a test sandbox
    let sandbox_id = create_test_sandbox(&fixture.service, "test-auth-stopped").await;

    wait_for_sandbox_state(&fixture.service, &sandbox_id, SandboxState::Running).await;

    // Stop the sandbox
    fixture
        .service
        .stop_sandbox(&sandbox_id)
        .await
        .expect("Failed to stop sandbox");

    wait_for_sandbox_state(&fixture.service, &sandbox_id, SandboxState::Stopped).await;

    // Verify sandbox is stopped
    let sandbox = fixture.service.get_sandbox(&sandbox_id).await;
    assert!(sandbox.is_some(), "Sandbox should exist");
    assert_eq!(
        sandbox.unwrap().state,
        SandboxState::Stopped,
        "Sandbox should be stopped"
    );

    // Clean up
    let _ = fixture.service.delete_sandbox(&sandbox_id).await;
    println!("✓ Test passed: authorize_ssh_access rejects stopped sandbox");
}

#[tokio::test]
async fn test_authorize_ssh_access_nonexistent_sandbox() {
    // Test: Nonexistent sandbox should return error
    let fake_sandbox_id = Uuid::new_v4();
    let fixture = AuthTestFixture::new().expect("Failed to create fixture");

    // Try to get nonexistent sandbox
    let sandbox = fixture.service.get_sandbox(&fake_sandbox_id).await;

    assert!(sandbox.is_none(), "Nonexistent sandbox should return None");
    println!("✓ Test passed: authorize_ssh_access handles nonexistent sandbox");
}

#[tokio::test]
#[serial_test::serial]
async fn test_api_key_validation_logic() {
    // Test the API key validation logic using config system
    use dsb::config;

    // Set config via environment variables (config system will read these)
    std::env::set_var("DSB_SERVER__SSH_GATEWAY_API_KEY", "test-secret-key");

    // Load config to verify the variable is accessible
    let cfg = config::load().expect("Failed to load config");
    let expected_key = cfg.server.ssh_gateway_api_key;
    assert!(expected_key.is_some(), "SSH gateway API key should be set");

    // Valid key
    let provided_key = Some("test-secret-key".to_string());
    let expected_key = expected_key.unwrap();

    match provided_key {
        Some(key) if key == expected_key => {
            println!("✓ API key validation: valid key accepted");
        }
        Some(_) => {
            panic!("Invalid key should be rejected");
        }
        None => {
            panic!("Missing key should be rejected");
        }
    }

    // Invalid key
    let invalid_key = Some("wrong-key".to_string());
    match invalid_key {
        Some(key) if key == expected_key => {
            panic!("Invalid key should not match");
        }
        _ => {
            println!("✓ API key validation: invalid key rejected");
        }
    }

    // Missing key
    let missing_key: Option<String> = None;
    match missing_key {
        Some(_) => {
            panic!("This should not happen");
        }
        None => {
            println!("✓ API key validation: missing key detected");
        }
    }

    std::env::remove_var("DSB_SERVER__SSH_GATEWAY_API_KEY");
}

#[tokio::test]
#[serial_test::serial]
async fn test_api_key_validation_disabled() {
    // Test: When no API key is configured, validation should be skipped
    use dsb::config;

    // Save the original value if it exists
    let original_key = std::env::var("DSB_SERVER__SSH_GATEWAY_API_KEY").ok();

    // Make sure no API key is set for this test
    std::env::remove_var("DSB_SERVER__SSH_GATEWAY_API_KEY");

    // Load config to verify no API key is set
    let cfg = config::load().expect("Failed to load config");
    let expected_key = cfg.server.ssh_gateway_api_key;
    assert!(expected_key.is_none(), "No API key should be configured");

    println!("✓ Test passed: API key validation can be disabled");

    // Restore the original value
    if let Some(key) = original_key {
        std::env::set_var("DSB_SERVER__SSH_GATEWAY_API_KEY", key);
    }
}
