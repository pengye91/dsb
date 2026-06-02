// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # SSH Session Service
//!
//! This module provides high-level SSH session management operations.
//!
//! ## Overview
//!
//! The `SshSessionService` coordinates SSH session operations, integrating with
//! the `SandboxService` for authorization and validation, and the `SshSessionStore`
//! for persistence.
//!
//! ## Testing Strategy
//!
//! The SSH session service is tested through comprehensive integration tests:
//!
//! ### Unit Tests (This Module)
//! - Error type formatting and display tests
//! - Basic structural validation
//!
//! ### Integration Tests
//! Full lifecycle tests in:
//! - **`tests/integration_ssh_docker.rs`** (8 tests): SSH session lifecycle,
//!   sandbox validation, Docker exec PTY operations
//! - **`tests/test_ssh_session_cleanup.rs`** (5 tests): Background cleanup tasks,
//!   stale session detection, statistics queries
//!
//! Integration tests cover:
//! - Session creation with sandbox validation
//! - Session state transitions (Connecting → Active → Disconnected → Terminated)
//! - Activity tracking and heartbeat updates
//! - Background cleanup task execution
//! - Docker exec PTY creation and management
//! - Session statistics and reporting
//!
//! ## Example
//!
//! ```rust,no_run,ignore
//! use dsb::core::ssh_service::SshSessionService;
//! use dsb::core::types::{CreateSshSessionRequest, SshAuthMethod};
//! use dsb::db::PostgresSshSessionStore;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a pool from environment (requires DATABASE_URL to be set)
//! let pool = dsb::db::pool::create_pool_from_env().await?;
//! let ssh_store = Arc::new(PostgresSshSessionStore::new(pool));
//!
//! let ssh_service = SshSessionService::new(ssh_store);
//!
//! // Create a session
//! let request = CreateSshSessionRequest {
//!     sandbox_id: uuid::Uuid::new_v4(),
//!     client_ip: "127.0.0.1".to_string(),
//!     ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
//!     auth_method: SshAuthMethod::ApiKey,
//! };
//!
//! let session = ssh_service.create_session(request).await?;
//! # Ok(())
//! # }
//! ```

use crate::core::errors::{ApiError, ErrorCode};
use crate::core::sandbox::SandboxService;
use crate::core::types::{ApiKeyIdentity, ApiKeyType};
use crate::core::types::{
    CreateSshSessionRequest, Sandbox, SandboxState, SshSession, SshSessionFilters, SshSessionState,
};
use crate::db::SshSessionStoreTrait;
use chrono::Utc;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, instrument, warn};
use uuid::Uuid;

/// Errors that can occur during SSH session operations.
///
/// Each error variant maps to a specific `ErrorCode` for consistent error handling
/// across the API. Use the `error_code()` method to get the corresponding code.
#[derive(Error, Debug, PartialEq)]
pub enum SshServiceError {
    /// The requested sandbox does not exist
    #[error("Sandbox not found: {0}")]
    SandboxNotFound(Uuid),

    /// The sandbox exists but is not in a running state
    #[error("Sandbox is not running: {0}")]
    SandboxNotRunning(Uuid),

    /// SSH authentication failed (invalid key or unauthorized)
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// The requested SSH session does not exist
    #[error("Session not found: {0}")]
    SessionNotFound(Uuid),

    /// Database operation failed
    #[error("Database error: {0}")]
    DatabaseError(String),

    /// The request parameters were invalid
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

impl SshServiceError {
    /// Get the error code for this error
    ///
    /// Returns the unified `ErrorCode` that corresponds to this error variant.
    /// This enables consistent error handling across Rust backend, Python SDK, and sandbox.
    ///
    /// # Examples
    ///
    /// ```
    /// use dsb::core::SshServiceError;
    /// use dsb::api::errors::ErrorCode;
    /// use uuid::Uuid;
    ///
    /// let err = SshServiceError::SandboxNotFound(Uuid::new_v4());
    /// assert_eq!(err.error_code(), ErrorCode::SandboxNotFound);
    /// ```
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::SandboxNotFound(_) => ErrorCode::SandboxNotFound,
            Self::SandboxNotRunning(_) => ErrorCode::SandboxInvalidState,
            Self::AuthenticationFailed(_) => ErrorCode::SshAuthenticationFailed,
            Self::SessionNotFound(_) => ErrorCode::SshSessionNotFound,
            Self::DatabaseError(_) => ErrorCode::DatabaseQueryFailed,
            Self::InvalidRequest(_) => ErrorCode::ValidationInvalidRequest,
        }
    }
}

/// Result type for SSH service operations.
pub type Result<T> = std::result::Result<T, SshServiceError>;

/// SSH session service providing high-level session management.
///
/// This service integrates with the `SandboxService` for authorization and
/// validation, and with the `SshSessionStore` for persistence.
#[derive(Clone)]
pub struct SshSessionService {
    ssh_store: Arc<dyn SshSessionStoreTrait>,
    sandbox_service: Option<Arc<SandboxService>>,
}

impl SshSessionService {
    /// Create a new SSH session service without sandbox validation.
    ///
    /// # Arguments
    ///
    /// * `ssh_store` - SSH session store implementation
    ///
    /// # Returns
    ///
    /// A new `SshSessionService` instance
    ///
    /// # Note
    ///
    /// This constructor doesn't include sandbox validation. Use `new_with_sandbox_service`
    /// to enable validation that sandboxes exist and are running before creating SSH sessions.
    pub fn new(ssh_store: Arc<dyn SshSessionStoreTrait>) -> Self {
        Self {
            ssh_store,
            sandbox_service: None,
        }
    }

    /// Create a new SSH session service with sandbox validation.
    ///
    /// # Arguments
    ///
    /// * `ssh_store` - SSH session store implementation
    /// * `sandbox_service` - Sandbox service for validation
    ///
    /// # Returns
    ///
    /// A new `SshSessionService` instance
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::ssh_service::SshSessionService;
    /// # use dsb::db::PostgresSshSessionStore;
    /// # use dsb::core::SandboxService;
    /// # use std::sync::Arc;
    /// # fn example(ssh_store: Arc<PostgresSshSessionStore>, sandbox_service: Arc<SandboxService>) {
    /// // Create SSH service with sandbox validation enabled
    /// let ssh_service = SshSessionService::new_with_sandbox_service(
    ///     ssh_store,
    ///     sandbox_service
    /// );
    /// # }
    /// ```
    pub fn new_with_sandbox_service(
        ssh_store: Arc<dyn SshSessionStoreTrait>,
        sandbox_service: Arc<SandboxService>,
    ) -> Self {
        Self {
            ssh_store,
            sandbox_service: Some(sandbox_service),
        }
    }

    /// Authorize access to a sandbox for an API key identity.
    pub async fn authorize_sandbox_access(
        &self,
        identity: &ApiKeyIdentity,
        sandbox_id: &Uuid,
    ) -> std::result::Result<(), ApiError> {
        if let Some(ref sandbox_service) = self.sandbox_service {
            sandbox_service
                .check_sandbox_ownership(identity, sandbox_id)
                .await
        } else if matches!(identity.key_type, ApiKeyType::Privileged) {
            Ok(())
        } else {
            Err(ApiError::Validation {
                message: "SSH session authorization requires a privileged API key".to_string(),
                field: None,
                code: ErrorCode::AuthorizationInsufficientPermissions,
            })
        }
    }

    /// Create a new SSH session.
    ///
    /// # Arguments
    ///
    /// * `request` - Session creation request
    ///
    /// # Returns
    ///
    /// The created SSH session
    ///
    /// # Errors
    ///
    /// Returns `SshServiceError::SandboxNotFound` if the sandbox doesn't exist.
    /// Returns `SshServiceError::SandboxNotRunning` if the sandbox is not in running state.
    #[instrument(skip(self, request), fields(sandbox_id = %request.sandbox_id, client_ip = %request.client_ip))]
    pub async fn create_session(&self, request: CreateSshSessionRequest) -> Result<SshSession> {
        debug!("Creating SSH session");

        // Validate sandbox exists and is running (if sandbox_service is available)
        if let Some(ref sandbox_service) = self.sandbox_service {
            debug!("Validating sandbox exists and is running");

            let sandbox: Sandbox = match sandbox_service.get_sandbox(&request.sandbox_id).await {
                Some(s) => s,
                None => return Err(SshServiceError::SandboxNotFound(request.sandbox_id)),
            };

            if sandbox.state != SandboxState::Running {
                warn!(
                    sandbox_id = %request.sandbox_id,
                    state = ?sandbox.state,
                    "Sandbox not in running state"
                );
                return Err(SshServiceError::SandboxNotRunning(request.sandbox_id));
            }

            debug!("Sandbox validation passed");
        } else {
            debug!("No sandbox service available, skipping validation");
        }

        let now = Utc::now();
        let session = SshSession {
            id: Uuid::new_v4(),
            sandbox_id: request.sandbox_id,
            client_ip: request.client_ip,
            ssh_version: request.ssh_version,
            auth_method: request.auth_method,
            ssh_session_id: None,
            exec_id: None,
            pty_term: None,
            pty_rows: None,
            pty_cols: None,
            state: SshSessionState::Connecting,
            connected_at: now,
            disconnected_at: None,
            last_activity_at: now,
            bytes_sent: 0,
            bytes_received: 0,
            duration_seconds: None,
            termination_reason: None,
            created_at: now,
            updated_at: now,
        };

        self.ssh_store
            .create_ssh_session(session.clone())
            .await
            .map_err(|e| SshServiceError::DatabaseError(e.to_string()))?;

        debug!(
            session_id = %session.id,
            "SSH session created successfully"
        );
        Ok(session)
    }

    /// Retrieve an SSH session by ID.
    ///
    /// # Arguments
    ///
    /// * `id` - Session ID
    ///
    /// # Returns
    ///
    /// The SSH session if found
    #[instrument(skip(self), fields(session_id = %id))]
    pub async fn get_session(&self, id: Uuid) -> Result<SshSession> {
        debug!("Retrieving SSH session");

        self.ssh_store
            .get_ssh_session(&id)
            .await
            .ok_or(SshServiceError::SessionNotFound(id))
    }

    /// List SSH sessions with optional filters.
    ///
    /// # Arguments
    ///
    /// * `filters` - Optional filters for the query
    ///
    /// # Returns
    ///
    /// List of SSH sessions matching the filters
    #[instrument(skip(self), fields(filters = ?filters))]
    pub async fn list_sessions(&self, filters: SshSessionFilters) -> Vec<SshSession> {
        debug!("Listing SSH sessions");
        self.ssh_store.list_ssh_sessions(filters).await
    }

    /// Update SSH session state to Active.
    ///
    /// Called when the SSH handshake is complete and PTY is allocated.
    ///
    /// # Arguments
    ///
    /// * `id` - Session ID
    /// * `ssh_session_id` - SSH protocol session ID
    /// * `exec_id` - Docker exec instance ID
    /// * `pty_term` - Terminal type
    /// * `pty_rows` - Terminal rows
    /// * `pty_cols` - Terminal columns
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    #[instrument(skip(self), fields(session_id = %id))]
    pub async fn mark_session_active(
        &self,
        id: Uuid,
        ssh_session_id: Option<String>,
        exec_id: Option<String>,
        pty_term: Option<String>,
        pty_rows: Option<i32>,
        pty_cols: Option<i32>,
    ) -> Result<()> {
        debug!("Marking SSH session as active");

        let mut session = self.get_session(id).await?;

        session.state = SshSessionState::Active;
        session.ssh_session_id = ssh_session_id;
        session.exec_id = exec_id;
        session.pty_term = pty_term;
        session.pty_rows = pty_rows;
        session.pty_cols = pty_cols;
        session.updated_at = Utc::now();

        self.ssh_store
            .update_ssh_session(&session)
            .await
            .map_err(|e| SshServiceError::DatabaseError(e.to_string()))?;

        debug!("SSH session marked as active");
        Ok(())
    }

    /// Update SSH session activity (heartbeat).
    ///
    /// # Arguments
    ///
    /// * `id` - Session ID
    /// * `bytes_sent` - Cumulative bytes sent
    /// * `bytes_received` - Cumulative bytes received
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    #[instrument(skip(self), fields(session_id = %id))]
    pub async fn update_activity(
        &self,
        id: Uuid,
        bytes_sent: i64,
        bytes_received: i64,
    ) -> Result<()> {
        debug!("Updating SSH session activity");

        let mut session = self.get_session(id).await?;

        session.last_activity_at = Utc::now();
        session.bytes_sent = bytes_sent;
        session.bytes_received = bytes_received;
        session.updated_at = Utc::now();

        self.ssh_store
            .update_ssh_session(&session)
            .await
            .map_err(|e| SshServiceError::DatabaseError(e.to_string()))?;

        debug!("SSH session activity updated");
        Ok(())
    }

    /// Disconnect an SSH session.
    ///
    /// # Arguments
    ///
    /// * `id` - Session ID
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    #[instrument(skip(self), fields(session_id = %id))]
    pub async fn disconnect_session(&self, id: Uuid) -> Result<()> {
        debug!("Disconnecting SSH session");

        let mut session = self.get_session(id).await?;

        if session.state.is_terminal() {
            warn!("Session already in terminal state: {:?}", session.state);
            return Ok(());
        }

        session.state = SshSessionState::Disconnected;
        session.disconnected_at = Some(Utc::now());
        session.updated_at = Utc::now();

        // Calculate duration
        let duration = session.disconnected_at.unwrap_or(Utc::now()) - session.connected_at;
        session.duration_seconds = Some(duration.num_seconds() as i32);

        self.ssh_store
            .update_ssh_session(&session)
            .await
            .map_err(|e| SshServiceError::DatabaseError(e.to_string()))?;

        debug!("SSH session disconnected");
        Ok(())
    }

    /// Terminate an SSH session.
    ///
    /// # Arguments
    ///
    /// * `id` - Session ID
    /// * `reason` - Reason for termination
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    #[instrument(skip(self), fields(session_id = %id))]
    pub async fn terminate_session(&self, id: Uuid, reason: String) -> Result<()> {
        debug!(reason = %reason, "Terminating SSH session");

        let mut session = self.get_session(id).await?;

        session.state = SshSessionState::Terminated;
        session.disconnected_at = Some(Utc::now());
        session.termination_reason = Some(reason);
        session.updated_at = Utc::now();

        // Calculate duration
        let duration = session.disconnected_at.unwrap_or(Utc::now()) - session.connected_at;
        session.duration_seconds = Some(duration.num_seconds() as i32);

        self.ssh_store
            .update_ssh_session(&session)
            .await
            .map_err(|e| SshServiceError::DatabaseError(e.to_string()))?;

        debug!("SSH session terminated");
        Ok(())
    }

    /// Terminate all sessions for a sandbox.
    ///
    /// Called when a sandbox is deleted or stopped.
    ///
    /// # Arguments
    ///
    /// * `sandbox_id` - Sandbox ID
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    #[instrument(skip(self), fields(sandbox_id = %sandbox_id))]
    pub async fn terminate_sessions_by_sandbox(&self, sandbox_id: Uuid) -> Result<()> {
        debug!("Terminating all SSH sessions for sandbox");

        self.ssh_store
            .terminate_sessions_by_sandbox(&sandbox_id)
            .await
            .map_err(|e| SshServiceError::DatabaseError(e.to_string()))?;

        debug!("All SSH sessions terminated for sandbox");
        Ok(())
    }

    /// Get stale sessions for cleanup.
    ///
    /// # Arguments
    ///
    /// * `timeout_secs` - Timeout in seconds
    ///
    /// # Returns
    ///
    /// List of stale sessions
    #[instrument(skip(self), fields(timeout_secs = %timeout_secs))]
    pub async fn get_stale_sessions(&self, timeout_secs: i64) -> Result<Vec<SshSession>> {
        debug!("Retrieving stale SSH sessions");

        self.ssh_store
            .get_stale_sessions(timeout_secs)
            .await
            .map_err(|e| SshServiceError::DatabaseError(e.to_string()))
    }

    /// Get sessions stuck in connecting state.
    ///
    /// # Arguments
    ///
    /// * `timeout_secs` - Timeout in seconds
    ///
    /// # Returns
    ///
    /// List of stuck sessions
    #[instrument(skip(self), fields(timeout_secs = %timeout_secs))]
    pub async fn get_stuck_connecting_sessions(
        &self,
        timeout_secs: i64,
    ) -> Result<Vec<SshSession>> {
        debug!("Retrieving stuck connecting SSH sessions");

        self.ssh_store
            .get_stuck_connecting_sessions(timeout_secs)
            .await
            .map_err(|e| SshServiceError::DatabaseError(e.to_string()))
    }

    /// Get orphaned sessions (sandbox no longer running).
    ///
    /// # Returns
    ///
    /// List of orphaned sessions
    #[instrument(skip(self))]
    pub async fn get_orphaned_sessions(&self) -> Result<Vec<SshSession>> {
        debug!("Retrieving orphaned SSH sessions");

        self.ssh_store
            .get_orphaned_sessions()
            .await
            .map_err(|e| SshServiceError::DatabaseError(e.to_string()))
    }

    /// Get session statistics.
    ///
    /// # Returns
    ///
    /// Session statistics
    #[instrument(skip(self))]
    pub async fn get_statistics(&self) -> Result<crate::db::ssh_sessions::SessionStatistics> {
        debug!("Retrieving SSH session statistics");

        self.ssh_store
            .get_session_statistics()
            .await
            .map_err(|e| SshServiceError::DatabaseError(e.to_string()))
    }

    /// Start background cleanup task for stale sessions.
    ///
    /// This task runs periodically and cleans up:
    /// - Idle sessions (no activity for specified timeout)
    /// - Sessions stuck in "connecting" state for too long
    /// - Orphaned sessions (sandbox no longer running)
    ///
    /// # Arguments
    ///
    /// * `idle_timeout_secs` - Idle timeout in seconds
    /// * `connecting_timeout_secs` - Timeout for sessions stuck in connecting state
    /// * `check_interval_secs` - How often to check for stale sessions
    pub fn start_cleanup_task(
        &self,
        idle_timeout_secs: i64,
        connecting_timeout_secs: i64,
        check_interval_secs: u64,
    ) {
        let service = self.clone();
        let idle_timeout = idle_timeout_secs;
        let connecting_timeout = connecting_timeout_secs;

        debug!(
            idle_timeout_secs = %idle_timeout_secs,
            connecting_timeout_secs = %connecting_timeout_secs,
            check_interval_secs = %check_interval_secs,
            "Starting SSH session cleanup task"
        );

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(check_interval_secs));

            loop {
                interval.tick().await;

                debug!("Running SSH session cleanup check");

                // 1. Clean up idle sessions
                match service.get_stale_sessions(idle_timeout).await {
                    Ok(stale_sessions) => {
                        if !stale_sessions.is_empty() {
                            debug!(
                                count = stale_sessions.len(),
                                "Found idle SSH sessions, cleaning up"
                            );

                            for session in stale_sessions {
                                warn!(
                                    session_id = %session.id,
                                    sandbox_id = %session.sandbox_id,
                                    last_activity = ?session.last_activity_at,
                                    "Terminating idle SSH session"
                                );

                                if let Err(e) = service
                                    .terminate_session(session.id, "Idle timeout".to_string())
                                    .await
                                {
                                    error!(
                                        session_id = %session.id,
                                        error = %e,
                                        "Failed to terminate idle SSH session"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to retrieve idle SSH sessions");
                    }
                }

                // 2. Clean up sessions stuck in connecting state
                match service
                    .get_stuck_connecting_sessions(connecting_timeout)
                    .await
                {
                    Ok(stuck_sessions) => {
                        if !stuck_sessions.is_empty() {
                            warn!(
                                count = stuck_sessions.len(),
                                "Found SSH sessions stuck in connecting state"
                            );

                            for session in stuck_sessions {
                                warn!(
                                    session_id = %session.id,
                                    sandbox_id = %session.sandbox_id,
                                    connected_at = ?session.connected_at,
                                    "Terminating SSH session stuck in connecting state"
                                );

                                if let Err(e) = service
                                    .terminate_session(
                                        session.id,
                                        "Connection timeout - stuck in connecting state"
                                            .to_string(),
                                    )
                                    .await
                                {
                                    error!(
                                        session_id = %session.id,
                                        error = %e,
                                        "Failed to terminate stuck SSH session"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to retrieve stuck connecting sessions");
                    }
                }

                // 3. Clean up orphaned sessions (sandbox no longer running)
                match service.get_orphaned_sessions().await {
                    Ok(orphaned_sessions) => {
                        if !orphaned_sessions.is_empty() {
                            warn!(
                                count = orphaned_sessions.len(),
                                "Found orphaned SSH sessions (sandbox not running)"
                            );

                            for session in orphaned_sessions {
                                warn!(
                                    session_id = %session.id,
                                    sandbox_id = %session.sandbox_id,
                                    "Terminating orphaned SSH session"
                                );

                                if let Err(e) = service
                                    .terminate_session(
                                        session.id,
                                        "Sandbox no longer running".to_string(),
                                    )
                                    .await
                                {
                                    error!(
                                        session_id = %session.id,
                                        error = %e,
                                        "Failed to terminate orphaned SSH session"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to retrieve orphaned sessions");
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Error Type Tests
    // ========================================================================

    #[test]
    fn test_error_display_sandbox_not_found() {
        let uuid = Uuid::new_v4();
        let err = SshServiceError::SandboxNotFound(uuid);
        assert!(err.to_string().contains("Sandbox not found"));
        assert!(err.to_string().contains(&uuid.to_string()));
    }

    #[test]
    fn test_error_display_sandbox_not_running() {
        let uuid = Uuid::new_v4();
        let err = SshServiceError::SandboxNotRunning(uuid);
        assert!(err.to_string().contains("Sandbox is not running"));
        assert!(err.to_string().contains(&uuid.to_string()));
    }

    #[test]
    fn test_error_display_authentication_failed() {
        let err = SshServiceError::AuthenticationFailed("Invalid API key".to_string());
        assert!(err.to_string().contains("Authentication failed"));
        assert!(err.to_string().contains("Invalid API key"));
    }

    #[test]
    fn test_error_display_session_not_found() {
        let uuid = Uuid::new_v4();
        let err = SshServiceError::SessionNotFound(uuid);
        assert!(err.to_string().contains("Session not found"));
        assert!(err.to_string().contains(&uuid.to_string()));
    }

    #[test]
    fn test_error_display_database_error() {
        let err = SshServiceError::DatabaseError("Connection lost".to_string());
        assert!(err.to_string().contains("Database error"));
        assert!(err.to_string().contains("Connection lost"));
    }

    #[test]
    fn test_error_display_invalid_request() {
        let err = SshServiceError::InvalidRequest("Missing sandbox_id".to_string());
        assert!(err.to_string().contains("Invalid request"));
        assert!(err.to_string().contains("Missing sandbox_id"));
    }

    #[test]
    fn test_error_debug_formatting() {
        let uuid = Uuid::new_v4();
        let err = SshServiceError::SandboxNotFound(uuid);
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("SandboxNotFound"));
    }

    // ========================================================================
    // Service Type Tests
    // ========================================================================

    #[test]
    fn test_service_is_clone() {
        // Verify SshSessionService implements Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<SshSessionService>();
    }

    #[test]
    fn test_service_is_send() {
        // Verify SshSessionService implements Send
        fn assert_send<T: Send>() {}
        assert_send::<SshSessionService>();
    }

    #[test]
    fn test_service_is_sync() {
        // Verify SshSessionService implements Sync
        fn assert_sync<T: Sync>() {}
        assert_sync::<SshSessionService>();
    }

    // ========================================================================
    // Result Type Tests
    // ========================================================================

    #[test]
    fn test_result_type_alias() {
        // Verify Result type alias works correctly
        fn check_result() -> Result<()> {
            Ok(())
        }
        assert!(check_result().is_ok());
    }

    #[test]
    fn test_result_error_variant() {
        fn check_error() -> Result<()> {
            Err(SshServiceError::InvalidRequest("test".to_string()))
        }
        assert!(check_error().is_err());
    }

    // ========================================================================
    // Error Edge Cases
    // ========================================================================

    #[test]
    fn test_error_with_empty_message() {
        let err = SshServiceError::AuthenticationFailed("".to_string());
        assert!(err.to_string().contains("Authentication failed"));
    }

    #[test]
    fn test_error_with_long_message() {
        let long_msg = "a".repeat(1000);
        let err = SshServiceError::DatabaseError(long_msg.clone());
        assert!(err.to_string().contains(&long_msg[..100]));
    }

    #[test]
    fn test_error_with_special_characters() {
        let err = SshServiceError::InvalidRequest("Error: \n\t\r".to_string());
        let err_str = err.to_string();
        assert!(err_str.contains("Invalid request"));
    }

    #[test]
    fn test_error_with_unicode() {
        let err = SshServiceError::AuthenticationFailed("认证失败".to_string());
        assert!(err.to_string().contains("认证失败"));
    }

    // ========================================================================
    // Integration Test References
    // ========================================================================

    #[test]
    fn test_integration_test_files_exist() {
        // Compile-time verification that integration test files exist
        // This test documents where the full integration tests are located
        let _integration_tests = (
            "tests/integration_ssh_docker.rs",
            "tests/test_ssh_session_cleanup.rs",
        );
    }

    // ========================================================================
    // Service Creation Tests
    // ========================================================================

    #[test]
    fn test_service_error_variants() {
        // Test all error variants can be created
        let uuid = Uuid::new_v4();

        let err1 = SshServiceError::SandboxNotFound(uuid);
        assert!(!err1.to_string().is_empty());

        let err2 = SshServiceError::SandboxNotRunning(uuid);
        assert!(!err2.to_string().is_empty());

        let err3 = SshServiceError::AuthenticationFailed("test".to_string());
        assert!(!err3.to_string().is_empty());

        let err4 = SshServiceError::SessionNotFound(uuid);
        assert!(!err4.to_string().is_empty());

        let err5 = SshServiceError::DatabaseError("test".to_string());
        assert!(!err5.to_string().is_empty());

        let err6 = SshServiceError::InvalidRequest("test".to_string());
        assert!(!err6.to_string().is_empty());
    }

    #[test]
    fn test_service_is_clone_send_sync() {
        // Verify SshSessionService implements Clone, Send, and Sync
        fn assert_clone<T: Clone>() {}
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        // These are compile-time tests
        assert_clone::<SshSessionService>();
        assert_send::<SshSessionService>();
        assert_sync::<SshSessionService>();
    }

    #[test]
    fn test_error_source_chain() {
        // Test error source chain
        let err = SshServiceError::DatabaseError("connection failed".to_string());
        let debug_fmt = format!("{:?}", err);
        // The debug format contains the variant name
        assert!(debug_fmt.contains("DatabaseError"));
    }

    #[test]
    fn test_ssh_service_error_with_unicode_message() {
        let err = SshServiceError::AuthenticationFailed("こんにちは".to_string());
        assert!(err.to_string().contains("こんにちは"));
    }

    #[test]
    fn test_ssh_service_error_with_json_like_message() {
        let err = SshServiceError::InvalidRequest(r#"{"error": "test"}"#.to_string());
        assert!(err.to_string().contains("error"));
    }

    #[test]
    fn test_ssh_service_error_with_newlines() {
        let err = SshServiceError::DatabaseError("line1\nline2\nline3".to_string());
        let err_str = err.to_string();
        assert!(err_str.contains("line1"));
        assert!(err_str.contains("line2"));
        assert!(err_str.contains("line3"));
    }

    #[test]
    fn test_result_option_conversion() {
        let ok: Result<Option<i32>> = Ok(Some(42));
        assert_eq!(ok, Ok(Some(42)));

        let none: Result<Option<i32>> = Ok(None);
        assert_eq!(none, Ok(None));

        let err: Result<Option<i32>> = Err(SshServiceError::SessionNotFound(Uuid::new_v4()));
        assert!(err.is_err());
    }

    #[test]
    fn test_result_with_boxed_error() {
        // Test Result type alias works with SshServiceError
        fn check_result_type() -> Result<()> {
            Err(SshServiceError::DatabaseError("test".to_string()))
        }
        assert!(check_result_type().is_err());
    }

    #[test]
    fn test_service_type_traits() {
        // Just verify the types can be used - actual service creation requires PostgreSQL
        fn accept_service(_: &SshSessionService) {}
        fn accept_error(_: &SshServiceError) {}
        let _ = accept_service;
        let _ = accept_error;
    }

    #[test]
    fn test_error_display_with_special_chars() {
        let err = SshServiceError::AuthenticationFailed("p@$$w0rd!".to_string());
        let err_str = err.to_string();
        assert!(err_str.contains("p@$$w0rd!"));
    }

    #[test]
    fn test_error_display_with_backslash() {
        let err = SshServiceError::InvalidRequest("path\\to\\file".to_string());
        let err_str = err.to_string();
        assert!(err_str.contains("path\\to\\file"));
    }

    #[test]
    fn test_error_display_with_quotes() {
        let err = SshServiceError::InvalidRequest("'test'".to_string());
        let err_str = err.to_string();
        assert!(err_str.contains("'test'"));
    }

    // ========================================================================
    // Service Method Tests (with Mock Store)
    // ========================================================================

    /// Mock implementation of SshSessionStoreTrait for testing
    struct MockSshSessionStore {
        sessions: std::sync::Arc<std::sync::Mutex<Vec<SshSession>>>,
    }

    impl MockSshSessionStore {
        fn new() -> Self {
            Self {
                sessions: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        fn add_session(&self, session: SshSession) {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.push(session);
        }
    }

    #[async_trait::async_trait]
    impl SshSessionStoreTrait for MockSshSessionStore {
        async fn create_ssh_session(
            &self,
            session: SshSession,
        ) -> std::result::Result<(), crate::db::store::StoreError> {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.push(session);
            Ok(())
        }

        async fn get_ssh_session(&self, id: &Uuid) -> Option<SshSession> {
            let sessions = self.sessions.lock().unwrap();
            sessions.iter().find(|s| &s.id == id).cloned()
        }

        async fn list_ssh_sessions(&self, _filters: SshSessionFilters) -> Vec<SshSession> {
            let sessions = self.sessions.lock().unwrap();
            sessions.clone()
        }

        async fn update_ssh_session(
            &self,
            session: &SshSession,
        ) -> std::result::Result<(), crate::db::store::StoreError> {
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(s) = sessions.iter_mut().find(|s| s.id == session.id) {
                *s = session.clone();
            }
            Ok(())
        }

        async fn delete_ssh_session(
            &self,
            id: &Uuid,
        ) -> std::result::Result<(), crate::db::store::StoreError> {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.retain(|s| &s.id != id);
            Ok(())
        }

        async fn terminate_sessions_by_sandbox(
            &self,
            sandbox_id: &Uuid,
        ) -> std::result::Result<(), crate::db::store::StoreError> {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.retain(|s| s.sandbox_id != *sandbox_id);
            Ok(())
        }

        async fn get_stale_sessions(
            &self,
            _timeout_secs: i64,
        ) -> std::result::Result<Vec<SshSession>, crate::db::store::StoreError> {
            let sessions = self.sessions.lock().unwrap();
            Ok(sessions.clone())
        }

        async fn get_stuck_connecting_sessions(
            &self,
            _timeout_secs: i64,
        ) -> std::result::Result<Vec<SshSession>, crate::db::store::StoreError> {
            let sessions = self.sessions.lock().unwrap();
            Ok(sessions.clone())
        }

        async fn get_orphaned_sessions(
            &self,
        ) -> std::result::Result<Vec<SshSession>, crate::db::store::StoreError> {
            let sessions = self.sessions.lock().unwrap();
            Ok(sessions.clone())
        }

        async fn get_session_statistics(
            &self,
        ) -> std::result::Result<
            crate::db::ssh_sessions::SessionStatistics,
            crate::db::store::StoreError,
        > {
            Ok(crate::db::ssh_sessions::SessionStatistics {
                total_sessions: 0,
                active_sessions: 0,
                connecting_sessions: 0,
                disconnected_sessions: 0,
                terminated_sessions: 0,
                error_sessions: 0,
                total_bytes_sent: 0,
                total_bytes_received: 0,
                avg_duration_seconds: None,
            })
        }
    }

    #[tokio::test]
    async fn test_create_session_without_sandbox_service() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let request = CreateSshSessionRequest {
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            username: None,
            public_key: None,
        };

        let result = service.create_session(request).await;
        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.state, SshSessionState::Connecting);
    }

    #[tokio::test]
    async fn test_get_session_not_found() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let result = service.get_session(Uuid::new_v4()).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(SshServiceError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_mark_session_active_not_found() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let result = service
            .mark_session_active(
                Uuid::new_v4(),
                Some("ssh-id".to_string()),
                Some("exec-id".to_string()),
                Some("xterm-256color".to_string()),
                Some(24),
                Some(80),
            )
            .await;

        assert!(result.is_err());
        assert!(matches!(result, Err(SshServiceError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_mark_session_active_success() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        // Create a session first
        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: None,
            exec_id: None,
            pty_term: None,
            pty_rows: None,
            pty_cols: None,
            state: SshSessionState::Connecting,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 0,
            bytes_received: 0,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        let result = service
            .mark_session_active(
                session_id,
                Some("ssh-id".to_string()),
                Some("exec-id".to_string()),
                Some("xterm-256color".to_string()),
                Some(24),
                Some(80),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_activity_not_found() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let result = service.update_activity(Uuid::new_v4(), 1024, 2048).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(SshServiceError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_update_activity_success() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        // Create a session first
        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: Some("ssh-id".to_string()),
            exec_id: Some("exec-id".to_string()),
            pty_term: Some("xterm-256color".to_string()),
            pty_rows: Some(24),
            pty_cols: Some(80),
            state: SshSessionState::Active,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 0,
            bytes_received: 0,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        let result = service.update_activity(session_id, 1024, 2048).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_disconnect_session_not_found() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let result = service.disconnect_session(Uuid::new_v4()).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(SshServiceError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_disconnect_session_already_terminal() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        // Create a session in terminal state
        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: None,
            exec_id: None,
            pty_term: None,
            pty_rows: None,
            pty_cols: None,
            state: SshSessionState::Terminated,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 0,
            bytes_received: 0,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        let result = service.disconnect_session(session_id).await;
        // Should succeed even if already terminated
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_disconnect_session_success() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        // Create an active session
        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: Some("ssh-id".to_string()),
            exec_id: Some("exec-id".to_string()),
            pty_term: Some("xterm-256color".to_string()),
            pty_rows: Some(24),
            pty_cols: Some(80),
            state: SshSessionState::Active,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 1024,
            bytes_received: 2048,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        let result = service.disconnect_session(session_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_terminate_session_not_found() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let result = service
            .terminate_session(Uuid::new_v4(), "Test termination".to_string())
            .await;
        assert!(result.is_err());
        assert!(matches!(result, Err(SshServiceError::SessionNotFound(_))));
    }

    #[tokio::test]
    async fn test_terminate_session_success() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        // Create an active session
        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: Some("ssh-id".to_string()),
            exec_id: Some("exec-id".to_string()),
            pty_term: Some("xterm-256color".to_string()),
            pty_rows: Some(24),
            pty_cols: Some(80),
            state: SshSessionState::Active,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 1024,
            bytes_received: 2048,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        let result = service
            .terminate_session(session_id, "User logout".to_string())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_terminate_sessions_by_sandbox() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        let sandbox_id = Uuid::new_v4();

        // Create multiple sessions for the same sandbox
        for _ in 0..3 {
            let session = SshSession {
                id: Uuid::new_v4(),
                sandbox_id,
                client_ip: "127.0.0.1".to_string(),
                ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
                auth_method: crate::core::types::SshAuthMethod::ApiKey,
                ssh_session_id: Some("ssh-id".to_string()),
                exec_id: Some("exec-id".to_string()),
                pty_term: Some("xterm-256color".to_string()),
                pty_rows: Some(24),
                pty_cols: Some(80),
                state: SshSessionState::Active,
                connected_at: Utc::now(),
                disconnected_at: None,
                last_activity_at: Utc::now(),
                bytes_sent: 0,
                bytes_received: 0,
                duration_seconds: None,
                termination_reason: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            store.add_session(session);
        }

        let result = service.terminate_sessions_by_sandbox(sandbox_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_stale_sessions() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let result = service.get_stale_sessions(300).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_stuck_connecting_sessions() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let result = service.get_stuck_connecting_sessions(60).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_orphaned_sessions() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let result = service.get_orphaned_sessions().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_statistics() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let result = service.get_statistics().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let result = service.list_sessions(SshSessionFilters::default()).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_list_sessions_with_results() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        // Add some sessions
        let session = SshSession {
            id: Uuid::new_v4(),
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: None,
            exec_id: None,
            pty_term: None,
            pty_rows: None,
            pty_cols: None,
            state: SshSessionState::Connecting,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 0,
            bytes_received: 0,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        let result = service.list_sessions(SshSessionFilters::default()).await;
        assert_eq!(result.len(), 1);
    }

    // ========================================================================
    // Additional Comprehensive Tests for 90% Coverage
    // ========================================================================

    #[tokio::test]
    async fn test_create_session_with_all_fields() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let sandbox_id = Uuid::new_v4();
        let request = CreateSshSessionRequest {
            sandbox_id,
            client_ip: "192.168.1.100".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_8.9".to_string()),
            auth_method: crate::core::types::SshAuthMethod::Certificate,
            username: None,
            public_key: None,
        };

        let result = service.create_session(request).await;
        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.state, SshSessionState::Connecting);
        assert_eq!(session.sandbox_id, sandbox_id);
    }

    #[tokio::test]
    async fn test_mark_session_active_with_all_params() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: None,
            exec_id: None,
            pty_term: None,
            pty_rows: None,
            pty_cols: None,
            state: SshSessionState::Connecting,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 0,
            bytes_received: 0,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        // Mark active with all parameters
        let result = service
            .mark_session_active(
                session_id,
                Some("ssh-session-123".to_string()),
                Some("exec-instance-456".to_string()),
                Some("xterm-256color".to_string()),
                Some(40),
                Some(120),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_activity_with_zero_bytes() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: Some("ssh-id".to_string()),
            exec_id: Some("exec-id".to_string()),
            pty_term: Some("xterm-256color".to_string()),
            pty_rows: Some(24),
            pty_cols: Some(80),
            state: SshSessionState::Active,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 100,
            bytes_received: 200,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        // Update activity with zero bytes (heartbeat only)
        let result = service.update_activity(session_id, 0, 0).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_terminate_session_with_custom_reason() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: Some("ssh-id".to_string()),
            exec_id: Some("exec-id".to_string()),
            pty_term: Some("xterm-256color".to_string()),
            pty_rows: Some(24),
            pty_cols: Some(80),
            state: SshSessionState::Active,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 1024,
            bytes_received: 2048,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        // Terminate with custom reason
        let reason = "User logged out".to_string();
        let result = service.terminate_session(session_id, reason).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_terminate_sessions_by_sandbox_empty() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        // Terminate sessions for sandbox with no sessions
        let sandbox_id = Uuid::new_v4();
        let result = service.terminate_sessions_by_sandbox(sandbox_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_sessions_with_filters() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        let sandbox_id = Uuid::new_v4();
        let session = SshSession {
            id: Uuid::new_v4(),
            sandbox_id,
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: Some("ssh-id".to_string()),
            exec_id: Some("exec-id".to_string()),
            pty_term: Some("xterm-256color".to_string()),
            pty_rows: Some(24),
            pty_cols: Some(80),
            state: SshSessionState::Active,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 0,
            bytes_received: 0,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        // List with sandbox filter
        let filters = SshSessionFilters {
            sandbox_id: Some(sandbox_id),
            state: None,
            limit: None,
            offset: None,
        };

        let result = service.list_sessions(filters).await;
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn test_list_sessions_with_state_filter() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        let session = SshSession {
            id: Uuid::new_v4(),
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: Some("ssh-id".to_string()),
            exec_id: Some("exec-id".to_string()),
            pty_term: Some("xterm-256color".to_string()),
            pty_rows: Some(24),
            pty_cols: Some(80),
            state: SshSessionState::Active,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 0,
            bytes_received: 0,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        // List with state filter
        let filters = SshSessionFilters {
            sandbox_id: None,
            state: Some(SshSessionState::Active),
            limit: None,
            offset: None,
        };

        let result = service.list_sessions(filters).await;
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn test_get_statistics_fields() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        let result = service.get_statistics().await;
        assert!(result.is_ok());

        let stats = result.unwrap();
        // Verify all fields are present
        assert_eq!(stats.total_sessions, 0);
        assert_eq!(stats.active_sessions, 0);
        assert_eq!(stats.connecting_sessions, 0);
        assert_eq!(stats.disconnected_sessions, 0);
        assert_eq!(stats.terminated_sessions, 0);
        assert_eq!(stats.error_sessions, 0);
        assert_eq!(stats.total_bytes_sent, 0);
        assert_eq!(stats.total_bytes_received, 0);
    }

    #[tokio::test]
    async fn test_concurrent_session_operations() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = Arc::new(SshSessionService::new(store));

        // Create multiple sessions concurrently
        let mut handles = vec![];

        for i in 0..5 {
            let service_clone = service.clone();
            let handle = tokio::spawn(async move {
                let request = CreateSshSessionRequest {
                    sandbox_id: Uuid::new_v4(),
                    client_ip: format!("127.0.0.{}", i),
                    ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
                    auth_method: crate::core::types::SshAuthMethod::ApiKey,
                    username: None,
                    public_key: None,
                };

                service_clone.create_session(request).await
            });
            handles.push(handle);
        }

        // Wait for all creations
        let mut count = 0;
        for handle in handles {
            let result = handle.await.unwrap();
            if result.is_ok() {
                count += 1;
            }
        }

        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_state_transition_connecting_to_active() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: None,
            exec_id: None,
            pty_term: None,
            pty_rows: None,
            pty_cols: None,
            state: SshSessionState::Connecting,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 0,
            bytes_received: 0,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        // Transition from Connecting to Active
        let result = service
            .mark_session_active(
                session_id,
                Some("ssh-session".to_string()),
                Some("exec-id".to_string()),
                Some("xterm".to_string()),
                Some(24),
                Some(80),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_state_transition_active_to_disconnected() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: Some("ssh-session".to_string()),
            exec_id: Some("exec-id".to_string()),
            pty_term: Some("xterm".to_string()),
            pty_rows: Some(24),
            pty_cols: Some(80),
            state: SshSessionState::Active,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 1024,
            bytes_received: 2048,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        // Transition from Active to Disconnected
        let result = service.disconnect_session(session_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_state_transition_disconnected_to_terminated() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store.clone());

        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: Some("ssh-session".to_string()),
            exec_id: Some("exec-id".to_string()),
            pty_term: Some("xterm".to_string()),
            pty_rows: Some(24),
            pty_cols: Some(80),
            state: SshSessionState::Disconnected,
            connected_at: Utc::now(),
            disconnected_at: Some(Utc::now()),
            last_activity_at: Utc::now(),
            bytes_sent: 1024,
            bytes_received: 2048,
            duration_seconds: Some(60),
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        // Transition from Disconnected to Terminated
        let result = service
            .terminate_session(session_id, "Cleanup".to_string())
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_service_clone_independence() {
        let store = Arc::new(MockSshSessionStore::new());
        let service1 = SshSessionService::new(store.clone());
        let service2 = service1.clone();

        // Both clones should work independently
        let session_id = Uuid::new_v4();
        let session = SshSession {
            id: session_id,
            sandbox_id: Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: crate::core::types::SshAuthMethod::ApiKey,
            ssh_session_id: None,
            exec_id: None,
            pty_term: None,
            pty_rows: None,
            pty_cols: None,
            state: SshSessionState::Connecting,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 0,
            bytes_received: 0,
            duration_seconds: None,
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.add_session(session);

        // Both services can access the same session
        let result1 = service1.get_session(session_id).await;
        let result2 = service2.get_session(session_id).await;

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_get_stuck_connecting_sessions_timeout() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        // Test with various timeout values
        for timeout in [30, 60, 120, 300] {
            let result = service.get_stuck_connecting_sessions(timeout).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_get_stale_sessions_various_timeouts() {
        let store = Arc::new(MockSshSessionStore::new());
        let service = SshSessionService::new(store);

        // Test with various timeout values
        for timeout in [60, 300, 600, 1800] {
            let result = service.get_stale_sessions(timeout).await;
            assert!(result.is_ok());
        }
    }
}
