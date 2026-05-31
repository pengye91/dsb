// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Session Token Store Module
//!
//! This module provides database-backed session token storage.
//!
//! ## Architecture
//!
//! ```text
//! SessionTokenStore (trait)
//!     |
//!     v
//! PostgresSessionTokenStore (implementation)
//!     |
//!     v
//! PostgreSQL Database (session_tokens table)
//!         - token (UUID)
//!         - sandbox_id
//!         - service
//!         - created_at
//!         - expires_at
//! ```
//!
//! ## Usage Example
//!
//! ```rust,no_run,ignore
//! use dsb::db::{PostgresSessionTokenStore, SessionTokenStore};
//! use dsb::session_token::SessionToken;
//! use deadpool_postgres::Pool;
//!
//! # async fn example(pool: Pool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! // Create store
//! let store = PostgresSessionTokenStore::new(pool);
//!
//! // Create session token
//! let token = SessionToken::new("sandbox-123", "openclaw", 300);
//! store.create_session_token(&token).await?;
//!
//! // Validate session token
//! let retrieved = store.get_session_token(&token.token).await?;
//! assert!(retrieved.is_some());
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use deadpool_postgres::Pool;

/// Database error type for session token operations
pub type DbError = Box<dyn std::error::Error + Send + Sync>;

/// Trait for session token storage operations
///
/// This trait defines the interface for session token management.
/// Implementations can use different storage backends (PostgreSQL, Redis, etc.).
#[async_trait]
pub trait SessionTokenStore: Send + Sync {
    /// Create a new session token
    ///
    /// # Arguments
    ///
    /// * `token` - The session token to store
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Token created successfully
    /// * `Err(...)` - Database error
    async fn create_session_token(
        &self,
        token: &crate::session_token::SessionToken,
    ) -> Result<(), DbError>;

    /// Get a session token by token string
    ///
    /// # Arguments
    ///
    /// * `token` - The token string to lookup
    ///
    /// # Returns
    ///
    /// * `Ok(Some(SessionToken))` - Token found
    /// * `Ok(None)` - Token not found
    /// * `Err(...)` - Database error
    async fn get_session_token(
        &self,
        token: &str,
    ) -> Result<Option<crate::session_token::SessionToken>, DbError>;

    /// Delete expired session tokens
    ///
    /// # Returns
    ///
    /// * `Ok(count)` - Number of tokens deleted
    /// * `Err(...)` - Database error
    async fn delete_expired_tokens(&self) -> Result<u64, DbError>;
}

/// PostgreSQL implementation of SessionTokenStore
#[derive(Clone)]
pub struct PostgresSessionTokenStore {
    pool: Pool,
}

impl PostgresSessionTokenStore {
    /// Create a new PostgresSessionTokenStore
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionTokenStore for PostgresSessionTokenStore {
    async fn create_session_token(
        &self,
        token: &crate::session_token::SessionToken,
    ) -> Result<(), DbError> {
        let client = self.pool.get().await.map_err(|e| Box::new(e) as DbError)?;

        const QUERY: &str = r#"
            INSERT INTO session_tokens (
                token, sandbox_id, service, created_at, expires_at
            ) VALUES ($1, $2, $3, $4, $5)
        "#;

        client
            .execute(
                QUERY,
                &[
                    &token.token,
                    &token.sandbox_id,
                    &token.service,
                    &token.created_at,
                    &token.expires_at,
                ],
            )
            .await
            .map_err(|e| Box::new(e) as DbError)?;

        tracing::debug!(
            "Created session token: sandbox={}, service={}",
            token.sandbox_id,
            token.service
        );

        Ok(())
    }

    async fn get_session_token(
        &self,
        token: &str,
    ) -> Result<Option<crate::session_token::SessionToken>, DbError> {
        let client = self.pool.get().await.map_err(|e| Box::new(e) as DbError)?;

        const QUERY: &str = r#"
            SELECT token, sandbox_id, service, created_at, expires_at
            FROM session_tokens
            WHERE token = $1
        "#;

        let row = match client
            .query_opt(QUERY, &[&token])
            .await
            .map_err(|e| Box::new(e) as DbError)?
        {
            Some(r) => r,
            None => return Ok(None),
        };

        Ok(Some(crate::session_token::SessionToken {
            token: row.get("token"),
            sandbox_id: row.get("sandbox_id"),
            service: row.get("service"),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
        }))
    }

    async fn delete_expired_tokens(&self) -> Result<u64, DbError> {
        let client = self.pool.get().await.map_err(|e| Box::new(e) as DbError)?;

        // Use UTC timezone to ensure consistent comparison with timestamps
        const QUERY: &str =
            "DELETE FROM session_tokens WHERE expires_at < (NOW() AT TIME ZONE 'UTC')";

        let rows_affected = client
            .execute(QUERY, &[])
            .await
            .map_err(|e| Box::new(e) as DbError)?;

        if rows_affected > 0 {
            tracing::info!("Deleted {} expired session tokens", rows_affected);
        }

        Ok(rows_affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpool_postgres::{Config, Pool, Runtime};
    use tokio_postgres::NoTls;

    /// Creates a test database pool
    async fn create_test_pool() -> Pool {
        // Load test config to get database credentials
        let config = crate::config::load_for_tests().expect("Failed to load test config");

        let mut pg_config = Config::new();
        let mut host = config.database.host;
        let mut port = config.database.port;
        let name = config.database.name;
        let user = config.database.user;
        let password = config.database.password.unwrap_or_default();

        // Check if running inside Docker
        if std::env::var("INSIDE_DOCKER").is_ok() || std::path::Path::new("/.dockerenv").exists() {
            host = "postgres-test".to_string();
            port = 5432;
        }

        pg_config.host = Some(host);
        pg_config.port = Some(port);
        pg_config.dbname = Some(name);
        pg_config.user = Some(user);
        pg_config.password = Some(password);

        pg_config
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .expect("Failed to create pool")
    }

    #[test]
    fn test_postgres_session_token_store_creation() {
        // This is a compile-time test to ensure the type exists
        fn assert_clone<T: Clone>() {}
        assert_clone::<PostgresSessionTokenStore>();
    }

    #[tokio::test]
    async fn test_create_session_token_success() {
        let pool = create_test_pool().await;
        let store = PostgresSessionTokenStore::new(pool.clone());

        let token = crate::session_token::SessionToken::new(
            &format!("test-sandbox-{}", uuid::Uuid::new_v4()),
            "openclaw",
            300,
        );

        let result = store.create_session_token(&token).await;
        assert!(result.is_ok(), "Should successfully create session token");
    }

    #[tokio::test]
    async fn test_get_session_token_success() {
        let pool = create_test_pool().await;
        let store = PostgresSessionTokenStore::new(pool.clone());

        // Create a token first
        let token = crate::session_token::SessionToken::new(
            &format!("test-sandbox-{}", uuid::Uuid::new_v4()),
            "openclaw",
            300,
        );
        store
            .create_session_token(&token)
            .await
            .expect("Failed to create token");

        // Retrieve the token
        let result = store.get_session_token(&token.token).await;
        assert!(result.is_ok(), "Should successfully get session token");

        let retrieved = result.unwrap();
        assert!(retrieved.is_some(), "Token should exist");
        let retrieved_token = retrieved.unwrap();
        assert_eq!(retrieved_token.token, token.token);
        assert_eq!(retrieved_token.sandbox_id, token.sandbox_id);
        assert_eq!(retrieved_token.service, token.service);
    }

    #[tokio::test]
    async fn test_get_session_token_not_found() {
        let pool = create_test_pool().await;
        let store = PostgresSessionTokenStore::new(pool);

        // Try to get a non-existent token
        let fake_token = "00000000-0000-0000-0000-000000000000";
        let result = store.get_session_token(fake_token).await;
        assert!(result.is_ok(), "Should not error on missing token");

        let retrieved = result.unwrap();
        assert!(
            retrieved.is_none(),
            "Should return None for non-existent token"
        );
    }

    #[tokio::test]
    async fn test_delete_expired_tokens_no_expired() {
        let pool = create_test_pool().await;
        let store = PostgresSessionTokenStore::new(pool.clone());

        // Create a token that won't expire soon
        let token = crate::session_token::SessionToken::new(
            &format!("test-sandbox-{}", uuid::Uuid::new_v4()),
            "openclaw",
            3600,
        );
        store
            .create_session_token(&token)
            .await
            .expect("Failed to create token");

        // Delete expired tokens
        let result = store.delete_expired_tokens().await;
        assert!(result.is_ok(), "Should successfully delete expired tokens");

        // Token should still exist (it's not expired)
        let retrieved = store.get_session_token(&token.token).await.unwrap();
        assert!(
            retrieved.is_some(),
            "Token should still exist after cleanup"
        );
    }

    #[tokio::test]
    async fn test_delete_expired_tokens_with_expired() {
        let pool = create_test_pool().await;
        let store = PostgresSessionTokenStore::new(pool.clone());

        // Create an expired token by setting expires_at in the past
        let sandbox_id = format!("test-sandbox-expired-{}", uuid::Uuid::new_v4());
        let token_str = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let created_at = now - chrono::Duration::hours(25); // 25 hours ago
        let expires_at = now - chrono::Duration::hours(24); // 24 hours ago (well expired)

        let client = pool.get().await.expect("Failed to get client");
        client
            .execute(
                "INSERT INTO session_tokens (token, sandbox_id, service, created_at, expires_at)
                 VALUES ($1, $2, $3, $4, $5)",
                &[
                    &token_str,
                    &sandbox_id.as_str(),
                    &"openclaw",
                    &created_at,
                    &expires_at,
                ],
            )
            .await
            .expect("Failed to insert expired token");

        // Create a non-expired token with unique sandbox ID
        let valid_sandbox_id = format!("test-sandbox-valid-{}", uuid::Uuid::new_v4());
        let valid_token =
            crate::session_token::SessionToken::new(&valid_sandbox_id, "openclaw", 3600);
        store
            .create_session_token(&valid_token)
            .await
            .expect("Failed to create valid token");

        // Delete expired tokens
        let result = store.delete_expired_tokens().await;
        assert!(result.is_ok(), "Should successfully delete expired tokens");

        // Note: We don't assert on the count because parallel tests may also
        // The important assertions are on the token state below.

        // Expired token should be gone (deleted by delete_expired_tokens or parallel cleanup)
        let retrieved = store.get_session_token(&token_str).await.unwrap();
        assert!(retrieved.is_none(), "Expired token should be deleted");

        // Valid token should still exist
        let retrieved = store.get_session_token(&valid_token.token).await.unwrap();
        assert!(retrieved.is_some(), "Valid token should still exist");
    }

    #[tokio::test]
    async fn test_create_multiple_tokens_same_sandbox() {
        let pool = create_test_pool().await;
        let store = PostgresSessionTokenStore::new(pool.clone());

        // Use UUID-based sandbox ID for better test isolation
        let sandbox_id = format!("test-sandbox-multi-{}", uuid::Uuid::new_v4());

        // Create multiple tokens for the same sandbox
        let token1 = crate::session_token::SessionToken::new(&sandbox_id, "openclaw", 300);
        let token2 = crate::session_token::SessionToken::new(&sandbox_id, "vnc", 300);

        store
            .create_session_token(&token1)
            .await
            .expect("Failed to create token1");
        store
            .create_session_token(&token2)
            .await
            .expect("Failed to create token2");

        // Both tokens should exist
        let retrieved1 = store.get_session_token(&token1.token).await.unwrap();
        assert!(retrieved1.is_some(), "Token1 should exist");

        let retrieved2 = store.get_session_token(&token2.token).await.unwrap();
        assert!(retrieved2.is_some(), "Token2 should exist");
    }

    #[tokio::test]
    async fn test_concurrent_token_operations() {
        let pool = create_test_pool().await;

        // Create multiple tokens concurrently
        let mut handles = vec![];

        for _ in 0..5 {
            let store_clone = PostgresSessionTokenStore::new(pool.clone());
            let sandbox_id = format!("test-sandbox-concurrent-{}", uuid::Uuid::new_v4());

            let handle = tokio::spawn(async move {
                let token = crate::session_token::SessionToken::new(&sandbox_id, "openclaw", 300);
                store_clone.create_session_token(&token).await
            });

            handles.push(handle);
        }

        // All operations should succeed
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok(), "Concurrent token creation should succeed");
        }
    }
}
