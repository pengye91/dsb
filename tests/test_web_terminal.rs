// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Integration tests for web_terminal module
//!
//! These tests use mocked dependencies to test the WebSocket handlers
//! and other complex functions without requiring real Docker/Database.

use dsb::core::manager::SandboxManager;
use dsb::docker::DockerManager;
use dsb::web_terminal::{validate_api_key, WebTerminalError, WebTerminalState};
use std::sync::Arc;

mod common;

/// Helper to create a test WebTerminalState
#[allow(dead_code)]
fn create_test_state() -> WebTerminalState {
    // Load test configuration
    let config = dsb::config::load_for_tests().expect("Failed to load test config");

    // Create a DockerManager which implements SandboxManager
    match DockerManager::new_with_config(&config) {
        Ok(docker_manager) => {
            let backend: Arc<dyn SandboxManager> = Arc::new(docker_manager);
            WebTerminalState::new(backend, None, Arc::new(config))
        }
        Err(_) => {
            // If Docker isn't available, we can't create a state
            // This is expected in CI environments
            panic!("Docker not available for testing");
        }
    }
}

#[test]
fn test_web_terminal_error_variants() {
    // Test all error variants can be created and displayed
    use dsb::web_terminal::WebTerminalError;

    let errors = vec![
        WebTerminalError::ContainerNotFound("test-container".to_string()),
        WebTerminalError::ExecCreationFailed("permission denied".to_string()),
        WebTerminalError::ExecStartFailed("connection lost".to_string()),
        WebTerminalError::WebSocketError("frame too large".to_string()),
        WebTerminalError::BackendConnectionFailed("unix socket missing".to_string()),
        WebTerminalError::Unauthorized("invalid token".to_string()),
    ];

    for error in errors {
        let error_string = format!("{}", error);
        assert!(!error_string.is_empty());
        assert!(error_string.len() > 10); // Minimum reasonable error message length
    }
}

#[test]
fn test_server_message_serialization_roundtrip() {
    use dsb::web_terminal::ServerMessage;
    use serde_json;

    // Test Output message
    let output = ServerMessage::Output("test output\n".to_string());
    let json = serde_json::to_string(&output).unwrap();
    let deserialized: ServerMessage = serde_json::from_str(&json).unwrap();
    match deserialized {
        ServerMessage::Output(s) => assert_eq!(s, "test output\n"),
        _ => panic!("Wrong message type"),
    }

    // Test Error message
    let error = ServerMessage::Error("test error".to_string());
    let json = serde_json::to_string(&error).unwrap();
    let deserialized: ServerMessage = serde_json::from_str(&json).unwrap();
    match deserialized {
        ServerMessage::Error(s) => assert_eq!(s, "test error"),
        _ => panic!("Wrong message type"),
    }

    // Test End message
    let end = ServerMessage::End;
    let json = serde_json::to_string(&end).unwrap();
    let deserialized: ServerMessage = serde_json::from_str(&json).unwrap();
    match deserialized {
        ServerMessage::End => {}
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_client_message_serialization_roundtrip() {
    use dsb::web_terminal::ClientMessage;
    use serde_json;

    // Test Input message
    let input = ClientMessage::Input("ls -la\n".to_string());
    let json = serde_json::to_string(&input).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    match deserialized {
        ClientMessage::Input(s) => assert_eq!(s, "ls -la\n"),
        _ => panic!("Wrong message type"),
    }

    // Test Resize message
    let resize = ClientMessage::Resize { rows: 24, cols: 80 };
    let json = serde_json::to_string(&resize).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    match deserialized {
        ClientMessage::Resize { rows, cols } => {
            assert_eq!(rows, 24);
            assert_eq!(cols, 80);
        }
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_message_serialization_with_whitespace() {
    use dsb::web_terminal::ClientMessage;
    use serde_json;

    // Test various whitespace patterns
    let test_cases = vec![
        "  spaces  ",
        "\ttabs\t",
        "\nnewlines\n",
        "\r\ncarriage\r\n",
        "  mix \t of \n everything  ",
    ];

    for input in test_cases {
        let msg = ClientMessage::Input(input.to_string());
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            ClientMessage::Input(s) => assert_eq!(s, input),
            _ => panic!("Wrong message type"),
        }
    }
}

#[test]
fn test_message_serialization_with_quotes() {
    use dsb::web_terminal::ClientMessage;
    use serde_json;

    // Test messages with quotes
    let input_with_quotes = r#"echo "hello 'world'"#;
    let msg = ClientMessage::Input(input_with_quotes.to_string());
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    match deserialized {
        ClientMessage::Input(s) => assert_eq!(s, input_with_quotes),
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_message_serialization_with_backslashes() {
    use dsb::web_terminal::ClientMessage;
    use serde_json;

    // Test messages with backslashes
    let input_with_backslashes = r"cd path\to\directory";
    let msg = ClientMessage::Input(input_with_backslashes.to_string());
    let json = serde_json::to_string(&msg).unwrap();
    let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
    match deserialized {
        ClientMessage::Input(s) => assert_eq!(s, input_with_backslashes),
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_invalid_client_message_formats() {
    use dsb::web_terminal::ClientMessage;
    use serde_json;

    let invalid_cases = vec![
        r#"{"type":"invalid","data":"test"}"#,
        r#"{"type":"input"}"#, // missing data field
        r#"{"data":"test"}"#,  // missing type field
        r#"{}"#,               // empty object
        r#"not json at all"#,
        r#"{"type":"input","data":123}"#, // wrong data type
        r#"{"type":"resize","data":{"rows":"24","cols":80}}"#, // wrong type
    ];

    for case in invalid_cases {
        let result: Result<ClientMessage, _> = serde_json::from_str(case);
        assert!(result.is_err(), "Expected error for: {}", case);
    }
}

#[test]
fn test_edge_case_resize_messages() {
    use dsb::web_terminal::ClientMessage;
    use serde_json;

    // Test boundary values for resize
    let test_cases = vec![
        (0, 0),
        (1, 1),
        (u16::MAX, u16::MAX),
        (999, 999),
        (80, 24), // Common terminal size
    ];

    for (rows, cols) in test_cases {
        let msg = ClientMessage::Resize { rows, cols };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: ClientMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            ClientMessage::Resize { rows: r, cols: c } => {
                assert_eq!(r, rows);
                assert_eq!(c, cols);
            }
            _ => panic!("Wrong message type for {}, {}", rows, cols),
        }
    }
}

#[tokio::test]
async fn test_concurrent_validation() {
    use tokio::task::JoinSet;

    // Test that multiple concurrent validations work correctly
    let expected_key = Some("test-key".to_string());

    let mut tasks = JoinSet::new();

    for i in 0..10 {
        let expected_key = expected_key.clone();
        tasks.spawn(async move {
            let key = if i % 2 == 0 {
                Some("test-key".to_string())
            } else {
                None
            };
            validate_api_key(&key, &expected_key, &None) as Result<(), WebTerminalError>
        });
    }

    let mut success_count = 0;
    let mut fail_count = 0;

    while let Some(result) = tasks.join_next().await {
        let result: Result<Result<(), WebTerminalError>, _> = result;
        match result.unwrap() {
            Ok(()) => success_count += 1,
            Err(_) => fail_count += 1,
        }
    }

    assert_eq!(success_count, 5); // Only the ones with correct key
    assert_eq!(fail_count, 5); // The ones without key
}
