// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Session Token API
//!
//! API endpoints for creating and validating session tokens.
//!
//! ## Endpoints
//!
//! - `POST /api/session-tokens` - Create a new session token
//! - `GET /api/session-tokens/{token}/validate` - Validate a session token

use axum::{
    extract::{Extension, Path as AxumPath, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    api::{
        auth::ApiKeyIdentity,
        errors::{ApiError, ErrorCode},
    },
    core::SandboxService,
    db::SessionTokenStore,
    session_token::SessionToken,
};

/// Session token API state
#[derive(Clone)]
pub struct SessionTokenApiState {
    /// Database pool for session token storage
    pub db_pool: deadpool_postgres::Pool,
    /// Sandbox service used for ownership checks before token creation
    pub sandbox_service: Arc<SandboxService>,
}

/// Request to create a session token
#[derive(Debug, Deserialize)]
pub struct CreateSessionTokenRequest {
    /// Sandbox ID
    pub sandbox_id: String,
    /// Service name (e.g., "vnc")
    pub service: String,
    /// Optional TTL in seconds (default: 300)
    #[serde(default = "default_ttl")]
    pub ttl_secs: u64,
}

fn default_ttl() -> u64 {
    300 // 5 minutes
}

async fn authorize_session_token_creation(
    sandbox_service: &SandboxService,
    identity: &ApiKeyIdentity,
    sandbox_id: &str,
) -> Result<uuid::Uuid, ApiError> {
    let sandbox_id = uuid::Uuid::parse_str(sandbox_id).map_err(|_| ApiError::Validation {
        message: "sandbox_id must be a valid UUID".to_string(),
        field: Some("sandbox_id".to_string()),
        code: ErrorCode::ValidationInvalidRequest,
    })?;
    sandbox_service
        .check_sandbox_ownership(identity, &sandbox_id)
        .await?;
    Ok(sandbox_id)
}

/// Response for session token creation
#[derive(Debug, Serialize)]
pub struct CreateSessionTokenResponse {
    /// The session token
    pub token: String,
    /// Expiration timestamp
    pub expires_at: String,
}

/// Response for session token validation
#[derive(Debug, Serialize)]
pub struct ValidateSessionTokenResponse {
    /// Whether the token is valid
    pub valid: bool,
    /// Sandbox ID (if valid)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_id: Option<String>,
    /// Service name (if valid)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
}

/// Create a new session token
///
/// # Arguments
///
/// * `State(state)` - API state with database pool
/// * `req` - Create session token request
///
/// # Returns
///
/// JSON response with the token and expiration time
///
/// # Errors
///
/// Returns 500 if database operation fails
pub async fn create_session_token(
    State(state): State<SessionTokenApiState>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Json(req): Json<CreateSessionTokenRequest>,
) -> Result<Json<CreateSessionTokenResponse>, ApiError> {
    let sandbox_id =
        authorize_session_token_creation(&state.sandbox_service, &identity, &req.sandbox_id)
            .await?;

    // Create session token
    let token = SessionToken::new(&sandbox_id.to_string(), &req.service, req.ttl_secs as i64);
    let token_str = token.token.clone();
    let expires_at = token.expires_at.to_rfc3339();

    // Store in database
    let store = crate::db::PostgresSessionTokenStore::new(state.db_pool.clone());
    store.create_session_token(&token).await.map_err(|e| {
        tracing::error!("Failed to create session token: {}", e);
        ApiError::Database {
            message: "Failed to create session token".to_string(),
            code: ErrorCode::DatabaseQueryFailed,
            source: Some(e),
        }
    })?;

    tracing::info!(
        sandbox_id = %sandbox_id,
        api_key_id = ?identity.id,
        service = %req.service,
        "Created session token"
    );

    Ok(Json(CreateSessionTokenResponse {
        token: token_str,
        expires_at,
    }))
}

/// Validate a session token
///
/// # Arguments
///
/// * `State(state)` - API state with database pool
/// * `Path(token)` - Session token to validate
///
/// # Returns
///
/// JSON response indicating validity and token details
pub async fn validate_session_token(
    State(state): State<SessionTokenApiState>,
    AxumPath(token): AxumPath<String>,
) -> Result<Json<ValidateSessionTokenResponse>, StatusCode> {
    let store = crate::db::PostgresSessionTokenStore::new(state.db_pool.clone());

    match store.get_session_token(&token).await {
        Ok(Some(st)) => {
            let valid = st.validate(&st.sandbox_id, &st.service);
            tracing::debug!(
                "Validated token for sandbox={}, service={}, valid={}",
                st.sandbox_id,
                st.service,
                valid
            );

            Ok(Json(ValidateSessionTokenResponse {
                valid,
                sandbox_id: if valid { Some(st.sandbox_id) } else { None },
                service: if valid { Some(st.service) } else { None },
            }))
        }
        Ok(None) => {
            tracing::debug!("Session token not found");
            Ok(Json(ValidateSessionTokenResponse {
                valid: false,
                sandbox_id: None,
                service: None,
            }))
        }
        Err(e) => {
            tracing::error!("Failed to validate session token: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::auth::{api_key_auth, AuthState};
    use crate::core::types::{ApiKeyType, ImageDetails, ImageSummary};
    use crate::core::manager::{ManagerResult, SandboxManager};
    use crate::core::types::{
        ActivityTracking, ContainerStats, Sandbox, SandboxConfig, SandboxInfo, SandboxState,
    };
    use crate::core::StateStore;
    use async_trait::async_trait;
    use axum::{
        body::{to_bytes, Body},
        http::{Method, Request},
        middleware::from_fn_with_state,
        routing::post,
        Router,
    };
    use deadpool_postgres::{Config as PgConfig, Runtime};
    use std::collections::HashMap;
    use tokio_postgres::NoTls;
    use tower::ServiceExt;

    #[test]
    fn test_default_ttl() {
        assert_eq!(default_ttl(), 300);
    }

    #[tokio::test]
    async fn test_authorize_session_token_creation_rejects_invalid_uuid() {
        let service = build_test_service();
        let identity = ApiKeyIdentity {
            id: Some(uuid::Uuid::new_v4()),
            key_type: ApiKeyType::Database,
        };

        let result = authorize_session_token_creation(&service, &identity, "not-a-uuid").await;
        assert!(matches!(
            result,
            Err(ApiError::Validation {
                code: ErrorCode::ValidationInvalidRequest,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn test_database_key_cannot_create_session_token_for_foreign_sandbox() {
        let key_a_id = uuid::Uuid::new_v4();
        let key_b_id = uuid::Uuid::new_v4();
        let sandbox_id = uuid::Uuid::new_v4();
        let state = Arc::new(StateStore::new());
        let service = build_test_service_with_state(state.clone());

        state
            .create_sandbox(Sandbox {
                id: sandbox_id,
                config: SandboxConfig {
                    image: "mock-image:latest".to_string(),
                    name: Some("owned-by-a".to_string()),
                    ..Default::default()
                },
                state: SandboxState::Running,
                container_id: Some("container-owned-by-a".to_string()),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                error_message: None,
                volume_mounts: vec![],
                activity: ActivityTracking {
                    last_api_activity: chrono::Utc::now(),
                    last_container_activity: None,
                    activity_count: 0,
                },
                inactivity_timeout_minutes: Some(30),
                deleted_at: None,
                deleted_by: None,
                api_key_id: Some(key_a_id),
            })
            .await
            .unwrap();

        let app = Router::new()
            .route("/session-tokens", post(create_session_token))
            .with_state(SessionTokenApiState {
                db_pool: test_pool(),
                sandbox_service: service,
            })
            .layer(from_fn_with_state(
                AuthState {
                    config_api_key: Some("admin-key".to_string()),
                    admin_api_key: None,
                    require_auth: true,
                    static_server_require_auth: false,
                    vnc_require_auth: false,
                    api_key_store: Some(Arc::new(MockApiKeyStore {
                        keys: HashMap::from([
                            ("key-a".to_string(), key_a_id),
                            ("key-b".to_string(), key_b_id),
                        ]),
                    })),
                    cookie_key: axum_extra::extract::cookie::Key::generate(),
                },
                api_key_auth,
            ));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/session-tokens")
                    .header("host", "localhost")
                    .header("x-api-key", "key-b")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "sandbox_id": sandbox_id,
                            "service": "vnc",
                            "ttl_secs": 300
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error_code"], "SANDBOX_NOT_FOUND");
    }

    fn build_test_service() -> Arc<SandboxService> {
        build_test_service_with_state(Arc::new(StateStore::new()))
    }

    fn build_test_service_with_state(state: Arc<StateStore>) -> Arc<SandboxService> {
        Arc::new(SandboxService::new(Arc::new(MockSandboxManager), state))
    }

    fn test_pool() -> deadpool_postgres::Pool {
        let mut cfg = PgConfig::new();
        cfg.host = Some("127.0.0.1".to_string());
        cfg.port = Some(1);
        cfg.dbname = Some("dsb".to_string());
        cfg.user = Some("postgres".to_string());
        cfg.password = Some("postgres".to_string());
        cfg.create_pool(Some(Runtime::Tokio1), NoTls)
            .expect("failed to create test pool")
    }

    struct MockApiKeyStore {
        keys: HashMap<String, uuid::Uuid>,
    }

    #[async_trait]
    impl crate::db::ApiKeyStore for MockApiKeyStore {
        async fn validate_api_key(
            &self,
            key: &str,
        ) -> Result<Option<uuid::Uuid>, Box<dyn std::error::Error + Send + Sync>> {
            Ok(self.keys.get(key).copied())
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

    struct MockSandboxManager;

    #[async_trait]
    impl SandboxManager for MockSandboxManager {
        async fn create(
            &self,
            sandbox_id: Option<&uuid::Uuid>,
            _config: &SandboxConfig,
        ) -> ManagerResult<String> {
            Ok(format!(
                "container-{}",
                sandbox_id.copied().unwrap_or_else(uuid::Uuid::new_v4)
            ))
        }

        async fn start(&self, _id: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn stop(&self, _id: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn delete(&self, _id: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn exec(&self, _id: &str, _cmd: Vec<String>) -> ManagerResult<String> {
            Ok("ok".to_string())
        }

        async fn stats(&self, _id: &str) -> ManagerResult<ContainerStats> {
            Ok(ContainerStats {
                cpu_percent: 0.0,
                memory_usage_mb: 0,
                memory_limit_mb: 0,
                memory_percent: 0.0,
                network_rx_bytes: 0,
                network_tx_bytes: 0,
                block_read_bytes: 0,
                block_write_bytes: 0,
                timestamp: chrono::Utc::now(),
            })
        }

        async fn is_running(&self, _id: &str) -> ManagerResult<bool> {
            Ok(true)
        }

        async fn get_exit_info(&self, _id: &str) -> ManagerResult<(i64, bool)> {
            Ok((0, false))
        }

        async fn get_workdir(&self, _id: &str) -> ManagerResult<String> {
            Ok("/workspace".to_string())
        }

        async fn list(
            &self,
            _all: bool,
            _filters: Option<HashMap<String, Vec<String>>>,
        ) -> ManagerResult<Vec<SandboxInfo>> {
            Ok(vec![])
        }

        async fn remove_volume(&self, _name: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn get_image_features(&self, image: &str) -> ManagerResult<ImageDetails> {
            Ok(ImageDetails {
                id: image.to_string(),
                repo_tags: vec![image.to_string()],
                size: 0,
                virtual_size: 0,
                created: 0,
                architecture: "amd64".to_string(),
                os: "linux".to_string(),
                labels: None,
                env: None,
                features: vec![],
            })
        }

        async fn list_images(&self) -> ManagerResult<Vec<ImageSummary>> {
            Ok(vec![])
        }

        async fn pull_image(&self, _image: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn pull_image_with_progress(
            &self,
            _image: &str,
            _callback: Box<dyn FnMut(String, Option<u64>, Option<u64>) + Send + 'static>,
        ) -> ManagerResult<()> {
            Ok(())
        }

        async fn delete_image(&self, _id: &str) -> ManagerResult<()> {
            Ok(())
        }

        async fn image_exists(&self, _image: &str) -> ManagerResult<bool> {
            Ok(true)
        }

        async fn exec_http(
            &self,
            _id: &str,
            _path: &str,
            _method: &str,
            _body: Option<serde_json::Value>,
            _timeout_secs: Option<u64>,
        ) -> ManagerResult<serde_json::Value> {
            Ok(serde_json::json!({}))
        }

        async fn exec_with_stdin(
            &self,
            _id: &str,
            _cmd: Vec<String>,
            _stdin: Option<String>,
            _timeout_secs: Option<u64>,
        ) -> ManagerResult<String> {
            Ok("ok".to_string())
        }

        async fn upload_archive(
            &self,
            _id: &str,
            _path: &str,
            _tar_data: Vec<u8>,
        ) -> ManagerResult<()> {
            Ok(())
        }
    }
}
