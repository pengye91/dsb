// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # VNC Token Store
//!
//! Storage abstraction for VNC session tokens.
//!
//! ## Architecture
//!
//! ```text
//! VncTokenStore (trait)
//!    │
//!    ├─► InMemoryVncTokenStore (testing/development)
//!    │    └─► Fast in-memory storage (lost on restart)
//!    │
//!    └─► PostgresVncTokenStore (production)
//!         └─► Token storage in PostgreSQL
//!         └─► SHA-256 hashed tokens for security
//! ```

use crate::auth::tokens::types::VncSessionToken;
use crate::db::store::StoreError;
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Errors that can occur during token store operations
#[derive(Error, Debug)]
pub enum TokenStoreError {
    #[error("Database error: {0}")]
    Database(#[from] StoreError),

    #[error("Token not found: {0}")]
    TokenNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Invalid token format: {0}")]
    InvalidFormat(String),
}

/// Trait defining VNC token storage operations.
///
/// This trait allows for multiple storage backends (PostgreSQL, in-memory)
/// and enables easy testing with mock implementations.
#[async_trait]
pub trait VncTokenStore: Send + Sync {
    /// Store a new token with TTL.
    ///
    /// # Arguments
    ///
    /// * `token` - Token to store
    /// * `api_key_id` - Optional API key ID that created this token (for audit)
    async fn create_token(
        &self,
        token: &VncSessionToken,
        api_key_id: Option<Uuid>,
    ) -> Result<(), TokenStoreError>;

    /// Validate token and return sandbox_id if valid.
    ///
    /// Returns `None` if token is not found or expired.
    async fn validate_token(&self, token: &str) -> Result<Option<Uuid>, TokenStoreError>;

    /// Revoke all tokens for a sandbox.
    ///
    /// Called when a sandbox is deleted or access should be revoked.
    async fn revoke_sandbox_tokens(&self, sandbox_id: &Uuid) -> Result<(), TokenStoreError>;

    /// Cleanup expired tokens.
    ///
    /// Returns the number of tokens cleaned up.
    /// For PostgreSQL, this deletes expired records.
    async fn cleanup_expired_tokens(&self) -> Result<u64, TokenStoreError>;
}

/// Store type constants for configuration
pub const POSTGRES_VNC_TOKEN_STORE: &str = "postgres";

/// In-memory token store for testing and development.
///
/// WARNING: Tokens are lost on restart. Only use for testing!
#[derive(Debug, Clone)]
pub struct InMemoryVncTokenStore {
    tokens: Arc<RwLock<std::collections::HashMap<String, VncSessionToken>>>,
}

impl InMemoryVncTokenStore {
    /// Create a new in-memory token store.
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Start background cleanup task.
    ///
    /// Spawns a task that periodically removes expired tokens.
    pub fn cleanup_task(self: Arc<Self>, interval_secs: u64) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                if let Err(e) = self.cleanup_expired_tokens().await {
                    tracing::warn!("Failed to cleanup expired VNC tokens: {}", e);
                }
            }
        });
    }
}

impl Default for InMemoryVncTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VncTokenStore for InMemoryVncTokenStore {
    async fn create_token(
        &self,
        token: &VncSessionToken,
        _api_key_id: Option<Uuid>,
    ) -> Result<(), TokenStoreError> {
        let mut tokens = self.tokens.write().await;
        tokens.insert(token.token.clone(), token.clone());
        tracing::debug!(
            "Stored VNC token for sandbox {} (expires in {}s)",
            token.sandbox_id,
            token.ttl_seconds
        );
        Ok(())
    }

    async fn validate_token(&self, token_str: &str) -> Result<Option<Uuid>, TokenStoreError> {
        let tokens = self.tokens.read().await;
        if let Some(token) = tokens.get(token_str) {
            if token.is_valid() {
                Ok(Some(token.sandbox_id))
            } else {
                Ok(None) // Expired
            }
        } else {
            Ok(None) // Not found
        }
    }

    async fn revoke_sandbox_tokens(&self, sandbox_id: &Uuid) -> Result<(), TokenStoreError> {
        let mut tokens = self.tokens.write().await;
        tokens.retain(|_, token| token.sandbox_id != *sandbox_id);
        tracing::debug!("Revoked all VNC tokens for sandbox {}", sandbox_id);
        Ok(())
    }

    async fn cleanup_expired_tokens(&self) -> Result<u64, TokenStoreError> {
        let mut tokens = self.tokens.write().await;
        let before = tokens.len();
        tokens.retain(|_, token| token.is_valid());
        let after = tokens.len();
        let cleaned = (before - after) as u64;
        if cleaned > 0 {
            tracing::debug!("Cleaned up {} expired VNC tokens", cleaned);
        }
        Ok(cleaned)
    }
}

/// PostgreSQL-backed token store with audit logging.
///
/// This implementation stores tokens in PostgreSQL with SHA-256 hashing.
/// It provides persistence and an audit trail.
#[derive(Debug, Clone)]
pub struct PostgresVncTokenStore {
    pool: Pool,
}

impl PostgresVncTokenStore {
    /// Create a new PostgreSQL token store.
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Hash token for storage (never store plaintext).
    fn hash_token(token: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[async_trait]
impl VncTokenStore for PostgresVncTokenStore {
    async fn create_token(
        &self,
        token: &VncSessionToken,
        api_key_id: Option<Uuid>,
    ) -> Result<(), TokenStoreError> {
        let token_hash = Self::hash_token(&token.token);
        let client = self.pool.get().await.map_err(StoreError::from)?;

        client
            .execute(
                "INSERT INTO vnc_tokens (token_hash, sandbox_id, api_key_id, expires_at, ttl_seconds)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (token_hash) DO UPDATE SET
                     expires_at = EXCLUDED.expires_at,
                     ttl_seconds = EXCLUDED.ttl_seconds",
                &[
                    &token_hash,
                    &token.sandbox_id,
                    &api_key_id,
                    &token.expires_at,
                    &(token.ttl_seconds as i64),
                ],
            )
            .await
            .map_err(StoreError::from)?;

        tracing::debug!(
            "Stored VNC token in PostgreSQL for sandbox {} (expires at {})",
            token.sandbox_id,
            token.expires_at
        );
        Ok(())
    }

    async fn validate_token(&self, token_str: &str) -> Result<Option<Uuid>, TokenStoreError> {
        let token_hash = Self::hash_token(token_str);
        let client = self.pool.get().await.map_err(StoreError::from)?;

        let row = client
            .query_opt(
                "SELECT sandbox_id FROM vnc_tokens
                 WHERE token_hash = $1 AND expires_at > NOW()
                 LIMIT 1",
                &[&token_hash],
            )
            .await
            .map_err(StoreError::from)?;

        if let Some(row) = row {
            let sandbox_id: Uuid = row.get("sandbox_id");

            // Update usage stats
            let _ = client
                .execute(
                    "UPDATE vnc_tokens SET last_used_at = NOW(), usage_count = usage_count + 1
                     WHERE token_hash = $1",
                    &[&token_hash],
                )
                .await;

            tracing::debug!("Validated VNC token for sandbox {}", sandbox_id);
            Ok(Some(sandbox_id))
        } else {
            tracing::debug!("VNC token not found or expired");
            Ok(None)
        }
    }

    async fn revoke_sandbox_tokens(&self, sandbox_id: &Uuid) -> Result<(), TokenStoreError> {
        let client = self.pool.get().await.map_err(StoreError::from)?;

        let result = client
            .execute(
                "DELETE FROM vnc_tokens WHERE sandbox_id = $1",
                &[sandbox_id],
            )
            .await
            .map_err(StoreError::from)?;

        tracing::debug!(
            "Revoked {} VNC tokens for sandbox {}",
            result,
            sandbox_id
        );
        Ok(())
    }

    async fn cleanup_expired_tokens(&self) -> Result<u64, TokenStoreError> {
        let client = self.pool.get().await.map_err(StoreError::from)?;

        let result = client
            .execute("DELETE FROM vnc_tokens WHERE expires_at < NOW()", &[])
            .await
            .map_err(StoreError::from)?;

        if result > 0 {
            tracing::debug!("Cleaned up {} expired VNC tokens from PostgreSQL", result);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_in_memory_token_store() {
        let store = InMemoryVncTokenStore::new();

        let token = VncSessionToken {
            token: "test-token-123".to_string(),
            sandbox_id: Uuid::new_v4(),
            user_id: None,
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            ttl_seconds: 3600,
        };

        // Create token
        store
            .create_token(&token, None)
            .await
            .unwrap();

        // Validate token
        let result = store.validate_token("test-token-123").await.unwrap();
        assert_eq!(result, Some(token.sandbox_id));

        // Cleanup expired
        let cleaned = store.cleanup_expired_tokens().await.unwrap();
        assert_eq!(cleaned, 0); // No expired tokens
    }

    #[tokio::test]
    async fn test_in_memory_token_expired() {
        let store = InMemoryVncTokenStore::new();

        let token = VncSessionToken {
            token: "expired-token".to_string(),
            sandbox_id: Uuid::new_v4(),
            user_id: None,
            created_at: Utc::now() - chrono::Duration::hours(2),
            expires_at: Utc::now() - chrono::Duration::hours(1),
            ttl_seconds: 3600,
        };

        store
            .create_token(&token, None)
            .await
            .unwrap();

        // Validate expired token
        let result = store.validate_token("expired-token").await.unwrap();
        assert_eq!(result, None); // Expired

        // Cleanup removes expired
        let cleaned = store.cleanup_expired_tokens().await.unwrap();
        assert_eq!(cleaned, 1);
    }

    #[test]
    fn test_hash_token() {
        let token1 = "test-token";
        let token2 = "test-token";
        let token3 = "different-token";

        let hash1 = PostgresVncTokenStore::hash_token(token1);
        let hash2 = PostgresVncTokenStore::hash_token(token2);
        let hash3 = PostgresVncTokenStore::hash_token(token3);

        // Same input produces same hash
        assert_eq!(hash1, hash2);

        // Different input produces different hash
        assert_ne!(hash1, hash3);

        // Hash is hex-encoded SHA256 (64 chars)
        assert_eq!(hash1.len(), 64);
    }
}
