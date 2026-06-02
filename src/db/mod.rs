// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Database Module
//!
//! This module provides PostgreSQL database integration for the DSB project.
//!
//! ## Overview
//!
//! Replaces the in-memory state store with persistent PostgreSQL storage.
//!
//! ## Architecture
//!
//! ```text
//! PostgresStateStore
//!     |-> deadpool::Pool (Connection Pool Manager)
//!                      |
//!                      v
//!              PostgreSQL Database
//!                 - sandboxes table
//!                 - id (UUID PK)
//!                 - config (JSONB columns)
//!                 - state, timestamps
//!                 - activity tracking
//! ```
//!
//! ## Usage Example
//!
//! ```rust,no_run,ignore
//! use dsb::db::{PostgresStateStore, pool::create_pool_from_env, migration::run_migrations, store::StoreError};
//! use dsb::core::types::{Sandbox, SandboxConfig, SandboxState, ActivityTracking};
//! use chrono::Utc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! // Create connection pool
//! let pool = create_pool_from_env().await
//!     .map_err(|e| Box::new(StoreError::from(e)) as Box<dyn std::error::Error + Send + Sync>)?;
//!
//! // Run migrations
//! run_migrations(&pool).await?;
//!
//! // Create store
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
//! ## Modules
//!
//! - [`migration`](crate::db::migration) - Database schema and migrations
//! - [`pool`](crate::db::pool) - Connection pool management
//! - [`store`](crate::db::store) - PostgresStateStore implementation
//! - [`activities`](crate::db::activities) - Activity tracking database operations
//! - [`ssh_sessions`](crate::db::ssh_sessions) - SSH session database operations
//! - [`api_key_store`](crate::db::api_key_store) - API key management with bcrypt hashing
//! - [`session_token_store`](crate::db::session_token_store) - Session token storage for service authentication

pub mod activities;
pub mod api_key_store;
pub mod migration;
pub mod pool;
pub mod session_token_store;
pub mod ssh_sessions;
pub mod store;

#[cfg(test)]
pub mod test_db;

// Re-export the main types for convenience
pub use activities::ActivityStore;
pub use api_key_store::{
    ApiKey, ApiKeyResponse, ApiKeyStore, CreateApiKeyRequest, PostgresApiKeyStore,
};
pub use session_token_store::{PostgresSessionTokenStore, SessionTokenStore};
pub use ssh_sessions::{NoopSshSessionStore, PostgresSshSessionStore, SshSessionStoreTrait};
pub use store::{PostgresStateStore, StoreError};
