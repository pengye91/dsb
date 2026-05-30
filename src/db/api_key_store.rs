// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # API Key Store Module
//!
//! This module provides database-backed API key management with bcrypt hashing.
//!
//! ## Architecture
//!
//! ```text
//! ApiKeyStore (trait)
//!     |
//!     v
//! PostgresApiKeyStore (implementation)
//!     |
//!     v
//! PostgreSQL Database (api_keys table)
//!         - key_hash (bcrypt)
//!         - key_prefix (first 8 chars)
//!         - metadata (name, description, scopes, etc.)
//! ```
//!
//! ## Security
//!
//! - API keys are hashed with bcrypt before storage
//! - Only the key prefix (first 8 characters) is stored in plaintext for identification
//! - Full API key is only returned once on creation
//!
//! ## Usage Example
//!
//! ```rust,no_run,ignore
//! use dsb::db::{PostgresApiKeyStore, CreateApiKeyRequest};
//! use deadpool_postgres::Pool;
//!
//! # async fn example(pool: Pool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! // Create store
//! let store = PostgresApiKeyStore::new(pool);
//!
//! // Create API key
//! let req = CreateApiKeyRequest {
//!     name: "CLI Key".to_string(),
//!     description: Some("For CLI access".to_string()),
//!     scopes: Some(vec!["sandbox:read".to_string(), "sandbox:write".to_string()]),
//!     expires_in_days: Some(365),
//!     created_by: Some("admin".to_string()),
//! };
//!
//! let response = store.create_api_key(req).await?;
//! println!("API Key: {} (save this, you won't see it again!)", response.api_key);
//!
//! // Validate API key
//! let api_key_id = store.validate_api_key(&response.api_key).await?;
//! assert!(api_key_id.is_some());
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};

/// API key record with metadata
///
/// This struct represents an API key stored in the database.
/// The `key_hash` field contains the bcrypt hash, while `key_prefix`
/// (first 8 characters) is stored in plaintext for identification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Primary key
    pub id: uuid::Uuid,

    /// Bcrypt hash of the API key (never exposed in API responses)
    #[serde(skip_serializing)]
    pub key_hash: String,

    /// First 8 characters of the key (for identification)
    pub key_prefix: String,

    /// Human-readable name
    pub name: String,

    /// Optional description
    pub description: Option<String>,

    /// Permission scopes (e.g., ["sandbox:read", "sandbox:write"])
    pub scopes: Vec<String>,

    /// Whether the key is active
    pub is_active: bool,

    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Optional expiration timestamp
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Last successful authentication timestamp
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Optional creator identifier
    pub created_by: Option<String>,
}

/// Request to create a new API key
#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    /// Human-readable name (required)
    pub name: String,

    /// Optional description
    pub description: Option<String>,

    /// Optional permission scopes
    pub scopes: Option<Vec<String>>,

    /// Optional expiration in days from now
    pub expires_in_days: Option<i64>,

    /// Optional creator identifier
    pub created_by: Option<String>,
}

/// Response containing the newly created API key
///
/// The `api_key` field contains the full API key and is only shown once.
#[derive(Debug, Serialize)]
pub struct ApiKeyResponse {
    /// The full API key (only shown on creation)
    pub api_key: String,

    /// The key metadata (without the hash)
    pub key: ApiKey,
}

/// Trait for API key storage operations
///
/// This trait defines the interface for API key management.
/// Implementations can use different storage backends (PostgreSQL, Redis, etc.).
#[async_trait]
pub trait ApiKeyStore: Send + Sync {
    /// Validate an API key and return its ID if valid.
    ///
    /// Checks if the key is valid, active, and not expired.
    /// Updates the `last_used_at` timestamp on successful validation.
    ///
    /// # Arguments
    ///
    /// * `key` - The API key to validate
    ///
    /// # Returns
    ///
    /// * `Ok(Some(uuid))` - Key is valid, returns the API key ID
    /// * `Ok(None)` - Key is invalid
    /// * `Err(...)` - Database error
    async fn validate_api_key(
        &self,
        key: &str,
    ) -> Result<Option<uuid::Uuid>, Box<dyn std::error::Error + Send + Sync>>;

    /// Create a new API key
    ///
    /// Generates a new API key, hashes it, and stores it in the database.
    ///
    /// # Arguments
    ///
    /// * `req` - Key creation request
    ///
    /// # Returns
    ///
    /// * `Ok(ApiKeyResponse)` - Contains the new key (only shown once) and metadata
    /// * `Err(...)` - Database error
    async fn create_api_key(
        &self,
        req: CreateApiKeyRequest,
    ) -> Result<ApiKeyResponse, Box<dyn std::error::Error + Send + Sync>>;

    /// List all API keys
    ///
    /// Returns all API keys with the hash redacted.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<ApiKey>)` - List of keys (hashes redacted)
    /// * `Err(...)` - Database error
    async fn list_api_keys(&self) -> Result<Vec<ApiKey>, Box<dyn std::error::Error + Send + Sync>>;

    /// Get a specific API key by ID
    ///
    /// Returns the key metadata with the hash redacted.
    ///
    /// # Arguments
    ///
    /// * `id` - Key UUID
    ///
    /// # Returns
    ///
    /// * `Ok(Some(ApiKey))` - Key found
    /// * `Ok(None)` - Key not found
    /// * `Err(...)` - Database error
    async fn get_api_key(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<ApiKey>, Box<dyn std::error::Error + Send + Sync>>;

    /// Delete an API key
    ///
    /// # Arguments
    ///
    /// * `id` - Key UUID
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Key deleted
    /// * `Ok(false)` - Key not found
    /// * `Err(...)` - Database error
    async fn delete_api_key(
        &self,
        id: uuid::Uuid,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>;

    /// Rotate (replace) an API key
    ///
    /// Generates a new API key for the same metadata.
    /// The old key becomes invalid immediately.
    ///
    /// # Arguments
    ///
    /// * `id` - Key UUID
    ///
    /// # Returns
    ///
    /// * `Ok(ApiKeyResponse)` - Contains the new key and metadata
    /// * `Err(...)` - Database error or key not found
    async fn rotate_api_key(
        &self,
        id: uuid::Uuid,
    ) -> Result<ApiKeyResponse, Box<dyn std::error::Error + Send + Sync>>;
}

/// PostgreSQL implementation of ApiKeyStore
pub struct PostgresApiKeyStore {
    pool: Pool,
}

impl PostgresApiKeyStore {
    /// Create a new PostgresApiKeyStore
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Generate a new API key
    ///
    /// Format: `dsb_pk_` + 32 random alphanumeric characters
    fn generate_api_key() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        const PREFIX: &str = "dsb_pk_";
        const KEY_LENGTH: usize = 32;

        let mut rng = rand::thread_rng();
        let random_part: String = (0..KEY_LENGTH)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();

        format!("{}{}", PREFIX, random_part)
    }

    /// Hash an API key using bcrypt
    ///
    /// # Arguments
    ///
    /// * `key` - The API key to hash
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Bcrypt hash
    /// * `Err(bcrypt::BcryptError)` - Hashing error
    fn hash_api_key(key: &str) -> Result<String, bcrypt::BcryptError> {
        bcrypt::hash(key, bcrypt::DEFAULT_COST)
    }

    /// Verify an API key against its hash
    ///
    /// # Arguments
    ///
    /// * `key` - The API key to verify
    /// * `hash` - The bcrypt hash to verify against
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Key matches hash
    /// * `Ok(false)` - Key doesn't match
    /// * `Err(bcrypt::BcryptError)` - Verification error
    fn verify_api_key(key: &str, hash: &str) -> Result<bool, bcrypt::BcryptError> {
        bcrypt::verify(key, hash)
    }
}

#[async_trait]
impl ApiKeyStore for PostgresApiKeyStore {
    async fn validate_api_key(
        &self,
        key: &str,
    ) -> Result<Option<uuid::Uuid>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Extract prefix for efficient lookup
        let key_prefix = &key[..8.min(key.len())];

        const QUERY: &str = r#"
            SELECT key_hash, id, is_active, expires_at
            FROM api_keys
            WHERE key_prefix = $1 AND is_active = TRUE
        "#;

        let rows = client
            .query(QUERY, &[&key_prefix])
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        for row in rows {
            let key_hash: &str = row.get("key_hash");
            let _is_active: bool = row.get("is_active");
            let expires_at: Option<chrono::DateTime<chrono::Utc>> = row.get("expires_at");

            // Check if key is expired
            if let Some(expiry) = expires_at {
                if expiry < chrono::Utc::now() {
                    continue;
                }
            }

            // Verify key hash
            if Self::verify_api_key(key, key_hash)? {
                // Update last_used_at
                let id: uuid::Uuid = row.get("id");
                const UPDATE_QUERY: &str = "UPDATE api_keys SET last_used_at = NOW() WHERE id = $1";
                client
                    .execute(UPDATE_QUERY, &[&id])
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

                tracing::debug!("API key validated successfully (prefix: {})", key_prefix);
                return Ok(Some(id));
            }
        }

        tracing::warn!("API key validation failed (prefix: {})", key_prefix);
        Ok(None)
    }

    async fn create_api_key(
        &self,
        req: CreateApiKeyRequest,
    ) -> Result<ApiKeyResponse, Box<dyn std::error::Error + Send + Sync>> {
        let api_key = Self::generate_api_key();
        let key_hash = Self::hash_api_key(&api_key)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        let key_prefix = api_key[..8].to_string();

        let expires_at = req
            .expires_in_days
            .map(|days| chrono::Utc::now() + chrono::Duration::days(days));

        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        const QUERY: &str = r#"
            INSERT INTO api_keys (
                key_hash, key_prefix, name, description, scopes, expires_at, created_by
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, created_at, is_active
        "#;

        let row = client
            .query_one(
                QUERY,
                &[
                    &key_hash,
                    &key_prefix,
                    &req.name,
                    &req.description,
                    &serde_json::to_value(req.scopes.clone().unwrap_or_default())
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
                    &expires_at,
                    &req.created_by,
                ],
            )
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let key_prefix_clone = key_prefix.clone();
        let key = ApiKey {
            id: row.get("id"),
            key_hash,
            key_prefix,
            name: req.name,
            description: req.description,
            scopes: req.scopes.unwrap_or_default(),
            is_active: row.get("is_active"),
            created_at: row.get("created_at"),
            expires_at,
            last_used_at: None,
            created_by: req.created_by,
        };

        tracing::info!(
            "Created API key: {} (prefix: {})",
            key.name,
            key_prefix_clone
        );
        Ok(ApiKeyResponse { api_key, key })
    }

    async fn list_api_keys(&self) -> Result<Vec<ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        const QUERY: &str = r#"
            SELECT id, key_prefix, name, description, scopes, is_active,
                   created_at, expires_at, last_used_at, created_by
            FROM api_keys
            ORDER BY created_at DESC
        "#;

        let rows = client
            .query(QUERY, &[])
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let keys = rows
            .iter()
            .map(|row| {
                let scopes_json: serde_json::Value = row.get("scopes");
                let scopes: Vec<String> = serde_json::from_value(scopes_json).unwrap_or_default();

                ApiKey {
                    id: row.get("id"),
                    key_hash: "[REDACTED]".to_string(),
                    key_prefix: row.get("key_prefix"),
                    name: row.get("name"),
                    description: row.get("description"),
                    scopes,
                    is_active: row.get("is_active"),
                    created_at: row.get("created_at"),
                    expires_at: row.get("expires_at"),
                    last_used_at: row.get("last_used_at"),
                    created_by: row.get("created_by"),
                }
            })
            .collect();

        Ok(keys)
    }

    async fn get_api_key(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        const QUERY: &str = r#"
            SELECT id, key_prefix, name, description, scopes, is_active,
                   created_at, expires_at, last_used_at, created_by
            FROM api_keys
            WHERE id = $1
        "#;

        let row = match client
            .query_opt(QUERY, &[&id])
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
        {
            Some(r) => r,
            None => return Ok(None),
        };

        let scopes_json: serde_json::Value = row.get("scopes");
        let scopes: Vec<String> = serde_json::from_value(scopes_json).unwrap_or_default();

        Ok(Some(ApiKey {
            id,
            key_hash: "[REDACTED]".to_string(),
            key_prefix: row.get("key_prefix"),
            name: row.get("name"),
            description: row.get("description"),
            scopes,
            is_active: row.get("is_active"),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
            last_used_at: row.get("last_used_at"),
            created_by: row.get("created_by"),
        }))
    }

    async fn delete_api_key(
        &self,
        id: uuid::Uuid,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        const QUERY: &str = "DELETE FROM api_keys WHERE id = $1";
        let rows_affected = client
            .execute(QUERY, &[&id])
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let deleted = rows_affected > 0;
        if deleted {
            tracing::info!("Deleted API key: {}", id);
        }
        Ok(deleted)
    }

    async fn rotate_api_key(
        &self,
        id: uuid::Uuid,
    ) -> Result<ApiKeyResponse, Box<dyn std::error::Error + Send + Sync>> {
        let api_key = Self::generate_api_key();
        let key_hash = Self::hash_api_key(&api_key)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        let key_prefix = api_key[..8].to_string();

        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        const QUERY: &str = r#"
            UPDATE api_keys
            SET key_hash = $1, key_prefix = $2
            WHERE id = $3
            RETURNING name, description, scopes, is_active, created_at, expires_at, created_by
        "#;

        let row = match client
            .query_opt(QUERY, &[&key_hash, &key_prefix, &id])
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
        {
            Some(r) => r,
            None => {
                return Err(format!("API key not found: {}", id).into());
            }
        };

        let scopes_json: serde_json::Value = row.get("scopes");
        let scopes: Vec<String> = serde_json::from_value(scopes_json).unwrap_or_default();

        let key_prefix_clone = key_prefix.clone();
        let key = ApiKey {
            id,
            key_hash,
            key_prefix,
            name: row.get("name"),
            description: row.get("description"),
            scopes,
            is_active: row.get("is_active"),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
            last_used_at: None, // Reset on rotation
            created_by: row.get("created_by"),
        };

        tracing::info!(
            "Rotated API key: {} (prefix: {})",
            key.name,
            key_prefix_clone
        );
        Ok(ApiKeyResponse { api_key, key })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_api_key() {
        let key = PostgresApiKeyStore::generate_api_key();
        assert!(key.starts_with("dsb_pk_"));
        assert_eq!(key.len(), 39); // "dsb_pk_" (7) + 32 chars
    }

    #[test]
    fn test_hash_and_verify_api_key() {
        let key = "test_api_key_12345";
        let hash = PostgresApiKeyStore::hash_api_key(key).unwrap();

        // Verify correct key
        assert!(PostgresApiKeyStore::verify_api_key(key, &hash).unwrap());

        // Reject incorrect key
        assert!(!PostgresApiKeyStore::verify_api_key("wrong_key", &hash).unwrap());
    }

    #[test]
    fn test_multiple_keys_hash_differently() {
        let key1 = "dsb_pk_abc123xyz789";
        let key2 = "dsb_pk_def456uvw012";

        let hash1 = PostgresApiKeyStore::hash_api_key(key1).unwrap();
        let hash2 = PostgresApiKeyStore::hash_api_key(key2).unwrap();

        // Different keys should produce different hashes
        assert_ne!(hash1, hash2);

        // Each key should verify with its own hash
        assert!(PostgresApiKeyStore::verify_api_key(key1, &hash1).unwrap());
        assert!(PostgresApiKeyStore::verify_api_key(key2, &hash2).unwrap());

        // Keys should not verify with each other's hashes
        assert!(!PostgresApiKeyStore::verify_api_key(key1, &hash2).unwrap());
        assert!(!PostgresApiKeyStore::verify_api_key(key2, &hash1).unwrap());
    }

    #[test]
    fn test_hash_is_not_deterministic_due_to_salt() {
        let key = "test_api_key";

        let hash1 = PostgresApiKeyStore::hash_api_key(key).unwrap();
        let hash2 = PostgresApiKeyStore::hash_api_key(key).unwrap();

        // Bcrypt uses random salt, so same key produces different hashes
        assert_ne!(hash1, hash2);

        // But both hashes should verify the same key
        assert!(PostgresApiKeyStore::verify_api_key(key, &hash1).unwrap());
        assert!(PostgresApiKeyStore::verify_api_key(key, &hash2).unwrap());
    }

    #[test]
    fn test_api_key_prefix_extraction() {
        let key = "dsb_pk_1234567890abcdefghijklmnopqrstuvwxyz";
        let prefix = &key[..8];

        assert_eq!(prefix, "dsb_pk_1");
        assert_eq!(prefix.len(), 8);
    }

    #[test]
    fn test_create_api_key_request_default_scopes() {
        let req = CreateApiKeyRequest {
            name: "Test Key".to_string(),
            description: None,
            scopes: None,
            expires_in_days: None,
            created_by: None,
        };

        assert_eq!(req.name, "Test Key");
        assert!(req.description.is_none());
        assert!(req.scopes.is_none());
        assert!(req.expires_in_days.is_none());
        assert!(req.created_by.is_none());
    }

    #[test]
    fn test_create_api_key_request_with_all_fields() {
        let req = CreateApiKeyRequest {
            name: "Full Key".to_string(),
            description: Some("A complete key".to_string()),
            scopes: Some(vec![
                "sandbox:read".to_string(),
                "sandbox:write".to_string(),
            ]),
            expires_in_days: Some(365),
            created_by: Some("admin".to_string()),
        };

        assert_eq!(req.name, "Full Key");
        assert_eq!(req.description, Some("A complete key".to_string()));
        assert_eq!(
            req.scopes,
            Some(vec![
                "sandbox:read".to_string(),
                "sandbox:write".to_string()
            ])
        );
        assert_eq!(req.expires_in_days, Some(365));
        assert_eq!(req.created_by, Some("admin".to_string()));
    }

    #[test]
    fn test_api_key_serialization_skips_hash() {
        let key = ApiKey {
            id: uuid::Uuid::new_v4(),
            key_hash: "secret_hash".to_string(),
            key_prefix: "dsb_pk_a".to_string(),
            name: "Test Key".to_string(),
            description: None,
            scopes: vec![],
            is_active: true,
            created_at: chrono::Utc::now(),
            expires_at: None,
            last_used_at: None,
            created_by: None,
        };

        // Serialize to JSON
        let json = serde_json::to_string(&key).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Hash should not be present in JSON at all
        assert!(parsed.get("key_hash").is_none());

        // Other fields should be present
        assert_eq!(parsed["key_prefix"], "dsb_pk_a");
        assert_eq!(parsed["name"], "Test Key");
    }

    #[test]
    fn test_api_key_response_serialization_includes_full_key() {
        let response = ApiKeyResponse {
            api_key: "dsb_pk_abcdefghijklmnopqrstuvwxyz123456".to_string(),
            key: ApiKey {
                id: uuid::Uuid::new_v4(),
                key_hash: "secret_hash".to_string(),
                key_prefix: "dsb_pk_a".to_string(),
                name: "Test Key".to_string(),
                description: None,
                scopes: vec![],
                is_active: true,
                created_at: chrono::Utc::now(),
                expires_at: None,
                last_used_at: None,
                created_by: None,
            },
        };

        // Serialize to JSON
        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Response should include full API key
        assert_eq!(parsed["api_key"], "dsb_pk_abcdefghijklmnopqrstuvwxyz123456");

        // Key metadata should not include hash
        assert!(parsed["key"].get("key_hash").is_none());

        // But should include other fields
        assert_eq!(parsed["key"]["key_prefix"], "dsb_pk_a");
        assert_eq!(parsed["key"]["name"], "Test Key");
    }

    #[tokio::test]
    async fn test_generate_api_key_uniqueness() {
        // Generate multiple keys and verify they're unique
        let mut keys = std::collections::HashSet::new();

        for _ in 0..100 {
            let key = PostgresApiKeyStore::generate_api_key();
            keys.insert(key);
        }

        // All 100 keys should be unique
        assert_eq!(keys.len(), 100);
    }

    #[test]
    fn test_verify_api_key_with_empty_key() {
        let key = "";
        let hash = PostgresApiKeyStore::hash_api_key(key).unwrap();

        // Empty key should verify correctly
        assert!(PostgresApiKeyStore::verify_api_key(key, &hash).unwrap());

        // Different key should not verify
        assert!(!PostgresApiKeyStore::verify_api_key("non_empty", &hash).unwrap());
    }
}
