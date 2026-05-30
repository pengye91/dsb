// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Mock HTTP client tests
//!
//! Tests for the mock HTTP client implementation.

mod mocks;

use mocks::{HttpClientTrait, HttpError, MockHttpClient};

#[tokio::test]
async fn test_mock_http_client_get_success() {
    let client = MockHttpClient::new();

    let mock_response = serde_json::json!({
        "message": "success",
        "data": [1, 2, 3]
    });

    client.set_response("/api/test", mock_response).await;

    let response = client.get("/api/test").await.unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body["message"], "success");
}

#[tokio::test]
async fn test_mock_http_client_get_not_found() {
    let client = MockHttpClient::new();

    let result = client.get("/api/nonexistent").await;
    assert!(matches!(result, Err(HttpError::NotFound)));
}

#[tokio::test]
async fn test_mock_http_client_post_success() {
    let client = MockHttpClient::new();

    let mock_response = serde_json::json!({
        "id": "123",
        "status": "created"
    });

    client.set_response("/api/create", mock_response).await;

    let body = serde_json::json!({"name": "test"});
    let response = client.post("/api/create", Some(body)).await.unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.body["id"], "123");
}

#[tokio::test]
async fn test_mock_http_client_error_response() {
    let client = MockHttpClient::new();

    let error = HttpError::Http {
        status: 500,
        message: "Internal server error".to_string(),
    };

    client.set_error("/api/error", error).await;

    let result = client.get("/api/error").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mock_http_client_clear() {
    let client = MockHttpClient::new();

    client
        .set_response("/api/test", serde_json::json!({"test": true}))
        .await;

    let response = client.get("/api/test").await;
    assert!(response.is_ok());

    client.clear().await;

    let response = client.get("/api/test").await;
    assert!(matches!(response, Err(HttpError::NotFound)));
}

#[tokio::test]
async fn test_mock_http_client_multiple_endpoints() {
    let client = MockHttpClient::new();

    client
        .set_response("/api/users", serde_json::json!({"users": []}))
        .await;
    client
        .set_response("/api/posts", serde_json::json!({"posts": [1, 2]}))
        .await;

    let users = client.get("/api/users").await.unwrap();
    assert_eq!(users.body["users"].as_array().unwrap().len(), 0);

    let posts = client.get("/api/posts").await.unwrap();
    assert_eq!(posts.body["posts"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_mock_http_client_json_response() {
    let client = MockHttpClient::new();

    let complex_response = serde_json::json!({
        "id": "sandbox-123",
        "state": "Running",
        "config": {
            "image": "nginx:latest",
            "name": "test-nginx"
        },
        "activity": {
            "last_api_activity": "2025-01-05T10:00:00Z",
            "activity_count": 5
        }
    });

    client
        .set_response("/sandboxes/sandbox-123", complex_response)
        .await;

    let response = client.get("/sandboxes/sandbox-123").await.unwrap();

    assert_eq!(response.body["id"], "sandbox-123");
    assert_eq!(response.body["state"], "Running");
    assert_eq!(response.body["config"]["image"], "nginx:latest");
    assert_eq!(response.body["activity"]["activity_count"], 5);
}

#[tokio::test]
async fn test_mock_http_client_sse_streaming() {
    let client = MockHttpClient::new();

    // Simulate SSE response format
    let sse_response = serde_json::json!({
        "type": "progress",
        "status": "Pulling image",
        "current": 50,
        "total": 100
    });

    client
        .set_response("/sandboxes/create-stream", sse_response)
        .await;

    let response = client.get("/sandboxes/create-stream").await.unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body["type"], "progress");
    assert_eq!(response.body["current"], 50);
}
