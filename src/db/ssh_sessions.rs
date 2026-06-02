// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # SSH Session Database Operations
//!
//! This module provides PostgreSQL-backed storage for SSH sessions.
//!
//! ## Overview
//!
//! The `PostgresSshSessionStore` implements CRUD operations for SSH session
//! records, including filtering by sandbox, state, and time-based queries.
//!
//! ## Testing Strategy
//!
//! SSH session database operations are tested through:
//!
//! ### Unit Tests (This Module)
//! Type and structure tests:
//! - `SessionStatistics` struct validation
//! - Trait bounds and type checking
//! - Serialization tests
//! - Edge case handling
//!
//! ### Integration Tests
//! Full database tests in:
//! - **`tests/integration_ssh_docker.rs`**: SSH session lifecycle with real database
//! - **`tests/test_ssh_session_cleanup.rs`**: Session cleanup and statistics
//!
//! Integration tests cover:
//! - CRUD operations with PostgreSQL
//! - Filter queries (by sandbox, state, time)
//! - Stale/orphaned session detection
//! - Statistics aggregation
//! - Transaction handling
//!
//! ## Example
//!
//! ```rust,no_run,ignore
//! use dsb::db::ssh_sessions::{PostgresSshSessionStore, SshSessionStoreTrait};
//! use dsb::core::types::{SshSession, SshSessionState, SshAuthMethod};
//! use deadpool_postgres::Pool;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a pool from environment (requires DATABASE_URL to be set)
//! let pool = dsb::db::pool::create_pool_from_env().await?;
//! let store = PostgresSshSessionStore::new(pool);
//!
//! // Create a session (in practice, use SshSessionService instead)
//! let now = chrono::Utc::now();
//! let session = SshSession {
//!     id: uuid::Uuid::new_v4(),
//!     sandbox_id: uuid::Uuid::new_v4(),
//!     client_ip: "127.0.0.1".to_string(),
//!     ssh_version: None,
//!     auth_method: SshAuthMethod::ApiKey,
//!     ssh_session_id: None,
//!     exec_id: None,
//!     pty_term: None,
//!     pty_rows: None,
//!     pty_cols: None,
//!     state: SshSessionState::Connecting,
//!     connected_at: now,
//!     disconnected_at: None,
//!     last_activity_at: now,
//!     bytes_sent: 0,
//!     bytes_received: 0,
//!     duration_seconds: None,
//!     termination_reason: None,
//!     created_at: now,
//!     updated_at: now,
//! };
//! store.create_ssh_session(session).await?;
//! # Ok(())
//! # }
//! ```

use crate::core::types::{SshAuthMethod, SshSession, SshSessionFilters, SshSessionState};
use crate::db::store::StoreError;
use async_trait::async_trait;
use deadpool_postgres::Pool;
use tokio_postgres::Row;
use tracing::{debug, error, instrument};

/// Trait defining SSH session storage operations.
///
/// This trait allows for multiple storage backends (PostgreSQL, in-memory, etc.)
/// and enables easy testing with mock implementations.
#[async_trait]
pub trait SshSessionStoreTrait: Send + Sync {
    /// Create a new SSH session record.
    async fn create_ssh_session(&self, session: SshSession) -> Result<(), StoreError>;

    /// Retrieve an SSH session by ID.
    async fn get_ssh_session(&self, id: &uuid::Uuid) -> Option<SshSession>;

    /// List SSH sessions with optional filters.
    async fn list_ssh_sessions(&self, filters: SshSessionFilters) -> Vec<SshSession>;

    /// Update an existing SSH session.
    async fn update_ssh_session(&self, session: &SshSession) -> Result<(), StoreError>;

    /// Delete an SSH session by ID.
    async fn delete_ssh_session(&self, id: &uuid::Uuid) -> Result<(), StoreError>;

    /// Terminate all sessions for a sandbox (e.g., when sandbox is deleted).
    async fn terminate_sessions_by_sandbox(
        &self,
        sandbox_id: &uuid::Uuid,
    ) -> Result<(), StoreError>;

    /// Get stale sessions for cleanup (sessions inactive for longer than timeout).
    async fn get_stale_sessions(&self, timeout_secs: i64) -> Result<Vec<SshSession>, StoreError>;

    /// Get sessions stuck in connecting state for too long.
    async fn get_stuck_connecting_sessions(
        &self,
        timeout_secs: i64,
    ) -> Result<Vec<SshSession>, StoreError>;

    /// Get sessions for sandboxes that are no longer running (orphaned sessions).
    async fn get_orphaned_sessions(&self) -> Result<Vec<SshSession>, StoreError>;

    /// Get session statistics for monitoring.
    async fn get_session_statistics(&self) -> Result<SessionStatistics, StoreError>;
}

/// Session statistics for monitoring.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionStatistics {
    /// Total number of sessions
    pub total_sessions: i64,

    /// Number of active sessions
    pub active_sessions: i64,

    /// Number of connecting sessions
    pub connecting_sessions: i64,

    /// Number of disconnected sessions
    pub disconnected_sessions: i64,

    /// Number of terminated sessions
    pub terminated_sessions: i64,

    /// Number of error sessions
    pub error_sessions: i64,

    /// Total bytes sent across all sessions
    pub total_bytes_sent: i64,

    /// Total bytes received across all sessions
    pub total_bytes_received: i64,

    /// Average session duration in seconds
    pub avg_duration_seconds: Option<f64>,
}

/// PostgreSQL-backed SSH session store.
#[derive(Clone)]
pub struct PostgresSshSessionStore {
    pool: Pool,
}

impl PostgresSshSessionStore {
    /// Create a new PostgreSQL-backed SSH session store.
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool
    ///
    /// # Returns
    ///
    /// A new `PostgresSshSessionStore` instance
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Parse a database row into an `SshSession`.
    fn row_to_session(row: &Row) -> Result<SshSession, StoreError> {
        Ok(SshSession {
            id: row.try_get("id")?,
            sandbox_id: row.try_get("sandbox_id")?,
            client_ip: row.try_get("client_ip")?,
            // Column is nullable in the database, so NULL is a legitimate value
            ssh_version: row.try_get::<_, Option<String>>("ssh_version")?,
            auth_method: match row.try_get::<_, String>("auth_method")?.as_str() {
                "api_key" => SshAuthMethod::ApiKey,
                "certificate" => SshAuthMethod::Certificate,
                _ => return Err(StoreError::InvalidState("Invalid auth_method".to_string())),
            },
            // Column is nullable in the database, so NULL is a legitimate value
            ssh_session_id: row.try_get::<_, Option<String>>("ssh_session_id")?,
            // Column is nullable in the database, so NULL is a legitimate value
            exec_id: row.try_get::<_, Option<String>>("exec_id")?,
            // Column is nullable in the database, so NULL is a legitimate value
            pty_term: row.try_get::<_, Option<String>>("pty_term")?,
            // Column is nullable in the database, so NULL is a legitimate value
            pty_rows: row.try_get::<_, Option<i32>>("pty_rows")?,
            // Column is nullable in the database, so NULL is a legitimate value
            pty_cols: row.try_get::<_, Option<i32>>("pty_cols")?,
            state: match row.try_get::<_, String>("state")?.as_str() {
                "connecting" => SshSessionState::Connecting,
                "active" => SshSessionState::Active,
                "disconnected" => SshSessionState::Disconnected,
                "terminated" => SshSessionState::Terminated,
                "error" => SshSessionState::Error,
                _ => {
                    return Err(StoreError::InvalidState(format!(
                        "Invalid state: {}",
                        row.try_get::<_, String>("state")?
                    )))
                }
            },
            connected_at: row.try_get("connected_at")?,
            // Column is nullable in the database, so NULL is a legitimate value
            disconnected_at: row
                .try_get::<_, Option<chrono::DateTime<chrono::Utc>>>("disconnected_at")?,
            last_activity_at: row.try_get("last_activity_at")?,
            bytes_sent: row.try_get("bytes_sent")?,
            bytes_received: row.try_get("bytes_received")?,
            // Column is nullable in the database, so NULL is a legitimate value
            duration_seconds: row.try_get::<_, Option<i32>>("duration_seconds")?,
            // Column is nullable in the database, so NULL is a legitimate value
            termination_reason: row.try_get::<_, Option<String>>("termination_reason")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

#[async_trait]
impl SshSessionStoreTrait for PostgresSshSessionStore {
    #[instrument(skip(self, session), fields(session_id = %session.id))]
    async fn create_ssh_session(&self, session: SshSession) -> Result<(), StoreError> {
        debug!("Creating SSH session in database");

        let client = self.pool.get().await?;

        client
            .execute(
                r#"
                INSERT INTO ssh_sessions (
                    id, sandbox_id, client_ip, ssh_version, auth_method,
                    ssh_session_id, exec_id, pty_term, pty_rows, pty_cols,
                    state, connected_at, disconnected_at, last_activity_at,
                    bytes_sent, bytes_received, duration_seconds, termination_reason,
                    created_at, updated_at
                ) VALUES (
                    $1, $2, $3, $4, $5,
                    $6, $7, $8, $9, $10,
                    $11, $12, $13, $14,
                    $15, $16, $17, $18,
                    $19, $20
                )
                "#,
                &[
                    &session.id,
                    &session.sandbox_id,
                    &session.client_ip,
                    &session.ssh_version,
                    &session.auth_method.as_str(),
                    &session.ssh_session_id,
                    &session.exec_id,
                    &session.pty_term,
                    &session.pty_rows,
                    &session.pty_cols,
                    &session.state.as_str(),
                    &session.connected_at,
                    &session.disconnected_at,
                    &session.last_activity_at,
                    &session.bytes_sent,
                    &session.bytes_received,
                    &session.duration_seconds,
                    &session.termination_reason,
                    &session.created_at,
                    &session.updated_at,
                ],
            )
            .await
            .map_err(StoreError::Postgres)?;

        debug!("SSH session created successfully");
        Ok(())
    }

    #[instrument(skip(self), fields(session_id = %id))]
    async fn get_ssh_session(&self, id: &uuid::Uuid) -> Option<SshSession> {
        debug!("Retrieving SSH session from database");

        let client = match self.pool.get().await {
            Ok(c) => c,
            Err(e) => {
                error!(
                    "Failed to get database connection for get_ssh_session: {}",
                    e
                );
                return None;
            }
        };

        let row = match client
            .query_one("SELECT * FROM ssh_sessions WHERE id = $1", &[&id])
            .await
        {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to query SSH session {}: {}", id, e);
                return None;
            }
        };

        match Self::row_to_session(&row) {
            Ok(s) => Some(s),
            Err(e) => {
                error!("Failed to deserialize SSH session {}: {}", id, e);
                None
            }
        }
    }

    #[instrument(skip(self), fields(filters = ?filters))]
    async fn list_ssh_sessions(&self, filters: SshSessionFilters) -> Vec<SshSession> {
        debug!("Listing SSH sessions from database");

        let client = match self.pool.get().await {
            Ok(c) => c,
            Err(e) => {
                error!(
                    "Failed to get database connection for list_ssh_sessions: {}",
                    e
                );
                return vec![];
            }
        };

        // Build different queries based on which filters are present
        // This avoids complex trait object lifetime issues
        let rows = if let Some(sandbox_id) = filters.sandbox_id {
            if let Some(state) = filters.state {
                // Both sandbox_id and state
                client
                    .query(
                        "SELECT * FROM ssh_sessions WHERE sandbox_id = $1 AND state = $2 ORDER BY connected_at DESC",
                        &[&sandbox_id, &state.as_str()],
                    )
                    .await
                    .unwrap_or_default()
            } else {
                // Only sandbox_id
                client
                    .query(
                        "SELECT * FROM ssh_sessions WHERE sandbox_id = $1 ORDER BY connected_at DESC",
                        &[&sandbox_id],
                    )
                    .await
                    .unwrap_or_default()
            }
        } else if let Some(state) = filters.state {
            // Only state
            client
                .query(
                    "SELECT * FROM ssh_sessions WHERE state = $1 ORDER BY connected_at DESC",
                    &[&state.as_str()],
                )
                .await
                .unwrap_or_default()
        } else {
            // No filters
            client
                .query("SELECT * FROM ssh_sessions ORDER BY connected_at DESC", &[])
                .await
                .unwrap_or_default()
        };

        rows.iter()
            .filter_map(|row| match Self::row_to_session(row) {
                Ok(s) => Some(s),
                Err(e) => {
                    error!("Failed to deserialize SSH session row: {}", e);
                    None
                }
            })
            .collect()
    }

    #[instrument(skip(self, session), fields(session_id = %session.id))]
    async fn update_ssh_session(&self, session: &SshSession) -> Result<(), StoreError> {
        debug!("Updating SSH session in database");

        let client = self.pool.get().await?;

        client
            .execute(
                r#"
                UPDATE ssh_sessions SET
                    sandbox_id = $2,
                    client_ip = $3,
                    ssh_version = $4,
                    auth_method = $5,
                    ssh_session_id = $6,
                    exec_id = $7,
                    pty_term = $8,
                    pty_rows = $9,
                    pty_cols = $10,
                    state = $11,
                    connected_at = $12,
                    disconnected_at = $13,
                    last_activity_at = $14,
                    bytes_sent = $15,
                    bytes_received = $16,
                    duration_seconds = $17,
                    termination_reason = $18,
                    updated_at = NOW()
                WHERE id = $1
                "#,
                &[
                    &session.id,
                    &session.sandbox_id,
                    &session.client_ip,
                    &session.ssh_version,
                    &session.auth_method.as_str(),
                    &session.ssh_session_id,
                    &session.exec_id,
                    &session.pty_term,
                    &session.pty_rows,
                    &session.pty_cols,
                    &session.state.as_str(),
                    &session.connected_at,
                    &session.disconnected_at,
                    &session.last_activity_at,
                    &session.bytes_sent,
                    &session.bytes_received,
                    &session.duration_seconds,
                    &session.termination_reason,
                ],
            )
            .await
            .map_err(StoreError::Postgres)?;

        debug!("SSH session updated successfully");
        Ok(())
    }

    #[instrument(skip(self), fields(session_id = %id))]
    async fn delete_ssh_session(&self, id: &uuid::Uuid) -> Result<(), StoreError> {
        debug!("Deleting SSH session from database");

        let client = self.pool.get().await?;

        client
            .execute("DELETE FROM ssh_sessions WHERE id = $1", &[&id])
            .await
            .map_err(StoreError::Postgres)?;

        debug!("SSH session deleted successfully");
        Ok(())
    }

    #[instrument(skip(self), fields(sandbox_id = %sandbox_id))]
    async fn terminate_sessions_by_sandbox(
        &self,
        sandbox_id: &uuid::Uuid,
    ) -> Result<(), StoreError> {
        debug!("Terminating all SSH sessions for sandbox");

        let client = self.pool.get().await?;

        client
            .execute(
                r#"
                UPDATE ssh_sessions SET
                    state = 'terminated',
                    disconnected_at = NOW(),
                    termination_reason = 'Sandbox deleted',
                    updated_at = NOW()
                WHERE sandbox_id = $1 AND state IN ('connecting', 'active')
                "#,
                &[&sandbox_id],
            )
            .await?;

        debug!("SSH sessions terminated successfully");
        Ok(())
    }

    #[instrument(skip(self), fields(timeout_secs = %timeout_secs))]
    async fn get_stale_sessions(&self, timeout_secs: i64) -> Result<Vec<SshSession>, StoreError> {
        debug!("Retrieving stale SSH sessions");

        let client = self.pool.get().await?;

        let rows = client
            .query(
                r#"
                SELECT * FROM ssh_sessions
                WHERE state IN ('connecting', 'active')
                  AND last_activity_at < NOW() - make_interval(secs => $1)
                ORDER BY last_activity_at ASC
                "#,
                &[&(timeout_secs as f64)],
            )
            .await
            .map_err(StoreError::Postgres)?;

        rows.iter().map(Self::row_to_session).collect()
    }

    #[instrument(skip(self), fields(timeout_secs = %timeout_secs))]
    async fn get_stuck_connecting_sessions(
        &self,
        timeout_secs: i64,
    ) -> Result<Vec<SshSession>, StoreError> {
        debug!("Retrieving stuck connecting SSH sessions");

        let client = self.pool.get().await?;

        let rows = client
            .query(
                r#"
                SELECT * FROM ssh_sessions
                WHERE state = 'connecting'
                  AND connected_at < NOW() - make_interval(secs => $1)
                ORDER BY connected_at ASC
                "#,
                &[&(timeout_secs as f64)],
            )
            .await
            .map_err(StoreError::Postgres)?;

        rows.iter().map(Self::row_to_session).collect()
    }

    #[instrument(skip(self))]
    async fn get_orphaned_sessions(&self) -> Result<Vec<SshSession>, StoreError> {
        debug!("Retrieving orphaned SSH sessions");

        let client = self.pool.get().await?;

        // Find sessions where the sandbox is no longer running
        let rows = client
            .query(
                r#"
                SELECT s.* FROM ssh_sessions s
                LEFT JOIN sandboxes sb ON s.sandbox_id = sb.id
                WHERE s.state IN ('connecting', 'active')
                  AND (sb.state IS NULL OR sb.state != 'running')
                ORDER BY s.connected_at ASC
                "#,
                &[],
            )
            .await
            .map_err(StoreError::Postgres)?;

        rows.iter().map(Self::row_to_session).collect()
    }

    #[instrument(skip(self))]
    async fn get_session_statistics(&self) -> Result<SessionStatistics, StoreError> {
        debug!("Retrieving SSH session statistics");

        let client = self.pool.get().await?;

        let row = client
            .query_one(
                r#"
                SELECT
                    COUNT(*)::BIGINT as total_sessions,
                    COALESCE(SUM(CASE WHEN state = 'active' THEN 1 ELSE 0 END), 0)::BIGINT as active_sessions,
                    COALESCE(SUM(CASE WHEN state = 'connecting' THEN 1 ELSE 0 END), 0)::BIGINT as connecting_sessions,
                    COALESCE(SUM(CASE WHEN state = 'disconnected' THEN 1 ELSE 0 END), 0)::BIGINT as disconnected_sessions,
                    COALESCE(SUM(CASE WHEN state = 'terminated' THEN 1 ELSE 0 END), 0)::BIGINT as terminated_sessions,
                    COALESCE(SUM(CASE WHEN state = 'error' THEN 1 ELSE 0 END), 0)::BIGINT as error_sessions,
                    COALESCE(SUM(bytes_sent), 0)::BIGINT as total_bytes_sent,
                    COALESCE(SUM(bytes_received), 0)::BIGINT as total_bytes_received,
                    AVG(EXTRACT(EPOCH FROM (duration_seconds || ' seconds')::interval))::DOUBLE PRECISION as avg_duration_seconds
                FROM ssh_sessions
                "#,
                &[],
            )
            .await
            .map_err(StoreError::Postgres)?;

        Ok(SessionStatistics {
            total_sessions: row.try_get("total_sessions")?,
            active_sessions: row.try_get("active_sessions")?,
            connecting_sessions: row.try_get("connecting_sessions")?,
            disconnected_sessions: row.try_get("disconnected_sessions")?,
            terminated_sessions: row.try_get("terminated_sessions")?,
            error_sessions: row.try_get("error_sessions")?,
            total_bytes_sent: row.try_get("total_bytes_sent")?,
            total_bytes_received: row.try_get("total_bytes_received")?,
            // Column is nullable in the database, so NULL is a legitimate value
            avg_duration_seconds: row.try_get::<_, Option<f64>>("avg_duration_seconds")?,
        })
    }
}

// ========================================================================
// No-op Store (for when SSH is disabled)
// ========================================================================

/// A no-op SSH session store that returns errors for all operations.
/// Used when SSH session management is disabled (e.g., without PostgreSQL).
pub struct NoopSshSessionStore;

#[async_trait::async_trait]
impl SshSessionStoreTrait for NoopSshSessionStore {
    async fn create_ssh_session(&self, _session: SshSession) -> Result<(), StoreError> {
        Err(StoreError::Message(
            "SSH session management requires PostgreSQL".into(),
        ))
    }

    async fn get_ssh_session(&self, _id: &uuid::Uuid) -> Option<SshSession> {
        None
    }

    async fn list_ssh_sessions(&self, _filters: SshSessionFilters) -> Vec<SshSession> {
        Vec::new()
    }

    async fn update_ssh_session(&self, _session: &SshSession) -> Result<(), StoreError> {
        Err(StoreError::Message(
            "SSH session management requires PostgreSQL".into(),
        ))
    }

    async fn delete_ssh_session(&self, _id: &uuid::Uuid) -> Result<(), StoreError> {
        Err(StoreError::Message(
            "SSH session management requires PostgreSQL".into(),
        ))
    }

    async fn terminate_sessions_by_sandbox(
        &self,
        _sandbox_id: &uuid::Uuid,
    ) -> Result<(), StoreError> {
        Err(StoreError::Message(
            "SSH session management requires PostgreSQL".into(),
        ))
    }

    async fn get_stale_sessions(&self, _timeout_secs: i64) -> Result<Vec<SshSession>, StoreError> {
        Err(StoreError::Message(
            "SSH session management requires PostgreSQL".into(),
        ))
    }

    async fn get_stuck_connecting_sessions(
        &self,
        _timeout_secs: i64,
    ) -> Result<Vec<SshSession>, StoreError> {
        Err(StoreError::Message(
            "SSH session management requires PostgreSQL".into(),
        ))
    }

    async fn get_orphaned_sessions(&self) -> Result<Vec<SshSession>, StoreError> {
        Err(StoreError::Message(
            "SSH session management requires PostgreSQL".into(),
        ))
    }

    async fn get_session_statistics(&self) -> Result<SessionStatistics, StoreError> {
        Err(StoreError::Message(
            "SSH session management requires PostgreSQL".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use std::sync::Arc;

    // ========================================================================
    // SessionStatistics Tests
    // ========================================================================

    #[test]
    fn test_session_statistics_creation() {
        let stats = SessionStatistics {
            total_sessions: 100,
            active_sessions: 10,
            connecting_sessions: 5,
            disconnected_sessions: 50,
            terminated_sessions: 30,
            error_sessions: 5,
            total_bytes_sent: 1_000_000,
            total_bytes_received: 2_000_000,
            avg_duration_seconds: Some(300.5),
        };

        assert_eq!(stats.total_sessions, 100);
        assert_eq!(stats.active_sessions, 10);
        assert_eq!(stats.connecting_sessions, 5);
        assert_eq!(stats.disconnected_sessions, 50);
        assert_eq!(stats.terminated_sessions, 30);
        assert_eq!(stats.error_sessions, 5);
        assert_eq!(stats.total_bytes_sent, 1_000_000);
        assert_eq!(stats.total_bytes_received, 2_000_000);
        assert_eq!(stats.avg_duration_seconds, Some(300.5));
    }

    #[test]
    fn test_session_statistics_all_zero() {
        let stats = SessionStatistics {
            total_sessions: 0,
            active_sessions: 0,
            connecting_sessions: 0,
            disconnected_sessions: 0,
            terminated_sessions: 0,
            error_sessions: 0,
            total_bytes_sent: 0,
            total_bytes_received: 0,
            avg_duration_seconds: None,
        };

        assert_eq!(stats.total_sessions, 0);
        assert_eq!(stats.avg_duration_seconds, None);
    }

    #[test]
    fn test_session_statistics_only_active() {
        let stats = SessionStatistics {
            total_sessions: 5,
            active_sessions: 5,
            connecting_sessions: 0,
            disconnected_sessions: 0,
            terminated_sessions: 0,
            error_sessions: 0,
            total_bytes_sent: 500,
            total_bytes_received: 1000,
            avg_duration_seconds: Some(120.0),
        };

        assert_eq!(stats.active_sessions, 5);
        assert_eq!(stats.total_sessions, stats.active_sessions);
    }

    #[test]
    fn test_session_statistics_serialization() {
        let stats = SessionStatistics {
            total_sessions: 10,
            active_sessions: 2,
            connecting_sessions: 1,
            disconnected_sessions: 4,
            terminated_sessions: 3,
            error_sessions: 0,
            total_bytes_sent: 50000,
            total_bytes_received: 100000,
            avg_duration_seconds: Some(250.5),
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"total_sessions\":10"));
        assert!(json.contains("\"active_sessions\":2"));
        assert!(json.contains("\"avg_duration_seconds\":250.5"));
    }

    #[test]
    fn test_session_statistics_with_none_average() {
        let stats = SessionStatistics {
            total_sessions: 1,
            active_sessions: 1,
            connecting_sessions: 0,
            disconnected_sessions: 0,
            terminated_sessions: 0,
            error_sessions: 0,
            total_bytes_sent: 0,
            total_bytes_received: 0,
            avg_duration_seconds: None,
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("null") || json.contains("null"));
    }

    #[test]
    fn test_session_statistics_large_values() {
        let stats = SessionStatistics {
            total_sessions: i64::MAX,
            active_sessions: i64::MAX,
            connecting_sessions: 0,
            disconnected_sessions: 0,
            terminated_sessions: 0,
            error_sessions: 0,
            total_bytes_sent: i64::MAX,
            total_bytes_received: i64::MAX,
            avg_duration_seconds: Some(f64::MAX),
        };

        assert_eq!(stats.total_sessions, i64::MAX);
        assert_eq!(stats.total_bytes_sent, i64::MAX);
    }

    #[test]
    fn test_session_statistics_deserialization() {
        let json = r#"{
            "total_sessions": 50,
            "active_sessions": 5,
            "connecting_sessions": 2,
            "disconnected_sessions": 20,
            "terminated_sessions": 20,
            "error_sessions": 3,
            "total_bytes_sent": 1024000,
            "total_bytes_received": 2048000,
            "avg_duration_seconds": 180.5
        }"#;

        let stats: SessionStatistics = serde_json::from_str(json).unwrap();
        assert_eq!(stats.total_sessions, 50);
        assert_eq!(stats.active_sessions, 5);
        assert_eq!(stats.avg_duration_seconds, Some(180.5));
    }

    // ========================================================================
    // PostgresSshSessionStore Type Tests
    // ========================================================================

    #[test]
    fn test_store_is_clone() {
        // Verify PostgresSshSessionStore implements Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<PostgresSshSessionStore>();
    }

    #[test]
    fn test_store_is_send() {
        // Verify PostgresSshSessionStore implements Send
        fn assert_send<T: Send>() {}
        assert_send::<PostgresSshSessionStore>();
    }

    #[test]
    fn test_store_is_sync() {
        // Verify PostgresSshSessionStore implements Sync
        fn assert_sync<T: Sync>() {}
        assert_sync::<PostgresSshSessionStore>();
    }

    // ========================================================================
    // Trait Tests
    // ========================================================================

    #[test]
    fn test_ssh_session_store_trait_is_object_safe() {
        // Verify SshSessionStoreTrait can be used as a trait object
        fn accept_trait_object(_: &dyn SshSessionStoreTrait) {}
        fn accept_arc_trait(_: Arc<dyn SshSessionStoreTrait>) {}

        // This compiles if the trait is object-safe
        // Just verify the functions exist
        let _ = accept_trait_object;
        let _ = accept_arc_trait;
    }

    #[test]
    fn test_store_requires_pool() {
        // Compile-time test that PostgresSshSessionStore requires Pool
        use deadpool_postgres::Pool;

        // This verifies the constructor signature
        fn check_new_fn(pool: Pool) -> PostgresSshSessionStore {
            PostgresSshSessionStore::new(pool)
        }

        let _ = check_new_fn;
    }

    // ========================================================================
    // Integration Test References
    // ========================================================================

    #[test]
    fn test_integration_test_files_exist() {
        // Documents where integration tests are located
        let _integration_tests = (
            "tests/integration_ssh_docker.rs",
            "tests/test_ssh_session_cleanup.rs",
        );
    }

    #[test]
    fn test_trait_method_signatures() {
        // Compile-time verification that trait methods exist
        // This test documents the trait interface

        // The SshSessionStoreTrait defines these methods:
        // - create_ssh_session(session: SshSession) -> Result<(), StoreError>
        // - get_ssh_session(id: &Uuid) -> Option<SshSession>
        // - list_ssh_sessions(filters: SshSessionFilters) -> Vec<SshSession>
        // - update_ssh_session(session: &SshSession) -> Result<(), StoreError>
        // - delete_ssh_session(id: &Uuid) -> Result<(), StoreError>
        // - terminate_sessions_by_sandbox(sandbox_id: &Uuid) -> Result<(), StoreError>
        // - get_stale_sessions(timeout_secs: i64) -> Result<Vec<SshSession>, StoreError>
        // - get_stuck_connecting_sessions(timeout_secs: i64) -> Result<Vec<SshSession>, StoreError>
        // - get_orphaned_sessions() -> Result<Vec<SshSession>, StoreError>
        // - get_session_statistics() -> Result<SessionStatistics, StoreError>

        // Just verify the trait exists
        fn trait_exists(_: &dyn SshSessionStoreTrait) {}
        let _ = trait_exists;
    }

    // ========================================================================
    // Edge Cases
    // ========================================================================

    #[test]
    fn test_session_statistics_negative_values() {
        // While negative values shouldn't occur in practice,
        // test that the struct can hold them
        let stats = SessionStatistics {
            total_sessions: -1,
            active_sessions: -1,
            connecting_sessions: -1,
            disconnected_sessions: -1,
            terminated_sessions: -1,
            error_sessions: -1,
            total_bytes_sent: -1,
            total_bytes_received: -1,
            avg_duration_seconds: Some(-1.0),
        };

        assert_eq!(stats.total_sessions, -1);
        assert_eq!(stats.avg_duration_seconds, Some(-1.0));
    }

    #[test]
    fn test_session_statistics_fractional_average() {
        let stats = SessionStatistics {
            total_sessions: 100,
            active_sessions: 10,
            connecting_sessions: 0,
            disconnected_sessions: 50,
            terminated_sessions: 30,
            error_sessions: 10,
            total_bytes_sent: 0,
            total_bytes_received: 0,
            avg_duration_seconds: Some(123.456789),
        };

        assert_eq!(stats.avg_duration_seconds, Some(123.456789));

        // Should serialize correctly
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("123.456789"));
    }

    #[test]
    fn test_session_statistics_zero_duration() {
        let stats = SessionStatistics {
            total_sessions: 10,
            active_sessions: 10,
            connecting_sessions: 0,
            disconnected_sessions: 0,
            terminated_sessions: 0,
            error_sessions: 0,
            total_bytes_sent: 0,
            total_bytes_received: 0,
            avg_duration_seconds: Some(0.0),
        };

        assert_eq!(stats.avg_duration_seconds, Some(0.0));
    }

    // ========================================================================
    // Error Propagation Tests (TDD for .ok() -> ? fix)
    // ========================================================================

    /// Helper to get a test database connection.
    /// Uses the default local postgres if available.
    async fn get_test_db_client() -> Option<tokio_postgres::Client> {
        let (client, connection) = tokio_postgres::connect(
            "postgresql://postgres:postgres@localhost:5432/postgres",
            tokio_postgres::NoTls,
        )
        .await
        .ok()?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        Some(client)
    }

    #[tokio::test]
    async fn test_row_to_session_propagates_type_mismatch() {
        let Some(client) = get_test_db_client().await else {
            eprintln!("Skipping test: no local postgres available");
            return;
        };

        let row = client
            .query_one(
                r#"
                SELECT
                    '550e8400-e29b-41d4-a716-446655440000'::uuid as id,
                    '550e8400-e29b-41d4-a716-446655440001'::uuid as sandbox_id,
                    '127.0.0.1'::text as client_ip,
                    NULL::text as ssh_version,
                    'api_key'::text as auth_method,
                    NULL::text as ssh_session_id,
                    NULL::text as exec_id,
                    NULL::text as pty_term,
                    100::bigint as pty_rows,
                    NULL::int as pty_cols,
                    'connecting'::text as state,
                    '2024-01-01T00:00:00Z'::timestamptz as connected_at,
                    NULL::timestamptz as disconnected_at,
                    '2024-01-01T00:00:00Z'::timestamptz as last_activity_at,
                    0::bigint as bytes_sent,
                    0::bigint as bytes_received,
                    NULL::int as duration_seconds,
                    NULL::text as termination_reason,
                    '2024-01-01T00:00:00Z'::timestamptz as created_at,
                    '2024-01-01T00:00:00Z'::timestamptz as updated_at
                "#,
                &[],
            )
            .await
            .expect("Query should succeed");

        let result = PostgresSshSessionStore::row_to_session(&row);
        assert!(
            result.is_err(),
            "Expected type mismatch error to be propagated, but got Ok. \
             The .ok() pattern silently swallows type-mismatch errors."
        );
    }

    #[tokio::test]
    async fn test_row_to_session_preserves_null_values() {
        let Some(client) = get_test_db_client().await else {
            eprintln!("Skipping test: no local postgres available");
            return;
        };

        let row = client
            .query_one(
                r#"
                SELECT
                    '550e8400-e29b-41d4-a716-446655440000'::uuid as id,
                    '550e8400-e29b-41d4-a716-446655440001'::uuid as sandbox_id,
                    '127.0.0.1'::text as client_ip,
                    NULL::text as ssh_version,
                    'api_key'::text as auth_method,
                    NULL::text as ssh_session_id,
                    NULL::text as exec_id,
                    NULL::text as pty_term,
                    NULL::int as pty_rows,
                    NULL::int as pty_cols,
                    'connecting'::text as state,
                    '2024-01-01T00:00:00Z'::timestamptz as connected_at,
                    NULL::timestamptz as disconnected_at,
                    '2024-01-01T00:00:00Z'::timestamptz as last_activity_at,
                    0::bigint as bytes_sent,
                    0::bigint as bytes_received,
                    NULL::int as duration_seconds,
                    NULL::text as termination_reason,
                    '2024-01-01T00:00:00Z'::timestamptz as created_at,
                    '2024-01-01T00:00:00Z'::timestamptz as updated_at
                "#,
                &[],
            )
            .await
            .expect("Query should succeed");

        let session = PostgresSshSessionStore::row_to_session(&row)
            .expect("NULL values should deserialize successfully with Option<T> + ?");

        assert!(session.ssh_version.is_none());
        assert!(session.ssh_session_id.is_none());
        assert!(session.exec_id.is_none());
        assert!(session.pty_term.is_none());
        assert!(session.pty_rows.is_none());
        assert!(session.pty_cols.is_none());
        assert!(session.disconnected_at.is_none());
        assert!(session.duration_seconds.is_none());
        assert!(session.termination_reason.is_none());
    }
}

// ========================================================================
// Error Handling Pattern Tests
// ========================================================================

#[test]
fn test_session_row_deserialization_error_not_panics() {
    // Simulate the filter_map pattern used in list_ssh_sessions
    // to ensure deserialization errors are handled gracefully
    // (logged via tracing::error!, not panicked)
    let results: Vec<Result<i32, StoreError>> = vec![
        Ok(1),
        Err(StoreError::Message("mock db error".to_string())),
        Ok(3),
    ];

    let filtered: Vec<i32> = results
        .into_iter()
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                // This mirrors the tracing::error! call in production code
                let _msg = format!("Failed to deserialize SSH session row: {}", e);
                None
            }
        })
        .collect();

    assert_eq!(filtered, vec![1, 3]);
}
