// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Integration tests for execution tools
//!
//! Tests the 2 execution-related tools:
//! - execute_code
//! - execute_bash

mod common;
use common::{mock_dsb_api::MockDSBServer, test_fixtures::test_sandbox_id};
use dsb_mcp_server::settings::Settings;

#[tokio::test]
async fn test_exec_command_success() {
    let mock_server = MockDSBServer::start().await;
    let sandbox_id = test_sandbox_id();

    mock_server
        .mock_exec_command(sandbox_id, "42\n".to_string(), 0)
        .await;

    let mut settings = Settings::default();
    settings.dsb.api_url = mock_server.url();
    settings.server.port = 3000;
    settings.dsb.timeout_secs = 30;

    let client = dsb_mcp_server::dsb_client::DSBClient::new(settings).unwrap();

    let result = client
        .exec_command(
            sandbox_id,
            vec![
                "python".to_string(),
                "-c".to_string(),
                "print(42)".to_string(),
            ],
        )
        .await;

    assert!(result.is_ok(), "exec_command should succeed");
    let exec_result = result.unwrap();
    assert_eq!(exec_result.output, "42\n");
    assert_eq!(exec_result.exit_code, 0);
}

#[tokio::test]
async fn test_exec_command_with_error() {
    let mock_server = MockDSBServer::start().await;
    let sandbox_id = test_sandbox_id();

    mock_server
        .mock_exec_command(sandbox_id, "Error: command failed\n".to_string(), 1)
        .await;

    let mut settings = Settings::default();
    settings.dsb.api_url = mock_server.url();
    settings.server.port = 3000;
    settings.dsb.timeout_secs = 30;

    let client = dsb_mcp_server::dsb_client::DSBClient::new(settings).unwrap();

    let result = client
        .exec_command(
            sandbox_id,
            vec!["ls".to_string(), "/nonexistent".to_string()],
        )
        .await;

    assert!(
        result.is_ok(),
        "exec_command should succeed even if command fails"
    );
    let exec_result = result.unwrap();
    assert_eq!(exec_result.exit_code, 1);
    assert!(exec_result.output.contains("Error"));
}

#[tokio::test]
async fn test_exec_command_empty_output() {
    let mock_server = MockDSBServer::start().await;
    let sandbox_id = test_sandbox_id();

    mock_server
        .mock_exec_command(sandbox_id, "".to_string(), 0)
        .await;

    let mut settings = Settings::default();
    settings.dsb.api_url = mock_server.url();
    settings.server.port = 3000;
    settings.dsb.timeout_secs = 30;

    let client = dsb_mcp_server::dsb_client::DSBClient::new(settings).unwrap();

    let result = client
        .exec_command(
            sandbox_id,
            vec!["true".to_string()], // Command that produces no output
        )
        .await;

    assert!(result.is_ok(), "exec_command should succeed");
    let exec_result = result.unwrap();
    assert_eq!(exec_result.output, "");
    assert_eq!(exec_result.exit_code, 0);
}
