// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Core Business Logic
//!
//! This module contains the core business logic for managing sandboxes.

pub mod activities;
pub mod features;
pub mod manager;
pub mod sandbox;
pub mod ssh_service;
pub mod state;
pub mod static_files;
pub mod store_trait;
pub mod types;

pub mod errors {
    #![allow(missing_docs)]

    use axum::http::StatusCode;
    use std::collections::HashMap;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ErrorCode {
        SandboxNotFound, SandboxInvalidState, SandboxAlreadyExists,
        SandboxCreationFailed, SandboxExecutionFailed, ToolNotFound,
        ToolExecutionFailed, ToolValidationError, ToolTimeout,
        BackendImagePullFailed, BackendContainerCreateFailed, BackendContainerStartFailed,
        BackendVolumeError, BackendContainerNotFound, BackendExecFailed,
        SshSessionNotFound, SshAuthenticationFailed, SshConnectionFailed,
        TerminalOperationFailed, ValidationError, ValidationInvalidPort,
        ValidationMissingField, ValidationInvalidImageName, ValidationInvalidRequest,
        AuthenticationMissing, AuthenticationInvalidApiKey, AuthorizationInsufficientPermissions,
        DatabaseConnectionFailed, DatabaseQueryFailed, ServiceUnavailable,
        RateLimitExceeded, UpstreamError, RequestTimeout, InternalError, ConfigurationError,
    }

    impl ErrorCode {
        pub fn as_str(&self) -> &'static str {
            match self {
                Self::SandboxNotFound => "SANDBOX_NOT_FOUND",
                Self::SandboxInvalidState => "SANDBOX_INVALID_STATE",
                Self::SandboxAlreadyExists => "SANDBOX_ALREADY_EXISTS",
                Self::SandboxCreationFailed => "SANDBOX_CREATION_FAILED",
                Self::SandboxExecutionFailed => "SANDBOX_EXECUTION_FAILED",
                Self::ToolNotFound => "TOOL_NOT_FOUND",
                Self::ToolExecutionFailed => "TOOL_EXECUTION_FAILED",
                Self::ToolValidationError => "TOOL_VALIDATION_ERROR",
                Self::ToolTimeout => "TOOL_TIMEOUT",
                Self::BackendImagePullFailed => "BACKEND_IMAGE_PULL_FAILED",
                Self::BackendContainerCreateFailed => "BACKEND_CONTAINER_CREATE_FAILED",
                Self::BackendContainerStartFailed => "BACKEND_CONTAINER_START_FAILED",
                Self::BackendVolumeError => "BACKEND_VOLUME_ERROR",
                Self::BackendContainerNotFound => "BACKEND_CONTAINER_NOT_FOUND",
                Self::BackendExecFailed => "BACKEND_EXEC_FAILED",
                Self::SshSessionNotFound => "SSH_SESSION_NOT_FOUND",
                Self::SshAuthenticationFailed => "SSH_AUTHENTICATION_FAILED",
                Self::SshConnectionFailed => "SSH_CONNECTION_FAILED",
                Self::TerminalOperationFailed => "TERMINAL_OPERATION_FAILED",
                Self::ValidationError => "VALIDATION_ERROR",
                Self::ValidationInvalidPort => "VALIDATION_INVALID_PORT",
                Self::ValidationMissingField => "VALIDATION_MISSING_FIELD",
                Self::ValidationInvalidImageName => "VALIDATION_INVALID_IMAGE_NAME",
                Self::ValidationInvalidRequest => "VALIDATION_INVALID_REQUEST",
                Self::AuthenticationMissing => "AUTHENTICATION_MISSING",
                Self::AuthenticationInvalidApiKey => "AUTHENTICATION_INVALID_API_KEY",
                Self::AuthorizationInsufficientPermissions => "AUTHORIZATION_INSUFFICIENT_PERMISSIONS",
                Self::DatabaseConnectionFailed => "DATABASE_CONNECTION_FAILED",
                Self::DatabaseQueryFailed => "DATABASE_QUERY_FAILED",
                Self::ServiceUnavailable => "SERVICE_UNAVAILABLE",
                Self::RateLimitExceeded => "RATE_LIMIT_EXCEEDED",
                Self::UpstreamError => "UPSTREAM_ERROR",
                Self::RequestTimeout => "REQUEST_TIMEOUT",
                Self::InternalError => "INTERNAL_ERROR",
                Self::ConfigurationError => "CONFIGURATION_ERROR",
            }
        }

        pub fn parse(s: &str) -> Option<Self> {
            match s {
                "SANDBOX_NOT_FOUND" => Some(Self::SandboxNotFound),
                "SANDBOX_INVALID_STATE" => Some(Self::SandboxInvalidState),
                "SANDBOX_ALREADY_EXISTS" => Some(Self::SandboxAlreadyExists),
                "SANDBOX_CREATION_FAILED" => Some(Self::SandboxCreationFailed),
                "SANDBOX_EXECUTION_FAILED" => Some(Self::SandboxExecutionFailed),
                "TOOL_NOT_FOUND" => Some(Self::ToolNotFound),
                "TOOL_EXECUTION_FAILED" => Some(Self::ToolExecutionFailed),
                "TOOL_VALIDATION_ERROR" => Some(Self::ToolValidationError),
                "TOOL_TIMEOUT" => Some(Self::ToolTimeout),
                "BACKEND_IMAGE_PULL_FAILED" => Some(Self::BackendImagePullFailed),
                "BACKEND_CONTAINER_CREATE_FAILED" => Some(Self::BackendContainerCreateFailed),
                "BACKEND_CONTAINER_START_FAILED" => Some(Self::BackendContainerStartFailed),
                "BACKEND_VOLUME_ERROR" => Some(Self::BackendVolumeError),
                "BACKEND_CONTAINER_NOT_FOUND" => Some(Self::BackendContainerNotFound),
                "BACKEND_EXEC_FAILED" => Some(Self::BackendExecFailed),
                "DOCKER_IMAGE_PULL_FAILED" => Some(Self::BackendImagePullFailed),
                "DOCKER_CONTAINER_CREATE_FAILED" => Some(Self::BackendContainerCreateFailed),
                "DOCKER_CONTAINER_START_FAILED" => Some(Self::BackendContainerStartFailed),
                "DOCKER_VOLUME_ERROR" => Some(Self::BackendVolumeError),
                "DOCKER_CONTAINER_NOT_FOUND" => Some(Self::BackendContainerNotFound),
                "DOCKER_EXEC_FAILED" => Some(Self::BackendExecFailed),
                "SSH_SESSION_NOT_FOUND" => Some(Self::SshSessionNotFound),
                "SSH_AUTHENTICATION_FAILED" => Some(Self::SshAuthenticationFailed),
                "SSH_CONNECTION_FAILED" => Some(Self::SshConnectionFailed),
                "TERMINAL_OPERATION_FAILED" => Some(Self::TerminalOperationFailed),
                "VALIDATION_ERROR" => Some(Self::ValidationError),
                "VALIDATION_INVALID_PORT" => Some(Self::ValidationInvalidPort),
                "VALIDATION_MISSING_FIELD" => Some(Self::ValidationMissingField),
                "VALIDATION_INVALID_IMAGE_NAME" => Some(Self::ValidationInvalidImageName),
                "VALIDATION_INVALID_REQUEST" => Some(Self::ValidationInvalidRequest),
                "AUTHENTICATION_MISSING" => Some(Self::AuthenticationMissing),
                "AUTHENTICATION_INVALID_API_KEY" => Some(Self::AuthenticationInvalidApiKey),
                "AUTHORIZATION_INSUFFICIENT_PERMISSIONS" => Some(Self::AuthorizationInsufficientPermissions),
                "DATABASE_CONNECTION_FAILED" => Some(Self::DatabaseConnectionFailed),
                "DATABASE_QUERY_FAILED" => Some(Self::DatabaseQueryFailed),
                "SERVICE_UNAVAILABLE" => Some(Self::ServiceUnavailable),
                "RATE_LIMIT_EXCEEDED" => Some(Self::RateLimitExceeded),
                "UPSTREAM_ERROR" => Some(Self::UpstreamError),
                "REQUEST_TIMEOUT" => Some(Self::RequestTimeout),
                "INTERNAL_ERROR" => Some(Self::InternalError),
                "CONFIGURATION_ERROR" => Some(Self::ConfigurationError),
                _ => None,
            }
        }

        pub fn http_status(&self) -> StatusCode {
            match self {
                Self::ValidationError | Self::ValidationInvalidPort | Self::ValidationMissingField
                | Self::ValidationInvalidImageName | Self::ValidationInvalidRequest | Self::ToolValidationError => StatusCode::BAD_REQUEST,
                Self::AuthenticationMissing | Self::AuthenticationInvalidApiKey => StatusCode::UNAUTHORIZED,
                Self::AuthorizationInsufficientPermissions => StatusCode::FORBIDDEN,
                Self::SandboxNotFound | Self::ToolNotFound | Self::BackendContainerNotFound | Self::SshSessionNotFound => StatusCode::NOT_FOUND,
                Self::ToolTimeout | Self::RequestTimeout => StatusCode::REQUEST_TIMEOUT,
                Self::SandboxAlreadyExists | Self::SandboxInvalidState => StatusCode::CONFLICT,
                Self::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
                Self::BackendImagePullFailed | Self::BackendContainerCreateFailed | Self::BackendContainerStartFailed
                | Self::BackendVolumeError | Self::BackendExecFailed | Self::SshConnectionFailed | Self::UpstreamError => StatusCode::BAD_GATEWAY,
                Self::DatabaseConnectionFailed | Self::DatabaseQueryFailed | Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
                Self::SandboxCreationFailed | Self::SandboxExecutionFailed | Self::ToolExecutionFailed
                | Self::SshAuthenticationFailed | Self::TerminalOperationFailed | Self::InternalError | Self::ConfigurationError => StatusCode::INTERNAL_SERVER_ERROR,
            }
        }

        pub fn is_retryable(&self) -> bool {
            matches!(self, Self::ServiceUnavailable | Self::RateLimitExceeded | Self::DatabaseConnectionFailed
                | Self::BackendImagePullFailed | Self::BackendContainerCreateFailed | Self::BackendContainerStartFailed
                | Self::BackendExecFailed | Self::UpstreamError | Self::RequestTimeout | Self::ToolTimeout)
        }
    }

    #[derive(Debug, thiserror::Error)]
    pub enum ApiError {
        #[error("Sandbox not found: {0}")]
        SandboxNotFound(String),
        #[error("Validation error: {message}")]
        Validation { message: String, field: Option<String>, code: ErrorCode },
        #[error("Backend error: {message}")]
        Backend { message: String, operation: String, code: ErrorCode, #[source] source: Option<Box<dyn std::error::Error + Send + Sync>> },
        #[error("Database error: {message}")]
        Database { message: String, code: ErrorCode, #[source] source: Option<Box<dyn std::error::Error + Send + Sync>> },
        #[error("Authentication failed: {0}")]
        Authentication(String),
        #[error("Internal server error: {0}")]
        Internal(String),
    }

    impl ApiError {
        pub fn error_code(&self) -> ErrorCode {
            match self {
                Self::SandboxNotFound(_) => ErrorCode::SandboxNotFound,
                Self::Validation { code, .. } => *code,
                Self::Backend { code, .. } => *code,
                Self::Database { code, .. } => *code,
                Self::Authentication(_) => ErrorCode::AuthenticationInvalidApiKey,
                Self::Internal(_) => ErrorCode::InternalError,
            }
        }

        pub fn suggestions(&self) -> Vec<String> {
            match self {
                Self::SandboxNotFound(_) => vec!["Verify the sandbox ID is correct".to_string(), "List all sandboxes with GET /sandboxes".to_string(), "Check if the sandbox has been deleted".to_string()],
                Self::Backend { operation, .. } => vec![format!("Backend operation '{}' failed. Check backend service status.", operation), "Verify image name is correct".to_string(), "Check system resources (disk, memory)".to_string()],
                Self::Validation { field, .. } => {
                    let mut suggs = vec!["Check the request body format".to_string()];
                    if let Some(f) = field { suggs.push(format!("Verify the '{}' field value", f)); }
                    suggs
                }
                Self::Authentication(_) => vec!["Provide a valid API key via X-API-Key header".to_string(), "Generate a new API key from the admin dashboard".to_string()],
                Self::Database { .. } => vec!["The database is temporarily unavailable".to_string(), "Try again in a few moments".to_string(), "Contact support if the problem persists".to_string()],
                Self::Internal(_) => vec!["An unexpected error occurred".to_string(), "Check the server logs for details".to_string(), "Contact support if the problem persists".to_string()],
            }
        }

        pub(crate) fn extract_metadata(&self) -> HashMap<String, serde_json::Value> {
            let mut metadata = HashMap::new();
            match self {
                Self::SandboxNotFound(id) => { metadata.insert("sandbox_id".to_string(), serde_json::json!(id)); }
                Self::Validation { field: Some(f), .. } => { metadata.insert("field".to_string(), serde_json::json!(f)); }
                Self::Backend { operation, .. } => { metadata.insert("operation".to_string(), serde_json::json!(operation)); }
                _ => {}
            }
            metadata
        }
    }
}

pub use activities::ActivityService;
pub use errors::{ApiError, ErrorCode};
pub use manager::{ManagerError, ManagerResult, SandboxManager};
pub use sandbox::{ExecToolHttpRequest, ListSandboxesFilter, SandboxService};
pub use ssh_service::{SshServiceError, SshSessionService};
pub use state::StateStore;
pub use static_files::{StaticFileMetadata, StaticFileService};
pub use store_trait::StateStoreTrait;
pub use types::{
    ActivityResponse, ActivityType, ApiKeyIdentity, ApiKeyType, CreateSandboxRequest,
    CreateSshSessionRequest, ImageDetails, ImageSummary, KubernetesInfo, PortMapping, PortProtocol,
    ResourceLimits, Sandbox, SandboxConfig, SandboxInfo, SandboxResponse, SandboxState,
    SshAuthMethod, SshSession, SshSessionFilters, SshSessionResponse, SshSessionState,
};
