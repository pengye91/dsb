// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Health Check Handler
//!
//! Provides a simple health check endpoint for monitoring and load balancers.

use axum::{http::StatusCode, Json};
use chrono::{DateTime, Utc};

/// Health check response
#[derive(serde::Serialize, serde::Deserialize)]
pub struct HealthResponse {
    /// Service status (e.g., "ok")
    pub status: String,
    /// Service version from Cargo.toml
    pub version: Option<String>,
    /// Uptime in seconds (currently not tracked)
    pub uptime_seconds: Option<u64>,
    /// Timestamp of the health check response
    pub timestamp: Option<DateTime<Utc>>,
}

/// Health check endpoint
///
/// Returns the current service health status. This endpoint is public
/// and does not require authentication.
///
/// # Returns
///
/// `200 OK` with JSON containing status, version, and timestamp.
pub async fn health_check() -> (StatusCode, Json<HealthResponse>) {
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok".to_string(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            uptime_seconds: None, // Could track this in app state if needed
            timestamp: Some(Utc::now()),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_health_response_serialization() {
        let response = HealthResponse {
            status: "ok".to_string(),
            version: None,
            uptime_seconds: None,
            timestamp: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"ok\""));
        assert!(json.contains("\"status\""));
    }

    #[test]
    fn test_health_response_with_different_status() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            version: None,
            uptime_seconds: None,
            timestamp: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("healthy"));
    }

    #[tokio::test]
    async fn test_health_check_returns_correct_status() {
        let (status, json) = health_check().await;
        assert_eq!(status, StatusCode::OK);

        let response = json.0;
        assert_eq!(response.status, "ok");
        assert!(response.version.is_some());
        assert!(response.timestamp.is_some());
    }

    #[test]
    fn test_health_response_with_empty_status() {
        let response = HealthResponse {
            status: "".to_string(),
            version: None,
            uptime_seconds: None,
            timestamp: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\""));
    }

    #[test]
    fn test_health_response_with_unicode_status() {
        let response = HealthResponse {
            status: "系统正常".to_string(),
            version: None,
            uptime_seconds: None,
            timestamp: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("系统正常"));
    }

    #[test]
    fn test_health_response_can_be_deserialized() {
        let json_str = r#"{"status":"ok"}"#;
        let response: HealthResponse = serde_json::from_str(json_str).unwrap();

        assert_eq!(response.status, "ok");
    }

    #[test]
    fn test_health_response_roundtrip() {
        let original = HealthResponse {
            status: "healthy".to_string(),
            version: Some("1.0.0".to_string()),
            uptime_seconds: Some(3600),
            timestamp: Some(Utc::now()),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: HealthResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.status, original.status);
        assert_eq!(deserialized.version, original.version);
        assert_eq!(deserialized.uptime_seconds, original.uptime_seconds);
    }
}
