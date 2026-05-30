// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Session Token Module
//!
//! Short-lived tokens for authenticating users to internal services.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Session token for service authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToken {
    /// Random token string (UUID)
    pub token: String,
    /// Sandbox ID this token is authorized for
    pub sandbox_id: String,
    /// Service name (e.g., "vnc", "terminal")
    pub service: String,
    /// Token creation timestamp
    pub created_at: DateTime<Utc>,
    /// Token expiration timestamp
    pub expires_at: DateTime<Utc>,
}

impl SessionToken {
    /// Create a new session token with specified TTL
    pub fn new(sandbox_id: &str, service: &str, ttl_secs: i64) -> Self {
        let token = Self::generate_token();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(ttl_secs);

        Self {
            token,
            sandbox_id: sandbox_id.to_string(),
            service: service.to_string(),
            created_at: now,
            expires_at,
        }
    }

    /// Generate a cryptographically random token
    fn generate_token() -> String {
        Uuid::new_v4().to_string()
    }

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Validate token matches expected sandbox and service
    pub fn validate(&self, sandbox_id: &str, service: &str) -> bool {
        !self.is_expired() && self.sandbox_id == sandbox_id && self.service == service
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_token_creation() {
        let token = SessionToken::new("sandbox-123", "vnc", 300);
        assert_eq!(token.sandbox_id, "sandbox-123");
        assert_eq!(token.service, "vnc");
        assert!(!token.is_expired());
        assert!(token.validate("sandbox-123", "vnc"));
    }

    #[test]
    fn test_token_expiration() {
        let token = SessionToken::new("sandbox-123", "vnc", -1); // Already expired
        assert!(token.is_expired());
        assert!(!token.validate("sandbox-123", "vnc"));
    }

    #[test]
    fn test_token_validation_fails_on_wrong_sandbox() {
        let token = SessionToken::new("sandbox-123", "vnc", 300);
        assert!(!token.validate("wrong-sandbox", "vnc"));
    }

    #[test]
    fn test_token_validation_fails_on_wrong_service() {
        let token = SessionToken::new("sandbox-123", "vnc", 300);
        assert!(!token.validate("sandbox-123", "web"));
    }

    #[test]
    fn test_token_is_unique() {
        let token1 = SessionToken::new("sandbox-123", "vnc", 300);
        let token2 = SessionToken::new("sandbox-123", "vnc", 300);
        assert_ne!(token1.token, token2.token);
    }
}
