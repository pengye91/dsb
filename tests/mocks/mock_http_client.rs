// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Mock HTTP client for testing
//!
//! Provides a mock implementation of HTTP client for testing CLI commands
//! and API handlers without requiring a running server.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// Trait for HTTP client operations
#[async_trait]
pub trait HttpClientTrait: Send + Sync {
    /// Perform a GET request
    async fn get(&self, url: &str) -> Result<HttpResponse, HttpError>;

    /// Perform a POST request with JSON body
    async fn post(&self, url: &str, body: Option<Value>) -> Result<HttpResponse, HttpError>;

    /// Set bearer token for authentication
    fn set_bearer_token(&mut self, token: String);
}

/// HTTP response
#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Value,
}

/// HTTP error type
#[derive(Debug, Clone, thiserror::Error)]
pub enum HttpError {
    #[allow(dead_code)]
    #[error("Network error: {0}")]
    Network(String),

    #[error("HTTP error {status}: {message}")]
    Http { status: u16, message: String },

    #[allow(dead_code)]
    #[error("JSON error: {0}")]
    Json(String),

    #[error("Not found")]
    NotFound,
}

/// Mock HTTP client for testing
#[derive(Clone)]
pub struct MockHttpClient {
    bearer_token: std::sync::Arc<RwLock<Option<String>>>,
    responses: std::sync::Arc<RwLock<HashMap<String, Value>>>,
    error_responses: std::sync::Arc<RwLock<HashMap<String, HttpError>>>,
}

impl MockHttpClient {
    /// Create a new mock HTTP client
    pub fn new() -> Self {
        Self {
            bearer_token: std::sync::Arc::new(RwLock::new(None)),
            responses: std::sync::Arc::new(RwLock::new(HashMap::new())),
            error_responses: std::sync::Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set a mock response for a specific URL
    pub async fn set_response(&self, url: &str, response: Value) {
        let mut responses = self.responses.write().await;
        responses.insert(url.to_string(), response);
    }

    /// Set a mock error for a specific URL
    pub async fn set_error(&self, url: &str, error: HttpError) {
        let mut errors = self.error_responses.write().await;
        errors.insert(url.to_string(), error);
    }

    /// Clear all mock responses
    pub async fn clear(&self) {
        let mut responses = self.responses.write().await;
        let mut errors = self.error_responses.write().await;
        responses.clear();
        errors.clear();
    }
}

impl Default for MockHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpClientTrait for MockHttpClient {
    async fn get(&self, url: &str) -> Result<HttpResponse, HttpError> {
        // Check if there's a mock error
        let errors = self.error_responses.read().await;
        if let Some(error) = errors.get(url) {
            return Err(error.clone());
        }
        drop(errors);

        // Check if there's a mock response
        let responses = self.responses.read().await;
        if let Some(response) = responses.get(url) {
            return Ok(HttpResponse {
                status: 200,
                body: response.clone(),
            });
        }
        drop(responses);

        // Default 404 response
        Err(HttpError::NotFound)
    }

    async fn post(&self, url: &str, _body: Option<Value>) -> Result<HttpResponse, HttpError> {
        // Check if there's a mock error
        let errors = self.error_responses.read().await;
        if let Some(error) = errors.get(url) {
            return Err(error.clone());
        }
        drop(errors);

        // Check if there's a mock response
        let responses = self.responses.read().await;
        if let Some(response) = responses.get(url) {
            return Ok(HttpResponse {
                status: 200,
                body: response.clone(),
            });
        }
        drop(responses);

        // Default 404 response
        Err(HttpError::NotFound)
    }

    fn set_bearer_token(&mut self, token: String) {
        let mut bearer = self.bearer_token.try_write().unwrap();
        *bearer = Some(token);
    }
}

///////////////////////////////////////////////////////////////////////////////
// Tests
///////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn test_mock_http_client_bearer_token() {
        let mut client = MockHttpClient::new();

        client.set_bearer_token("test-token-123".to_string());

        let bearer = client.bearer_token.try_read().unwrap();
        assert_eq!(bearer.as_ref().unwrap(), "test-token-123");
    }
}
