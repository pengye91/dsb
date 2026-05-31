// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Mock DSB API server for integration tests
//!
//! Uses wiremock to simulate DSB API responses

use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mock DSB API server
pub struct MockDSBServer {
    pub mock_server: MockServer,
}

impl MockDSBServer {
    /// Start a new mock DSB server
    pub async fn start() -> Self {
        let mock_server = MockServer::start().await;
        Self { mock_server }
    }

    /// Get the base URL of the mock server
    pub fn url(&self) -> String {
        self.mock_server.uri()
    }

    /// Mock: POST /sandboxes - Create a sandbox
    #[allow(dead_code)]
    pub async fn mock_create_sandbox(&self, sandbox_id: Uuid) {
        Mock::given(method("POST"))
            .and(path("/sandboxes"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
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
            })))
            .mount(&self.mock_server)
            .await;
    }

    /// Mock: GET /sandboxes - List sandboxes (returns paginated response)
    #[allow(dead_code)]
    pub async fn mock_list_sandboxes(&self, sandboxes: Vec<serde_json::Value>) {
        Mock::given(method("GET"))
            .and(path("/sandboxes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": sandboxes})))
            .mount(&self.mock_server)
            .await;
    }

    /// Mock: DELETE /sandboxes/{id} - Delete a sandbox
    #[allow(dead_code)]
    pub async fn mock_delete_sandbox(&self, _sandbox_id: Uuid) {
        Mock::given(method("DELETE"))
            .and(path_regex(r"^/sandboxes/[^/]+$"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&self.mock_server)
            .await;
    }

    /// Mock: POST /sandboxes/{id}/exec - Execute command in sandbox
    #[allow(dead_code)]
    pub async fn mock_exec_command(&self, _sandbox_id: Uuid, output: String, exit_code: i32) {
        Mock::given(method("POST"))
            .and(path_regex(r"^/sandboxes/[^/]+/exec$"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "output": output,
                "exit_code": exit_code
            })))
            .mount(&self.mock_server)
            .await;
    }
}
