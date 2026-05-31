// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Integration tests for sandbox tools
//!
//! Tests the 3 sandbox-related tools:
//! - create_sandbox
//! - list_sandboxes
//! - delete_sandbox

mod common;
use common::{mock_dsb_api::MockDSBServer, test_fixtures::test_sandbox_id};
use dsb_mcp_server::settings::Settings;

#[tokio::test]
async fn test_create_sandbox() {
    let mock_server = MockDSBServer::start().await;
    let expected_id = test_sandbox_id();
    mock_server.mock_create_sandbox(expected_id).await;

    let mut settings = Settings::default();
    settings.dsb.api_url = mock_server.url();
    settings.server.port = 3000;
    settings.dsb.timeout_secs = 30;

    let client = dsb_mcp_server::dsb_client::DSBClient::new(settings).unwrap();

    let result = client
        .create_sandbox("python:3.12".to_string(), Some("test-sandbox".to_string()))
        .await;

    assert!(result.is_ok(), "create_sandbox should succeed");
    let sandbox = result.unwrap();
    assert_eq!(sandbox.id, expected_id);
    assert_eq!(sandbox.state, "running");
}

#[tokio::test]
async fn test_list_sandboxes_empty() {
    let mock_server = MockDSBServer::start().await;
    mock_server.mock_list_sandboxes(vec![]).await;

    let mut settings = Settings::default();
    settings.dsb.api_url = mock_server.url();
    settings.server.port = 3000;
    settings.dsb.timeout_secs = 30;

    let client = dsb_mcp_server::dsb_client::DSBClient::new(settings).unwrap();

    let result = client.list_sandboxes().await;

    assert!(result.is_ok(), "list_sandboxes should succeed");
    let sandboxes = result.unwrap();
    assert_eq!(sandboxes.len(), 0);
}

#[tokio::test]
async fn test_list_sandboxes_with_results() {
    let mock_server = MockDSBServer::start().await;
    let sandbox_id = test_sandbox_id();

    let sandbox_json = serde_json::json!({
        "id": sandbox_id,
        "state": "running",
        "config": {
            "image": "python:3.12",
            "name": "test-sandbox",
            "environment": {}
        },
        "container_id": "container-123",
        "created_at": "2025-01-08T10:00:00Z",
        "updated_at": "2025-01-08T10:00:00Z"
    });

    mock_server.mock_list_sandboxes(vec![sandbox_json]).await;

    let mut settings = Settings::default();
    settings.dsb.api_url = mock_server.url();
    settings.server.port = 3000;
    settings.dsb.timeout_secs = 30;

    let client = dsb_mcp_server::dsb_client::DSBClient::new(settings).unwrap();

    let result = client.list_sandboxes().await;

    assert!(result.is_ok(), "list_sandboxes should succeed");
    let sandboxes = result.unwrap();
    assert_eq!(sandboxes.len(), 1);
    assert_eq!(sandboxes[0].id, sandbox_id);
    assert_eq!(sandboxes[0].state, "running");
}

#[tokio::test]
async fn test_delete_sandbox() {
    let mock_server = MockDSBServer::start().await;
    let sandbox_id = test_sandbox_id();
    mock_server.mock_delete_sandbox(sandbox_id).await;

    let mut settings = Settings::default();
    settings.dsb.api_url = mock_server.url();
    settings.server.port = 3000;
    settings.dsb.timeout_secs = 30;

    let client = dsb_mcp_server::dsb_client::DSBClient::new(settings).unwrap();

    let result = client.delete_sandbox(sandbox_id).await;

    assert!(result.is_ok(), "delete_sandbox should succeed");
}

#[tokio::test]
async fn test_create_sandbox_with_volumes() {
    let mock_server = MockDSBServer::start().await;
    let expected_id = test_sandbox_id();
    mock_server.mock_create_sandbox(expected_id).await;

    let mut settings = Settings::default();
    settings.dsb.api_url = mock_server.url();
    settings.server.port = 3000;
    settings.dsb.timeout_secs = 30;

    let client = dsb_mcp_server::dsb_client::DSBClient::new(settings).unwrap();

    let volumes = vec![dsb_mcp_server::dsb_client::VolumeMount {
        r#type: "bind".to_string(),
        host_path: "/host/path".to_string(), // Now required (not optional)
        container_path: "/container/path".to_string(),
        read_only: false,
    }];

    let result = client
        .create_sandbox_full(dsb_mcp_server::dsb_client::CreateSandboxConfig {
            image: "python:3.12".to_string(),
            name: Some("test-with-volumes".to_string()),
            environment: None,
            port_mappings: None,
            resource_limits: None,
            volumes: Some(volumes),
            command: None,
            inactivity_timeout_minutes: None,
            pull_policy: None,
        })
        .await;

    assert!(result.is_ok(), "create_sandbox with volumes should succeed");
    let sandbox = result.unwrap();
    assert_eq!(sandbox.id, expected_id);
}
