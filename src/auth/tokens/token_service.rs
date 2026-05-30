// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # VNC Token Service
//!
//! Business logic layer for VNC session token management.
//!
//! ## Responsibilities
//!
//! - Token generation (secure random tokens)
//! - Token validation (checking storage and sandbox state)
//! - Sandbox ownership validation
//! - Token lifecycle management

use crate::auth::tokens::token_store::{TokenStoreError, VncTokenStore};
use crate::auth::tokens::types::{
    CreateVncTokenRequest, TokenValidationResult, VncSessionToken,
};
use crate::core::sandbox::SandboxService;
use crate::core::types::{Sandbox, SandboxState};
use rand::Rng;
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, warn};

/// Errors that can occur during token service operations
#[derive(Error, Debug)]
pub enum TokenServiceError {
    #[error("Sandbox not found: {0}")]
    SandboxNotFound(String),

    #[error("Sandbox not running: {0}")]
    SandboxNotRunning(String),

    #[error("Token store error: {0}")]
    TokenStore(#[from] TokenStoreError),

    #[error("Invalid token format: {0}")]
    InvalidFormat(String),
}

impl axum::response::IntoResponse for TokenServiceError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            TokenServiceError::SandboxNotFound(msg) => (axum::http::StatusCode::NOT_FOUND, msg.as_str()),
            TokenServiceError::SandboxNotRunning(msg) => (axum::http::StatusCode::CONFLICT, msg.as_str()),
            TokenServiceError::TokenStore(err) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err.to_string().as_str()),
            TokenServiceError::InvalidFormat(msg) => (axum::http::StatusCode::BAD_REQUEST, msg.as_str()),
        };

        let body = axum::Json(serde_json::json!({
            "error": message,
            "type": std::any::type_name::<TokenServiceError>()
        }));

        (status, body).into_response()
    }
}

/// VNC token service business logic.
///
/// This service handles the complete token lifecycle:
/// - Generating secure random tokens
/// - Validating sandbox existence and state
/// - Storing tokens with TTL
/// - Validating tokens against storage and sandbox state
pub struct VncTokenService {
    store: Arc<dyn VncTokenStore>,
    sandbox_service: Arc<SandboxService>,
    default_ttl_secs: u64,
}

impl VncTokenService {
    /// Create a new VNC token service.
    ///
    /// # Arguments
    ///
    /// * `store` - Token storage backend (Redis, PostgreSQL, in-memory)
    /// * `sandbox_service` - Sandbox service for validation
    /// * `default_ttl_secs` - Default token TTL in seconds
    pub fn new(
        store: Arc<dyn VncTokenStore>,
        sandbox_service: Arc<SandboxService>,
        default_ttl_secs: u64,
    ) -> Self {
        Self {
            store,
            sandbox_service,
            default_ttl_secs,
        }
    }

    /// Generate a new VNC session token for a sandbox.
    ///
    /// # Arguments
    ///
    /// * `request` - Token creation request with sandbox_id and optional TTL
    /// * `api_key_id` - Optional API key ID that created this token (for audit)
    ///
    /// # Returns
    ///
    /// Returns `VncSessionToken` with the generated token and metadata.
    ///
    /// # Errors
    ///
    /// - `TokenServiceError::SandboxNotFound` - Sandbox doesn't exist
    /// - `TokenServiceError::SandboxNotRunning` - Sandbox exists but not running
    /// - `TokenServiceError::TokenStore` - Storage backend error
    pub async fn create_token(
        &self,
        request: CreateVncTokenRequest,
        api_key_id: Option<uuid::Uuid>,
    ) -> Result<VncSessionToken, TokenServiceError> {
        // 1. Validate sandbox exists
        let sandbox = self
            .sandbox_service
            .get_sandbox(&request.sandbox_id)
            .await
            .map_err(|e| TokenServiceError::SandboxNotFound(format!("{}", e)))?;

        // 2. Validate sandbox is running
        if sandbox.state != SandboxState::Running {
            return Err(TokenServiceError::SandboxNotRunning(format!(
                "Sandbox {} is not running (current state: {:?})",
                request.sandbox_id, sandbox.state
            )));
        }

        // 3. Generate secure random token (256-bit, hex-encoded)
        let token = Self::generate_secure_token();

        // 4. Calculate expiration
        let ttl_secs = if request.ttl_secs > 0 {
            request.ttl_secs
        } else {
            self.default_ttl_secs
        };

        let created_at = chrono::Utc::now();
        let expires_at = created_at + chrono::Duration::seconds(ttl_secs as i64);

        // 5. Create token record
        let session_token = VncSessionToken {
            token: token.clone(),
            sandbox_id: request.sandbox_id,
            user_id: None, // TODO: Extract from API key context when multi-user is added
            created_at,
            expires_at,
            ttl_seconds: ttl_secs,
        };

        // 6. Store token
        self.store
            .create_token(&session_token, api_key_id)
            .await?;

        info!(
            "Created VNC token for sandbox {} (expires in {}s, token_id: {})",
            request.sandbox_id,
            ttl_secs,
            &session_token.token[..session_token.token.len().min(12)]
        );

        Ok(session_token)
    }

    /// Validate a VNC session token.
    ///
    /// # Arguments
    ///
    /// * `token_str` - Token string to validate
    ///
    /// # Returns
    ///
    /// Returns `TokenValidationResult` indicating validity and sandbox_id if valid.
    ///
    /// # Errors
    ///
    /// - `TokenServiceError::TokenStore` - Storage backend error
    pub async fn validate_token(&self, token_str: &str) -> Result<TokenValidationResult, TokenServiceError> {
        // 1. Validate token format (hex string, reasonable length)
        if token_str.len() < 32 || !token_str.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(TokenValidationResult::Invalid);
        }

        // 2. Check storage
        let sandbox_id = match self.store.validate_token(token_str).await? {
            Some(id) => id,
            None => return Ok(TokenValidationResult::NotFound),
        };

        // 3. Verify sandbox still exists and is running
        match self.sandbox_service.get_sandbox(&sandbox_id).await {
            Ok(sandbox) => {
                if sandbox.state == SandboxState::Running {
                    Ok(TokenValidationResult::Valid { sandbox_id })
                } else {
                    warn!(
                        "VNC token validated but sandbox {} is not running (state: {:?})",
                        sandbox_id, sandbox.state
                    );
                    Ok(TokenValidationResult::Expired) // Treat as expired since sandbox not running
                }
            }
            Err(_) => {
                warn!("VNC token validated but sandbox {} no longer exists", sandbox_id);
                Ok(TokenValidationResult::NotFound) // Sandbox deleted
            }
        }
    }

    /// Revoke all tokens for a sandbox.
    ///
    /// Called when a sandbox is deleted or access should be revoked.
    ///
    /// # Arguments
    ///
    /// * `sandbox_id` - Sandbox ID to revoke tokens for
    pub async fn revoke_sandbox_tokens(&self, sandbox_id: &uuid::Uuid) -> Result<(), TokenServiceError> {
        self.store
            .revoke_sandbox_tokens(sandbox_id)
            .await?;
        info!("Revoked all VNC tokens for sandbox {}", sandbox_id);
        Ok(())
    }

    /// Cleanup expired tokens.
    ///
    /// Should be called periodically (e.g., every hour).
    ///
    /// # Returns
    ///
    /// Returns the number of tokens cleaned up.
    pub async fn cleanup_expired_tokens(&self) -> Result<u64, TokenServiceError> {
        let count = self.store.cleanup_expired_tokens().await?;
        if count > 0 {
            info!("Cleaned up {} expired VNC tokens", count);
        }
        Ok(count)
    }

    /// Generate a cryptographically secure random token.
    ///
    /// Token format: 64 hex characters (256 bits of randomness).
    fn generate_secure_token() -> String {
        let bytes: [u8; 32] = rand::thread_rng().gen(); // 256 bits
        hex::encode(bytes)
    }

    /// Start background cleanup task.
    ///
    /// Spawns a task that periodically cleans up expired tokens.
    ///
    /// # Arguments
    ///
    /// * `interval_secs` - Cleanup interval in seconds (default: 3600 = 1 hour)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::tokens::token_store::InMemoryVncTokenStore;
    use std::sync::Arc;

    #[test]
    fn test_generate_secure_token() {
        let token1 = VncTokenService::generate_secure_token();
        let token2 = VncTokenService::generate_secure_token();

        // Tokens should be different
        assert_ne!(token1, token2);

        // Token should be 64 hex chars (256 bits)
        assert_eq!(token1.len(), 64);
        assert!(token1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_secure_token_unique() {
        // Generate 1000 tokens and check they're all unique
        use std::collections::HashSet;
        let mut tokens = HashSet::new();

        for _ in 0..1000 {
            let token = VncTokenService::generate_secure_token();
            assert!(tokens.insert(token), "Generated duplicate token!");
        }
    }

    #[tokio::test]
    async fn test_token_validation_result() {
        let sandbox_id = Uuid::new_v4();

        let result_valid = TokenValidationResult::Valid {
            sandbox_id,
        };
        assert!(result_valid.is_valid());
        assert_eq!(result_valid.sandbox_id(), Some(&sandbox_id));

        let result_invalid = TokenValidationResult::Invalid;
        assert!(!result_invalid.is_valid());
        assert_eq!(result_invalid.sandbox_id(), None);

        let result_expired = TokenValidationResult::Expired;
        assert!(!result_expired.is_valid());
        assert_eq!(result_expired.sandbox_id(), None);

        let result_not_found = TokenValidationResult::NotFound;
        assert!(!result_not_found.is_valid());
        assert_eq!(result_not_found.sandbox_id(), None);
    }

    #[tokio::test]
    async fn test_create_token_request_default_ttl() {
        let request = CreateVncTokenRequest {
            sandbox_id: Uuid::new_v4(),
            ttl_secs: 0, // Should use default
        };

        assert_eq!(request.ttl_secs, 0); // serde default not applied in test
    }
}
