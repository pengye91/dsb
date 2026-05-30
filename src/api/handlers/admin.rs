// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Admin API Handlers
//!
//! This module provides admin API endpoints for managing API keys.
//!
//! ## Endpoints
//!
//! - `POST /admin/api-keys` - Create new API key
//! - `GET /admin/api-keys` - List all API keys
//! - `GET /admin/api-keys/:id` - Get specific API key
//! - `DELETE /admin/api-keys/:id` - Delete API key
//! - `POST /admin/api-keys/:id/rotate` - Rotate (replace) API key
//!
//! ## Authentication
//!
//! All admin endpoints require authentication with the admin API key.
//! The admin key is set via the `server.admin_api_key` configuration.
//!
//! ## Usage Example
//!
//! ```bash
//! # Set admin key
//! export ADMIN_KEY="dsb_admin_secret"
//!
//! # Create new API key
//! curl -X POST http://localhost:8080/admin/api-keys \
//!   -H "X-API-Key: $ADMIN_KEY" \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "name": "CLI Key",
//!     "description": "For CLI access",
//!     "scopes": ["sandbox:read", "sandbox:write"],
//!     "expires_in_days": 365
//!   }'
//! ```

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

/// Admin API state
///
/// This struct holds the dependencies needed for admin operations.
#[derive(Clone)]
pub struct AdminState {
    /// API key store for CRUD operations
    pub api_key_store: Arc<dyn crate::db::ApiKeyStore>,

    /// Admin API key (for authentication)
    pub admin_api_key: Option<String>,
}

/// Create a new API key
///
/// # Arguments
///
/// * `state` - Admin state with API key store
/// * `req` - API key creation request
///
/// # Returns
///
/// * `201 Created` - With API key response (includes the actual key, shown only once)
/// * `401 Unauthorized` - If not authenticated with admin key
/// * `500 Internal Server Error` - On database errors
///
/// # Example Request
///
/// ```json
/// {
///   "name": "CLI Key",
///   "description": "For CLI access",
///   "scopes": ["sandbox:read", "sandbox:write"],
///   "expires_in_days": 365,
///   "created_by": "admin@example.com"
/// }
/// ```
///
/// # Example Response
///
/// ```json
/// {
///   "api_key": "dsb_pk_7xK9Mn2PqR4tY6VwZ8...",
///   "key": {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "key_prefix": "dsb_pk_7x",
///     "name": "CLI Key",
///     "description": "For CLI access",
///     "scopes": ["sandbox:read", "sandbox:write"],
///     "is_active": true,
///     "created_at": "2026-01-14T11:22:53Z",
///     "expires_at": "2027-01-14T11:22:53Z",
///     "last_used_at": null,
///     "created_by": "admin@example.com"
///   }
/// }
/// ```
///
/// **Important**: Save the `api_key` value immediately - you won't be able to see it again!
pub async fn create_api_key(
    State(state): State<AdminState>,
    Json(req): Json<crate::db::CreateApiKeyRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    match state.api_key_store.create_api_key(req).await {
        Ok(response) => Ok((StatusCode::CREATED, Json(response))),
        Err(e) => {
            tracing::error!("Failed to create API key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// List all API keys
///
/// Returns all API keys with their metadata (excluding the actual key hashes).
///
/// # Arguments
///
/// * `state` - Admin state with API key store
///
/// # Returns
///
/// * `200 OK` - With array of API keys
/// * `401 Unauthorized` - If not authenticated with admin key
/// * `500 Internal Server Error` - On database errors
///
/// # Example Response
///
/// ```json
/// [
///   {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "key_prefix": "dsb_pk_7x",
///     "name": "CLI Key",
///     "description": "For CLI access",
///     "scopes": ["sandbox:read", "sandbox:write"],
///     "is_active": true,
///     "created_at": "2026-01-14T11:22:53Z",
///     "expires_at": "2027-01-14T11:22:53Z",
///     "last_used_at": "2026-01-14T12:30:00Z",
///     "created_by": "admin@example.com"
///   }
/// ]
/// ```
pub async fn list_api_keys(
    State(state): State<AdminState>,
) -> Result<impl IntoResponse, StatusCode> {
    match state.api_key_store.list_api_keys().await {
        Ok(keys) => Ok(Json(keys)),
        Err(e) => {
            tracing::error!("Failed to list API keys: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get a specific API key by ID
///
/// Returns API key metadata (excluding the actual key hash).
///
/// # Arguments
///
/// * `state` - Admin state with API key store
/// * `id` - API key UUID
///
/// # Returns
///
/// * `200 OK` - With API key metadata
/// * `401 Unauthorized` - If not authenticated with admin key
/// * `404 Not Found` - If API key doesn't exist
/// * `500 Internal Server Error` - On database errors
///
/// # Example Response
///
/// ```json
/// {
///   "id": "550e8400-e29b-41d4-a716-446655440000",
///   "key_prefix": "dsb_pk_7x",
///   "name": "CLI Key",
///   "description": "For CLI access",
///   "scopes": ["sandbox:read", "sandbox:write"],
///   "is_active": true,
///   "created_at": "2026-01-14T11:22:53Z",
///   "expires_at": "2027-01-14T11:22:53Z",
///   "last_used_at": "2026-01-14T12:30:00Z",
///   "created_by": "admin@example.com"
/// }
/// ```
pub async fn get_api_key(
    State(state): State<AdminState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    match state.api_key_store.get_api_key(id).await {
        Ok(Some(key)) => Ok(Json(key)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get API key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete an API key
///
/// Permanently deletes an API key. This action cannot be undone.
///
/// # Arguments
///
/// * `state` - Admin state with API key store
/// * `id` - API key UUID
///
/// # Returns
///
/// * `204 No Content` - On successful deletion
/// * `401 Unauthorized` - If not authenticated with admin key
/// * `404 Not Found` - If API key doesn't exist
/// * `500 Internal Server Error` - On database errors
///
/// # Example
///
/// ```bash
/// curl -X DELETE http://localhost:8080/admin/api-keys/550e8400-e29b-41d4-a716-446655440000 \
///   -H "X-API-Key: $ADMIN_KEY"
/// ```
pub async fn delete_api_key(
    State(state): State<AdminState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    match state.api_key_store.delete_api_key(id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to delete API key: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Rotate (replace) an API key
///
/// Generates a new API key for the same metadata. The old key becomes invalid immediately.
/// The new key is returned in the response (shown only once).
///
/// # Arguments
///
/// * `state` - Admin state with API key store
/// * `id` - API key UUID
///
/// # Returns
///
/// * `200 OK` - With new API key response
/// * `401 Unauthorized` - If not authenticated with admin key
/// * `404 Not Found` - If API key doesn't exist
/// * `500 Internal Server Error` - On database errors
///
/// # Example Response
///
/// ```json
/// {
///   "api_key": "dsb_pk_Q2wE4rT6...",
///   "key": {
///     "id": "550e8400-e29b-41d4-a716-446655440000",
///     "key_prefix": "dsb_pk_Q2",
///     "name": "CLI Key",
///     "description": "For CLI access",
///     "scopes": ["sandbox:read", "sandbox:write"],
///     "is_active": true,
///     "created_at": "2026-01-14T11:22:53Z",
///     "expires_at": "2027-01-14T11:22:53Z",
///     "last_used_at": null,
///     "created_by": "admin@example.com"
///   }
/// }
/// ```
///
/// **Note**: The old key becomes invalid immediately after rotation. Update all clients using the old key.
pub async fn rotate_api_key(
    State(state): State<AdminState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    match state.api_key_store.rotate_api_key(id).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            tracing::error!(
                error = %e,
                operation = "rotate_api_key",
                key_id = %id,
                "Failed to rotate API key"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Admin authentication middleware
///
/// Validates that requests to admin endpoints use the admin API key.
///
/// # Arguments
///
/// * `state` - Admin state with admin API key
/// * `headers` - Request headers
/// * `req` - Request
/// * `next` - Next middleware/handler
///
/// # Returns
///
/// * `Response` - If authenticated with admin key
/// * `401 Unauthorized` - If not authenticated
///
/// # Behavior
///
/// - Requires `X-API-Key` header
/// - Key must match `admin_api_key` from config
/// - Regular API keys (from database or config) are NOT accepted
pub async fn admin_auth_middleware(
    State(state): State<AdminState>,
    headers: axum::http::HeaderMap,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<Response, StatusCode> {
    let provided_key = match headers.get("x-api-key") {
        Some(header) => match header.to_str() {
            Ok(v) => v,
            Err(_) => {
                tracing::warn!("Invalid UTF-8 in x-api-key header");
                return Err(StatusCode::UNAUTHORIZED);
            }
        },
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    // Must use admin API key (not database keys or config keys)
    match &state.admin_api_key {
        Some(admin_key) if provided_key == admin_key => {
            tracing::debug!("Admin request authenticated with admin API key");
            Ok(next.run(req).await)
        }
        Some(_) => {
            tracing::warn!("Unauthorized admin access attempt (non-admin key used)");
            Err(StatusCode::UNAUTHORIZED)
        }
        None => {
            tracing::warn!("Admin access attempt but no admin API key configured");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    /// Mock API key store for testing
    struct MockApiKeyStore {
        create_should_fail: bool,
        list_should_fail: bool,
    }

    #[async_trait::async_trait]
    impl crate::db::ApiKeyStore for MockApiKeyStore {
        async fn validate_api_key(
            &self,
            _key: &str,
        ) -> Result<Option<uuid::Uuid>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(None)
        }

        async fn create_api_key(
            &self,
            _req: crate::db::CreateApiKeyRequest,
        ) -> Result<crate::db::ApiKeyResponse, Box<dyn std::error::Error + Send + Sync>> {
            if self.create_should_fail {
                Err("Database error".into())
            } else {
                Ok(crate::db::ApiKeyResponse {
                    api_key: "dsb_pk_test".to_string(),
                    key: crate::db::ApiKey {
                        id: Uuid::new_v4(),
                        key_hash: "hash".to_string(),
                        key_prefix: "dsb_pk_t".to_string(),
                        name: "Test Key".to_string(),
                        description: None,
                        scopes: vec![],
                        is_active: true,
                        created_at: chrono::Utc::now(),
                        expires_at: None,
                        last_used_at: None,
                        created_by: None,
                    },
                })
            }
        }

        async fn list_api_keys(
            &self,
        ) -> Result<Vec<crate::db::ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
            if self.list_should_fail {
                Err("Database error".into())
            } else {
                Ok(vec![])
            }
        }

        async fn get_api_key(
            &self,
            _id: Uuid,
        ) -> Result<Option<crate::db::ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(None)
        }

        async fn delete_api_key(
            &self,
            _id: Uuid,
        ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
            Ok(false)
        }

        async fn rotate_api_key(
            &self,
            _id: Uuid,
        ) -> Result<crate::db::ApiKeyResponse, Box<dyn std::error::Error + Send + Sync>> {
            Err("Not implemented".into())
        }
    }

    #[tokio::test]
    async fn test_create_api_key_success() {
        let state = AdminState {
            api_key_store: Arc::new(MockApiKeyStore {
                create_should_fail: false,
                list_should_fail: false,
            }),
            admin_api_key: Some("admin_key".to_string()),
        };

        let req = crate::db::CreateApiKeyRequest {
            name: "Test Key".to_string(),
            description: None,
            scopes: None,
            expires_in_days: None,
            created_by: None,
        };

        let result = create_api_key(State(state), Json(req)).await;
        assert!(result.is_ok());
        let response = result.unwrap().into_response();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_create_api_key_database_error() {
        let state = AdminState {
            api_key_store: Arc::new(MockApiKeyStore {
                create_should_fail: true,
                list_should_fail: false,
            }),
            admin_api_key: Some("admin_key".to_string()),
        };

        let req = crate::db::CreateApiKeyRequest {
            name: "Test Key".to_string(),
            description: None,
            scopes: None,
            expires_in_days: None,
            created_by: None,
        };

        let result = create_api_key(State(state), Json(req)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_api_keys_success() {
        let state = AdminState {
            api_key_store: Arc::new(MockApiKeyStore {
                create_should_fail: false,
                list_should_fail: false,
            }),
            admin_api_key: Some("admin_key".to_string()),
        };

        let result = list_api_keys(State(state)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_api_keys_database_error() {
        let state = AdminState {
            api_key_store: Arc::new(MockApiKeyStore {
                create_should_fail: false,
                list_should_fail: true,
            }),
            admin_api_key: Some("admin_key".to_string()),
        };

        let result = list_api_keys(State(state)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_api_key_not_found() {
        let state = AdminState {
            api_key_store: Arc::new(MockApiKeyStore {
                create_should_fail: false,
                list_should_fail: false,
            }),
            admin_api_key: Some("admin_key".to_string()),
        };

        let id = Uuid::new_v4();
        let result = get_api_key(State(state), Path(id)).await;
        assert!(result.is_err());
        match result {
            Err(StatusCode::NOT_FOUND) => {}
            _ => panic!("Expected NOT_FOUND status code"),
        }
    }

    #[tokio::test]
    async fn test_delete_api_key_not_found() {
        let state = AdminState {
            api_key_store: Arc::new(MockApiKeyStore {
                create_should_fail: false,
                list_should_fail: false,
            }),
            admin_api_key: Some("admin_key".to_string()),
        };

        let id = Uuid::new_v4();
        let result = delete_api_key(State(state), Path(id)).await;
        assert!(result.is_err());
        match result {
            Err(StatusCode::NOT_FOUND) => {}
            _ => panic!("Expected NOT_FOUND status code"),
        }
    }

    // Note: Middleware tests (admin_auth_middleware) require integration testing
    // with actual axum Router setup. They will be tested in integration tests.
}
