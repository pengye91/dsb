// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # RFC 9457 Error Handling for DSB API
//!
//! This module provides structured error handling following RFC 9457 (Problem Details for HTTP APIs).
//!
//! ## Features
//!
//! - **Machine-readable error codes** for programmatic error handling
//! - **Human-readable messages** with helpful suggestions
//! - **Request tracking** with unique request IDs
//! - **Retryable flag** to indicate if operation should be retried
//! - **Rich metadata** for debugging and troubleshooting
//!
//! ## Error Response Format
//!
//! ```json
//! {
//!   "type": "https://docs.dsb.dev/errors/SANDBOX_NOT_FOUND",
//!   "title": "Sandbox Not Found",
//!   "status": 404,
//!   "detail": "No sandbox found with ID '...'",
//!   "instance": "/sandboxes/...",
//!   "error_code": "SANDBOX_NOT_FOUND",
//!   "timestamp": "2026-02-06T10:30:00Z",
//!   "request_id": "req_abc123",
//!   "retryable": false,
//!   "metadata": { "sandbox_id": "..." },
//!   "suggestions": ["Verify the sandbox ID", "List all sandboxes"]
//! }
//! ```

pub use crate::core::errors::{ApiError, ErrorCode};
use crate::core::manager::ManagerError;
use crate::db::StoreError;
use crate::docker::DockerError;
use axum::{
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::collections::HashMap;

/// Error codes returned by the DSB API, synchronized across Rust, Python SDK, and sandbox.
///
/// Each variant maps to an HTTP status code and indicates whether the error is retryable.
/// This enum is the single source of truth for error codes across the entire system.
///
/// # Synchronization
///
/// When adding new error codes, update all three files:
/// - This file (`src/api/errors.rs`)
/// - `docker/images/sandbox/error_codes.py`
/// - `sdks/python/src/dsb_sdk/error_codes.py`
///
/// Then run `python scripts/verify_error_codes.py` to verify consistency.
/// RFC 9457 Problem Details response.
///
/// This structure provides a standardized error response format as specified
/// in [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457.html) (Problem Details for HTTP APIs).
///
/// # Response Format
///
/// ```json
/// {
///   "type": "https://docs.dsb.dev/errors/SANDBOX_NOT_FOUND",
///   "title": "Sandbox Not Found",
///   "status": 404,
///   "detail": "No sandbox found with ID '...'",
///   "instance": "/sandboxes/...",
///   "error_code": "SANDBOX_NOT_FOUND",
///   "timestamp": "2026-02-06T10:30:00Z",
///   "request_id": "req_abc123",
///   "retryable": false,
///   "metadata": { "sandbox_id": "..." },
///   "suggestions": ["Verify the sandbox ID", "List all sandboxes"]
/// }
/// ```
#[derive(Debug, Serialize)]
pub struct ProblemDetails {
    /// URI reference to the error type documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,

    /// Short, human-readable title of the error type.
    pub title: String,

    /// HTTP status code.
    pub status: u16,

    /// Detailed error message with context.
    pub detail: String,

    /// URI reference to the specific occurrence of the problem.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,

    /// Machine-readable error code string (e.g., "SANDBOX_NOT_FOUND").
    pub error_code: String,

    /// ISO 8601 timestamp when the error occurred.
    pub timestamp: String,

    /// Unique request identifier for troubleshooting and log correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// Whether the operation should be retried.
    pub retryable: bool,

    /// Additional structured metadata about the error.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,

    /// Suggested remediation steps for the client.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
}

/// Main API error type.
///
/// This enum represents all possible errors that can occur in the API layer.
/// Each variant contains relevant context about the error and maps to an
/// appropriate [`ErrorCode`] for consistent HTTP responses.
///
/// # Conversion
///
/// `ApiError` implements `IntoResponse`, automatically converting to an
/// RFC 9457 `ProblemDetails` JSON response with the correct HTTP status code.
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.error_code();
        let status = code.http_status();

        let problem = ProblemDetails {
            r#type: Some(format!("https://docs.dsb.dev/errors/{}", code.as_str())),
            title: code.as_str().replace('_', " "),
            status: status.as_u16(),
            detail: self.to_string(),
            instance: None, // Set by middleware
            error_code: code.as_str().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            request_id: None, // Set by middleware
            retryable: code.is_retryable(),
            metadata: self.extract_metadata(),
            suggestions: self.suggestions(),
        };

        let mut response = Json(problem).into_response();
        *response.status_mut() = status;
        response
            .headers_mut()
            .insert("Content-Type", "application/problem+json".parse().unwrap());
        response
    }
}

// ============================================================================
// Conversion Traits for Existing Error Types
// ============================================================================

impl From<StoreError> for ApiError {
    fn from(err: StoreError) -> Self {
        let code = err.error_code();
        match err {
            StoreError::NotFound(id) => ApiError::SandboxNotFound(id.to_string()),
            StoreError::Postgres(e) => ApiError::Database {
                message: "Database operation failed".to_string(),
                code,
                source: Some(Box::new(e)),
            },
            StoreError::Serialization(e) => ApiError::Database {
                message: "Data serialization failed".to_string(),
                code,
                source: Some(Box::new(e)),
            },
            StoreError::InvalidState(msg) => ApiError::Validation {
                message: msg,
                field: None,
                code,
            },
            StoreError::Message(msg) => ApiError::Internal(msg),
        }
    }
}

impl From<DockerError> for ApiError {
    fn from(err: DockerError) -> Self {
        let code = err.error_code();
        match &err {
            DockerError::ImageNotFound(img) => ApiError::Backend {
                message: format!("Image not found: {}", img),
                operation: "pull_image".to_string(),
                code,
                source: Some(Box::new(err)),
            },
            DockerError::ContainerNotFound(id) => ApiError::Backend {
                message: format!("Container not found: {}", id),
                operation: "container_operation".to_string(),
                code,
                source: Some(Box::new(err)),
            },
            DockerError::ToolProxy { message, .. } => ApiError::Backend {
                message: message.clone(),
                operation: "tool_proxy".to_string(),
                code,
                source: Some(Box::new(err)),
            },
            DockerError::Api(msg) => ApiError::Backend {
                message: msg.clone(),
                operation: "docker_api".to_string(),
                code,
                source: Some(Box::new(err)),
            },
            DockerError::Volume(msg) => ApiError::Backend {
                message: msg.clone(),
                operation: "volume".to_string(),
                code,
                source: Some(Box::new(err)),
            },
            DockerError::ExecFailed(msg) => ApiError::Backend {
                message: msg.clone(),
                operation: "exec".to_string(),
                code,
                source: Some(Box::new(err)),
            },
            DockerError::Io(io_err) => ApiError::Backend {
                message: io_err.to_string(),
                operation: "io".to_string(),
                code,
                source: Some(Box::new(err)),
            },
        }
    }
}

/// Conversion from ManagerError
///
/// This conversion maps ManagerError to ApiError for consistent error handling
/// across different backend implementations (Docker, Kubernetes, etc.).
impl From<ManagerError> for ApiError {
    fn from(err: ManagerError) -> Self {
        match err {
            ManagerError::NotFound(ref msg) => ApiError::SandboxNotFound(msg.clone()),
            ManagerError::Api(ref msg) => ApiError::Backend {
                message: msg.clone(),
                operation: "manager_api".to_string(),
                code: ErrorCode::InternalError,
                source: Some(Box::new(err)),
            },
            ManagerError::OperationFailed(ref msg) => ApiError::Backend {
                message: msg.clone(),
                operation: "manager_operation".to_string(),
                code: ErrorCode::InternalError,
                source: Some(Box::new(err)),
            },
            ManagerError::NotSupported(ref msg) => ApiError::Validation {
                message: format!("Operation not supported: {}", msg),
                field: None,
                code: ErrorCode::InternalError,
            },
            ManagerError::Timeout(ref msg) => ApiError::Backend {
                message: msg.clone(),
                operation: "timeout".to_string(),
                code: ErrorCode::RequestTimeout,
                source: Some(Box::new(err)),
            },
            ManagerError::Conflict(ref msg) => ApiError::Backend {
                message: msg.clone(),
                operation: "conflict".to_string(),
                code: ErrorCode::SandboxAlreadyExists,
                source: Some(Box::new(err)),
            },
            ManagerError::Io(ref io_err) => ApiError::Backend {
                message: io_err.to_string(),
                operation: "io".to_string(),
                code: ErrorCode::InternalError,
                source: Some(Box::new(err)),
            },
        }
    }
}

/// Generic conversion from boxed errors
///
/// This allows any boxed error to be converted to ApiError,
/// wrapping it as an internal server error.
impl From<Box<dyn std::error::Error + Send + Sync>> for ApiError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        ApiError::Internal(err.to_string())
    }
}

/// Conversion from SshServiceError
///
/// This conversion uses the `error_code()` method from `SshServiceError` to ensure
/// consistent error code mapping. When adding new error variants, update both
/// `SshServiceError::error_code()` and this conversion.
impl From<crate::core::SshServiceError> for ApiError {
    fn from(err: crate::core::SshServiceError) -> Self {
        let code = err.error_code();
        match err {
            crate::core::SshServiceError::SandboxNotFound(id) => {
                ApiError::SandboxNotFound(id.to_string())
            }
            crate::core::SshServiceError::SandboxNotRunning(id) => ApiError::Validation {
                message: format!("Sandbox is not running: {}", id),
                field: None,
                code,
            },
            crate::core::SshServiceError::AuthenticationFailed(msg) => ApiError::Validation {
                message: msg,
                field: None,
                code,
            },
            crate::core::SshServiceError::SessionNotFound(id) => ApiError::Validation {
                message: format!("SSH session not found: {}", id),
                field: Some("session_id".to_string()),
                code,
            },
            crate::core::SshServiceError::DatabaseError(msg) => ApiError::Database {
                message: msg,
                code,
                source: None,
            },
            crate::core::SshServiceError::InvalidRequest(msg) => ApiError::Validation {
                message: msg,
                field: None,
                code,
            },
        }
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use uuid::Uuid;

    #[test]
    fn test_error_code_http_status_mapping() {
        assert_eq!(
            ErrorCode::SandboxNotFound.http_status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ErrorCode::ValidationError.http_status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ErrorCode::BackendImagePullFailed.http_status(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            ErrorCode::RateLimitExceeded.http_status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            ErrorCode::AuthenticationMissing.http_status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            ErrorCode::AuthorizationInsufficientPermissions.http_status(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn test_error_code_retryable() {
        assert!(ErrorCode::ServiceUnavailable.is_retryable());
        assert!(ErrorCode::BackendImagePullFailed.is_retryable());
        assert!(ErrorCode::RateLimitExceeded.is_retryable());
        assert!(ErrorCode::DatabaseConnectionFailed.is_retryable());
        assert!(!ErrorCode::SandboxNotFound.is_retryable());
        assert!(!ErrorCode::ValidationError.is_retryable());
        assert!(!ErrorCode::AuthenticationInvalidApiKey.is_retryable());
    }

    #[test]
    fn test_error_code_as_str() {
        assert_eq!(ErrorCode::SandboxNotFound.as_str(), "SANDBOX_NOT_FOUND");
        assert_eq!(ErrorCode::ValidationError.as_str(), "VALIDATION_ERROR");
        assert_eq!(
            ErrorCode::BackendImagePullFailed.as_str(),
            "BACKEND_IMAGE_PULL_FAILED"
        );
        assert_eq!(
            ErrorCode::AuthenticationMissing.as_str(),
            "AUTHENTICATION_MISSING"
        );
        // New error codes
        assert_eq!(ErrorCode::ToolNotFound.as_str(), "TOOL_NOT_FOUND");
        assert_eq!(ErrorCode::ToolTimeout.as_str(), "TOOL_TIMEOUT");
        assert_eq!(ErrorCode::InternalError.as_str(), "INTERNAL_ERROR");
    }

    #[test]
    fn test_error_code_from_str() {
        // Sandbox errors
        assert_eq!(
            ErrorCode::parse("SANDBOX_NOT_FOUND"),
            Some(ErrorCode::SandboxNotFound)
        );
        assert_eq!(
            ErrorCode::parse("SANDBOX_INVALID_STATE"),
            Some(ErrorCode::SandboxInvalidState)
        );

        // Tool errors
        assert_eq!(
            ErrorCode::parse("TOOL_NOT_FOUND"),
            Some(ErrorCode::ToolNotFound)
        );
        assert_eq!(
            ErrorCode::parse("TOOL_EXECUTION_FAILED"),
            Some(ErrorCode::ToolExecutionFailed)
        );
        assert_eq!(
            ErrorCode::parse("TOOL_TIMEOUT"),
            Some(ErrorCode::ToolTimeout)
        );

        // Backend errors
        assert_eq!(
            ErrorCode::parse("BACKEND_CONTAINER_NOT_FOUND"),
            Some(ErrorCode::BackendContainerNotFound)
        );
        assert_eq!(
            ErrorCode::parse("BACKEND_EXEC_FAILED"),
            Some(ErrorCode::BackendExecFailed)
        );

        // Backend errors (legacy DOCKER_* aliases)
        assert_eq!(
            ErrorCode::parse("DOCKER_CONTAINER_NOT_FOUND"),
            Some(ErrorCode::BackendContainerNotFound)
        );
        assert_eq!(
            ErrorCode::parse("DOCKER_EXEC_FAILED"),
            Some(ErrorCode::BackendExecFailed)
        );

        // SSH errors
        assert_eq!(
            ErrorCode::parse("SSH_SESSION_NOT_FOUND"),
            Some(ErrorCode::SshSessionNotFound)
        );
        assert_eq!(
            ErrorCode::parse("SSH_AUTHENTICATION_FAILED"),
            Some(ErrorCode::SshAuthenticationFailed)
        );

        // Infrastructure errors
        assert_eq!(
            ErrorCode::parse("UPSTREAM_ERROR"),
            Some(ErrorCode::UpstreamError)
        );
        assert_eq!(
            ErrorCode::parse("REQUEST_TIMEOUT"),
            Some(ErrorCode::RequestTimeout)
        );

        // Internal errors
        assert_eq!(
            ErrorCode::parse("INTERNAL_ERROR"),
            Some(ErrorCode::InternalError)
        );
        assert_eq!(
            ErrorCode::parse("CONFIGURATION_ERROR"),
            Some(ErrorCode::ConfigurationError)
        );

        // Unknown error code
        assert_eq!(ErrorCode::parse("UNKNOWN_ERROR"), None);
        assert_eq!(ErrorCode::parse(""), None);
    }

    #[test]
    fn test_error_code_roundtrip() {
        // Test that all error codes can be converted to string and back
        let error_codes = vec![
            ErrorCode::SandboxNotFound,
            ErrorCode::SandboxInvalidState,
            ErrorCode::SandboxAlreadyExists,
            ErrorCode::SandboxCreationFailed,
            ErrorCode::SandboxExecutionFailed,
            ErrorCode::ToolNotFound,
            ErrorCode::ToolExecutionFailed,
            ErrorCode::ToolValidationError,
            ErrorCode::ToolTimeout,
            ErrorCode::BackendImagePullFailed,
            ErrorCode::BackendContainerCreateFailed,
            ErrorCode::BackendContainerStartFailed,
            ErrorCode::BackendVolumeError,
            ErrorCode::BackendContainerNotFound,
            ErrorCode::BackendExecFailed,
            ErrorCode::SshSessionNotFound,
            ErrorCode::SshAuthenticationFailed,
            ErrorCode::SshConnectionFailed,
            ErrorCode::TerminalOperationFailed,
            ErrorCode::ValidationError,
            ErrorCode::ValidationInvalidPort,
            ErrorCode::ValidationMissingField,
            ErrorCode::ValidationInvalidImageName,
            ErrorCode::ValidationInvalidRequest,
            ErrorCode::AuthenticationMissing,
            ErrorCode::AuthenticationInvalidApiKey,
            ErrorCode::AuthorizationInsufficientPermissions,
            ErrorCode::DatabaseConnectionFailed,
            ErrorCode::DatabaseQueryFailed,
            ErrorCode::ServiceUnavailable,
            ErrorCode::RateLimitExceeded,
            ErrorCode::UpstreamError,
            ErrorCode::RequestTimeout,
            ErrorCode::InternalError,
            ErrorCode::ConfigurationError,
        ];

        for code in error_codes {
            let str_repr = code.as_str();
            let parsed = ErrorCode::parse(str_repr);
            assert_eq!(
                parsed,
                Some(code),
                "Error code {:?} should roundtrip through string '{}'",
                code,
                str_repr
            );
        }
    }

    #[test]
    fn test_api_error_into_response() {
        let error = ApiError::SandboxNotFound("test-id".to_string());
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/problem+json"
        );
    }

    #[test]
    fn test_problem_details_serialization() {
        let mut metadata = HashMap::new();
        metadata.insert("sandbox_id".to_string(), serde_json::json!("test-id"));

        let problem = ProblemDetails {
            r#type: Some("https://docs.dsb.dev/errors/SANDBOX_NOT_FOUND".to_string()),
            title: "Sandbox Not Found".to_string(),
            status: 404,
            detail: "Test detail".to_string(),
            instance: Some("/sandboxes/123".to_string()),
            error_code: "SANDBOX_NOT_FOUND".to_string(),
            timestamp: "2026-02-06T10:00:00Z".to_string(),
            request_id: Some("req-123".to_string()),
            retryable: false,
            metadata,
            suggestions: vec!["Check the ID".to_string()],
        };

        let json = serde_json::to_string(&problem).unwrap();
        assert!(json.contains("SANDBOX_NOT_FOUND"));
        assert!(json.contains("https://docs.dsb.dev/errors/"));
        assert!(json.contains("request_id"));
        assert!(json.contains("sandbox_id"));
    }

    #[test]
    fn test_store_error_conversion() {
        let id = Uuid::new_v4();
        let store_error = StoreError::NotFound(id);
        let api_error: ApiError = store_error.into();

        assert!(matches!(api_error, ApiError::SandboxNotFound(_)));
    }

    #[test]
    fn test_docker_error_conversion() {
        let docker_error = DockerError::ImageNotFound("python:3.12".to_string());
        let api_error: ApiError = docker_error.into();

        assert!(matches!(api_error, ApiError::Backend { .. }));
        assert_eq!(api_error.error_code(), ErrorCode::BackendImagePullFailed);
    }

    #[test]
    fn test_validation_error_suggestions() {
        let error = ApiError::Validation {
            message: "Invalid port".to_string(),
            field: Some("port_mappings".to_string()),
            code: ErrorCode::ValidationInvalidPort,
        };

        let suggestions = error.suggestions();
        assert!(suggestions
            .iter()
            .any(|s| s.contains("request body format")));
        assert!(suggestions.iter().any(|s| s.contains("port_mappings")));
    }

    #[test]
    fn test_metadata_extraction() {
        let error = ApiError::SandboxNotFound("test-uuid-123".to_string());
        let metadata = error.extract_metadata();

        assert_eq!(
            metadata.get("sandbox_id"),
            Some(&serde_json::json!("test-uuid-123"))
        );
    }

    #[test]
    fn test_authentication_error() {
        let error = ApiError::Authentication("Invalid API key".to_string());
        let code = error.error_code();

        assert_eq!(code, ErrorCode::AuthenticationInvalidApiKey);
        assert_eq!(code.http_status(), StatusCode::UNAUTHORIZED);
        assert!(!code.is_retryable());
    }
}
