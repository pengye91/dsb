// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # VNC Token Types
//!
//! Data structures for VNC session token authentication.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// VNC session token with metadata
///
/// Generated when a user requests VNC access to a sandbox.
/// The token is short-lived and bound to a specific sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VncSessionToken {
    /// Cryptographically secure random token (hex-encoded)
    pub token: String,

    /// Sandbox ID this token grants access to
    pub sandbox_id: Uuid,

    /// Optional user identifier (for future multi-user support)
    pub user_id: Option<String>,

    /// Token creation timestamp
    pub created_at: DateTime<Utc>,

    /// Token expiration timestamp
    pub expires_at: DateTime<Utc>,

    /// Original TTL in seconds (for audit/display)
    pub ttl_seconds: u64,
}

impl VncSessionToken {
    /// Check if token is currently valid (not expired)
    pub fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at
    }

    /// Get remaining time until expiration
    pub fn remaining_secs(&self) -> i64 {
        (self.expires_at - Utc::now()).num_seconds().max(0)
    }
}

/// Request to generate a VNC token
#[derive(Debug, Deserialize)]
pub struct CreateVncTokenRequest {
    /// Sandbox ID to generate token for
    pub sandbox_id: Uuid,

    /// Token time-to-live in seconds (optional, defaults to config value)
    #[serde(default = "default_ttl")]
    pub ttl_secs: u64,
}

fn default_ttl() -> u64 {
    3600 // 1 hour default
}

/// Token validation result
#[derive(Debug, Clone, PartialEq)]
pub enum TokenValidationResult {
    /// Token is valid and grants access to the specified sandbox
    Valid { sandbox_id: Uuid },

    /// Token is invalid (wrong format, signature, etc.)
    Invalid,

    /// Token was valid but has expired
    Expired,

    /// Token not found in storage
    NotFound,
}

impl TokenValidationResult {
    /// Check if result indicates a valid token
    pub fn is_valid(&self) -> bool {
        matches!(self, TokenValidationResult::Valid { .. })
    }

    /// Get sandbox ID if token is valid
    pub fn sandbox_id(&self) -> Option<&Uuid> {
        match self {
            TokenValidationResult::Valid { sandbox_id } => Some(sandbox_id),
            _ => None,
        }
    }
}

/// Token validation query parameters
#[derive(Debug, Deserialize)]
pub struct ValidateTokenQuery {
    pub token: String,
}

/// Token validation response
#[derive(Debug, Serialize)]
pub struct ValidateTokenResponse {
    pub valid: bool,
    pub sandbox_id: Option<Uuid>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vnc_session_token_is_valid() {
        let token = VncSessionToken {
            token: "test".to_string(),
            sandbox_id: Uuid::new_v4(),
            user_id: None,
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            ttl_seconds: 3600,
        };

        assert!(token.is_valid());
        assert!(token.remaining_secs() > 0);
    }

    #[test]
    fn test_vnc_session_token_expired() {
        let token = VncSessionToken {
            token: "test".to_string(),
            sandbox_id: Uuid::new_v4(),
            user_id: None,
            created_at: Utc::now() - chrono::Duration::hours(2),
            expires_at: Utc::now() - chrono::Duration::hours(1),
            ttl_seconds: 3600,
        };

        assert!(!token.is_valid());
        assert_eq!(token.remaining_secs(), 0);
    }

    #[test]
    fn test_token_validation_result_valid() {
        let sandbox_id = Uuid::new_v4();
        let result = TokenValidationResult::Valid {
            sandbox_id,
        };

        assert!(result.is_valid());
        assert_eq!(result.sandbox_id(), Some(&sandbox_id));
    }

    #[test]
    fn test_token_validation_result_invalid() {
        let result = TokenValidationResult::Invalid;

        assert!(!result.is_valid());
        assert_eq!(result.sandbox_id(), None);
    }
}
