// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Web Terminal Module
//!
//! This module provides WebSocket-based terminal access to sandbox containers.
//!
//! ## Architecture
//!
//! ```text
//! Browser (xterm.js) ←WebSocket→ Web Terminal Handler ←TerminalStream→ Container PTY
//! ```
//!
//! The web terminal uses the `SandboxManager` trait's `exec_terminal()` method
//! to create interactive terminal sessions. This abstracts over Docker exec
//! and (future) Kubernetes exec protocols via the `TerminalStream` trait.
//!
//! ## Features
//!
//! - Bidirectional communication over WebSocket
//! - PTY size synchronization
//! - Automatic session cleanup
//! - Support for multiple concurrent sessions
//! - Optional API key authentication
//! - Backend-agnostic via SandboxManager trait (Docker, K8s, etc.)
//!
//! ## Testing Strategy
//!
//! The web terminal module is tested through:
//!
//! ### Unit Tests (`tests.rs` - 28 tests)
//! Message and validation tests:
//! - Client/Server message serialization/deserialization
//! - API key validation logic
//! - Error type display formatting
//! - Edge cases (Unicode, control characters, large messages)
//! - Boundary value testing
//! - Concurrent validation stress testing
//!
//! ### Integration Tests
//! WebSocket handler tests require:
//! - Real WebSocket connections
//! - Running Docker containers
//! - PTY allocation
//!
//! These are tested manually or through E2E tests. The unit tests provide
//! comprehensive coverage of all testable logic without requiring infrastructure.

use crate::config::Config;
use crate::core::manager::{SandboxManager, TerminalFrame};
use crate::core::types::{ApiKeyIdentity, ApiKeyType};
use axum::extract::{
    ws::{Message, WebSocket, WebSocketUpgrade},
    Extension, FromRequestParts, Path, State,
};
use axum::http::StatusCode;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, instrument};

/// Errors that can occur during web terminal operations.
#[derive(Error, Debug)]
pub enum WebTerminalError {
    /// The target container was not found
    #[error("Container not found: {0}")]
    ContainerNotFound(String),

    /// Failed to create the Docker exec instance
    #[error("Failed to create exec: {0}")]
    ExecCreationFailed(String),

    /// Failed to start the Docker exec instance
    #[error("Failed to start exec: {0}")]
    ExecStartFailed(String),

    /// WebSocket communication error
    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    /// Backend Docker/K8s connection failed
    #[error("Backend connection failed: {0}")]
    BackendConnectionFailed(String),

    /// Authentication or authorization failed
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

/// WebSocket message from client to server.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientMessage {
    /// Input data to send to the PTY
    #[serde(rename = "input")]
    Input(String),

    /// Resize PTY window
    #[serde(rename = "resize")]
    Resize {
        /// Number of rows
        rows: u16,
        /// Number of columns
        cols: u16,
    },
}

/// WebSocket message from server to client.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerMessage {
    /// Output data from PTY
    #[serde(rename = "output")]
    Output(String),

    /// Error message
    #[serde(rename = "error")]
    Error(String),

    /// Session end notification
    #[serde(rename = "end")]
    End,
}

/// Optional API key extractor for web terminal authentication.
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
        // Try to get API key from header first, then from query parameter
        // (WebSocket connections don't support custom headers in browsers)
        let api_key = match parts.headers.get("X-API-Key") {
            Some(value) => match value.to_str() {
                Ok(v) => Some(v.to_string()),
                Err(_) => return Err((StatusCode::UNAUTHORIZED, "Invalid header encoding")),
            },
            None => None,
        }
        .or_else(|| {
            // Try query parameter for WebSocket connections
            parts.uri.query().and_then(|query| {
                // Simple query parameter parsing
                query.split('&').find_map(|pair| {
                    let mut parts = pair.splitn(2, '=');
                    match (parts.next(), parts.next()) {
                        (Some("api_key"), Some(value)) => Some(value.to_string()),
                        _ => None,
                    }
                })
            })
        });

        Ok(OptionalApiKey(api_key))
    }
}

/// Validate API key for web terminal access.
///
/// Returns Ok(()) if the key is valid or if no key is required.
/// Returns Err with error message if validation fails.
pub fn validate_api_key(
    api_key: &Option<String>,
    expected_key: &Option<String>,
    admin_key: &Option<String>,
) -> Result<(), WebTerminalError> {
    // 1. Check if provided key matches expected specific key
    if let Some(expected) = expected_key {
        match api_key {
            Some(provided_key) if provided_key == expected => {
                debug!("API key validated successfully for web terminal");
                return Ok(());
            }
            _ => {} // Continue to admin key check
        }
    } else {
        // No specific key required
        debug!("No specific API key configured for web terminal, checking admin key");
    }

    // 2. Check if provided key matches admin key
    if let Some(admin) = admin_key {
        match api_key {
            Some(provided_key) if provided_key == admin => {
                debug!("Admin API key validated successfully for web terminal");
                return Ok(());
            }
            _ => {} // Continue to error handling
        }
    }

    // 3. Fall back: if no expected key and no admin key, skip validation
    if expected_key.is_none() && admin_key.is_none() {
        debug!("No API keys configured, skipping API key validation");
        return Ok(());
    }

    // 4. Validation failed
    match api_key {
        Some(_) => Err(WebTerminalError::Unauthorized(
            "Invalid API key".to_string(),
        )),
        None => Err(WebTerminalError::Unauthorized(
            "Missing API key".to_string(),
        )),
    }
}

/// Shared state for web terminal handler.
///
/// Uses the `SandboxManager` trait to abstract over different backends
/// (Docker, Kubernetes, etc.) for terminal access.
#[derive(Clone)]
pub struct WebTerminalState {
    backend: Arc<dyn SandboxManager>,
    api_key: Option<String>,
    config: Arc<Config>,
}

impl WebTerminalState {
    /// Create a new web terminal state with explicit API key.
    ///
    /// # Arguments
    ///
    /// * `backend` - Sandbox manager backend (Docker, K8s, etc.)
    /// * `api_key` - Optional API key for authentication
    /// * `config` - Application configuration
    pub fn new(
        backend: Arc<dyn SandboxManager>,
        api_key: Option<String>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            backend,
            api_key,
            config,
        }
    }
}

/// Handle WebSocket upgrade request for terminal access.
#[instrument(skip(state, ws_upgrade, api_key, identity), fields(sandbox_id = %sandbox_id))]
pub async fn terminal_websocket(
    State(state): State<WebTerminalState>,
    Path(sandbox_id): Path<String>,
    ws_upgrade: WebSocketUpgrade,
    identity: Option<Extension<ApiKeyIdentity>>,
    api_key: OptionalApiKey,
) -> impl axum::response::IntoResponse {
    info!("WebSocket connection requested for sandbox: {}", sandbox_id);

    if identity.is_none() {
        if let Err(e) = validate_api_key(
            &api_key.0,
            &state.api_key,
            &state.config.server.admin_api_key,
        ) {
            error!("API key validation failed: {}", e);
            return axum::response::Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(axum::body::Body::from(format!("Unauthorized: {}", e)))
                .unwrap_or_else(|_| {
                    axum::response::Response::new(axum::body::Body::from("Internal error"))
                });
        }
    }

    let identity = identity.as_ref().map(|Extension(identity)| identity);
    let container_id = match get_container_id(&sandbox_id, &state.config, identity).await {
        Ok(id) => id,
        Err(e) => {
            return axum::response::Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(axum::body::Body::from(format!("Not found: {}", e)))
                .unwrap_or_else(|_| {
                    axum::response::Response::new(axum::body::Body::from("Internal error"))
                });
        }
    };

    ws_upgrade
        .on_failed_upgrade(|error| {
            error!("WebSocket upgrade failed: {}", error);
        })
        .on_upgrade(move |socket| handle_terminal_socket(socket, sandbox_id, container_id, state))
}

/// Serve the terminal HTML page.
pub async fn terminal_page(
    State(state): State<WebTerminalState>,
    identity: Option<Extension<ApiKeyIdentity>>,
    api_key: OptionalApiKey,
) -> axum::response::Response {
    if identity.is_none() {
        if let Err(e) = validate_api_key(
            &api_key.0,
            &state.api_key,
            &state.config.server.admin_api_key,
        ) {
            error!("API key validation failed for terminal page: {}", e);
            return axum::response::Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "text/plain")
                .body(axum::body::Body::from(format!("Unauthorized: {}", e)))
                .unwrap_or_else(|_| {
                    axum::response::Response::new(axum::body::Body::from("Internal error"))
                });
        }
    }

    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/html; charset=utf-8")
        .body(axum::body::Body::from(include_str!("static/terminal.html")))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::from("Internal error")))
}

/// Handle WebSocket connection for terminal.
///
/// Creates an interactive terminal session using the `SandboxManager::exec_terminal()`
/// method and bridges the `TerminalStream` to the WebSocket connection.
///
/// The terminal stream handles read, write, and resize operations in a single
/// task using `tokio::select!` for concurrent I/O, since `TerminalStream` is a
/// single object that cannot be split across tasks.
async fn handle_terminal_socket(
    socket: WebSocket,
    sandbox_id: String,
    container_id: String,
    state: WebTerminalState,
) {
    info!(
        "WebSocket connection established for sandbox: {}",
        sandbox_id
    );

    // Try bash first, fall back to sh
    let mut terminal_stream = match state.backend.exec_terminal(&container_id, None).await {
        Ok(stream) => {
            debug!("Created bash terminal stream");
            stream
        }
        Err(_) => {
            debug!("Bash not available, trying sh");
            match state
                .backend
                .exec_terminal(&container_id, Some("sh".to_string()))
                .await
            {
                Ok(stream) => stream,
                Err(e) => {
                    let mut s = socket;
                    let _ = send_error(&mut s, &format!("Failed to create terminal: {}", e)).await;
                    return;
                }
            }
        }
    };

    // Split WebSocket into sender and receiver
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Main I/O loop: handle terminal output, WebSocket input, and resize concurrently.
    // The TerminalStream must be used from a single task, so we use tokio::select!
    loop {
        tokio::select! {
            // Read from terminal → send to WebSocket
            frame = terminal_stream.read_frame() => {
                match frame {
                    Ok(Some(TerminalFrame::Data(data))) => {
                        let data_str = String::from_utf8_lossy(&data).to_string();
                        if !data_str.is_empty() {
                            let msg = ServerMessage::Output(data_str);
                            if let Ok(json) = serde_json::to_string(&msg) {
                                use futures_util::SinkExt;
                                if ws_sender.send(Message::Text(json.into())).await.is_err() {
                                    error!("Failed to send output to WebSocket");
                                    break;
                                }
                            }
                        }
                    }
                    Ok(Some(TerminalFrame::Closed)) | Ok(None) => {
                        debug!("Terminal stream ended");
                        use futures_util::SinkExt;
                        if let Ok(json) = serde_json::to_string(&ServerMessage::End) {
                            let _ = ws_sender.send(Message::Text(json.into())).await;
                        }
                        break;
                    }
                    Err(e) => {
                        error!("Error reading from terminal: {}", e);
                        break;
                    }
                }
            }
            // Read from WebSocket → write to terminal
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                            match client_msg {
                                ClientMessage::Input(data) => {
                                    if let Err(e) = terminal_stream.write(data.as_bytes()).await {
                                        error!("Failed to write to terminal: {}", e);
                                        break;
                                    }
                                }
                                ClientMessage::Resize { rows, cols } => {
                                    if let Err(e) = terminal_stream.resize(rows, cols).await {
                                        error!("Failed to resize terminal: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        debug!("Client requested close");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        debug!("WebSocket closed");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    info!("WebSocket connection closed for sandbox: {}", sandbox_id);
}

/// Send an error message to the client.
async fn send_error(socket: &mut WebSocket, message: &str) -> Result<(), axum::Error> {
    let msg = ServerMessage::Error(message.to_string());
    if let Ok(json) = serde_json::to_string(&msg) {
        let _ = socket.send(Message::Text(json.into())).await;
    }
    Ok(())
}

/// Get container ID from sandbox ID.
///
/// Queries the database to resolve a sandbox UUID to its container ID.
/// This function is backend-agnostic and works with any SandboxManager.
async fn get_container_id(
    sandbox_id: &str,
    config: &Config,
    identity: Option<&ApiKeyIdentity>,
) -> Result<String, WebTerminalError> {
    // Parse the sandbox_id as UUID
    let uuid = uuid::Uuid::parse_str(sandbox_id).map_err(|_| {
        WebTerminalError::ContainerNotFound("Invalid sandbox ID format".to_string())
    })?;

    // Query database to get container_id using config
    let database_url = config.database.get_url().ok_or_else(|| {
        WebTerminalError::BackendConnectionFailed("Database URL not configured".to_string())
    })?;

    // Connect to PostgreSQL
    let (client, connection) = tokio_postgres::connect(&database_url, tokio_postgres::NoTls)
        .await
        .map_err(|e| {
            WebTerminalError::BackendConnectionFailed(format!(
                "Failed to connect to database: {}",
                e
            ))
        })?;

    // Spawn connection handler
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            error!("Database connection error: {}", e);
        }
    });

    let row = if matches!(
        identity.map(|identity| &identity.key_type),
        Some(ApiKeyType::Database)
    ) {
        let api_key_id = identity.and_then(|identity| identity.id).ok_or_else(|| {
            WebTerminalError::ContainerNotFound("Database API key missing identity".to_string())
        })?;

        client
            .query_opt(
                "SELECT container_id FROM sandboxes WHERE id = $1 AND api_key_id = $2 AND deleted_at IS NULL",
                &[&uuid, &api_key_id],
            )
            .await
            .map_err(|e| {
                WebTerminalError::ContainerNotFound(format!("Failed to query sandbox: {}", e))
            })?
            .ok_or_else(|| {
                WebTerminalError::ContainerNotFound("Sandbox not found".to_string())
            })?
    } else {
        client
            .query_opt(
                "SELECT container_id FROM sandboxes WHERE id = $1 AND deleted_at IS NULL",
                &[&uuid],
            )
            .await
            .map_err(|e| {
                WebTerminalError::ContainerNotFound(format!("Failed to query sandbox: {}", e))
            })?
            .ok_or_else(|| WebTerminalError::ContainerNotFound("Sandbox not found".to_string()))?
    };

    let container_id: String = row.try_get("container_id").map_err(|e| {
        WebTerminalError::ContainerNotFound(format!("Failed to get container_id: {}", e))
    })?;

    if container_id.is_empty() {
        return Err(WebTerminalError::ContainerNotFound(
            "Sandbox has no container ID".to_string(),
        ));
    }

    Ok(container_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use std::sync::OnceLock;
    use tokio::sync::Mutex;

    // Use a mutex to serialize tests that touch environment variables
    static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_mutex() -> &'static Mutex<()> {
        ENV_MUTEX.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_client_message_input_serialization() {
        let json = r#"{"type":"input","data":"hello\n"}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("valid JSON");
        assert!(matches!(msg, ClientMessage::Input(s) if s == "hello\n"));
    }

    #[test]
    fn test_client_message_resize_serialization() {
        let json = r#"{"type":"resize","data":{"rows":24,"cols":80}}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("valid JSON");
        assert!(matches!(msg, ClientMessage::Resize { rows: 24, cols: 80 }));
    }

    #[test]
    fn test_client_message_invalid_type() {
        let json = r#"{"type":"unknown","data":"test"}"#;
        let result: Result<ClientMessage, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_server_message_output_serialization() {
        let msg = ServerMessage::Output("hello world".to_string());
        let json = serde_json::to_string(&msg).expect("infallible serialization");
        assert!(json.contains("\"output\""));
        assert!(json.contains("hello world"));
    }

    #[test]
    fn test_server_message_end_serialization() {
        let msg = ServerMessage::End;
        let json = serde_json::to_string(&msg).expect("infallible serialization");
        // Serde may omit null fields, just check it contains "end"
        assert!(json.contains("\"end\""));
        assert!(json.contains("\"type\""));
    }

    #[test]
    fn test_web_terminal_error_display() {
        let err = WebTerminalError::ContainerNotFound("abc123".to_string());
        assert!(err.to_string().contains("abc123"));
    }

    #[test]
    fn test_env_mutex_works() {
        let guard = env_mutex().try_lock();
        assert!(guard.is_ok());
        // Just verify the mutex works
    }

    #[test]
    fn test_web_terminal_error_exec_creation_failed() {
        let err = WebTerminalError::ExecCreationFailed("Failed to create exec".to_string());
        assert!(err.to_string().contains("Failed to create exec"));
        assert!(err.to_string().contains("Failed to create exec"));
    }

    #[test]
    fn test_web_terminal_error_exec_start_failed() {
        let err = WebTerminalError::ExecStartFailed("Failed to start exec".to_string());
        assert!(err.to_string().contains("Failed to start exec"));
    }

    #[test]
    fn test_web_terminal_error_websocket() {
        let err = WebTerminalError::WebSocketError("Connection closed".to_string());
        assert!(err.to_string().contains("Connection closed"));
        assert!(err.to_string().contains("WebSocket error"));
    }

    #[test]
    fn test_web_terminal_error_docker_connection() {
        let err = WebTerminalError::BackendConnectionFailed("Backend not running".to_string());
        assert!(err.to_string().contains("Backend not running"));
        assert!(err.to_string().contains("Backend connection failed"));
    }

    #[test]
    fn test_web_terminal_error_unauthorized() {
        let err = WebTerminalError::Unauthorized("Missing API key".to_string());
        assert!(err.to_string().contains("Missing API key"));
        assert!(err.to_string().contains("Unauthorized"));
    }

    #[test]
    fn test_client_message_with_empty_input() {
        let json = r#"{"type":"input","data":""}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("valid JSON");
        assert!(matches!(msg, ClientMessage::Input(s) if s.is_empty()));
    }

    #[test]
    fn test_client_message_resize_min_values() {
        let json = r#"{"type":"resize","data":{"rows":1,"cols":1}}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("valid JSON");
        assert!(matches!(msg, ClientMessage::Resize { rows: 1, cols: 1 }));
    }

    #[test]
    fn test_client_message_resize_large_values() {
        let json = r#"{"type":"resize","data":{"rows":1000,"cols":500}}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("valid JSON");
        assert!(matches!(
            msg,
            ClientMessage::Resize {
                rows: 1000,
                cols: 500
            }
        ));
    }

    #[test]
    fn test_server_message_with_special_chars() {
        let msg = ServerMessage::Output("hello\t\nworld\rtest".to_string());
        let json = serde_json::to_string(&msg).expect("infallible serialization");
        assert!(json.contains("hello"));
        assert!(json.contains("world"));
    }

    #[test]
    fn test_server_message_with_unicode() {
        let msg = ServerMessage::Output("Hello 世界 🌍".to_string());
        let json = serde_json::to_string(&msg).expect("infallible serialization");
        assert!(json.contains("世界"));
        assert!(json.contains("🌍"));
    }

    #[test]
    fn test_server_message_error() {
        let msg = ServerMessage::Error("Something went wrong".to_string());
        let json = serde_json::to_string(&msg).expect("infallible serialization");
        assert!(json.contains("\"error\""));
        assert!(json.contains("Something went wrong"));
    }

    #[test]
    fn test_client_message_with_control_characters() {
        // Use proper JSON encoding for control characters
        let json = r#"{"type":"input","data":"\u0000\u0001\u0002\u0003"}"#;
        let msg: ClientMessage = serde_json::from_str(json).expect("valid JSON");
        assert!(matches!(msg, ClientMessage::Input(data) if data.len() == 4));
    }

    #[test]
    fn test_web_terminal_error_debug_format() {
        let err = WebTerminalError::ContainerNotFound("test-id".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("ContainerNotFound"));
    }
}
