// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # SSH Session Management API Handlers
//!
//! This module provides HTTP handlers for SSH session management.
//!
//! ## Endpoints
//!
//! - `POST /ssh-sessions` - Create a new SSH session
//! - `GET /ssh-sessions` - List SSH sessions (with optional filters)
//! - `GET /ssh-sessions/:id` - Get SSH session details
//! - `POST /ssh-sessions/:id/terminate` - Terminate an SSH session
//! - `POST /ssh-sessions/:id/heartbeat` - Update session activity
//! - `GET /ssh/authorize/:sandbox_id` - Internal authorization endpoint for SSH gateway

use axum::extract::FromRequestParts;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, instrument};

use crate::api::{
    auth::{ApiKeyIdentity, ApiKeyType},
    ApiError, ErrorCode,
};
use crate::core::{
    CreateSshSessionRequest, SandboxService, SshSessionFilters, SshSessionResponse,
    SshSessionService,
};
use crate::db::ssh_sessions::SessionStatistics;

/// Query parameters for listing SSH sessions.
#[derive(Debug, Deserialize)]
pub struct ListSshSessionsQuery {
    /// Filter by sandbox ID
    pub sandbox_id: Option<uuid::Uuid>,

    /// Filter by session state
    pub state: Option<String>,

    /// Maximum number of results
    pub limit: Option<usize>,

    /// Offset for pagination
    pub offset: Option<usize>,
}

/// Request body for terminating a session.
#[derive(Debug, Deserialize)]
pub struct TerminateSessionRequest {
    /// Reason for termination
    pub reason: Option<String>,
}

/// Request body for updating session activity (heartbeat).
#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    /// Cumulative bytes sent
    pub bytes_sent: i64,

    /// Cumulative bytes received
    pub bytes_received: i64,
}

/// Authorization context for SSH access.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthContext {
    /// Whether access is authorized
    pub authorized: bool,

    /// Sandbox information
    pub sandbox: Option<SandboxInfo>,

    /// Granted permissions
    pub permissions: Vec<String>,
}

/// Basic sandbox information for authorization response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxInfo {
    /// Sandbox ID (UUID)
    pub id: uuid::Uuid,
    /// Current sandbox state (e.g., "running")
    pub state: String,
    /// Container or pod ID
    pub container_id: Option<String>,
}

/// Error response type.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error message
    pub error: String,
    /// Optional hint for resolving the error
    pub hint: Option<String>,
}

/// Create a new SSH session.
///
/// This endpoint is called by the SSH gateway when establishing a new SSH connection.
#[axum::debug_handler]
pub async fn create_ssh_session(
    State(service): State<Arc<SshSessionService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Json(req): Json<CreateSshSessionRequest>,
) -> Result<Json<SshSessionResponse>, ApiError> {
    service
        .authorize_sandbox_access(&identity, &req.sandbox_id)
        .await?;

    // Validate the request (username, public_key format)
    req.validate()?;

    let session = service.create_session(req).await?;
    Ok(Json(SshSessionResponse::from(session)))
}

/// List SSH sessions.
///
/// Supports filtering by sandbox_id, state, and pagination.
#[axum::debug_handler]
pub async fn list_ssh_sessions(
    State(service): State<Arc<SshSessionService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Query(params): Query<ListSshSessionsQuery>,
) -> Result<Json<Vec<SshSessionResponse>>, ApiError> {
    if !matches!(identity.key_type, ApiKeyType::Privileged) {
        let sandbox_id = params.sandbox_id.ok_or(ApiError::Validation {
            message: "sandbox_id is required for non-privileged SSH session queries".to_string(),
            field: Some("sandbox_id".to_string()),
            code: ErrorCode::AuthorizationInsufficientPermissions,
        })?;

        service
            .authorize_sandbox_access(&identity, &sandbox_id)
            .await?;
    }

    // Parse state filter
    let state_filter = params.state.and_then(|s| match s.as_str() {
        "connecting" => Some(crate::core::SshSessionState::Connecting),
        "active" => Some(crate::core::SshSessionState::Active),
        "disconnected" => Some(crate::core::SshSessionState::Disconnected),
        "terminated" => Some(crate::core::SshSessionState::Terminated),
        "error" => Some(crate::core::SshSessionState::Error),
        _ => None,
    });

    let filters = SshSessionFilters {
        sandbox_id: params.sandbox_id,
        state: state_filter,
        limit: params.limit,
        offset: params.offset,
    };

    let sessions = service.list_sessions(filters).await;
    Ok(Json(
        sessions.into_iter().map(SshSessionResponse::from).collect(),
    ))
}

/// Get SSH session details by ID.
#[axum::debug_handler]
pub async fn get_ssh_session(
    State(service): State<Arc<SshSessionService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<SshSessionResponse>, ApiError> {
    let session = service.get_session(id).await?;
    service
        .authorize_sandbox_access(&identity, &session.sandbox_id)
        .await?;
    Ok(Json(SshSessionResponse::from(session)))
}

/// Terminate an SSH session.
#[axum::debug_handler]
pub async fn terminate_ssh_session(
    State(service): State<Arc<SshSessionService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<TerminateSessionRequest>,
) -> Result<StatusCode, ApiError> {
    let session = service.get_session(id).await?;
    service
        .authorize_sandbox_access(&identity, &session.sandbox_id)
        .await?;

    let reason = req.reason.unwrap_or_else(|| "API request".to_string());
    service.terminate_session(id, reason).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Update session activity (heartbeat).
///
/// Called by the SSH gateway to update activity timestamp and byte counts.
#[axum::debug_handler]
pub async fn update_session_activity(
    State(service): State<Arc<SshSessionService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<HeartbeatRequest>,
) -> Result<StatusCode, ApiError> {
    let session = service.get_session(id).await?;
    service
        .authorize_sandbox_access(&identity, &session.sandbox_id)
        .await?;

    service
        .update_activity(id, req.bytes_sent, req.bytes_received)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Get SSH session statistics.
///
/// Returns statistics about SSH sessions for monitoring and health checks.
#[axum::debug_handler]
pub async fn get_ssh_session_statistics(
    State(service): State<Arc<SshSessionService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
) -> Result<Json<SessionStatistics>, ApiError> {
    if !matches!(identity.key_type, ApiKeyType::Privileged) {
        return Err(ApiError::Validation {
            message: "SSH session statistics require a privileged API key".to_string(),
            field: None,
            code: ErrorCode::AuthorizationInsufficientPermissions,
        });
    }

    let stats = service.get_statistics().await?;
    Ok(Json(stats))
}

/// Optional API key header for internal service authentication.
#[derive(Debug, Clone)]
pub struct OptionalApiKey(pub Option<String>);

impl<S> FromRequestParts<S> for OptionalApiKey
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let api_key = match parts.headers.get("X-API-Key") {
            Some(value) => match value.to_str() {
                Ok(v) => Some(v.to_string()),
                Err(_) => return Err((StatusCode::UNAUTHORIZED, "Invalid header encoding")),
            },
            None => None,
        };

        Ok(OptionalApiKey(api_key))
    }
}

/// Internal authorization endpoint for SSH gateway.
///
/// This endpoint is called by the SSH gateway during SSH authentication
/// to validate that the sandbox exists and is running.
///
/// # Authentication
///
/// This endpoint requires X-API-Key header for internal service authentication.
/// In production, use mTLS between SSH gateway and API server for stronger security.
///
/// # Returns
///
/// State for SSH authorization handler
#[derive(Clone)]
pub struct SshAuthState {
    /// Sandbox service for looking up sandbox state
    pub service: Arc<SandboxService>,
    /// Expected API key for SSH gateway authentication
    pub api_key: Option<String>,
}

/// Returns authorization context with sandbox information and granted permissions.
#[instrument(skip(state, api_key_header), fields(sandbox_id = %sandbox_id))]
pub async fn authorize_ssh_access(
    Path(sandbox_id): Path<uuid::Uuid>,
    State(state): State<SshAuthState>,
    api_key_header: OptionalApiKey,
) -> Result<Json<AuthContext>, ApiError> {
    debug!("SSH authorization request for sandbox: {}", sandbox_id);
    let sandbox_service = &state.service;
    let api_key = &state.api_key;

    // 1. Validate API key if configured
    if let Some(expected_key) = &api_key {
        match &api_key_header.0 {
            Some(provided_key) if provided_key == expected_key => {
                debug!("API key validated successfully");
            }
            Some(_) => {
                return Err(ApiError::Authentication(
                    "Invalid API key for SSH gateway access".to_string(),
                ));
            }
            None => {
                return Err(ApiError::Authentication(
                    "Missing X-API-Key header for SSH gateway access".to_string(),
                ));
            }
        }
    } else {
        debug!("No SSH gateway API key configured, skipping API key validation");
    }

    // 2. Check sandbox exists in database
    let sandbox: crate::core::types::Sandbox = match sandbox_service.get_sandbox(&sandbox_id).await
    {
        Some(sandbox) => {
            debug!("Found sandbox: {:?}", sandbox);
            sandbox
        }
        None => {
            return Err(ApiError::SandboxNotFound(sandbox_id.to_string()));
        }
    };

    // 3. Check sandbox state is "running"
    let state_str = sandbox.state.as_str();
    if sandbox.state != crate::core::types::SandboxState::Running {
        return Err(ApiError::Validation {
            message: format!("Sandbox is not running (current state: {})", state_str),
            field: None,
            code: ErrorCode::SandboxInvalidState,
        });
    }

    // 4. Return container_id and permissions
    debug!("Sandbox authorized for SSH access");
    Ok(Json(AuthContext {
        authorized: true,
        sandbox: Some(SandboxInfo {
            id: sandbox_id,
            state: state_str.to_string(),
            container_id: sandbox.container_id,
        }),
        permissions: vec!["exec".to_string(), "pty".to_string()],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    // ============================================================================
    // Query Parameter Tests for list_ssh_sessions
    // ============================================================================

    #[test]
    fn test_list_ssh_sessions_query_default() {
        let query = ListSshSessionsQuery {
            sandbox_id: None,
            state: None,
            limit: None,
            offset: None,
        };

        assert!(query.sandbox_id.is_none());
        assert!(query.state.is_none());
        assert!(query.limit.is_none());
        assert!(query.offset.is_none());
    }

    #[test]
    fn test_list_ssh_sessions_query_with_uuid() {
        let uuid = uuid::Uuid::new_v4();
        let query = ListSshSessionsQuery {
            sandbox_id: Some(uuid),
            state: Some("active".to_string()),
            limit: Some(50),
            offset: Some(0),
        };

        assert_eq!(query.sandbox_id, Some(uuid));
        assert_eq!(query.state, Some("active".to_string()));
        assert_eq!(query.limit, Some(50));
        assert_eq!(query.offset, Some(0));
    }

    #[test]
    fn test_list_ssh_sessions_query_serialization() {
        let json = r#"{"sandbox_id":"550e8400-e29b-41d4-a716-446655440000","state":"active","limit":100,"offset":0}"#;
        let query: ListSshSessionsQuery = serde_json::from_str(json).unwrap();

        assert!(query.sandbox_id.is_some());
        assert_eq!(query.state, Some("active".to_string()));
        assert_eq!(query.limit, Some(100));
        assert_eq!(query.offset, Some(0));
    }

    // ============================================================================
    // TerminateSessionRequest Tests
    // ============================================================================

    #[test]
    fn test_terminate_session_request_default() {
        let req = TerminateSessionRequest { reason: None };
        assert!(req.reason.is_none());
    }

    #[test]
    fn test_terminate_session_request_with_reason() {
        let req = TerminateSessionRequest {
            reason: Some("User logout".to_string()),
        };
        assert_eq!(req.reason, Some("User logout".to_string()));
    }

    #[test]
    fn test_terminate_session_request_serialization() {
        let json = r#"{"reason":"Session timeout"}"#;
        let req: TerminateSessionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.reason, Some("Session timeout".to_string()));
    }

    #[test]
    fn test_terminate_session_request_empty_reason() {
        let json = r#"{"reason":""}"#;
        let req: TerminateSessionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.reason, Some("".to_string()));
    }

    // ============================================================================
    // HeartbeatRequest Tests
    // ============================================================================

    #[test]
    fn test_heartbeat_request_default() {
        let req = HeartbeatRequest {
            bytes_sent: 0,
            bytes_received: 0,
        };
        assert_eq!(req.bytes_sent, 0);
        assert_eq!(req.bytes_received, 0);
    }

    #[test]
    fn test_heartbeat_request_with_values() {
        let req = HeartbeatRequest {
            bytes_sent: 1024,
            bytes_received: 2048,
        };
        assert_eq!(req.bytes_sent, 1024);
        assert_eq!(req.bytes_received, 2048);
    }

    #[test]
    fn test_heartbeat_request_serialization() {
        let json = r#"{"bytes_sent":512,"bytes_received":1024}"#;
        let req: HeartbeatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.bytes_sent, 512);
        assert_eq!(req.bytes_received, 1024);
    }

    #[test]
    fn test_heartbeat_request_negative_bytes() {
        let req = HeartbeatRequest {
            bytes_sent: -1,
            bytes_received: -100,
        };
        assert_eq!(req.bytes_sent, -1);
        assert_eq!(req.bytes_received, -100);
    }

    #[test]
    fn test_heartbeat_request_max_values() {
        let req = HeartbeatRequest {
            bytes_sent: i64::MAX,
            bytes_received: i64::MAX,
        };
        assert_eq!(req.bytes_sent, i64::MAX);
        assert_eq!(req.bytes_received, i64::MAX);
    }

    // ============================================================================
    // AuthContext Tests
    // ============================================================================

    #[test]
    fn test_auth_context_authorized() {
        let auth = AuthContext {
            authorized: true,
            sandbox: None,
            permissions: vec!["read".to_string(), "write".to_string()],
        };

        assert!(auth.authorized);
        assert!(auth.sandbox.is_none());
        assert_eq!(auth.permissions.len(), 2);
    }

    #[test]
    fn test_auth_context_unauthorized() {
        let auth = AuthContext {
            authorized: false,
            sandbox: None,
            permissions: vec![],
        };

        assert!(!auth.authorized);
        assert!(auth.permissions.is_empty());
    }

    #[test]
    fn test_auth_context_serialization() {
        let sandbox = SandboxInfo {
            id: uuid::Uuid::new_v4(),
            state: "Running".to_string(),
            container_id: Some("container-123".to_string()),
        };

        let auth = AuthContext {
            authorized: true,
            sandbox: Some(sandbox),
            permissions: vec!["exec".to_string()],
        };

        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"authorized\":true"));
        assert!(json.contains("\"exec\""));
    }

    #[test]
    fn test_auth_context_with_sandbox() {
        let sandbox = SandboxInfo {
            id: uuid::Uuid::new_v4(),
            state: "Running".to_string(),
            container_id: Some("abc123".to_string()),
        };

        let auth = AuthContext {
            authorized: true,
            sandbox: Some(sandbox.clone()),
            permissions: vec![],
        };

        assert!(auth.sandbox.is_some());
        let s = auth.sandbox.unwrap();
        assert_eq!(s.id, sandbox.id);
        assert_eq!(s.state, "Running");
        assert_eq!(s.container_id, Some("abc123".to_string()));
    }

    // ============================================================================
    // SandboxInfo Tests
    // ============================================================================

    #[test]
    fn test_sandbox_info_creation() {
        let info = SandboxInfo {
            id: uuid::Uuid::new_v4(),
            state: "Running".to_string(),
            container_id: Some("container-123".to_string()),
        };

        assert_eq!(info.state, "Running");
        assert!(info.container_id.is_some());
    }

    #[test]
    fn test_sandbox_info_without_container() {
        let info = SandboxInfo {
            id: uuid::Uuid::new_v4(),
            state: "Stopped".to_string(),
            container_id: None,
        };

        assert_eq!(info.state, "Stopped");
        assert!(info.container_id.is_none());
    }

    #[test]
    fn test_sandbox_info_serialization() {
        let info = SandboxInfo {
            id: uuid::Uuid::new_v4(),
            state: "Creating".to_string(),
            container_id: Some("test-container".to_string()),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"Creating\""));
        assert!(json.contains("test-container"));
    }

    #[test]
    fn test_sandbox_info_all_states() {
        let states = vec!["Creating", "Running", "Stopped", "Error"];

        for state in states {
            let info = SandboxInfo {
                id: uuid::Uuid::new_v4(),
                state: state.to_string(),
                container_id: None,
            };

            assert_eq!(info.state, state);
        }
    }

    // ============================================================================
    // ErrorResponse Tests
    // ============================================================================

    #[test]
    fn test_error_response_creation() {
        let err = ErrorResponse {
            error: "Not found".to_string(),
            hint: Some("Check the ID".to_string()),
        };

        assert_eq!(err.error, "Not found");
        assert_eq!(err.hint, Some("Check the ID".to_string()));
    }

    #[test]
    fn test_error_response_without_hint() {
        let err = ErrorResponse {
            error: "Unauthorized".to_string(),
            hint: None,
        };

        assert_eq!(err.error, "Unauthorized");
        assert!(err.hint.is_none());
    }

    #[test]
    fn test_error_response_serialization() {
        let err = ErrorResponse {
            error: "Invalid input".to_string(),
            hint: Some("Provide valid UUID".to_string()),
        };

        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("Invalid input"));
        assert!(json.contains("Provide valid UUID"));
    }

    #[test]
    fn test_error_response_with_newlines() {
        let err = ErrorResponse {
            error: "Error on line 1\nLine 2".to_string(),
            hint: Some("Hint 1\nHint 2".to_string()),
        };

        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("line 1"));
        assert!(json.contains("Hint 1"));
    }

    #[test]
    fn test_error_response_with_unicode() {
        let err = ErrorResponse {
            error: "错误".to_string(),
            hint: Some("提示".to_string()),
        };

        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("错误"));
        assert!(json.contains("提示"));
    }

    // ============================================================================
    // Edge Cases and Boundary Values
    // ============================================================================

    #[test]
    fn test_query_params_with_zero_values() {
        let query = ListSshSessionsQuery {
            sandbox_id: None,
            state: None,
            limit: Some(0),
            offset: Some(0),
        };

        assert_eq!(query.limit, Some(0));
        assert_eq!(query.offset, Some(0));
    }

    #[test]
    fn test_query_params_with_max_values() {
        let query = ListSshSessionsQuery {
            sandbox_id: Some(uuid::Uuid::new_v4()),
            state: Some("active".to_string()),
            limit: Some(usize::MAX),
            offset: Some(usize::MAX),
        };

        assert_eq!(query.limit, Some(usize::MAX));
        assert_eq!(query.offset, Some(usize::MAX));
    }

    #[test]
    fn test_heartbeat_with_large_bytes() {
        let req = HeartbeatRequest {
            bytes_sent: 100_000_000_000,
            bytes_received: 200_000_000_000,
        };

        assert_eq!(req.bytes_sent, 100_000_000_000);
        assert_eq!(req.bytes_received, 200_000_000_000);
    }

    #[test]
    fn test_permissions_vector() {
        let auth = AuthContext {
            authorized: true,
            sandbox: None,
            permissions: vec![
                "read".to_string(),
                "write".to_string(),
                "exec".to_string(),
                "admin".to_string(),
            ],
        };

        assert_eq!(auth.permissions.len(), 4);
        assert!(auth.permissions.contains(&"exec".to_string()));
    }

    #[test]
    fn test_empty_permissions() {
        let auth = AuthContext {
            authorized: false,
            sandbox: None,
            permissions: vec![],
        };

        assert!(auth.permissions.is_empty());
        assert!(!auth.authorized);
    }

    #[test]
    fn test_auth_context_roundtrip() {
        let original = AuthContext {
            authorized: true,
            sandbox: Some(SandboxInfo {
                id: uuid::Uuid::new_v4(),
                state: "Running".to_string(),
                container_id: Some("container-123".to_string()),
            }),
            permissions: vec!["read".to_string()],
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: AuthContext = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.authorized, original.authorized);
        assert_eq!(deserialized.permissions, original.permissions);
    }

    #[test]
    fn test_multiple_error_responses() {
        let errors = [
            ErrorResponse {
                error: "Error 1".to_string(),
                hint: None,
            },
            ErrorResponse {
                error: "Error 2".to_string(),
                hint: Some("Hint 2".to_string()),
            },
        ];

        assert_eq!(errors[0].error, "Error 1");
        assert_eq!(errors[1].error, "Error 2");
        assert_eq!(errors[1].hint, Some("Hint 2".to_string()));
    }
}
