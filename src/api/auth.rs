// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # API Authentication Module
//!
//! This module provides API key authentication middleware for DSB.
//!
//! ## Architecture
//!
//! The authentication system supports multiple API key sources:
//!
//! ```text
//! Authentication Flow:
//! 1. Check if authentication is disabled (require_auth=false) → Allow all
//! 2. Skip /health endpoint → Always allow
//! 3. Extract X-API-Key header
//! 4. Check auth sources in order:
//!    a. Admin API key (for admin operations)
//!    b. Database-stored keys (PostgreSQL with bcrypt)
//!    c. Legacy config API key (backward compatibility)
//! 5. If no valid key → Return 401 Unauthorized
//! ```
//!
//! ## Usage Example
//!
//! ```rust,no_run,ignore
//! use dsb::api::auth::{AuthState, api_key_auth};
//! use dsb::db::PostgresApiKeyStore;
//! use std::sync::Arc;
//!
//! # async fn example() {
//! // Create auth state
//! let auth_state = AuthState {
//!     config_api_key: Some("legacy_key".to_string()),
//!     admin_api_key: Some("admin_key".to_string()),
//!     require_auth: true,
//!     api_key_store: Some(Arc::new(PostgresApiKeyStore::new(pool))),
//! };
//!
//! // Apply middleware
//! let app = Router::new()
//!     .layer(axum::middleware::from_fn_with_state(auth_state, api_key_auth));
//! # }
//! ```

use axum::extract::FromRef;
use axum::{
    extract::{Extension, Request, State},
    http::{HeaderMap, StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::{Cookie, Key, PrivateCookieJar};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub use crate::core::types::{ApiKeyIdentity, ApiKeyType};

/// Authentication state for the API key middleware
///
/// This struct holds all configuration and data sources needed for authentication.
#[derive(Clone)]
pub struct AuthState {
    /// Legacy single API key from config file (backward compatibility)
    pub config_api_key: Option<String>,

    /// Admin API key for admin operations and bootstrapping
    pub admin_api_key: Option<String>,

    /// Whether authentication is required (false = development mode)
    pub require_auth: bool,

    /// Whether static file server requires authentication (independent of server.require_auth)
    pub static_server_require_auth: bool,

    /// Whether VNC proxy requires authentication (independent of server.require_auth)
    pub vnc_require_auth: bool,

    /// Database-backed API key store (optional, if PostgreSQL is enabled)
    pub api_key_store: Option<Arc<dyn crate::db::ApiKeyStore>>,

    /// Cookie signing/encryption key
    pub cookie_key: Key,
}

impl FromRef<AuthState> for Key {
    fn from_ref(state: &AuthState) -> Self {
        state.cookie_key.clone()
    }
}

/// API key authentication middleware (new multi-key version)
///
/// Checks for X-API-Key header or api_key query parameter and validates against multiple sources:
/// 1. Admin API key (for admin operations)
/// 2. Database-stored keys (if available)
/// 3. Legacy config API key (backward compatibility)
///
/// Skips authentication for:
/// - `/health` endpoint (always accessible)
/// - All requests when `require_auth == false` (development mode)
///
/// # Arguments
///
/// * `State(auth_state)` - Authentication state with all configuration
///
/// # Behavior
///
/// - If `require_auth == false`: Allow all requests
/// - If `require_auth == true`:
///   - `/health` endpoint: No auth required
///   - All other endpoints: Require valid API key
///
/// # Authentication Sources (checked in order)
///
/// 1. Admin API key (from config)
/// 2. Database keys (from PostgreSQL)
/// 3. Legacy config API key (backward compatibility)
///
/// # API Key Sources
///
/// The API key can be provided via:
/// - `X-API-Key` header (preferred for HTTP requests)
/// - `api_key` query parameter (for SSE/WebSocket which don't support headers)
///
/// # Example
///
/// ```rust,no_run,ignore
/// # use axum::Router;
/// # use axum::middleware;
/// # use dsb::api::auth::{AuthState, api_key_auth};
/// # use dsb::db::PostgresApiKeyStore;
/// # use std::sync::Arc;
/// # fn main() {
/// let auth_state = AuthState {
///     config_api_key: Some("legacy_key".to_string()),
///     admin_api_key: Some("admin_key".to_string()),
///     require_auth: true,
///     api_key_store: Some(Arc::new(PostgresApiKeyStore::new(pool))),
/// };
///
/// let app = Router::new()
///     .layer(middleware::from_fn_with_state(auth_state, api_key_auth));
/// # }
/// ```
pub async fn api_key_auth(
    State(auth_state): State<AuthState>,
    jar: PrivateCookieJar,
    uri: Uri,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = uri.path();

    // 1. Unconditionally skip authentication for certain endpoints
    if path == "/health"
        || path.starts_with("/ssh/authorize/")
        || path == "/api/auth/login"
        || path.starts_with("/dashboard/")
    {
        return Ok(next.run(req).await);
    }

    // 2. For VNC routes with regular HTTP GET (not WebSocket), return helpful message
    if path.starts_with("/vnc/") {
        let is_websocket = headers
            .get("upgrade")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("websocket"))
            .unwrap_or(false);

        if !is_websocket {
            return Ok(axum::response::Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(axum::body::Body::from(
                    "This endpoint is for VNC WebSocket connections only. \
                    Please use the dashboard's VNC viewer or include an API key.\n\n\
                    For standalone VNC access, navigate to: http://localhost:3001",
                ))
                .unwrap()
                .into_response());
        }
    }

    // 3. Try to authenticate the request
    let mut identity = None;

    // 3a. Check session cookie first
    if let Some(cookie) = jar.get("dsb_session") {
        if let Ok(id) = serde_json::from_str::<ApiKeyIdentity>(cookie.value()) {
            identity = Some(id);
        }
    }

    // 3b. Check provided API key (from header or query parameter)
    if identity.is_none() {
        let provided_key = match headers.get("x-api-key") {
            Some(h) => match h.to_str() {
                Ok(s) => Some(s.to_string()),
                Err(_) => {
                    tracing::warn!("Invalid UTF-8 in x-api-key header");
                    return Ok(StatusCode::UNAUTHORIZED.into_response());
                }
            },
            None => None,
        }
        .or_else(|| {
                uri.query().and_then(|q| {
                    q.split('&').find_map(|pair| {
                        let mut parts = pair.split('=');
                        match (parts.next(), parts.next()) {
                            (Some("api_key"), Some(v)) => Some(v.to_string()),
                            _ => None,
                        }
                    })
                })
            });

        if let Some(key) = provided_key {
            // Check admin key
            if let Some(admin_key) = &auth_state.admin_api_key {
                if &key == admin_key {
                    identity = Some(ApiKeyIdentity {
                        id: None,
                        key_type: ApiKeyType::Privileged,
                    });
                }
            }

            // Check database keys
            if identity.is_none() {
                if let Some(store) = &auth_state.api_key_store {
                    if let Ok(Some(api_key_id)) = store.validate_api_key(&key).await {
                        identity = Some(ApiKeyIdentity {
                            id: Some(api_key_id),
                            key_type: ApiKeyType::Database,
                        });
                    }
                }
            }

            // Check config key
            if identity.is_none() {
                if let Some(config_key) = &auth_state.config_api_key {
                    if &key == config_key {
                        identity = Some(ApiKeyIdentity {
                            id: None,
                            key_type: ApiKeyType::Privileged,
                        });
                    }
                }
            }
        }
    }

    // 4. Handle authentication result
    match identity {
        Some(id) => {
            req.extensions_mut().insert(id);
            Ok(next.run(req).await)
        }
        None => {
            // Authentication failed or was not provided. Check if we should allow it anyway.

            // If global auth is disabled, allow with privileged identity
            if !auth_state.require_auth {
                req.extensions_mut().insert(ApiKeyIdentity {
                    id: None,
                    key_type: ApiKeyType::Privileged,
                });
                return Ok(next.run(req).await);
            }

            // VNC handler handles its own session_token and fallback auth logic
            if path.starts_with("/vnc/") {
                return Ok(next.run(req).await);
            }

            // Static files allow unauthenticated access if disabled in config
            if path.starts_with("/static/") && !auth_state.static_server_require_auth {
                return Ok(next.run(req).await);
            }

            // Otherwise, reject
            Ok(StatusCode::UNAUTHORIZED.into_response())
        }
    }
}

/// Helper to check if the given API key is valid (legacy helper for backward compatibility)
///
/// This function is kept for backward compatibility with existing code.
/// New code should use the `AuthState` and `api_key_auth` middleware instead.
///
/// # Arguments
///
/// * `provided_key` - The API key provided by the client
/// * `expected_key` - The expected API key
///
/// # Returns
///
/// * `true` - Key is valid or no key required
/// * `false` - Key is invalid
pub fn is_api_key_valid(provided_key: &Option<String>, expected_key: &Option<String>) -> bool {
    match (expected_key, provided_key) {
        (None, _) => true, // No key required
        (Some(expected), Some(provided)) => provided == expected,
        (Some(_), None) => false, // Key required but not provided
    }
}

#[derive(Deserialize)]
/// Request payload for login endpoint
pub struct LoginRequest {
    /// API key provided by the client
    pub api_key: String,
}

#[derive(Serialize)]
/// Response payload for login endpoint
pub struct LoginResponse {
    /// True if login was successful
    pub success: bool,
    /// Identity of the logged in user
    pub identity: ApiKeyIdentity,
}

/// Login endpoint to create a session cookie from an API key
pub async fn login(
    Extension(auth_state): Extension<AuthState>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let provided_key = payload.api_key.as_str();
    let mut identity = None;

    // 1. Check admin API key
    if let Some(admin_key) = &auth_state.admin_api_key {
        if provided_key == admin_key {
            identity = Some(ApiKeyIdentity {
                id: None,
                key_type: ApiKeyType::Privileged,
            });
        }
    }

    // 2. Check database keys
    if identity.is_none() {
        if let Some(store) = &auth_state.api_key_store {
            match store.validate_api_key(provided_key).await {
                Ok(Some(api_key_id)) => {
                    identity = Some(ApiKeyIdentity {
                        id: Some(api_key_id),
                        key_type: ApiKeyType::Database,
                    });
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::error!("Error validating API key against database: {}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    }

    // 3. Check legacy config API key
    if identity.is_none() {
        if let Some(config_key) = &auth_state.config_api_key {
            if provided_key == config_key {
                identity = Some(ApiKeyIdentity {
                    id: None,
                    key_type: ApiKeyType::Privileged,
                });
            }
        }
    }

    match identity {
        Some(identity) => {
            let session_data =
                serde_json::to_string(&identity).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let jar = PrivateCookieJar::from_headers(&headers, auth_state.cookie_key.clone());
            let cookie = Cookie::build(("dsb_session", session_data))
                .path("/")
                .http_only(true)
                .same_site(axum_extra::extract::cookie::SameSite::Lax)
                .build();

            let jar = jar.add(cookie);
            Ok((
                jar,
                Json(LoginResponse {
                    success: true,
                    identity,
                }),
            ))
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Logout endpoint to clear the session cookie
pub async fn logout(
    Extension(auth_state): Extension<AuthState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let jar = PrivateCookieJar::from_headers(&headers, auth_state.cookie_key.clone());
    let cookie = Cookie::build("dsb_session").path("/").build();
    let jar = jar.remove(cookie);
    (jar, StatusCode::OK)
}

/// Endpoint to get current session identity
pub async fn me(
    Extension(auth_state): Extension<AuthState>,
    headers: HeaderMap,
) -> Result<Json<ApiKeyIdentity>, StatusCode> {
    let jar = PrivateCookieJar::from_headers(&headers, auth_state.cookie_key.clone());
    if let Some(cookie) = jar.get("dsb_session") {
        match serde_json::from_str::<ApiKeyIdentity>(cookie.value()) {
            Ok(identity) => Ok(Json(identity)),
            Err(_) => Err(StatusCode::UNAUTHORIZED),
        }
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::Extension,
        http::{HeaderMap, Method, StatusCode},
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    /// Helper to create a test request
    fn create_test_request(uri: &str, headers: HeaderMap) -> Request {
        let mut request_builder = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("host", "localhost");

        // Add all headers from the HeaderMap
        for (name, value) in headers.iter() {
            request_builder = request_builder.header(name, value);
        }

        request_builder.body(Body::empty()).unwrap()
    }

    /// Helper to build a simple test app with new auth middleware
    async fn make_test_app_request(
        uri: &str,
        headers: HeaderMap,
        auth_state: AuthState,
    ) -> Result<StatusCode, Box<dyn std::error::Error>> {
        use axum::{routing::get, Router};

        async fn handler() -> &'static str {
            "OK"
        }

        let app = Router::new()
            .route("/health", get(handler))
            .route("/protected", get(handler))
            .layer(axum::middleware::from_fn_with_state(
                auth_state,
                api_key_auth,
            ));

        let request = create_test_request(uri, headers);
        let response = app.oneshot(request).await?;

        Ok(response.status())
    }

    struct MockApiKeyStore {
        valid_key: String,
        api_key_id: uuid::Uuid,
    }

    #[async_trait::async_trait]
    impl crate::db::ApiKeyStore for MockApiKeyStore {
        async fn validate_api_key(
            &self,
            key: &str,
        ) -> Result<Option<uuid::Uuid>, Box<dyn std::error::Error + Send + Sync>> {
            if key == self.valid_key {
                Ok(Some(self.api_key_id))
            } else {
                Ok(None)
            }
        }

        async fn create_api_key(
            &self,
            _req: crate::db::CreateApiKeyRequest,
        ) -> Result<crate::db::ApiKeyResponse, Box<dyn std::error::Error + Send + Sync>> {
            Err("not implemented".into())
        }

        async fn list_api_keys(
            &self,
        ) -> Result<Vec<crate::db::ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(vec![])
        }

        async fn get_api_key(
            &self,
            _id: uuid::Uuid,
        ) -> Result<Option<crate::db::ApiKey>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(None)
        }

        async fn delete_api_key(
            &self,
            _id: uuid::Uuid,
        ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
            Ok(false)
        }

        async fn rotate_api_key(
            &self,
            _id: uuid::Uuid,
        ) -> Result<crate::db::ApiKeyResponse, Box<dyn std::error::Error + Send + Sync>> {
            Err("not implemented".into())
        }
    }

    #[tokio::test]
    async fn test_auth_skip_health_endpoint() {
        // Test: Health endpoint should always pass, even with auth enabled
        let headers = HeaderMap::new();
        let auth_state = AuthState {
            config_api_key: Some("test-key".to_string()),
            admin_api_key: None,
            require_auth: true,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: None,
            cookie_key: Key::generate(),
        };

        let status = make_test_app_request("/health", headers, auth_state)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "Health endpoint should not require auth"
        );
    }

    #[tokio::test]
    async fn test_auth_disabled_allows_all() {
        // Test: When require_auth=false, all requests should pass
        let headers = HeaderMap::new();
        let auth_state = AuthState {
            config_api_key: None,
            admin_api_key: None,
            require_auth: false,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: None,
            cookie_key: Key::generate(),
        };

        let status = make_test_app_request("/protected", headers, auth_state)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "Requests should pass when auth is disabled"
        );
    }

    #[tokio::test]
    async fn test_auth_disabled_injects_privileged_identity() {
        use axum::{body::to_bytes, routing::get, Router};

        async fn handler(Extension(identity): Extension<ApiKeyIdentity>) -> String {
            format!("{:?}:{}", identity.key_type, identity.id.is_none())
        }

        let auth_state = AuthState {
            config_api_key: None,
            admin_api_key: None,
            require_auth: false,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: None,
            cookie_key: Key::generate(),
        };

        let app = Router::new().route("/protected", get(handler)).layer(
            axum::middleware::from_fn_with_state(auth_state, api_key_auth),
        );

        let response = app
            .oneshot(create_test_request("/protected", HeaderMap::new()))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(std::str::from_utf8(&body).unwrap(), "Privileged:true");
    }

    #[tokio::test]
    async fn test_auth_with_config_api_key() {
        // Test: Valid config API key should allow requests
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "config-secret-key".parse().unwrap());

        let auth_state = AuthState {
            config_api_key: Some("config-secret-key".to_string()),
            admin_api_key: None,
            require_auth: true,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: None,
            cookie_key: Key::generate(),
        };

        let status = make_test_app_request("/protected", headers, auth_state)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "Valid config API key should allow requests"
        );
    }

    #[tokio::test]
    async fn test_auth_with_admin_api_key() {
        // Test: Valid admin API key should allow requests
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "admin-secret-key".parse().unwrap());

        let auth_state = AuthState {
            config_api_key: None,
            admin_api_key: Some("admin-secret-key".to_string()),
            require_auth: true,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: None,
            cookie_key: Key::generate(),
        };

        let status = make_test_app_request("/protected", headers, auth_state)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "Valid admin API key should allow requests"
        );
    }

    #[tokio::test]
    async fn test_auth_with_invalid_api_key() {
        // Test: Invalid API key should be rejected
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "wrong-key".parse().unwrap());

        let auth_state = AuthState {
            config_api_key: Some("correct-key".to_string()),
            admin_api_key: None,
            require_auth: true,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: None,
            cookie_key: Key::generate(),
        };

        let status = make_test_app_request("/protected", headers, auth_state)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "Invalid API key should be rejected"
        );
    }

    #[tokio::test]
    async fn test_auth_missing_api_key_header() {
        // Test: Missing X-API-Key header should be rejected when auth is enabled
        let headers = HeaderMap::new();
        let auth_state = AuthState {
            config_api_key: Some("test-key".to_string()),
            admin_api_key: None,
            require_auth: true,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: None,
            cookie_key: Key::generate(),
        };

        let status = make_test_app_request("/protected", headers, auth_state)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "Missing API key header should be rejected"
        );
    }

    #[tokio::test]
    async fn test_auth_header_case_insensitive() {
        // Test: X-API-Key header should be case-insensitive

        for header_name in &["x-api-key", "X-API-KEY", "X-Api-Key"] {
            let mut headers = HeaderMap::new();
            headers.insert(*header_name, "test-key".parse().unwrap());

            let auth_state = AuthState {
                config_api_key: Some("test-key".to_string()),
                admin_api_key: None,
                require_auth: true,
                static_server_require_auth: false,
                vnc_require_auth: false,
                api_key_store: None,
                cookie_key: Key::generate(),
            };

            let status = make_test_app_request("/protected", headers, auth_state)
                .await
                .unwrap();
            assert_eq!(
                status,
                StatusCode::OK,
                "API key header should be case-insensitive"
            );
        }
    }

    #[tokio::test]
    async fn test_auth_with_empty_api_key() {
        // Test: Empty API key should not match non-empty expected key
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "".parse().unwrap());

        let auth_state = AuthState {
            config_api_key: Some("test-key".to_string()),
            admin_api_key: None,
            require_auth: true,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: None,
            cookie_key: Key::generate(),
        };

        let status = make_test_app_request("/protected", headers, auth_state)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "Empty API key should not match non-empty key"
        );
    }

    #[tokio::test]
    async fn test_auth_with_whitespace_api_key() {
        // Test: API keys with different whitespace should not match
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "test-key ".parse().unwrap());

        let auth_state = AuthState {
            config_api_key: Some("test-key".to_string()),
            admin_api_key: None,
            require_auth: true,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: None,
            cookie_key: Key::generate(),
        };

        let status = make_test_app_request("/protected", headers, auth_state)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "API key with trailing whitespace should not match"
        );
    }

    #[tokio::test]
    async fn test_auth_health_endpoint_with_auth_enabled() {
        // Test: Health endpoint should pass even when auth is enabled
        let headers = HeaderMap::new(); // No API key header
        let auth_state = AuthState {
            config_api_key: Some("test-key".to_string()),
            admin_api_key: None,
            require_auth: true,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: None,
            cookie_key: Key::generate(),
        };

        let status = make_test_app_request("/health", headers, auth_state)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "Health endpoint should pass without API key even when auth is enabled"
        );
    }

    #[tokio::test]
    async fn test_auth_special_characters_in_api_key() {
        // Test: API keys with special characters should work
        let special_keys = vec![
            "key-with-dashes",
            "key_with_underscores",
            "key.with.dots",
            "key/with/slashes",
            "key:with:colons",
            "key@with@ats",
        ];

        for key in special_keys {
            let mut headers = HeaderMap::new();
            headers.insert("x-api-key", key.parse().unwrap());

            let auth_state = AuthState {
                config_api_key: Some(key.to_string()),
                admin_api_key: None,
                require_auth: true,
                static_server_require_auth: false,
                vnc_require_auth: false,
                api_key_store: None,
                cookie_key: Key::generate(),
            };

            let status = make_test_app_request("/protected", headers, auth_state)
                .await
                .unwrap();
            assert_eq!(
                status,
                StatusCode::OK,
                "Special char API key should work: {}",
                key
            );
        }
    }

    #[tokio::test]
    async fn test_auth_long_api_key() {
        // Test: Long API keys should work
        let long_key =
            "very-long-api-key-with-many-characters-1234567890-abcdefghijklmnopqrstuvwxyz";

        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", long_key.parse().unwrap());

        let auth_state = AuthState {
            config_api_key: Some(long_key.to_string()),
            admin_api_key: None,
            require_auth: true,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: None,
            cookie_key: Key::generate(),
        };

        let status = make_test_app_request("/protected", headers, auth_state)
            .await
            .unwrap();
        assert_eq!(status, StatusCode::OK, "Long API key should work");
    }

    #[tokio::test]
    async fn test_auth_with_database_api_key_injects_identity() {
        use axum::{body::to_bytes, routing::get, Router};

        async fn handler(Extension(identity): Extension<ApiKeyIdentity>) -> String {
            format!("{}:{:?}", identity.id.unwrap(), identity.key_type)
        }

        let api_key_id = uuid::Uuid::new_v4();
        let auth_state = AuthState {
            config_api_key: None,
            admin_api_key: None,
            require_auth: true,
            static_server_require_auth: false,
            vnc_require_auth: false,
            api_key_store: Some(Arc::new(MockApiKeyStore {
                valid_key: "db-key".to_string(),
                api_key_id,
            })),
            cookie_key: Key::generate(),
        };

        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "db-key".parse().unwrap());

        let app = Router::new().route("/protected", get(handler)).layer(
            axum::middleware::from_fn_with_state(auth_state, api_key_auth),
        );

        let response = app
            .oneshot(create_test_request("/protected", headers))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            format!("{api_key_id}:Database")
        );
    }

    #[test]
    fn test_is_api_key_valid() {
        // Test the is_api_key_valid helper function
        assert!(is_api_key_valid(
            &Some("key".to_string()),
            &Some("key".to_string())
        ));
        assert!(is_api_key_valid(&None, &None));
        assert!(is_api_key_valid(&Some("key".to_string()), &None));
        assert!(!is_api_key_valid(&None, &Some("key".to_string())));
        assert!(!is_api_key_valid(
            &Some("wrong".to_string()),
            &Some("key".to_string())
        ));
    }
}
