// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # VNC Proxy Module
//!
//! WebSocket-based VNC access to sandbox containers.
//!
//! ## Architecture
//!
//! ```text
//! Browser (noVNC) ←WebSocket→ VNC Proxy ←TCP→ Container:5901 (x11vnc)
//! ```
//!
//! ## Features
//!
//! - Bidirectional binary WebSocket communication
//! - Direct TCP bridge to container VNC server
//! - Automatic session cleanup
//! - Support for multiple concurrent VNC connections
//! - Optional API key authentication
//! - Backend-agnostic via SandboxManager trait (Docker, K8s, etc.)
//!
//! ## Testing Strategy
//!
//! The VNC proxy module is tested through:
//!
//! ### Unit Tests
//! - API key validation logic
//! - Container ID resolution
//! - Error handling
//!
//! ### Integration Tests
//! WebSocket handler tests require:
//! - Real WebSocket connections
//! - Running containers with VNC server
//! - TCP connections to container:5901

use crate::core::types::{ApiKeyIdentity, ApiKeyType};
use crate::config::Config;
use crate::core::manager::SandboxManager;
use crate::db::session_token_store::SessionTokenStore;
use axum::body::Bytes;
use axum::extract::{
    ws::{Message, WebSocket, WebSocketUpgrade},
    Extension, FromRequestParts, Path, State,
};
use axum::http::StatusCode;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info, instrument, warn};

/// Errors that can occur during VNC proxy operations.
#[derive(Error, Debug)]
pub enum VncProxyError {
    /// The requested sandbox was not found
    #[error("Sandbox not found: {0}")]
    SandboxNotFound(String),

    /// The container for the sandbox was not found
    #[error("Container not found: {0}")]
    ContainerNotFound(String),

    /// Failed to establish TCP connection to the VNC server
    #[error("Failed to connect to VNC server: {0}")]
    VncConnectionFailed(String),

    /// WebSocket upgrade or communication failed
    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    /// Database connection pool error
    #[error("Database connection failed: {0}")]
    DatabaseConnectionFailed(String),

    /// Session token or API key validation failed
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

/// Optional API key extractor for VNC proxy authentication.
#[derive(Debug, Clone)]
pub struct OptionalApiKey(pub Option<String>);

/// Optional session token for VNC proxy authentication.
#[derive(Debug, Clone)]
pub struct OptionalSessionToken(pub Option<String>);

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

impl<S> FromRequestParts<S> for OptionalSessionToken
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        // Try to get session token from Authorization header first
        let auth_header = parts.headers.get("authorization");
        let token_from_header = match auth_header {
            Some(header) => match header.to_str() {
                Ok(v) => Some(v),
                Err(_) => return Err((StatusCode::UNAUTHORIZED, "Invalid header encoding")),
            },
            None => None,
        };

        let token_from_header = match token_from_header {
            Some(tok) if tok.starts_with("Bearer ") => Some(tok[7..].to_string()),
            Some(_) => None,
            None => None,
        };

        if token_from_header.is_some() {
            return Ok(OptionalSessionToken(token_from_header));
        }

        // Fall back to query parameter for WebSocket connections
        let token_from_query = parts.uri.query().and_then(|query| {
            query.split('&').find_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                match (parts.next(), parts.next()) {
                    (Some("token"), Some(value)) => Some(value.to_string()),
                    _ => None,
                }
            })
        });

        Ok(OptionalSessionToken(token_from_query))
    }
}

/// Validate API key for VNC proxy access.
///
/// Returns Ok(()) if the key is valid or if no key is required.
/// Returns Err with error message if validation fails.
pub fn validate_api_key(
    api_key: &Option<String>,
    expected_key: &Option<String>,
    admin_key: &Option<String>,
) -> Result<(), VncProxyError> {
    // 1. Check if provided key matches expected specific key
    if let Some(expected) = expected_key {
        match api_key {
            Some(provided_key) if provided_key == expected => {
                debug!("API key validated successfully for VNC proxy");
                return Ok(());
            }
            _ => {} // Continue to admin key check
        }
    } else {
        // No specific key required
        debug!("No specific API key configured for VNC proxy, checking admin key");
    }

    // 2. Check if provided key matches admin key
    if let Some(admin) = admin_key {
        match api_key {
            Some(provided_key) if provided_key == admin => {
                debug!("Admin API key validated successfully for VNC proxy");
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
        Some(_) => Err(VncProxyError::Unauthorized("Invalid API key".to_string())),
        None => Err(VncProxyError::Unauthorized("Missing API key".to_string())),
    }
}

/// Shared state for VNC proxy handler.
pub struct VncProxyState {
    backend: Arc<dyn SandboxManager>,
    api_key: Option<String>,
    config: Arc<Config>,
    /// Database pool for session token validation
    pub db_pool: Option<deadpool_postgres::Pool>,
}

impl std::fmt::Debug for VncProxyState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VncProxyState")
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("db_pool", &self.db_pool.is_some())
            .finish()
    }
}

impl Clone for VncProxyState {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
            api_key: self.api_key.clone(),
            config: self.config.clone(),
            db_pool: self.db_pool.clone(),
        }
    }
}

impl VncProxyState {
    /// Create a new VNC proxy state with explicit API key.
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
            db_pool: None,
        }
    }

    /// Set the database pool for session token validation
    pub fn with_db_pool(mut self, pool: deadpool_postgres::Pool) -> Self {
        self.db_pool = Some(pool);
        self
    }
}

/// Handle WebSocket upgrade request for VNC access.
#[instrument(skip(state, ws_upgrade, identity, api_key, session_token), fields(sandbox_id = %sandbox_id))]
pub async fn vnc_websocket(
    State(state): State<VncProxyState>,
    Path(sandbox_id): Path<String>,
    ws_upgrade: WebSocketUpgrade,
    identity: Option<Extension<ApiKeyIdentity>>,
    api_key: OptionalApiKey,
    session_token: OptionalSessionToken,
) -> impl axum::response::IntoResponse {
    info!(
        "WebSocket VNC connection requested for sandbox: {}",
        sandbox_id
    );

    // First, try to validate session token if provided
    if let Some(token) = session_token.0 {
        if let Some(pool) = &state.db_pool {
            let store = crate::db::PostgresSessionTokenStore::new(pool.clone());
            match store.get_session_token(&token).await {
                Ok(Some(st))
                    if st.validate(&st.sandbox_id, "vnc") && st.sandbox_id == sandbox_id =>
                {
                    debug!("Valid VNC session token provided");
                    // Token is valid, continue with connection
                }
                _ => {
                    warn!("Invalid or expired VNC session token");
                    return axum::response::Response::builder()
                        .status(StatusCode::UNAUTHORIZED)
                        .body(axum::body::Body::from(
                            "Unauthorized: Invalid session token",
                        ))
                        .unwrap_or_else(|_| {
                            axum::response::Response::new(axum::body::Body::from("Internal error"))
                        });
                }
            }
        } else {
            warn!("Session token provided but database not configured");
            return axum::response::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from("Database not configured"))
                .unwrap_or_else(|_| {
                    axum::response::Response::new(axum::body::Body::from("Internal error"))
                });
        }
    } else {
        // No session token, fall back to API key validation if auth is required
        if state.config.server.vnc_require_auth && identity.is_none() {
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
        .on_upgrade(move |socket| handle_vnc_socket(socket, sandbox_id, container_id, state))
}

/// Handle WebSocket connection for VNC proxy.
async fn handle_vnc_socket(
    mut socket: WebSocket,
    sandbox_id: String,
    container_id: String,
    state: VncProxyState,
) {
    info!(
        "WebSocket VNC connection established for sandbox: {}",
        sandbox_id
    );

    debug!(
        "Resolved container ID: {} for sandbox: {}",
        container_id, sandbox_id
    );

    // Resolve the VNC address through the backend trait
    let vnc_port = state.config.sandbox.vnc_port;
    let vnc_addr = match state
        .backend
        .get_sandbox_address(&container_id, vnc_port)
        .await
    {
        Ok(addr) => addr,
        Err(e) => {
            let error_msg = format!(
                "Failed to resolve VNC address for sandbox {}: {}",
                sandbox_id, e
            );
            error!("{}", error_msg);
            let _ = socket.send(Message::Text(error_msg.into())).await;
            let _ = socket.close().await;
            return;
        }
    };

    let tcp_stream = match TcpStream::connect(&vnc_addr).await {
        Ok(stream) => {
            debug!("Connected to VNC server at {}", vnc_addr);
            stream
        }
        Err(e) => {
            let error_msg = format!(
                "Failed to connect to VNC server at {}: {}. Make sure the sandbox is running and VNC is enabled.",
                vnc_addr, e
            );
            error!("{}", error_msg);
            let _ = socket.send(Message::Text(error_msg.into())).await;
            let _ = socket.close().await;
            return;
        }
    };

    // Split TCP stream for bidirectional bridging
    let (mut tcp_reader, mut tcp_writer) = tcp_stream.into_split();

    // Split WebSocket into sender and receiver
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Spawn a task to handle TCP → WebSocket forwarding
    let ws_to_tcp_task = tokio::spawn(async move {
        let mut buffer = vec![0u8; 8192]; // 8KB buffer for VNC data

        loop {
            match tcp_reader.read(&mut buffer).await {
                Ok(0) => {
                    debug!("VNC server closed connection");
                    break;
                }
                Ok(n) => {
                    // Forward TCP data to WebSocket as binary
                    if ws_sender
                        .send(Message::Binary(Bytes::copy_from_slice(&buffer[..n])))
                        .await
                        .is_err()
                    {
                        error!("Failed to send VNC data to WebSocket");
                        break;
                    }
                }
                Err(e) => {
                    error!("Error reading from VNC server: {}", e);
                    break;
                }
            }
        }
    });

    // Handle WebSocket → TCP forwarding
    let tcp_to_ws_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            match msg {
                Message::Binary(data) => {
                    // Forward WebSocket binary data to TCP
                    if let Err(e) = tcp_writer.write_all(&data).await {
                        error!("Failed to write to VNC server: {}", e);
                        break;
                    }
                }
                Message::Close(_) => {
                    debug!("Client requested close");
                    break;
                }
                _ => {
                    // Ignore text messages and other message types
                    debug!("Ignoring non-binary WebSocket message");
                }
            }
        }
    });

    // Wait for both tasks to complete
    let _ = tokio::join!(ws_to_tcp_task, tcp_to_ws_task);

    info!(
        "WebSocket VNC connection closed for sandbox: {}",
        sandbox_id
    );
}

/// Get container ID from sandbox ID by querying the database.
async fn get_container_id(
    sandbox_id: &str,
    config: &Config,
    identity: Option<&ApiKeyIdentity>,
) -> Result<String, VncProxyError> {
    // Parse the sandbox_id as UUID
    let uuid = uuid::Uuid::parse_str(sandbox_id)
        .map_err(|_| VncProxyError::SandboxNotFound("Invalid sandbox ID format".to_string()))?;

    // Query database to get container_id using config
    let database_url = config.database.get_url().ok_or_else(|| {
        VncProxyError::DatabaseConnectionFailed("Database URL not configured".to_string())
    })?;

    // Connect to PostgreSQL
    let (client, connection) = tokio_postgres::connect(&database_url, tokio_postgres::NoTls)
        .await
        .map_err(|e| {
            VncProxyError::DatabaseConnectionFailed(format!("Failed to connect to database: {}", e))
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
            VncProxyError::SandboxNotFound("Database API key missing identity".to_string())
        })?;

        client
            .query_opt(
                "SELECT container_id FROM sandboxes WHERE id = $1 AND api_key_id = $2 AND deleted_at IS NULL",
                &[&uuid, &api_key_id],
            )
            .await
            .map_err(|e| {
                VncProxyError::SandboxNotFound(format!("Failed to query sandbox: {}", e))
            })?
            .ok_or_else(|| VncProxyError::SandboxNotFound("Sandbox not found".to_string()))?
    } else {
        client
            .query_opt(
                "SELECT container_id FROM sandboxes WHERE id = $1 AND deleted_at IS NULL",
                &[&uuid],
            )
            .await
            .map_err(|e| VncProxyError::SandboxNotFound(format!("Failed to query sandbox: {}", e)))?
            .ok_or_else(|| VncProxyError::SandboxNotFound("Sandbox not found".to_string()))?
    };

    let container_id: String = row.try_get("container_id").map_err(|e| {
        VncProxyError::SandboxNotFound(format!("Failed to get container_id: {}", e))
    })?;

    if container_id.is_empty() {
        return Err(VncProxyError::SandboxNotFound(
            "Sandbox has no container ID".to_string(),
        ));
    }

    debug!(
        "Retrieved container ID: {} for sandbox: {}",
        container_id, sandbox_id
    );
    Ok(container_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_api_key_no_key_required() {
        let result = validate_api_key(&None, &None, &None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_api_key_with_valid_key() {
        let result = validate_api_key(
            &Some("test-key".to_string()),
            &Some("test-key".to_string()),
            &None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_api_key_with_invalid_key() {
        let result = validate_api_key(
            &Some("wrong-key".to_string()),
            &Some("test-key".to_string()),
            &None,
        );
        assert!(result.is_err());
        assert!(matches!(result, Err(VncProxyError::Unauthorized(_))));
    }

    #[test]
    fn test_validate_api_key_missing_when_required() {
        let result = validate_api_key(&None, &Some("test-key".to_string()), &None);
        assert!(result.is_err());
        assert!(matches!(result, Err(VncProxyError::Unauthorized(_))));
    }

    #[test]
    fn test_validate_api_key_with_admin_key() {
        let result = validate_api_key(
            &Some("admin-key".to_string()),
            &Some("test-key".to_string()),
            &Some("admin-key".to_string()),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_api_key_with_admin_key_no_specific() {
        let result = validate_api_key(
            &Some("admin-key".to_string()),
            &None,
            &Some("admin-key".to_string()),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_api_key_invalid_even_with_admin() {
        let result = validate_api_key(
            &Some("wrong-key".to_string()),
            &Some("test-key".to_string()),
            &Some("admin-key".to_string()),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_api_key_with_empty_string() {
        // Empty string should be treated as a key, not as None
        let result = validate_api_key(&Some("".to_string()), &Some("".to_string()), &None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_api_key_empty_vs_none() {
        // Empty key when non-empty expected should fail
        let result = validate_api_key(&Some("".to_string()), &Some("test-key".to_string()), &None);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_api_key_none_when_some_expected() {
        // None provided when Some expected should fail
        let result = validate_api_key(&None, &Some("expected-key".to_string()), &None);
        assert!(result.is_err());
        assert!(matches!(result, Err(VncProxyError::Unauthorized(_))));
    }

    #[test]
    fn test_validate_api_key_special_characters() {
        // Test with special characters in key
        let key = "key-with-special-chars-@#$%^&*()";
        let result = validate_api_key(&Some(key.to_string()), &Some(key.to_string()), &None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_api_key_case_sensitive() {
        // Keys should be case-sensitive
        let result = validate_api_key(
            &Some("Test-Key".to_string()),
            &Some("test-key".to_string()),
            &None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_api_key_whitespace() {
        // Keys with whitespace should be treated literally
        let result = validate_api_key(
            &Some("test key".to_string()),
            &Some("test key".to_string()),
            &None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_api_key_unicode() {
        // Test with unicode characters in key
        let unicode_key = "key-中文-日本語-한국어";
        let result = validate_api_key(
            &Some(unicode_key.to_string()),
            &Some(unicode_key.to_string()),
            &None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_api_key_very_long_key() {
        // Test with very long key
        let long_key = "a".repeat(1000);
        let result = validate_api_key(&Some(long_key.clone()), &Some(long_key), &None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_api_key_unicode_mismatch() {
        // Unicode keys should still be case-sensitive
        let result = validate_api_key(&Some("Key".to_string()), &Some("key".to_string()), &None);
        assert!(result.is_err());
    }
}
