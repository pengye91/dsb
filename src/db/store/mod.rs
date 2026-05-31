// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # PostgreSQL State Store Module
//!
//! This module provides a persistent state store using PostgreSQL.
//!
//! ## Overview
//!
//! The [`PostgresStateStore`] replaces the in-memory [`StateStore`](crate::core::state::StateStore)
//! with persistent PostgreSQL storage, maintaining the same API for seamless migration.
//!
//! ## Benefits
//!
//! - **Persistence**: Data survives process restarts
//! - **Scalability**: Multiple processes can share state
//! - **Querying**: Can run complex queries on sandbox data
//! - **History**: Can add audit trails and state history
//!
//! ## Usage Example
//!
//! ```rust,no_run,ignore
//! use dsb::db::{PostgresStateStore, pool::create_pool_from_env, migration::run_migrations, StoreError};
//! use dsb::core::types::{Sandbox, SandboxConfig, SandboxState, ActivityTracking};
//! use chrono::Utc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! // Create connection pool
//! let pool = create_pool_from_env().await
//!     .map_err(|e| Box::new(StoreError::from(e)) as Box<dyn std::error::Error + Send + Sync>)?;
//!
//! // Run migrations to create tables
//! run_migrations(&pool).await?;
//!
//! // Create state store
//! let store = PostgresStateStore::new(pool).await?;
//!
//! // Use same API as StateStore
//! let config = SandboxConfig::default();
//! let now = Utc::now();
//! let sandbox = Sandbox {
//!     id: uuid::Uuid::new_v4(),
//!     config: config.clone(),
//!     state: SandboxState::Creating,
//!     container_id: None,
//!     created_at: now,
//!     updated_at: now,
//!     error_message: None,
//!     volume_mounts: vec![],
//!     activity: ActivityTracking {
//!         last_api_activity: now,
//!         last_container_activity: None,
//!         activity_count: 0,
//!     },
//!     inactivity_timeout_minutes: None,
//! };
//! store.create_sandbox(sandbox).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Performance
//!
//! - Connection pooling reduces overhead
//! - Indexes on frequently queried columns
//! - JSONB for flexible configuration storage
//! - Prepared statements for repeated queries

use crate::core::errors::ErrorCode;
use crate::core::types::{Sandbox, SandboxState};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error types for the PostgreSQL state store.
///
/// This enum provides structured error information with proper error chains,
/// making debugging and error handling much more effective than string-based errors.
#[derive(Error, Debug)]
pub enum StoreError {
    /// PostgreSQL database error
    ///
    /// Wraps tokio-postgres errors with full context and error chain.
    #[error("PostgreSQL error: {0}")]
    Postgres(#[from] tokio_postgres::Error),

    /// JSON serialization error
    ///
    /// Occurs when converting Rust types to JSON for database storage.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Sandbox not found error
    ///
    /// Returned when attempting to update or delete a sandbox that doesn't exist.
    #[error("Sandbox {0} not found")]
    NotFound(uuid::Uuid),

    /// Invalid state error
    ///
    /// Returned when an invalid state transition is attempted.
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// Generic error message
    ///
    /// Used for custom error messages that don't fit other categories.
    #[error("{0}")]
    Message(String),
}

impl StoreError {
    /// Get the error code for this error
    ///
    /// Returns the unified `ErrorCode` that corresponds to this error variant.
    /// This enables consistent error handling across Rust backend, Python SDK, and sandbox.
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::Postgres(_) => ErrorCode::DatabaseQueryFailed,
            Self::Serialization(_) => ErrorCode::InternalError,
            Self::NotFound(_) => ErrorCode::SandboxNotFound,
            Self::InvalidState(_) => ErrorCode::SandboxInvalidState,
            Self::Message(_) => ErrorCode::InternalError,
        }
    }
}

impl From<String> for StoreError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

/// Implement From for pool errors to enable automatic conversion
impl From<deadpool_postgres::PoolError> for StoreError {
    fn from(err: deadpool_postgres::PoolError) -> Self {
        Self::Message(err.to_string())
    }
}

/// Filters for listing sandboxes with various criteria.
#[derive(Debug, Clone, Default)]
pub struct SandboxListFilters {
    /// Include deleted sandboxes (default: false)
    pub include_deleted: bool,
    /// Filter by state (optional)
    pub state: Option<SandboxState>,
    /// Filter by image name (partial match, optional)
    pub image: Option<String>,
    /// Filter by creation date range - after this timestamp (optional)
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter by creation date range - before this timestamp (optional)
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
    /// Pagination - page number (default: 1)
    pub page: Option<usize>,
    /// Pagination - items per page (default: 50, max: 200)
    pub per_page: Option<usize>,
}

/// Pagination metadata for list responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationMeta {
    /// Current page number
    pub page: usize,
    /// Items per page
    pub per_page: usize,
    /// Total number of items
    pub total: usize,
    /// Total number of pages
    pub total_pages: usize,
    /// Whether there's a next page
    pub has_next: bool,
    /// Whether there's a previous page
    pub has_prev: bool,
}

/// Paginated list response for sandboxes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxListResponse {
    /// Array of sandboxes
    pub data: Vec<Sandbox>,
    /// Pagination metadata
    pub pagination: PaginationMeta,
}
/// PostgreSQL-based persistent state store for managing sandbox state.
///
/// This provides the same API as [`StateStore`](crate::core::state::StateStore)
/// but persists data to PostgreSQL instead of memory.
///
/// # Example
///
/// See the module-level documentation for a complete usage example.
#[derive(Clone)]
pub struct PostgresStateStore {
    pool: Pool,
}

/// Serialized JSONB fields for sandbox storage.
///
/// This struct holds all the JSON-serialized fields needed for database operations,
/// providing a clean way to pass serialized data between functions.
struct SerializedSandboxFields {
    environment: serde_json::Value,
    port_mappings: serde_json::Value,
    resource_limits: serde_json::Value,
    volumes: serde_json::Value,
    volume_mounts: serde_json::Value,
    command: serde_json::Value,
    features: serde_json::Value,
}

mod helpers;
mod sandbox;
mod sandbox_ops;
#[cfg(test)]
#[cfg(test)]
mod tests;
mod trait_impl;
