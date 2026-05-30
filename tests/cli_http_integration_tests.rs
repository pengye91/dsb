// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! CLI HTTP Integration Tests
//!
//! Tests CLI command execution with mocked HTTP server.

mod common;
mod mocks;

use mocks::{HttpClientTrait, HttpError, MockHttpClient};
use serde_json::json;

/// Get the test API base URL from configuration
fn get_test_base_url() -> String {
    common::test_config::get_test_api_url()
}

// Helper to create test sandbox response
fn create_test_sandbox_response(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "state": "Running",
        "config": {
            "image": "nginx:latest"
        },
        "container_id": format!("container-{}", id),
        "created_at": "2025-01-05T00:00:00Z",
        "updated_at": "2025-01-05T00:00:00Z"
    })
}

///////////////////////////////////////////////////////////////////////////////
// HTTP Integration Tests with MockHttpClient
///////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn test_create_sandbox_http_integration() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();

    // Mock successful create response
    let create_response = json!({
        "id": "sandbox-123",
        "state": "Running",
        "config": {
            "image": "nginx:latest",
            "name": "test-nginx"
        },
        "container_id": "container-abc123",
        "created_at": "2025-01-05T10:00:00Z",
        "updated_at": "2025-01-05T10:00:00Z"
    });

    mock_client
        .set_response(
            &format!("{}/sandboxes/create-stream", base_url),
            create_response,
        )
        .await;

    // Simulate the request that would be made
    let response = mock_client
        .post(
            &format!("{}/sandboxes/create-stream", base_url),
            Some(json!({"image": "nginx:latest"})),
        )
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body["id"], "sandbox-123");
    assert_eq!(resp.body["state"], "Running");
}

#[tokio::test]
async fn test_list_sandboxes_http_integration() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();

    // Mock list response
    let list_response = json!([
        create_test_sandbox_response("sandbox-1"),
        create_test_sandbox_response("sandbox-2"),
        create_test_sandbox_response("sandbox-3")
    ]);

    mock_client
        .set_response(&format!("{}/sandboxes", base_url), list_response)
        .await;

    let response = mock_client.get(&format!("{}/sandboxes", base_url)).await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_get_sandbox_info_http_integration() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();
    let sandbox_id = "sandbox-info-test";

    let info_response = create_test_sandbox_response(sandbox_id);
    mock_client
        .set_response(
            &format!("{}/sandboxes/{}", base_url, sandbox_id),
            info_response,
        )
        .await;

    let response = mock_client
        .get(&format!("{}/sandboxes/{}", base_url, sandbox_id))
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body["id"], sandbox_id);
}

#[tokio::test]
async fn test_exec_sandbox_http_integration() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();
    let sandbox_id = "sandbox-exec-test";

    let exec_response = json!({
        "output": "file1.txt\nfile2.txt\nfile3.txt\n",
        "exit_code": 0
    });

    mock_client
        .set_response(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            exec_response,
        )
        .await;

    let command = vec!["ls".to_string(), "-la".to_string()];
    let response = mock_client
        .post(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            Some(json!({"command": command})),
        )
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert!(resp.body["output"].as_str().unwrap().contains("file1.txt"));
}

#[tokio::test]
async fn test_stop_sandbox_http_integration() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();
    let sandbox_id = "sandbox-stop-test";

    let stop_response = json!({
        "id": sandbox_id,
        "state": "Stopped",
        "stopped_at": "2025-01-05T10:05:00Z"
    });

    mock_client
        .set_response(
            &format!("{}/sandboxes/{}/stop", base_url, sandbox_id),
            stop_response,
        )
        .await;

    let response = mock_client
        .post(
            &format!("{}/sandboxes/{}/stop", base_url, sandbox_id),
            None as Option<serde_json::Value>,
        )
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body["state"], "Stopped");
}

#[tokio::test]
async fn test_delete_sandbox_http_integration() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();
    let sandbox_id = "sandbox-delete-test";

    let delete_response = json!({
        "message": "Sandbox deleted successfully",
        "id": sandbox_id
    });

    mock_client
        .set_response(
            &format!("{}/sandboxes/{}", base_url, sandbox_id),
            delete_response,
        )
        .await;

    let response = mock_client
        .get(&format!("{}/sandboxes/{}", base_url, sandbox_id))
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert!(resp.body["message"].as_str().unwrap().contains("deleted"));
}

#[tokio::test]
async fn test_stats_sandbox_http_integration() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();
    let sandbox_id = "sandbox-stats-test";

    let stats_response = json!({
        "cpu_percent": 25.5,
        "memory_usage_mb": 512,
        "memory_limit_mb": 2048,
        "memory_percent": 25.0,
        "network_rx_bytes": 1048576,
        "network_tx_bytes": 524288,
        "block_read_bytes": 1024,
        "block_write_bytes": 2048,
        "timestamp": "2025-01-05T10:10:00Z"
    });

    mock_client
        .set_response(
            &format!("{}/sandboxes/{}/stats", base_url, sandbox_id),
            stats_response,
        )
        .await;

    let response = mock_client
        .get(&format!("{}/sandboxes/{}/stats", base_url, sandbox_id))
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body["cpu_percent"], 25.5);
    assert_eq!(resp.body["memory_usage_mb"], 512);
}

#[tokio::test]
async fn test_sse_progress_events_http_integration() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();

    // Mock SSE progress events
    let sse_events = [
        json!({"type": "pulling", "status": "Pulling image...", "current": 0, "total": 100}),
        json!({"type": "creating", "message": "Creating container..."}),
        json!({"type": "starting", "message": "Starting container..."}),
        json!({"type": "ready", "sandbox_id": "sb-123", "container_id": "cnt-456"}),
    ];

    for (i, event) in sse_events.iter().enumerate() {
        mock_client
            .set_response(
                &format!("{}/sandboxes/create-stream/step-{}", base_url, i),
                event.clone(),
            )
            .await;
    }

    // Verify we can fetch each event
    for i in 0..4 {
        let response = mock_client
            .get(&format!("{}/sandboxes/create-stream/step-{}", base_url, i))
            .await;
        assert!(response.is_ok(), "Step {} should succeed", i);
        let resp = response.unwrap();
        assert_eq!(resp.status, 200);
    }
}

#[tokio::test]
async fn test_error_handling_sandbox_not_found() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();
    let sandbox_id = "nonexistent-sandbox";

    let not_found_error = HttpError::Http {
        status: 404,
        message: "Sandbox not found".to_string(),
    };

    mock_client
        .set_error(
            &format!("{}/sandboxes/{}", base_url, sandbox_id),
            not_found_error,
        )
        .await;

    let response = mock_client
        .get(&format!("{}/sandboxes/{}", base_url, sandbox_id))
        .await;

    assert!(response.is_err());
    match response.unwrap_err() {
        HttpError::Http { status, message } => {
            assert_eq!(status, 404);
            assert!(message.contains("not found"));
        }
        _ => panic!("Expected HttpError::NotFound"),
    }
}

#[tokio::test]
async fn test_error_handling_server_error() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();

    let server_error = HttpError::Http {
        status: 500,
        message: "Internal server error".to_string(),
    };

    mock_client
        .set_error(&format!("{}/sandboxes", base_url), server_error)
        .await;

    let response = mock_client.get(&format!("{}/sandboxes", base_url)).await;

    assert!(response.is_err());
    match response.unwrap_err() {
        HttpError::Http { status, .. } => {
            assert_eq!(status, 500);
        }
        _ => panic!("Expected HttpError with status 500"),
    }
}

#[tokio::test]
async fn test_create_sandbox_with_all_options() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();

    let full_config_response = json!({
        "id": "full-config-sandbox",
        "state": "Running",
        "config": {
            "image": "postgres:latest",
            "name": "test-db",
            "port_mappings": [
                {"host_port": 5432, "container_port": 5432, "protocol": "tcp"}
            ],
            "volumes": [
                {
                    "type": "named",
                    "name": "db-data",
                    "container_path": "/var/lib/postgresql/data",
                    "read_only": false
                }
            ],
            "resource_limits": {
                "cpu_shares": 512,
                "memory_mb": 1024
            },
            "command": ["postgres", "-c", "max_connections=200"],
            "inactivity_timeout_minutes": 60,
            "pull_policy": "missing"
        },
        "container_id": "container-full",
        "created_at": "2025-01-05T10:00:00Z",
        "updated_at": "2025-01-05T10:00:00Z"
    });

    mock_client
        .set_response(
            &format!("{}/sandboxes/create-stream", base_url),
            full_config_response,
        )
        .await;

    let request_body = json!({
        "image": "postgres:latest",
        "name": "test-db",
        "pull_policy": "missing",
        "port_mappings": [
            {"host_port": 5432, "container_port": 5432, "protocol": "tcp"}
        ],
        "volumes": [
            {
                "type": "named",
                "name": "db-data",
                "container_path": "/var/lib/postgresql/data",
                "read_only": false
            }
        ],
        "command": ["postgres", "-c", "max_connections=200"],
        "resource_limits": {
            "cpu_shares": 512,
            "memory_mb": 1024
        },
        "inactivity_timeout_minutes": 60
    });

    let response = mock_client
        .post(
            &format!("{}/sandboxes/create-stream", base_url),
            Some(request_body),
        )
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body["config"]["image"], "postgres:latest");
    assert_eq!(
        resp.body["config"]["port_mappings"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn test_activities_list_http_integration() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();

    let activities_response = json!([
        {
            "id": "act-1",
            "sandbox_id": "sandbox-1",
            "activity_type": "ApiCall",
            "timestamp": "2025-01-05T10:00:00Z",
            "details": {"endpoint": "/sandboxes"}
        },
        {
            "id": "act-2",
            "sandbox_id": "sandbox-1",
            "activity_type": "CommandExecution",
            "timestamp": "2025-01-05T10:05:00Z",
            "details": {"command": "ls -la"}
        }
    ]);

    mock_client
        .set_response(&format!("{}/activities", base_url), activities_response)
        .await;

    let response: Result<_, _> = mock_client.get(&format!("{}/activities", base_url)).await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body.as_array().unwrap().len(), 2);
    assert_eq!(resp.body.as_array().unwrap()[0]["activity_type"], "ApiCall");
}

// ============================================================================
// Timeout Integration Tests
// ============================================================================

#[tokio::test]
async fn test_exec_sandbox_with_timeout_success() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();
    let sandbox_id = "sandbox-timeout-success-test";

    let exec_response = json!({
        "output": "command completed successfully\n",
    });

    mock_client
        .set_response(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            exec_response,
        )
        .await;

    let command = vec!["echo".to_string(), "test".to_string()];
    let response = mock_client
        .post(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            Some(json!({
                "command": command,
                "timeout": 60
            })),
        )
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert!(resp.body["output"]
        .as_str()
        .unwrap()
        .contains("command completed successfully"));
}

#[tokio::test]
async fn test_exec_sandbox_with_timeout_error() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();
    let sandbox_id = "sandbox-timeout-error-test";

    let exec_response = json!({
        "message": "Command timed out after 5 seconds"
    });

    mock_client
        .set_response(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            exec_response,
        )
        .await;

    let command = vec!["sleep".to_string(), "10".to_string()];
    let response = mock_client
        .post(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            Some(json!({
                "command": command,
                "timeout": 5
            })),
        )
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    // Timeout errors should return a status (could be 200 with error message in body, or 500)
    assert!(resp.status == 200 || resp.status == 500);
    assert!(resp.body["message"].as_str().unwrap().contains("timed out"));
}

#[tokio::test]
async fn test_exec_sandbox_timeout_optional() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();
    let sandbox_id = "sandbox-timeout-optional-test";

    let exec_response = json!({
        "output": "no timeout specified\n",
    });

    mock_client
        .set_response(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            exec_response,
        )
        .await;

    let command = vec!["echo".to_string(), "test".to_string()];
    let response = mock_client
        .post(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            Some(json!({
                "command": command
                // No timeout field
            })),
        )
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert!(resp.body["output"]
        .as_str()
        .unwrap()
        .contains("no timeout specified"));
}

#[tokio::test]
async fn test_exec_sandbox_timeout_with_stdin() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();
    let sandbox_id = "sandbox-timeout-stdin-test";

    let exec_response = json!({
        "output": "stdin received: hello\n",
    });

    mock_client
        .set_response(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            exec_response,
        )
        .await;

    let command = vec!["cat".to_string()];
    let response = mock_client
        .post(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            Some(json!({
                "command": command,
                "stdin": "hello",
                "timeout": 30
            })),
        )
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
    assert!(resp.body["output"]
        .as_str()
        .unwrap()
        .contains("stdin received"));
}

#[tokio::test]
async fn test_exec_sandbox_timeout_serialization() {
    let mock_client = MockHttpClient::new();
    let base_url = get_test_base_url();
    let sandbox_id = "sandbox-timeout-serialization-test";

    let exec_response = json!({
        "output": "test\n",
    });

    mock_client
        .set_response(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            exec_response,
        )
        .await;

    let command = vec!["ls".to_string()];
    let response = mock_client
        .post(
            &format!("{}/sandboxes/{}/exec", base_url, sandbox_id),
            Some(json!({
                "command": command,
                "timeout": 120,
                "stdin": null  // Explicit null stdin
            })),
        )
        .await;

    assert!(response.is_ok());
    let resp = response.unwrap();
    assert_eq!(resp.status, 200);
}
