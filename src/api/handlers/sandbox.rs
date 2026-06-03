// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Sandbox API Handlers
//!
//! HTTP request handlers for sandbox CRUD operations.
//!
//! ## Overview
//!
//! This module provides async HTTP endpoint handlers for managing Docker sandboxes:
//! - Create sandboxes (with optional SSE progress streaming)
//! - Get/list/delete/stop sandboxes
//! - Execute commands in sandboxes
//! - Get/stream sandbox statistics
//! - Cleanup sandbox resources
//!
//! ## Testing Strategy
//!
//! ### Unit Tests (tests/sandbox/tests.rs - 50+ tests)
//! Testable pure logic without HTTP server:
//! - Request/response struct serialization/deserialization
//! - Shell command wrapping (`wrap_shell_command`)
//! - Error message classification
//! - Edge cases and boundary conditions
//! - Type trait bounds (Send, Sync, Clone)
//!
//! ### Integration Tests
//! HTTP handler logic tested in:
//! - **`tests/cli_http_integration_tests.rs`**: Full HTTP server tests
//! - **`tests/api_server_e2e.rs`**: End-to-end API tests
//!
//! Integration tests cover:
//! - Full request/response cycles
//! - Error handling with actual Docker operations
//! - SSE streaming functionality
//! - Authentication and authorization
//! - Concurrent requests
//!
//! ### Why Low Unit Test Coverage?
//!
//! The HTTP handler functions (e.g., `create_sandbox`, `get_sandbox`) require:
//! - Running HTTP server (Axum)
//! - Mocked `SandboxService` or real Docker
//! - Async runtime and state management
//!
//! These are better tested as integration tests rather than unit tests.
//! The pure logic that can be unit tested is already covered (50+ tests).

use axum::{
    extract::{Extension, Multipart, Path, Query as AxumQuery, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json;
use std::convert::Infallible;
use std::sync::Arc;

use crate::api::ApiError;
use crate::core::errors::ErrorCode;
use crate::core::types::ApiKeyIdentity;
use crate::core::types::SandboxState;
use crate::core::{
    CreateSandboxRequest, ExecToolHttpRequest, SandboxConfig, SandboxResponse, SandboxService,
};

/// Request for executing a shell command in a sandbox
///
/// Used for arbitrary shell command execution via subprocess.
///
/// Example:
/// ```json
/// {
///   "command": ["echo", "hello"],
///   "stdin": "optional input",
///   "working_dir": "/tmp",
///   "environment": {"KEY": "value"},
///   "timeout": 60
/// }
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecSandboxRequest {
    /// Command to execute (will be wrapped with `sh -c`)
    pub command: Vec<String>,
    /// Optional stdin passed to the process
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    /// Working directory for command execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Environment variables for command execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<std::collections::HashMap<String, String>>,
    /// Timeout in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// Response for sandbox command execution
#[derive(Debug, Serialize, Deserialize)]
pub struct ExecSandboxResponse {
    /// Command output (stdout + stderr combined)
    pub output: String,
    /// Process exit code
    pub exit_code: i32,
}

/// Request for executing a tool action in a sandbox
///
/// Used for HTTP-based tool execution via tool_proxy.py.
/// Supports web_tools.py, databend_tools.py, browser_tools.js.
///
/// Example:
/// ```json
/// {
///   "interpreter": "python",
///   "script_path": "/opt/tools/web_tools.py",
///   "action": "web_scrape",
///   "args": {"url": "https://example.com"},
///   "timeout": 60,
///   "environment": {"DATABEND_CONNECTION_STRING": "..."}
/// }
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolExecutionRequest {
    /// Interpreter: "python" or "node"
    pub interpreter: String,
    /// Path to the tool script (e.g., "/opt/tools/web_tools.py")
    pub script_path: String,
    /// Action/function to call (e.g., "web_scrape")
    pub action: String,
    /// Arguments to pass to the action (JSON object)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
    /// Timeout in seconds (default: 60s for databend_tools/web_tools, 120s for browser_tools)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    /// Environment variables to set for this tool execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<std::collections::HashMap<String, String>>,
}

/// File metadata for upload responses
#[derive(Debug, Serialize, Deserialize)]
pub struct FileInfo {
    /// File name
    pub name: String,
    /// File path in the sandbox
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Upload timestamp
    pub uploaded_at: chrono::DateTime<chrono::Utc>,
}

/// Response for file upload operations
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadFileResponse {
    /// Whether the upload was successful
    pub success: bool,
    /// Uploaded file metadata
    pub file: FileInfo,
}

/// Query parameters for listing sandboxes with filtering and pagination
#[derive(Debug, serde::Deserialize)]
pub struct ListSandboxesQuery {
    /// Include soft-deleted sandboxes in results
    #[serde(default)]
    pub include_deleted: bool,
    /// Filter by sandbox state (e.g., "running", "stopped")
    pub state: Option<String>,
    /// Filter by image name (partial match)
    pub image: Option<String>,
    /// Filter sandboxes created after this timestamp
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter sandboxes created before this timestamp
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
    /// Page number for pagination (1-based)
    pub page: Option<usize>,
    /// Number of items per page (max 200)
    pub per_page: Option<usize>,
}

/// Query parameters for retrieving a single sandbox
#[derive(Debug, serde::Deserialize)]
pub struct GetSandboxQuery {
    /// Include soft-deleted sandboxes in the lookup
    #[serde(default)]
    pub include_deleted: bool,
}

/// Query parameters for file download
#[derive(Debug, serde::Deserialize)]
pub struct DownloadParams {
    /// Source file path in the sandbox container
    pub path: Option<String>,
    /// Content disposition: "inline" or "attachment"
    pub disposition: Option<String>,
}

/// Create a new sandbox
///
/// POST /sandboxes
///
/// Creates a new sandbox container with the specified configuration.
/// Validates the request, checks ownership, and returns the created sandbox.
pub async fn create_sandbox(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Json(req): Json<CreateSandboxRequest>,
) -> Response {
    // Validate request before processing
    if let Err(e) = req.validate() {
        return e.into_response();
    }

    // Capture values for logging before moving into config
    let image = req.image.clone();
    let name = req.name.clone();

    // Log sandbox creation start (info level for business event)
    tracing::info!(
        image = %image,
        name = ?name,
        "Creating sandbox"
    );

    let config = SandboxConfig {
        image: req.image,
        name: req.name,
        environment: req.environment.unwrap_or_default(),
        port_mappings: req.port_mappings.unwrap_or_default(),
        exposed_ports: Vec::new(),
        resource_limits: req.resource_limits.unwrap_or_default(),
        volumes: req.volumes.unwrap_or_default(),
        command: req.command,
        inactivity_timeout_minutes: req.inactivity_timeout_minutes,
        pull_policy: req.pull_policy,
        features: req.features,
        enable_all_features: req.enable_all_features,
        vnc_resolution: req.vnc_resolution,
    };

    let result = service.create_sandbox(config, Some(identity)).await;

    match result {
        Ok(sandbox) => {
            // Log successful creation with structured context
            tracing::info!(
                sandbox_id = %sandbox.id,
                image = %sandbox.config.image,
                state = ?sandbox.state,
                "Sandbox created successfully"
            );

            let response = SandboxResponse::from(sandbox);
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => {
            // Log failure with structured context
            tracing::error!(
                error = %e,
                image = %image,
                name = ?name,
                "Failed to create sandbox"
            );
            let api_error: ApiError = e.into();
            api_error.into_response()
        }
    }
}

/// Creates a sandbox with real-time progress streaming via SSE.
///
/// This endpoint provides Server-Sent Events (SSE) streaming progress updates
/// during sandbox creation, including image pull progress, container creation,
/// and container startup.
///
/// # Progress Events
///
/// - `pulling`: Image pull progress with current/total bytes
/// - `creating`: Container creation started
/// - `starting`: Container startup started
/// - `ready`: Sandbox is ready and running
/// - `error`: Operation failed
///
/// # Example Client Usage
///
/// ```bash
/// curl -N -H "Accept: text/event-stream" \
///   -X POST http://localhost:8080/sandboxes/create-stream \
///   -H "Content-Type: application/json" \
///   -d '{"image": "nginx:latest"}'
/// ```
pub async fn create_sandbox_stream(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Json(req): Json<CreateSandboxRequest>,
) -> Response {
    // Validate request before processing
    if let Err(e) = req.validate() {
        return e.into_response();
    }

    let config = SandboxConfig {
        image: req.image,
        name: req.name,
        environment: req.environment.unwrap_or_default(),
        port_mappings: req.port_mappings.unwrap_or_default(),
        exposed_ports: Vec::new(),
        resource_limits: req.resource_limits.unwrap_or_default(),
        volumes: req.volumes.unwrap_or_default(),
        command: req.command,
        inactivity_timeout_minutes: req.inactivity_timeout_minutes,
        pull_policy: req.pull_policy,
        features: req.features,
        enable_all_features: req.enable_all_features,
        vnc_resolution: req.vnc_resolution,
    };

    match service
        .create_sandbox_with_progress(config, Some(identity))
        .await
    {
        Ok(mut receiver) => {
            // Create SSE stream from channel receiver
            let stream = async_stream::stream! {
                while let Some(event) = receiver.recv().await {
                    match serde_json::to_string(&event) {
                        Ok(json) => {
                            yield Ok::<_, Infallible>(Event::default().data(json));
                        }
                        Err(_) => {
                            // Failed to serialize, end stream
                            break;
                        }
                    }
                }
            };

            Sse::new(stream)
                .keep_alive(
                    axum::response::sse::KeepAlive::new()
                        .interval(std::time::Duration::from_secs(5))
                        .text("keepalive"),
                )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to start streaming: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Get a sandbox by ID
///
/// GET /sandboxes/{id}
///
/// Returns the sandbox details including state, config, and Kubernetes status if applicable.
/// Checks ownership before returning the sandbox.
pub async fn get_sandbox(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
    AxumQuery(query): AxumQuery<GetSandboxQuery>,
) -> Response {
    // Return 404 immediately if sandbox doesn't exist
    if service.get_sandbox_with_deleted(&id, true).await.is_none() {
        return (StatusCode::NOT_FOUND, "Sandbox not found").into_response();
    }

    // Check ownership first
    if let Err(e) = service
        .check_sandbox_ownership_with_deleted(&identity, &id, query.include_deleted)
        .await
    {
        return e.into_response();
    }

    match service
        .get_sandbox_with_deleted(&id, query.include_deleted)
        .await
    {
        Some(sandbox) => {
            // Get Kubernetes-specific status if available
            let kubernetes = service
                .backend
                .get_sandbox_k8s_status(&id)
                .await
                .ok()
                .flatten();

            let mut response = SandboxResponse::from(sandbox);
            response.kubernetes = kubernetes;
            (StatusCode::OK, Json(response)).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Sandbox not found").into_response(),
    }
}

/// List all sandboxes with optional filtering and pagination
///
/// GET /sandboxes
///
/// Returns a paginated list of sandboxes. Database API keys only see their own sandboxes;
/// privileged keys see all sandboxes. Supports filtering by state, image, and date range.
pub async fn list_sandboxes(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    AxumQuery(query): AxumQuery<ListSandboxesQuery>,
) -> Json<serde_json::Value> {
    // For database keys, only list sandboxes owned by this key
    // For privileged keys, list all sandboxes (including orphaned ones with api_key_id = NULL)
    let mut sandboxes = if matches!(identity.key_type, crate::core::types::ApiKeyType::Database) {
        match identity.id {
            Some(api_key_id) => {
                service
                    .list_sandboxes_owned_by(&api_key_id, query.include_deleted)
                    .await
            }
            None => Vec::new(),
        }
    } else {
        // Legacy/admin keys with no specific ID - list all sandboxes
        service.list_sandboxes().await
    };

    // Apply filters
    if !query.include_deleted {
        sandboxes.retain(|s| s.deleted_at.is_none());
    }

    if let Some(ref state_str) = query.state {
        if let Ok(state) = state_str.parse::<SandboxState>() {
            sandboxes.retain(|s| s.state == state);
        }
    }

    if let Some(ref image_pattern) = query.image {
        sandboxes.retain(|s| s.config.image.contains(image_pattern));
    }

    if let Some(after) = query.created_after {
        sandboxes.retain(|s| s.created_at >= after);
    }

    if let Some(before) = query.created_before {
        sandboxes.retain(|s| s.created_at <= before);
    }

    // Apply pagination
    let total = sandboxes.len();
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) * per_page;

    let total_pages = ((total as f64) / (per_page as f64)).ceil() as usize;
    let has_next = page < total_pages;
    let has_prev = page > 1;

    let paginated_sandboxes: Vec<_> = sandboxes
        .into_iter()
        .skip(offset)
        .take(per_page)
        .map(SandboxResponse::from)
        .collect();

    let response = serde_json::json!({
        "data": paginated_sandboxes,
        "pagination": {
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": total_pages,
            "has_next": has_next,
            "has_prev": has_prev,
        }
    });

    Json(response)
}

/// Delete a sandbox by ID
///
/// DELETE /sandboxes/{id}
///
/// Permanently removes a sandbox and its resources. Checks ownership before deletion.
/// Returns 204 No Content on success.
pub async fn delete_sandbox(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    // Return 404 immediately if sandbox doesn't exist
    if service.get_sandbox_with_deleted(&id, true).await.is_none() {
        return (StatusCode::NOT_FOUND, "Sandbox not found").into_response();
    }

    // Check ownership first
    if let Err(e) = service
        .check_sandbox_ownership_with_deleted(&identity, &id, true)
        .await
    {
        return e.into_response();
    }

    match service.delete_sandbox(&id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            let err_msg = e.to_string();
            if err_msg.contains("not found") {
                (StatusCode::NOT_FOUND, "Sandbox not found").into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to delete sandbox",
                )
                    .into_response()
            }
        }
    }
}

/// Restore a previously soft-deleted sandbox
///
/// POST /sandboxes/{id}/restore
///
/// Restores a sandbox that was soft-deleted. Checks ownership before restoring.
/// Returns the restored sandbox on success.
pub async fn restore_sandbox(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    // Check ownership first
    if let Err(e) = service
        .check_sandbox_ownership_with_deleted(&identity, &id, true)
        .await
    {
        return e.into_response();
    }

    // Note: record_api_activity is now called inside restore_sandbox method
    // to ensure it updates the timestamp after successful restore
    match service.restore_sandbox(&id).await {
        Ok(_) => {
            // Return the restored sandbox
            // Use get_sandbox_with_deleted to handle any timing issues with deleted_at field
            if let Some(sandbox) = service.get_sandbox_with_deleted(&id, true).await {
                let kubernetes = service
                    .backend
                    .get_sandbox_k8s_status(&id)
                    .await
                    .ok()
                    .flatten();
                let mut response = SandboxResponse::from(sandbox);
                response.kubernetes = kubernetes;
                (StatusCode::OK, Json(response)).into_response()
            } else {
                (StatusCode::NOT_FOUND, "Sandbox not found after restore").into_response()
            }
        }
        Err(e) => {
            let error_msg = e.to_string();
            let error_msg_lower = error_msg.to_lowercase();

            // Check for specific error types
            if error_msg_lower.contains("not found") {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "error": "Sandbox not found or cannot be restored"
                    })),
                )
                    .into_response()
            } else if error_msg_lower.contains("not deleted") {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "Sandbox is not deleted and cannot be restored"
                    })),
                )
                    .into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Failed to restore sandbox: {}", error_msg)
                    })),
                )
                    .into_response()
            }
        }
    }
}

/// Stop a running sandbox
///
/// POST /sandboxes/{id}/stop
///
/// Stops a running sandbox container. Checks ownership before stopping.
/// Returns the updated sandbox state on success.
pub async fn stop_sandbox(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    // Check ownership first
    if let Err(e) = service.check_sandbox_ownership(&identity, &id).await {
        return e.into_response();
    }

    // Record API activity
    let _ = service.record_api_activity(&id).await;

    match service.stop_sandbox(&id).await {
        Ok(_) => {
            if let Some(sandbox) = service.get_sandbox(&id).await {
                let kubernetes = service
                    .backend
                    .get_sandbox_k8s_status(&id)
                    .await
                    .ok()
                    .flatten();
                let mut response = SandboxResponse::from(sandbox);
                response.kubernetes = kubernetes;
                (StatusCode::OK, Json(response)).into_response()
            } else {
                (StatusCode::NOT_FOUND, "Sandbox not found").into_response()
            }
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to stop sandbox").into_response(),
    }
}

/// Wraps a command with `sh -c` for shell execution.
///
/// This ensures proper shell interpretation of the command, including:
/// - Word splitting for arguments
/// - Shell operators (pipes, redirections, chaining)
/// - Variable expansion and glob patterns
fn wrap_shell_command(command: Vec<String>) -> Vec<String> {
    // If already wrapped with sh -c, don't wrap again
    if command.len() >= 2 && command[0] == "sh" && command[1] == "-c" {
        return command;
    }

    // Join command parts with spaces for shell execution
    let shell_command = command.join(" ");
    vec!["sh".to_string(), "-c".to_string(), shell_command]
}

/// Sanitizes a file path to prevent directory traversal attacks.
///
/// Uses `std::path::Path::components()` to normalize the path, rejects
/// absolute paths, strips/rejects control characters and null bytes,
/// and removes `..` traversal.
///
/// This allows the service layer to resolve relative paths to the container's
/// working directory, which is the correct behavior.
fn sanitize_path(path: &str) -> Result<String, ApiError> {
    // Reject null bytes
    if path.contains('\0') {
        return Err(ApiError::Validation {
            message: "Invalid path: contains null bytes".to_string(),
            field: Some("path".to_string()),
            code: ErrorCode::ValidationError,
        });
    }

    // Reject control characters
    if path.chars().any(|c| c.is_control()) {
        return Err(ApiError::Validation {
            message: "Invalid path: contains control characters".to_string(),
            field: Some("path".to_string()),
            code: ErrorCode::ValidationError,
        });
    }

    // Reject backslashes
    if path.contains('\\') {
        return Err(ApiError::Validation {
            message: "Invalid path: contains backslashes".to_string(),
            field: Some("path".to_string()),
            code: ErrorCode::ValidationError,
        });
    }

    // Use Path::components() to normalize and validate
    let path_obj = std::path::Path::new(path);
    let mut components = Vec::new();
    let mut is_absolute = false;

    for component in path_obj.components() {
        match component {
            std::path::Component::Normal(os_str) => {
                let s = os_str.to_str().ok_or(ApiError::Validation {
                    message: "Invalid path: contains non-UTF-8 characters".to_string(),
                    field: Some("path".to_string()),
                    code: ErrorCode::ValidationError,
                })?;
                components.push(s.to_string());
            }
            std::path::Component::ParentDir => {
                return Err(ApiError::Validation {
                    message: "Invalid path: contains directory traversal".to_string(),
                    field: Some("path".to_string()),
                    code: ErrorCode::ValidationError,
                });
            }
            std::path::Component::RootDir => {
                is_absolute = true;
            }
            std::path::Component::Prefix(_) => {
                // Windows prefix - treat as absolute and skip
                is_absolute = true;
            }
            std::path::Component::CurDir => {
                // Skip current directory components
            }
        }
    }

    // Rebuild the normalized path
    let normalized = if is_absolute {
        format!("/{}", components.join("/"))
    } else {
        components.join("/")
    };

    // Ensure the path is not empty
    if normalized.is_empty() {
        return Err(ApiError::Validation {
            message: "Invalid path: path is empty".to_string(),
            field: Some("path".to_string()),
            code: ErrorCode::ValidationError,
        });
    }

    Ok(normalized)
}

/// Extracts the filename from a path for metadata purposes.
fn extract_filename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("uploaded_file")
        .to_string()
}

/// Creates a safe HTTP header value, falling back if the input contains invalid characters.
fn safe_header_value(value: &str, fallback: &'static str) -> header::HeaderValue {
    header::HeaderValue::from_str(value).unwrap_or_else(|_| {
        header::HeaderValue::from_str(fallback)
            .unwrap_or_else(|_| header::HeaderValue::from_static("download"))
    })
}

/// Execute a shell command in a sandbox
///
/// POST /sandboxes/{id}/exec
///
/// Runs a shell command inside the sandbox container using subprocess execution.
/// The command is automatically wrapped with `sh -c` for proper shell interpretation.
/// Checks ownership before executing.
pub async fn exec_sandbox(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<ExecSandboxRequest>,
) -> Response {
    // Check ownership first
    if let Err(e) = service.check_sandbox_ownership(&identity, &id).await {
        return e.into_response();
    }

    // Record API activity
    let _ = service.record_api_activity(&id).await;

    tracing::debug!(
        sandbox_id = %id,
        command = ?req.command,
        "Executing command via subprocess"
    );

    // Wrap command with sh -c for proper shell execution
    let wrapped_command = wrap_shell_command(req.command);

    match service
        .exec_sandbox_result_with_stdin(&id, wrapped_command.clone(), req.stdin, req.timeout)
        .await
    {
        Ok(result) => (
            StatusCode::OK,
            Json(ExecSandboxResponse {
                output: result.output,
                exit_code: result.exit_code,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(
                sandbox_id = %id,
                command = ?wrapped_command,
                error = %e,
                "Exec request failed"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "message": format!("Failed to exec: {}", e)
                })),
            )
                .into_response()
        }
    }
}

/// Execute a tool action in a sandbox via HTTP-based tool execution
///
/// POST /sandboxes/{id}/tools
///
/// This endpoint executes tool actions (web_tools.py, databend_tools.py, browser_tools.js)
/// via the tool_proxy.py HTTP interface, which calls the tool functions directly
/// and returns JSON responses without stdout/stderr parsing.
pub async fn execute_tool(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<ToolExecutionRequest>,
) -> Response {
    // Check ownership first
    if let Err(e) = service.check_sandbox_ownership(&identity, &id).await {
        return e.into_response();
    }

    // Record API activity
    let _ = service.record_api_activity(&id).await;

    let args = req.args.unwrap_or_else(|| serde_json::json!({}));

    // Capture values for logging before moving into the request struct
    let script_path = req.script_path.clone();
    let action = req.action.clone();

    tracing::debug!(
        sandbox_id = %id,
        interpreter = %req.interpreter,
        script = %script_path,
        action = %action,
        "Executing tool action via HTTP proxy"
    );

    let tool_request = ExecToolHttpRequest {
        interpreter: req.interpreter,
        script_path: req.script_path,
        action: req.action,
        args,
        timeout: req.timeout,
        environment: req.environment,
    };

    match service.exec_tool_http(&id, tool_request).await {
        Ok(result) => {
            // Return result directly as JSON
            (StatusCode::OK, Json(result)).into_response()
        }
        Err(e) => {
            tracing::error!(
                sandbox_id = %id,
                script = %script_path,
                action = %action,
                error = %e,
                "Tool execution failed"
            );

            // Try to downcast to DockerError to access error_code() method
            use crate::api::errors::ApiError;
            use crate::docker::DockerError;

            if let Some(docker_err) = e.downcast_ref::<DockerError>() {
                // DockerError has error_code() method - use it for proper error handling
                let api_error: ApiError = (*docker_err).clone().into();
                api_error.into_response()
            } else {
                // Fallback to string parsing for non-DockerError types
                // Format: "ERROR_CODE: message" or "HTTP error ..." (fallback)
                let error_str = e.to_string();

                // Check if it's in the format "ERROR_CODE: message"
                if let Some(colon_pos) = error_str.find(':') {
                    let error_code = &error_str[..colon_pos];
                    let error_msg = &error_str[colon_pos + 1..];

                    // Map error codes to proper ApiError responses
                    use crate::core::errors::ErrorCode;

                    let api_error = match error_code {
                        "VALIDATION_ERROR" => ApiError::Validation {
                            message: error_msg.to_string(),
                            field: None,
                            code: ErrorCode::ValidationError,
                        },
                        "NOT_FOUND" => ApiError::SandboxNotFound(error_msg.to_string()),
                        "INVALID_STATE" => ApiError::Validation {
                            message: error_msg.to_string(),
                            field: None,
                            code: ErrorCode::SandboxInvalidState,
                        },
                        _ => ApiError::Internal(error_msg.to_string()),
                    };

                    api_error.into_response()
                } else {
                    // Fallback for old format errors
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "error_message": error_str
                        })),
                    )
                        .into_response()
                }
            }
        }
    }
}

/// POST /sandboxes/{id}/upload - Upload a file to the sandbox filesystem.
///
/// This endpoint accepts a multipart/form-data upload with:
/// - `path`: Destination path in the container (e.g., "/app/data.txt")
/// - `file`: File contents
///
/// # File Size Limits
///
/// - Maximum: 10MB (enforced via Content-Length header + actual size check)
/// - In-memory processing (no temporary files)
///
/// # Security
///
/// - Path sanitization prevents directory traversal
/// - File size limits prevent resource exhaustion
/// - Sandbox isolation ensures files stay in container
///
/// # Example
///
/// ```bash
/// curl -X POST http://localhost:8080/sandboxes/{uuid}/upload \
///   -F "path=/app/config.json" \
///   -F "file=@local-config.json"
/// ```
pub async fn upload_file(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
    mut multipart: Multipart,
) -> Response {
    // Check ownership first
    if let Err(e) = service.check_sandbox_ownership(&identity, &id).await {
        return e.into_response();
    }

    // Record API activity
    let _ = service.record_api_activity(&id).await;

    let max_file_size = service.max_file_size_bytes;

    let mut dest_path: Option<String> = None;
    let mut file_data: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;

    // Process multipart fields
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Failed to read multipart field: {}", e)
                    })),
                )
                    .into_response();
            }
        };

        let field_name = match field.name() {
            Some(name) => name.to_string(),
            None => continue,
        };

        match field_name.as_str() {
            "path" => {
                dest_path = match field.text().await {
                    Ok(text) => Some(text),
                    Err(e) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({
                                "error": format!("Failed to read path field: {}", e)
                            })),
                        )
                            .into_response();
                    }
                };
            }
            "file" => {
                let filename = field.file_name().map(|s| s.to_string());
                let data = match field.bytes().await {
                    Ok(data) => data,
                    Err(e) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(serde_json::json!({
                                "error": format!("Failed to read file field: {}", e)
                            })),
                        )
                            .into_response();
                    }
                };

                // Validate file size
                if data.len() as u64 > max_file_size {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": format!("File size exceeds limit of {} bytes", max_file_size),
                            "max_size": max_file_size
                        })),
                    )
                        .into_response();
                }

                file_name = filename;
                file_data = Some(data.to_vec());
            }
            _ => {}
        }
    }

    // Validate required fields
    let dest_path = match dest_path {
        Some(path) => path,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Missing 'path' field" })),
            )
                .into_response();
        }
    };

    let file_data = match file_data {
        Some(data) => data,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Missing 'file' field" })),
            )
                .into_response();
        }
    };

    // Sanitize path
    let sanitized_path = match sanitize_path(&dest_path) {
        Ok(path) => path,
        Err(e) => return e.into_response(),
    };

    let filename = file_name.unwrap_or_else(|| extract_filename(&sanitized_path));

    // Upload to container
    match service
        .upload_file(&id, &sanitized_path, file_data.clone())
        .await
    {
        Ok(()) => {
            let response = UploadFileResponse {
                success: true,
                file: FileInfo {
                    name: filename,
                    path: sanitized_path,
                    size: file_data.len() as u64,
                    uploaded_at: chrono::Utc::now(),
                },
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Sandbox not found" })),
                )
                    .into_response()
            } else if error_msg.contains("not running") {
                (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({ "error": "Sandbox is not running" })),
                )
                    .into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": format!("Upload failed: {}", error_msg) })),
                )
                    .into_response()
            }
        }
    }
}

/// GET /sandboxes/{id}/download - Download a file from the sandbox filesystem.
///
/// This endpoint downloads files from the sandbox container filesystem with:
/// - `path`: Source path in the container (query parameter, required)
/// - `disposition`: Content disposition (optional, "inline" or "attachment")
///
/// # File Size Limits
///
/// - Maximum: 10MB (enforced before download)
/// - Binary response with proper Content-Type and Content-Disposition headers
///
/// # Response Headers
///
/// - `Content-Type`: Auto-detected MIME type
/// - `Content-Disposition`: inline or attachment with filename
/// - `X-File-Name`: Original filename
/// - `X-File-Path`: Full sanitized path
/// - `X-File-Size`: File size in bytes
///
/// # Example
///
/// ```bash
/// curl -O -J http://localhost:8080/sandboxes/{uuid}/download?path=/app/config.json
/// ```
pub async fn download_file(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
    AxumQuery(params): AxumQuery<DownloadParams>,
) -> Response {
    use crate::utils::mime::detect_mime_type;

    // Check ownership first
    if let Err(e) = service.check_sandbox_ownership(&identity, &id).await {
        return e.into_response();
    }

    // Record API activity
    let _ = service.record_api_activity(&id).await;

    // Validate path parameter
    let src_path = match params.path {
        Some(path) => path,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Missing 'path' query parameter"
                })),
            )
                .into_response();
        }
    };

    // Sanitize path
    let sanitized_path = match sanitize_path(&src_path) {
        Ok(path) => path,
        Err(e) => return e.into_response(),
    };

    // Download from container
    match service.download_file(&id, &sanitized_path).await {
        Ok(data) => {
            // Detect MIME type
            let content_type = detect_mime_type(&sanitized_path);

            // Extract filename for Content-Disposition
            let filename = extract_filename(&sanitized_path);

            // Determine disposition
            let disposition_value = match params.disposition.as_deref() {
                Some("inline") => "inline",
                _ => "attachment",
            };

            // Save data length before moving
            let data_len = data.len();

            // Build response with proper headers
            let mut response = data.into_response();

            // Set Content-Type
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                safe_header_value(content_type, "application/octet-stream"),
            );

            // Set Content-Disposition
            let disposition = format!("{}; filename=\"{}\"", disposition_value, filename);
            response.headers_mut().insert(
                header::CONTENT_DISPOSITION,
                safe_header_value(&disposition, "attachment"),
            );

            // Set metadata headers
            response.headers_mut().insert(
                header::HeaderName::from_static("x-file-name"),
                safe_header_value(&filename, "download"),
            );
            response.headers_mut().insert(
                header::HeaderName::from_static("x-file-path"),
                safe_header_value(&sanitized_path, "/"),
            );
            response.headers_mut().insert(
                header::HeaderName::from_static("x-file-size"),
                safe_header_value(&data_len.to_string(), "0"),
            );

            response
        }
        Err(e) => {
            let error_msg = e.to_string();
            // Check for more specific errors first
            if error_msg.contains("File not found") {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "File not found in sandbox" })),
                )
                    .into_response()
            } else if error_msg.contains("Sandbox not found") {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Sandbox not found" })),
                )
                    .into_response()
            } else if error_msg.contains("not running") {
                (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({ "error": "Sandbox is not running" })),
                )
                    .into_response()
            } else if error_msg.contains("exceeds limit") {
                (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    Json(serde_json::json!({
                        "error": error_msg,
                        "max_size": "10MB"
                    })),
                )
                    .into_response()
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": format!("Download failed: {}", error_msg)
                    })),
                )
                    .into_response()
            }
        }
    }
}

/// Get sandbox resource usage statistics
///
/// GET /sandboxes/{id}/stats
///
/// Returns CPU, memory, network, and disk usage for a running sandbox.
/// Checks ownership before returning stats.
pub async fn get_sandbox_stats(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    // Check ownership first
    if let Err(e) = service.check_sandbox_ownership(&identity, &id).await {
        return e.into_response();
    }

    match service.get_sandbox_stats(&id).await {
        Ok(stats) => (StatusCode::OK, Json(stats)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to get stats: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Stream sandbox statistics via Server-Sent Events
///
/// GET /sandboxes/{id}/stats-stream
///
/// Returns a real-time stream of sandbox resource statistics.
/// Checks ownership before streaming.
pub async fn stream_sandbox_stats(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    // Check ownership first
    if let Err(e) = service.check_sandbox_ownership(&identity, &id).await {
        return e.into_response();
    }

    match service.stream_sandbox_stats(&id).await {
        Ok(mut receiver) => {
            // Create SSE stream from channel receiver
            let stream = async_stream::stream! {
                while let Some(stats) = receiver.recv().await {
                    match serde_json::to_string(&stats) {
                        Ok(json) => {
                            yield Ok::<_, Infallible>(Event::default().data(json));
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }
            };

            Sse::new(stream)
                .keep_alive(
                    axum::response::sse::KeepAlive::new()
                        .interval(std::time::Duration::from_secs(10))
                        .text("keepalive"),
                )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to stream stats: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Force cleanup all resources for a sandbox
///
/// POST /sandboxes/{id}/cleanup
///
/// Immediately cleans up all resources (container, volumes, networks) for a sandbox.
/// Checks ownership before cleanup. Returns 204 No Content on success.
pub async fn cleanup_sandbox(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    // Check ownership first
    if let Err(e) = service.check_sandbox_ownership(&identity, &id).await {
        return e.into_response();
    }

    match service.cleanup_sandbox(&id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to cleanup: {}", e)
            })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::auth::{api_key_auth, AuthState};
    use crate::core::manager::{ManagerResult, SandboxManager};
    use crate::core::types::{ContainerStats, SandboxConfig, SandboxInfo, SandboxResponse};
    use crate::core::types::{ImageDetails, ImageSummary};
    use crate::core::{SandboxService, StateStore};
    use async_trait::async_trait;
    use axum::{
        body::{to_bytes, Body},
        http::{Method, Request},
        middleware::from_fn_with_state,
        routing::{get, post},
        Router,
    };
    use std::{collections::HashMap, sync::Arc};
    use tower::ServiceExt;

    // ============================================================================
    // Handler Logic Tests
    // ============================================================================

    #[test]
    fn test_wrap_shell_command_basic() {
        let command = vec!["ls".to_string(), "-la".to_string()];
        let wrapped = wrap_shell_command(command);

        assert_eq!(wrapped.len(), 3);
        assert_eq!(wrapped[0], "sh");
        assert_eq!(wrapped[1], "-c");
        assert_eq!(wrapped[2], "ls -la");
    }

    #[test]
    fn test_wrap_shell_command_already_wrapped() {
        let command = vec!["sh".to_string(), "-c".to_string(), "ls -la".to_string()];
        let wrapped = wrap_shell_command(command);

        // Should not wrap again
        assert_eq!(wrapped.len(), 3);
        assert_eq!(wrapped[0], "sh");
        assert_eq!(wrapped[1], "-c");
        assert_eq!(wrapped[2], "ls -la");
    }

    #[test]
    fn test_wrap_shell_command_with_pipes() {
        let command = vec![
            "cat".to_string(),
            "file.txt".to_string(),
            "|".to_string(),
            "grep".to_string(),
            "pattern".to_string(),
        ];
        let wrapped = wrap_shell_command(command);

        assert_eq!(wrapped[2], "cat file.txt | grep pattern");
    }

    #[test]
    fn test_wrap_shell_command_with_redirection() {
        let command = vec![
            "echo".to_string(),
            "hello".to_string(),
            ">".to_string(),
            "output.txt".to_string(),
        ];
        let wrapped = wrap_shell_command(command);

        assert_eq!(wrapped[2], "echo hello > output.txt");
    }

    #[test]
    fn test_wrap_shell_command_empty() {
        let command = vec![];
        let wrapped = wrap_shell_command(command);

        // Empty command becomes "sh -c "
        assert_eq!(wrapped.len(), 3);
        assert_eq!(wrapped[2], "");
    }

    #[test]
    fn test_wrap_shell_command_single_word() {
        let command = vec!["ls".to_string()];
        let wrapped = wrap_shell_command(command);

        assert_eq!(wrapped[2], "ls");
    }

    #[test]
    fn test_wrap_shell_command_with_quotes() {
        let command = vec!["echo".to_string(), "hello 'world'".to_string()];
        let wrapped = wrap_shell_command(command);

        assert_eq!(wrapped.len(), 3);
        assert_eq!(wrapped[0], "sh");
        assert_eq!(wrapped[1], "-c");
        assert_eq!(wrapped[2], "echo hello 'world'");
    }

    #[test]
    fn test_error_message_classification() {
        let error_messages = vec![
            ("no such image", StatusCode::NOT_FOUND),
            ("image not found", StatusCode::NOT_FOUND),
            ("failed to pull", StatusCode::BAD_GATEWAY),
            ("port 8080 already allocated", StatusCode::CONFLICT),
            ("unknown error", StatusCode::INTERNAL_SERVER_ERROR),
        ];

        for (msg, expected_code) in error_messages {
            let error_msg_lower = msg.to_lowercase();

            let code = if error_msg_lower.contains("no such image")
                || error_msg_lower.contains("image not found")
            {
                StatusCode::NOT_FOUND
            } else if error_msg_lower.contains("failed to pull") {
                StatusCode::BAD_GATEWAY
            } else if error_msg_lower.contains("port")
                && error_msg_lower.contains("already allocated")
            {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

            assert_eq!(code, expected_code, "Failed for message: {}", msg);
        }
    }

    // ========================================================================
    // Path Sanitization Tests
    // ========================================================================

    #[test]
    fn test_sanitize_path_preserves_relative() {
        let path = "app/data.txt";
        let sanitized = sanitize_path(path).unwrap();
        // Relative paths should stay relative
        assert!(!sanitized.starts_with('/'));
        assert_eq!(sanitized, "app/data.txt");
    }

    #[test]
    fn test_sanitize_path_normalizes_relative() {
        let path = "app//data.txt";
        let result = sanitize_path(path).unwrap();
        assert_eq!(result, "app/data.txt");
        assert!(!result.contains("//"));
    }

    #[test]
    fn test_sanitize_path_cleans_current_dir() {
        let path = "./app/data.txt";
        let result = sanitize_path(path).unwrap();
        assert_eq!(result, "app/data.txt");
    }

    #[test]
    fn test_sanitize_path_blocks_backslashes() {
        let path = "app\\data.txt";
        let result = sanitize_path(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_path_rejects_traversal() {
        let result = sanitize_path("../../../etc/passwd");
        assert!(result.is_err(), "Path traversal should be rejected");
    }

    #[test]
    fn test_sanitize_path_rejects_absolute() {
        let result = sanitize_path("/etc/passwd");
        assert!(result.is_err(), "Absolute paths should be rejected");
    }

    #[test]
    fn test_sanitize_path_rejects_null_bytes() {
        let result = sanitize_path("foo\0bar.txt");
        assert!(result.is_err(), "Null bytes should be rejected");
    }

    #[test]
    fn test_sanitize_path_rejects_control_chars() {
        let result = sanitize_path("foo\nbar.txt");
        assert!(result.is_err(), "Control characters should be rejected");
        let result = sanitize_path("foo\rbar.txt");
        assert!(result.is_err(), "Control characters should be rejected");
    }

    #[test]
    fn test_extract_filename() {
        assert_eq!(extract_filename("/app/config.json"), "config.json");
        assert_eq!(extract_filename("/app/data/"), "data");
        assert_eq!(extract_filename("file.txt"), "file.txt");
    }

    // ========================================================================
    // File Download Handler Tests
    // ========================================================================

    #[tokio::test]
    async fn test_download_file_with_control_chars_returns_bad_request() {
        let key_a_id = uuid::Uuid::new_v4();
        let key_b_id = uuid::Uuid::new_v4();
        let (service, _) = build_test_service_and_state();
        let app = build_test_app(service, key_a_id, key_b_id);
        let sandbox = create_sandbox_via_api(&app, "key-a", "download-test").await;

        let response = app
            .clone()
            .oneshot(request_with_body(
                Method::GET,
                &format!("/sandboxes/{}/download?path=foo%0Abar.txt", sandbox.id),
                "key-a",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_download_file_success() {
        let key_a_id = uuid::Uuid::new_v4();
        let key_b_id = uuid::Uuid::new_v4();
        let (service, _) = build_test_service_and_state();
        let app = build_test_app(service, key_a_id, key_b_id);
        let sandbox = create_sandbox_via_api(&app, "key-a", "download-test").await;

        let response = app
            .clone()
            .oneshot(request_with_body(
                Method::GET,
                &format!("/sandboxes/{}/download?path=test.txt", sandbox.id),
                "key-a",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    // ========================================================================
    // Request Parsing Tests
    // ========================================================================

    #[test]
    fn test_create_sandbox_request_missing_image() {
        let json = r#"{"name":"test"}"#;
        let result: Result<CreateSandboxRequest, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Should fail when image is missing");
    }

    #[test]
    fn test_exec_sandbox_request_with_working_dir() {
        let json = r#"{
            "command":["ls","-la"],
            "working_dir":"/app",
            "timeout":30
        }"#;

        let result: Result<ExecSandboxRequest, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let request = result.unwrap();
        assert_eq!(request.stdin, None);
        assert_eq!(request.working_dir, Some("/app".to_string()));
        assert_eq!(request.timeout, Some(30));
    }

    #[test]
    fn test_exec_sandbox_request_with_environment() {
        let json = r#"{
            "command":["env"],
            "environment":{"TEST_VAR":"test_value","ANOTHER_VAR":"another"}
        }"#;

        let result: Result<ExecSandboxRequest, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let request = result.unwrap();
        assert!(request.environment.is_some());
        let env = request.environment.unwrap();
        assert_eq!(env.get("TEST_VAR"), Some(&"test_value".to_string()));
    }

    #[test]
    fn test_exec_sandbox_request_with_stdin() {
        let json = r#"{
            "command":["cat"],
            "stdin":"hello world"
        }"#;

        let result: Result<ExecSandboxRequest, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let request = result.unwrap();
        assert_eq!(request.stdin, Some("hello world".to_string()));
    }

    #[test]
    fn test_tool_execution_request_with_nested_args() {
        let json = r#"{
            "interpreter":"python",
            "script_path":"/opt/tools/web_tools.py",
            "action":"web_scrape",
            "args":{
                "url":"https://example.com",
                "options":{"follow_links":true,"max_depth":3}
            }
        }"#;

        let result: Result<ToolExecutionRequest, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let request = result.unwrap();
        assert!(request.args.is_some());
        let args = request.args.unwrap();
        assert!(args.is_object());
        assert_eq!(args["url"], "https://example.com");
    }

    struct MockApiKeyStore {
        keys: HashMap<String, uuid::Uuid>,
    }

    #[async_trait]
    impl crate::db::ApiKeyStore for MockApiKeyStore {
        async fn validate_api_key(
            &self,
            key: &str,
        ) -> Result<Option<uuid::Uuid>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(self.keys.get(key).copied())
        }

        async fn create_api_key(
            &self,
            _req: crate::db::CreateApiKeyRequest,
        ) -> Result<crate::db::ApiKeyResponse, Box<dyn std::error::Error + Send + Sync>> {
            Err("not implemented".into())
        }

        async fn list_api_keys(
            &self,
        ) -> Result<Vec<crate::db::ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(vec![])
        }

        async fn get_api_key(
            &self,
            _id: uuid::Uuid,
        ) -> Result<Option<crate::db::ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(None)
        }

        async fn delete_api_key(
            &self,
            _id: uuid::Uuid,
        ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
            Ok(false)
        }

        async fn rotate_api_key(
            &self,
            _id: uuid::Uuid,
        ) -> Result<crate::db::ApiKeyResponse, Box<dyn std::error::Error + Send + Sync>> {
            Err("not implemented".into())
        }
    }

    struct MockSandboxManager;

    #[async_trait]
    impl SandboxManager for MockSandboxManager {
        async fn create(
            &self,
            sandbox_id: Option<&uuid::Uuid>,
            _config: &SandboxConfig,
        ) -> ManagerResult<String> {
            Ok(format!(
                "container-{}",
                sandbox_id.copied().unwrap_or_else(uuid::Uuid::new_v4)
            ))
        }

        async fn start(&self, _id: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn stop(&self, _id: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn delete(&self, _id: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn exec(&self, _id: &str, cmd: Vec<String>) -> ManagerResult<String> {
            let cmd_str = cmd.join(" ");
            if cmd_str.contains("test -f") && cmd_str.contains("exists") {
                return Ok("exists".to_string());
            }
            if cmd_str.contains("wc -c") {
                return Ok("5".to_string());
            }
            if cmd_str.contains("base64") {
                return Ok("aGVsbG8=".to_string());
            }
            Ok("ok".to_string())
        }

        async fn stats(&self, _id: &str) -> ManagerResult<ContainerStats> {
            Ok(ContainerStats {
                cpu_percent: 0.0,
                memory_usage_mb: 0,
                memory_limit_mb: 0,
                memory_percent: 0.0,
                network_rx_bytes: 0,
                network_tx_bytes: 0,
                block_read_bytes: 0,
                block_write_bytes: 0,
                timestamp: chrono::Utc::now(),
            })
        }

        async fn is_running(&self, _id: &str) -> ManagerResult<bool> {
            Ok(true)
        }

        async fn get_exit_info(&self, _id: &str) -> ManagerResult<(i64, bool)> {
            Ok((0, false))
        }

        async fn get_workdir(&self, _id: &str) -> ManagerResult<String> {
            Ok("/workspace".to_string())
        }

        async fn list(
            &self,
            _all: bool,
            _filters: Option<HashMap<String, Vec<String>>>,
        ) -> ManagerResult<Vec<SandboxInfo>> {
            Ok(vec![])
        }

        async fn remove_volume(&self, _name: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn get_image_features(&self, image: &str) -> ManagerResult<ImageDetails> {
            Ok(ImageDetails {
                id: image.to_string(),
                repo_tags: vec![image.to_string()],
                size: 0,
                virtual_size: 0,
                created: 0,
                architecture: "amd64".to_string(),
                os: "linux".to_string(),
                labels: None,
                env: None,
                features: vec![],
            })
        }

        async fn list_images(&self) -> ManagerResult<Vec<ImageSummary>> {
            Ok(vec![])
        }

        async fn pull_image(&self, _image: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn pull_image_with_progress(
            &self,
            _image: &str,
            _callback: Box<dyn FnMut(String, Option<u64>, Option<u64>) + Send + 'static>,
        ) -> ManagerResult<()> {
            Ok(())
        }

        async fn delete_image(&self, _id: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn image_exists(&self, _image: &str) -> ManagerResult<bool> {
            Ok(true)
        }

        async fn exec_http(
            &self,
            _id: &str,
            _path: &str,
            _method: &str,
            _body: Option<serde_json::Value>,
            _timeout_secs: Option<u64>,
        ) -> ManagerResult<serde_json::Value> {
            Ok(serde_json::json!({ "ok": true }))
        }

        async fn exec_with_stdin(
            &self,
            _id: &str,
            _cmd: Vec<String>,
            _stdin: Option<String>,
            _timeout_secs: Option<u64>,
        ) -> ManagerResult<String> {
            Ok("ok".to_string())
        }

        async fn upload_archive(
            &self,
            _id: &str,
            _path: &str,
            _tar_data: Vec<u8>,
        ) -> ManagerResult<()> {
            Ok(())
        }
    }

    fn build_test_service_and_state() -> (Arc<SandboxService>, Arc<StateStore>) {
        let state = Arc::new(StateStore::new());
        let service = Arc::new(SandboxService::new(
            Arc::new(MockSandboxManager),
            state.clone(),
        ));
        (service, state)
    }

    fn build_test_app(
        service: Arc<SandboxService>,
        key_a_id: uuid::Uuid,
        key_b_id: uuid::Uuid,
    ) -> Router {
        Router::new()
            .route("/sandboxes", get(list_sandboxes).post(create_sandbox))
            .route("/sandboxes/{id}", get(get_sandbox).delete(delete_sandbox))
            .route("/sandboxes/{id}/restore", post(restore_sandbox))
            .route("/sandboxes/{id}/stop", post(stop_sandbox))
            .route("/sandboxes/{id}/exec", post(exec_sandbox))
            .route("/sandboxes/{id}/tools", post(execute_tool))
            .route("/sandboxes/{id}/upload", post(upload_file))
            .route("/sandboxes/{id}/download", get(download_file))
            .route("/sandboxes/{id}/stats", get(get_sandbox_stats))
            .route("/sandboxes/{id}/stats-stream", get(stream_sandbox_stats))
            .route("/sandboxes/{id}/cleanup", post(cleanup_sandbox))
            .with_state(service)
            .layer(from_fn_with_state(
                AuthState {
                    config_api_key: Some("admin-key".to_string()),
                    admin_api_key: None,
                    require_auth: true,
                    static_server_require_auth: false,
                    vnc_require_auth: false,
                    api_key_store: Some(Arc::new(MockApiKeyStore {
                        keys: HashMap::from([
                            ("key-a".to_string(), key_a_id),
                            ("key-b".to_string(), key_b_id),
                        ]),
                    })),
                    cookie_key: axum_extra::extract::cookie::Key::generate(),
                },
                api_key_auth,
            ))
    }

    fn request_with_body(
        method: Method,
        uri: &str,
        api_key: &str,
        content_type: Option<&str>,
        body: impl Into<Body>,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("host", "localhost")
            .header("x-api-key", api_key);

        if let Some(content_type) = content_type {
            builder = builder.header("content-type", content_type);
        }

        builder.body(body.into()).unwrap()
    }

    async fn response_json<T: serde::de::DeserializeOwned>(
        response: axum::response::Response,
    ) -> T {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    async fn create_sandbox_via_api(app: &Router, api_key: &str, name: &str) -> SandboxResponse {
        let response = app
            .clone()
            .oneshot(request_with_body(
                Method::POST,
                "/sandboxes",
                api_key,
                Some("application/json"),
                serde_json::json!({
                    "image": "mock-image:latest",
                    "name": name
                })
                .to_string(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        response_json(response).await
    }

    async fn list_sandbox_ids(app: &Router, api_key: &str) -> Vec<uuid::Uuid> {
        let response = app
            .clone()
            .oneshot(request_with_body(
                Method::GET,
                "/sandboxes",
                api_key,
                None,
                Body::empty(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let payload: serde_json::Value = response_json(response).await;
        payload["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                uuid::Uuid::parse_str(entry["id"].as_str().unwrap())
                    .expect("sandbox id should be a valid UUID")
            })
            .collect()
    }

    #[tokio::test]
    async fn test_create_sandbox_persists_database_and_privileged_ownership() {
        let key_a_id = uuid::Uuid::new_v4();
        let key_b_id = uuid::Uuid::new_v4();
        let (service, state) = build_test_service_and_state();
        let app = build_test_app(service, key_a_id, key_b_id);

        let owned = create_sandbox_via_api(&app, "key-a", "owned-by-a").await;
        assert_eq!(owned.api_key_id, Some(key_a_id));
        assert_eq!(
            state.get_sandbox(&owned.id).await.unwrap().api_key_id,
            Some(key_a_id)
        );

        let admin = create_sandbox_via_api(&app, "admin-key", "owned-by-admin").await;
        assert_eq!(admin.api_key_id, None);
        assert_eq!(state.get_sandbox(&admin.id).await.unwrap().api_key_id, None);
    }

    #[tokio::test]
    async fn test_cross_key_isolation_and_admin_bypass() {
        let key_a_id = uuid::Uuid::new_v4();
        let key_b_id = uuid::Uuid::new_v4();
        let (service, _) = build_test_service_and_state();
        let app = build_test_app(service, key_a_id, key_b_id);
        let sandbox = create_sandbox_via_api(&app, "key-a", "cross-key-flow").await;

        let owner_get = app
            .clone()
            .oneshot(request_with_body(
                Method::GET,
                &format!("/sandboxes/{}", sandbox.id),
                "key-a",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(owner_get.status(), StatusCode::OK);

        let other_get = app
            .clone()
            .oneshot(request_with_body(
                Method::GET,
                &format!("/sandboxes/{}", sandbox.id),
                "key-b",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(other_get.status(), StatusCode::NOT_FOUND);

        let admin_get = app
            .clone()
            .oneshot(request_with_body(
                Method::GET,
                &format!("/sandboxes/{}", sandbox.id),
                "admin-key",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(admin_get.status(), StatusCode::OK);

        assert!(list_sandbox_ids(&app, "key-a").await.contains(&sandbox.id));
        assert!(!list_sandbox_ids(&app, "key-b").await.contains(&sandbox.id));
        assert!(list_sandbox_ids(&app, "admin-key")
            .await
            .contains(&sandbox.id));

        let owner_exec = app
            .clone()
            .oneshot(request_with_body(
                Method::POST,
                &format!("/sandboxes/{}/exec", sandbox.id),
                "key-a",
                Some("application/json"),
                serde_json::json!({ "command": ["echo", "ok"] }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(owner_exec.status(), StatusCode::OK);

        let other_exec = app
            .clone()
            .oneshot(request_with_body(
                Method::POST,
                &format!("/sandboxes/{}/exec", sandbox.id),
                "key-b",
                Some("application/json"),
                serde_json::json!({ "command": ["echo", "ok"] }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(other_exec.status(), StatusCode::NOT_FOUND);

        let other_delete = app
            .clone()
            .oneshot(request_with_body(
                Method::DELETE,
                &format!("/sandboxes/{}", sandbox.id),
                "key-b",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(other_delete.status(), StatusCode::NOT_FOUND);

        let owner_delete = app
            .clone()
            .oneshot(request_with_body(
                Method::DELETE,
                &format!("/sandboxes/{}", sandbox.id),
                "key-a",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(owner_delete.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_restore_deleted_sandbox_requires_deleted_aware_ownership() {
        let key_a_id = uuid::Uuid::new_v4();
        let key_b_id = uuid::Uuid::new_v4();
        let (service, state) = build_test_service_and_state();
        let app = build_test_app(service, key_a_id, key_b_id);
        let sandbox = create_sandbox_via_api(&app, "key-a", "restore-me").await;

        let delete_response = app
            .clone()
            .oneshot(request_with_body(
                Method::DELETE,
                &format!("/sandboxes/{}", sandbox.id),
                "key-a",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);
        assert!(state
            .get_sandbox(&sandbox.id)
            .await
            .unwrap()
            .deleted_at
            .is_some());

        let other_restore = app
            .clone()
            .oneshot(request_with_body(
                Method::POST,
                &format!("/sandboxes/{}/restore", sandbox.id),
                "key-b",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(other_restore.status(), StatusCode::NOT_FOUND);

        let owner_restore = app
            .clone()
            .oneshot(request_with_body(
                Method::POST,
                &format!("/sandboxes/{}/restore", sandbox.id),
                "key-a",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(owner_restore.status(), StatusCode::OK);

        let restored: SandboxResponse = response_json(owner_restore).await;
        assert_eq!(restored.api_key_id, Some(key_a_id));
        assert!(state
            .get_sandbox(&sandbox.id)
            .await
            .unwrap()
            .deleted_at
            .is_none());
    }

    #[tokio::test]
    async fn test_non_owner_receives_404_across_other_mutation_endpoints() {
        let key_a_id = uuid::Uuid::new_v4();
        let key_b_id = uuid::Uuid::new_v4();
        let (service, _) = build_test_service_and_state();
        let app = build_test_app(service, key_a_id, key_b_id);
        let sandbox = create_sandbox_via_api(&app, "key-a", "parity-check").await;

        let requests = vec![
            (
                Method::POST,
                format!("/sandboxes/{}/stop", sandbox.id),
                None,
                Body::empty(),
            ),
            (
                Method::POST,
                format!("/sandboxes/{}/tools", sandbox.id),
                Some("application/json"),
                Body::from(
                    serde_json::json!({
                        "interpreter": "python",
                        "script_path": "/opt/tools/web_tools.py",
                        "action": "ping"
                    })
                    .to_string(),
                ),
            ),
            (
                Method::GET,
                format!("/sandboxes/{}/download?path=/tmp/test.txt", sandbox.id),
                None,
                Body::empty(),
            ),
            (
                Method::GET,
                format!("/sandboxes/{}/stats", sandbox.id),
                None,
                Body::empty(),
            ),
            (
                Method::GET,
                format!("/sandboxes/{}/stats-stream", sandbox.id),
                None,
                Body::empty(),
            ),
            (
                Method::POST,
                format!("/sandboxes/{}/cleanup", sandbox.id),
                None,
                Body::empty(),
            ),
        ];

        for (method, uri, content_type, body) in requests {
            let response = app
                .clone()
                .oneshot(request_with_body(method, &uri, "key-b", content_type, body))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "uri={uri}");
        }

        let boundary = "X-BOUNDARY";
        let multipart = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"path\"\r\n\r\n/tmp/test.txt\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\nContent-Type: text/plain\r\n\r\nhello\r\n--{boundary}--\r\n"
        );
        let upload_response = app
            .clone()
            .oneshot(request_with_body(
                Method::POST,
                &format!("/sandboxes/{}/upload", sandbox.id),
                "key-b",
                Some(&format!("multipart/form-data; boundary={boundary}")),
                multipart,
            ))
            .await
            .unwrap();
        assert_eq!(upload_response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_database_keys_cannot_access_orphaned_sandboxes() {
        let key_a_id = uuid::Uuid::new_v4();
        let key_b_id = uuid::Uuid::new_v4();
        let (service, _) = build_test_service_and_state();
        let app = build_test_app(service, key_a_id, key_b_id);
        let orphan = create_sandbox_via_api(&app, "admin-key", "orphaned").await;

        let db_get = app
            .clone()
            .oneshot(request_with_body(
                Method::GET,
                &format!("/sandboxes/{}", orphan.id),
                "key-a",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(db_get.status(), StatusCode::NOT_FOUND);

        let admin_get = app
            .clone()
            .oneshot(request_with_body(
                Method::GET,
                &format!("/sandboxes/{}", orphan.id),
                "admin-key",
                None,
                Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(admin_get.status(), StatusCode::OK);

        assert!(!list_sandbox_ids(&app, "key-a").await.contains(&orphan.id));
        assert!(list_sandbox_ids(&app, "admin-key")
            .await
            .contains(&orphan.id));
    }
}
