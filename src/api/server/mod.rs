// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use axum::{
    extract::Extension,
    middleware,
    routing::{delete, get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::info;

use crate::api::auth::{api_key_auth, AuthState};
use crate::api::handlers::admin::{admin_auth_middleware, AdminState};
use crate::api::handlers::ssh::{
    authorize_ssh_access, create_ssh_session, get_ssh_session, get_ssh_session_statistics,
    list_ssh_sessions, terminate_ssh_session, update_session_activity,
};
use crate::api::handlers::static_files::{
    delete_sandbox_static_files, delete_static_file, download_sandbox_files_as_zip,
    list_sandbox_directory_tree, list_static_files, serve_static_file,
};
use crate::api::handlers::{
    cleanup_inactive_sandboxes, cleanup_sandbox, create_api_key, create_sandbox,
    create_sandbox_stream, delete_api_key, delete_image, delete_sandbox, download_file,
    exec_sandbox, execute_tool, get_activity, get_api_key, get_config, get_sandbox,
    get_sandbox_stats, health_check, inspect_image, list_activities, list_api_keys, list_images,
    list_sandbox_activities, list_sandboxes, pull_image, pull_image_stream, restore_sandbox,
    rotate_api_key, stop_sandbox, stream_sandbox_stats, upload_file,
};
use crate::api::logging::request_logging_middleware;
use crate::api::session_tokens::{
    create_session_token, validate_session_token, SessionTokenApiState,
};
use crate::config::BackendType;
use crate::config::Config;
use crate::core::manager::SandboxManager;
use crate::core::state::StateStore;
use crate::core::store_trait::StateStoreTrait;
use crate::core::{ActivityService, SandboxService, SshSessionService, StaticFileService};
use crate::db::{ApiKeyStore, PostgresApiKeyStore};
use crate::docker::manager::DockerManager;
use crate::vnc_proxy::{vnc_websocket, VncProxyState};
use crate::web_terminal::{terminal_page, terminal_websocket, WebTerminalState};

/// Starts the API server with the given configuration.
///
/// This function initializes all services and starts the HTTP API server:
/// - Selects backend (Docker or Kubernetes) based on config
/// - Creates the appropriate SandboxManager implementation
/// - Initializes state store (PostgreSQL or in-memory)
/// - Creates activity service (if PostgreSQL enabled)
/// - Creates sandbox service
/// - Creates SSH session service (requires PostgreSQL)
/// - Starts background tasks (cleanup, monitoring)
/// - Configures all HTTP routes
/// - Binds to configured port and serves
///
/// # Arguments
///
/// * `config` - Application configuration
///
/// # Returns
///
/// * `Ok(())` - Server started successfully
/// * `Err(...)` - Failed to start server
///
/// # Example
///
/// ```rust,no_run,ignore
/// # use dsb::api::start_server;
/// # use dsb::config;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// let config = config::load()?;
/// start_server(&config).await?;
/// # Ok(())
/// # }
/// ```
///
/// # Testing Strategy
///
/// The `start_server` function is tested in:
/// - **Unit tests** (`src/api/server/tests.rs`): 78 tests covering configuration,
///   route structure, type checking, and compile-time verification
/// - **Integration tests** (`tests/integration_test.rs`): Full E2E tests that
///   start the actual server with PostgreSQL and Docker, then make HTTP requests
///
/// Unit tests focus on:
/// - Configuration structure validation
/// - Route path verification
/// - Type trait bounds (Send + Sync, Arc wrapping)
/// - Error message formatting
/// - Compile-time function existence checks
///
/// Integration tests cover:
/// - Server startup and binding
/// - Route handler execution
/// - Service initialization
/// - Background task spawning
/// - Full request/response cycles
pub async fn start_server(config: &Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _start = std::time::Instant::now();
    tracing::info!("🚀 Starting DSB server initialization...");

    // Select backend based on configuration
    let backend: Arc<dyn SandboxManager> = match config.sandbox.backend {
        BackendType::Docker => {
            let docker = DockerManager::new_with_config(config)?;
            tracing::info!(
                "  ✓ Docker backend created ({:.2}s)",
                _start.elapsed().as_secs_f64()
            );
            Arc::new(docker)
        }
        BackendType::Kubernetes => {
            #[cfg(feature = "kubernetes")]
            {
                use crate::k8s::manager::KubernetesManager;
                use crate::k8s::operator::SandboxOperator;

                tracing::info!("Kubernetes backend selected, initializing...");

                // Create kube-rs Client (uses in-cluster config or kubeconfig)
                let k8s_client = kube::Client::try_default()
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                        format!(
                            "Failed to create K8s client: {}. Ensure kubeconfig or in-cluster service account is configured.",
                            e
                        )
                        .into()
                    })?;

                let namespace = config.sandbox.kubernetes.namespace.clone();

                // Create KubernetesManager
                let k8s_manager =
                    KubernetesManager::new(k8s_client.clone(), Arc::new(config.clone()));
                let backend: Arc<dyn SandboxManager> = Arc::new(k8s_manager);

                // Start the operator as a background task.
                //
                // NOTE: The operator is currently fire-and-forget. If the operator
                // task panics or fails, the error is logged via the controller's
                // error_policy (which requeues with backoff). However, the server
                // does not currently track the JoinHandle for failure monitoring.
                //
                // Future improvement: store _operator_handle in server state and
                // monitor it for panics, surfacing failures to the logs and/or
                // metrics endpoint.
                let operator = SandboxOperator::new(Arc::new(KubernetesManager::new(
                    k8s_client.clone(),
                    Arc::new(config.clone()),
                )));
                let _operator_handle = operator.start();

                tracing::info!(
                    "  ✓ K8s backend initialized in namespace '{}', operator started ({:.2}s)",
                    namespace,
                    _start.elapsed().as_secs_f64()
                );

                backend
            }
            #[cfg(not(feature = "kubernetes"))]
            {
                tracing::error!(
                    "Kubernetes backend selected but 'kubernetes' feature is not enabled. \
                     Recompile with --features kubernetes"
                );
                return Err(
                    "Kubernetes feature not enabled. Recompile with --features kubernetes".into(),
                );
            }
        }
    };

    // Determine if we should use PostgreSQL based on config
    let has_db_config = config.database.url.is_some() || config.database.password.is_some();

    // Use PostgreSQL store if configured, otherwise fall back to in-memory store
    #[allow(clippy::type_complexity)]
    let (state, activity_service, api_key_store, pg_state_for_cleanup): (
        Arc<dyn StateStoreTrait + Send + Sync>,
        Option<Arc<ActivityService>>,
        Option<Arc<dyn ApiKeyStore>>,
        Option<Arc<crate::db::PostgresStateStore>>,
    ) = if has_db_config {
        let _db_start = std::time::Instant::now();
        info!("╔════════════════════════════════════════════════════════════╗");
        info!("║  Storage Backend: PostgreSQL (Persistent)                  ║");
        info!("╚════════════════════════════════════════════════════════════╝");
        info!("");

        info!("Ensuring database exists...");
        tracing::info!("  📊 Database existence check started...");

        // Build database URL from config
        let database_url = if let Some(url) = &config.database.url {
            url.clone()
        } else {
            format!(
                "postgresql://{}:{}@{}:{}/{}",
                config.database.user,
                config.database.password.as_ref().ok_or(
                    "Database password is required (set database.password or database.url)"
                )?,
                config.database.host,
                config.database.port,
                config.database.name
            )
        };

        crate::db::migration::ensure_database_exists(&database_url)
            .await
            .map_err(|e| format!("Failed to ensure database exists: {}", e))?;
        tracing::info!(
            "  ✓ Database check completed ({:.2}s)",
            _db_start.elapsed().as_secs_f64()
        );
        info!("");

        info!("Connecting to PostgreSQL...");
        let _pool_start = std::time::Instant::now();

        // Create connection pool from config
        let pool = crate::db::pool::create_pool_from_config(config)
            .await
            .map_err(|e| format!("Failed to create database pool: {}", e))?;

        tracing::info!(
            "  ✓ Connection pool created ({:.2}s)",
            _pool_start.elapsed().as_secs_f64()
        );

        // Run migrations
        let _migration_start = std::time::Instant::now();
        info!("Running database migrations...");
        crate::db::migration::run_migrations(&pool).await?;
        tracing::info!(
            "  ✓ Migrations completed ({:.2}s)",
            _migration_start.elapsed().as_secs_f64()
        );

        // Create PostgresStateStore
        let store = crate::db::PostgresStateStore::new(pool.clone()).await?;
        info!("✓ PostgreSQL state store initialized");
        info!("");

        // Create ActivityService
        let act_service = Arc::new(ActivityService::new(pool.clone()));
        info!("✓ Activity tracking service initialized");

        // Create API key store
        let key_store = Arc::new(PostgresApiKeyStore::new(pool.clone())) as Arc<dyn ApiKeyStore>;
        info!("✓ API key store initialized");

        // Database diagnostics - show what we're connected to
        info!("");
        info!("═══════════════════════════════════════════════════════════");
        info!("📊 Database Diagnostics");
        info!("═══════════════════════════════════════════════════════════");

        // Show the database URL (with password masked)
        let masked_url = database_url.replacen(
            format!(
                ":{}@",
                config.database.password.as_ref().unwrap_or(&String::new())
            )
            .as_str(),
            ":****@",
            1,
        );
        info!("  Database URL: {}", masked_url);

        // Query and log the number of sandboxes in the database
        let sandbox_count: i64 = match pool.get().await {
            Ok(client) => {
                match client
                    .query_one(
                        "SELECT COUNT(*) FROM sandboxes WHERE deleted_at IS NULL",
                        &[],
                    )
                    .await
                {
                    Ok(row) => row.get(0),
                    Err(e) => {
                        tracing::warn!("Failed to query sandbox count: {}", e);
                        0
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to get database client: {}", e);
                0
            }
        };

        info!("  Active sandboxes: {}", sandbox_count);

        if sandbox_count == 0 {
            info!("");
            info!("  ℹ️  Database is empty - this is normal for first-time startup");
            info!("  📝 If you expected to see existing sandboxes, check:");
            info!("     1. You're connecting to the correct PostgreSQL volume");
            info!("     2. Run: docker volume ls to see all available volumes");
            info!("     3. Check docker/.env for POSTGRES_VOLUME_NAME");
        } else {
            // Show the most recent sandbox
            if let Ok(client) = pool.get().await {
                if let Ok(row) = client.query_one(
                    "SELECT name, state, created_at FROM sandboxes WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
                    &[]
                ).await {
                    let name: Option<String> = row.get("name");
                    let state: String = row.get("state");
                    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
                    info!("");
                    info!("  Most recent sandbox:");
                    info!("    Name: {} | State: {} | Created: {}", name.unwrap_or_else(|| "(unnamed)".to_string()), state, created_at.format("%Y-%m-%d %H:%M:%S UTC"));
                }
            }
        }
        info!("═══════════════════════════════════════════════════════════");
        info!("");

        let pg_state_for_cleanup = Arc::new(store.clone());
        let state_trait: Arc<dyn StateStoreTrait + Send + Sync> = pg_state_for_cleanup.clone();

        (
            state_trait,
            Some(act_service),
            Some(key_store),
            Some(pg_state_for_cleanup),
        )
    } else {
        info!("╔════════════════════════════════════════════════════════════╗");
        info!("║  Storage Backend: In-Memory (Ephemeral)                    ║");
        info!("╚════════════════════════════════════════════════════════════╝");
        info!("");
        info!("⚠️  Using in-memory storage - data will be lost on restart");
        info!("    To enable persistent storage, configure database settings:");
        info!("    - Set database.url in config file");
        info!("    - Or set database.password with individual database settings");
        info!("");
        info!("    Example environment variables:");
        info!("    export DSB_DATABASE__URL=\"postgresql://user:pass@localhost:5432/dsb\"");
        info!("    - OR -");
        info!("    export DSB_DATABASE__PASSWORD=\"your-password\"");
        info!("");
        info!("⚠️  API key management disabled (requires PostgreSQL)");
        info!("    Only admin and config API keys will work");
        info!("");

        let state_trait: Arc<dyn StateStoreTrait + Send + Sync> = Arc::new(StateStore::new());

        (state_trait, None, None, None)
    };

    // Create SandboxService with or without activity tracking
    let max_file_size_bytes =
        (config.static_server.sandbox_upload_max_file_size_mb as u64) * 1024 * 1024;
    let service = if let Some(act_service) = activity_service {
        Arc::new(
            SandboxService::new_with_activity(backend.clone(), state, act_service)
                .with_cleanup_config(
                    config.sandbox.default_inactivity_timeout,
                    config.sandbox.cleanup_dry_run,
                    config.sandbox.state_monitor_interval,
                    config.sandbox.deleted_sandbox_retention_days,
                )
                .with_frontend_config(
                    config.docker.default_image.clone(),
                    config.server.require_auth,
                )
                .with_file_upload_config(max_file_size_bytes)
                .with_resource_limits(config.sandbox.default_resource_limits.clone())
                .with_max_browser_tabs(config.sandbox.max_browser_tabs),
        )
    } else {
        Arc::new(
            SandboxService::new(backend.clone(), state)
                .with_cleanup_config(
                    config.sandbox.default_inactivity_timeout,
                    config.sandbox.cleanup_dry_run,
                    config.sandbox.state_monitor_interval,
                    config.sandbox.deleted_sandbox_retention_days,
                )
                .with_frontend_config(
                    config.docker.default_image.clone(),
                    config.server.require_auth,
                )
                .with_file_upload_config(max_file_size_bytes)
                .with_resource_limits(config.sandbox.default_resource_limits.clone())
                .with_max_browser_tabs(config.sandbox.max_browser_tabs),
        )
    };

    // Create SSH session service (only if PostgreSQL is enabled)
    let ssh_service: Arc<SshSessionService> = if has_db_config {
        info!("SSH session management enabled");

        // Get the SSH session store from the database
        let ssh_store = {
            let pool = crate::db::pool::create_pool_from_config(config)
                .await
                .map_err(|e| format!("Failed to create database pool: {}", e))?;
            Arc::new(crate::db::PostgresSshSessionStore::new(pool))
                as Arc<dyn crate::db::SshSessionStoreTrait>
        };

        // Create SSH service with sandbox validation
        let ssh_svc = Arc::new(SshSessionService::new_with_sandbox_service(
            ssh_store,
            service.clone(),
        ));

        // Start SSH session cleanup task
        // - 5 minute idle timeout
        // - 30 second timeout for sessions stuck in connecting state
        // - Check every 60 seconds
        ssh_svc.clone().start_cleanup_task(300, 30, 60);

        ssh_svc
    } else {
        info!("SSH session management disabled (requires PostgreSQL)");
        // SSH routes won't be available without PostgreSQL
        // Create a no-op store
        let noop_ssh_store =
            Arc::new(crate::db::NoopSshSessionStore) as Arc<dyn crate::db::SshSessionStoreTrait>;
        Arc::new(SshSessionService::new(noop_ssh_store))
    };

    // Start auto-cleanup background task
    service.clone().start_auto_cleanup_task();

    // Start state monitor background task
    service.clone().start_state_monitor_task();

    // Start orphan cleanup background task
    service.clone().start_orphan_cleanup_task();

    // Start expired deletion cleanup task (only if using PostgreSQL)
    if let Some(ref pg_state) = pg_state_for_cleanup {
        let retention_days = config.sandbox.deleted_sandbox_retention_days;
        let pg_state_clone = Arc::clone(pg_state);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600)); // Every hour

            tracing::info!(
                "Expired deletion cleanup task started: retention_period={} days",
                retention_days
            );

            loop {
                interval.tick().await;
                match pg_state_clone
                    .cleanup_expired_sandboxes(retention_days as i64)
                    .await
                {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!(
                                "Permanently deleted {} expired sandboxes (older than {} days)",
                                count,
                                retention_days
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to cleanup expired sandboxes: {}", e);
                    }
                }
            }
        });
    }

    // Startup recovery: recover sandboxes stuck in "Creating" state
    if let Some(ref pg_state) = pg_state_for_cleanup {
        let pg_state_clone = Arc::clone(pg_state);
        tokio::spawn(async move {
            const STUCK_SANDBOX_TIMEOUT_SECS: i64 = 300; // 5 minutes

            tracing::info!("Running startup recovery for stuck sandboxes...");

            match pg_state_clone
                .recover_stuck_sandboxes(STUCK_SANDBOX_TIMEOUT_SECS)
                .await
            {
                Ok((recovered, failed)) => {
                    if recovered > 0 || failed > 0 {
                        tracing::info!(
                            "Startup recovery completed: {} recovered to Running, {} marked as Failed",
                            recovered,
                            failed
                        );
                    } else {
                        tracing::info!("Startup recovery completed: no stuck sandboxes found");
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to recover stuck sandboxes during startup: {}", e);
                }
            }
        });
    }

    // K8s startup reconciliation: sync DB state with K8s cluster state
    // This is separate from the operator (which watches CRDs for changes).
    // Startup reconciliation verifies that DB records match actual K8s resources.
    #[cfg(feature = "kubernetes")]
    if matches!(config.sandbox.backend, BackendType::Kubernetes) {
        if let Some(ref pg_state) = pg_state_for_cleanup {
            let backend_clone = Arc::clone(&backend);
            let pg_state_clone = Arc::clone(pg_state);
            let namespace = config.sandbox.kubernetes.namespace.clone();
            tokio::spawn(async move {
                reconcile_k8s_state(backend_clone, pg_state_clone, &namespace).await;
            });
        }
    }

    // Start orphaned container cleanup task (cleans up containers for destroyed sandboxes)
    if has_db_config {
        let service_clone = Arc::clone(&service);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300)); // Every 5 minutes

            tracing::info!("Orphaned container cleanup task started: interval=5min");

            loop {
                interval.tick().await;
                match service_clone.cleanup_destroyed_containers().await {
                    Ok(removed) => {
                        if removed > 0 {
                            tracing::info!("Cleaned up {} orphaned containers", removed);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to cleanup destroyed containers: {}", e);
                    }
                }
            }
        });
    }

    // Start session token cleanup task (only if using PostgreSQL)
    if has_db_config {
        let pool = crate::db::pool::create_pool_from_config(config)
            .await
            .map_err(|e| {
                format!(
                    "Failed to create database pool for session token cleanup: {}",
                    e
                )
            })?;

        let cleanup_interval = config.server.session_token_ttl_secs;
        tokio::spawn(async move {
            tracing::info!(
                "Session token cleanup task started: cleanup_interval={}s",
                cleanup_interval
            );
            crate::tasks::session_token_cleanup_task(pool, cleanup_interval).await;
        });
    }

    // Create session token routes (only if PostgreSQL is enabled)
    let session_token_routes = if has_db_config {
        let pool = crate::db::pool::create_pool_from_config(config)
            .await
            .map_err(|e| {
                format!(
                    "Failed to create database pool for session token routes: {}",
                    e
                )
            })?;

        let session_token_state = SessionTokenApiState {
            db_pool: pool,
            sandbox_service: service.clone(),
        };
        Some(
            Router::new()
                .route("/session-tokens", post(create_session_token))
                .route(
                    "/session-tokens/{token}/validate",
                    get(validate_session_token),
                )
                .with_state(session_token_state),
        )
    } else {
        None
    };

    // Create SSH routes - SSH session endpoints use ssh_service, authorization uses sandbox_service
    let ssh_session_routes = Router::new()
        .route(
            "/ssh-sessions",
            post(create_ssh_session).get(list_ssh_sessions),
        )
        .route("/ssh-sessions/{id}", get(get_ssh_session))
        .route("/ssh-sessions/{id}/terminate", post(terminate_ssh_session))
        .route(
            "/ssh-sessions/{id}/heartbeat",
            post(update_session_activity),
        )
        .route("/ssh-sessions/statistics", get(get_ssh_session_statistics))
        .with_state(ssh_service.clone());

    // SSH auth routes with API key from config
    let ssh_api_key = config
        .server
        .ssh_gateway_api_key
        .clone()
        .or(config.server.api_key.clone());
    use crate::api::handlers::ssh::SshAuthState;
    let ssh_auth_state = SshAuthState {
        service: service.clone(),
        api_key: ssh_api_key,
    };
    let ssh_auth_routes = Router::new()
        .route("/ssh/authorize/{sandbox_id}", get(authorize_ssh_access))
        .with_state(ssh_auth_state);

    // Merge SSH routes
    let ssh_routes = ssh_session_routes.merge(ssh_auth_routes);

    // Create web terminal routes using the SandboxManager backend
    let _terminal_start = std::time::Instant::now();
    let terminal_routes = {
        tracing::info!(
            "  ✓ Web terminal using SandboxManager backend ({:.2}s)",
            _terminal_start.elapsed().as_secs_f64()
        );
        let terminal_api_key = config
            .server
            .web_terminal_api_key
            .clone()
            .or(config.server.api_key.clone());
        let terminal_state =
            WebTerminalState::new(backend.clone(), terminal_api_key, Arc::new(config.clone()));
        Router::new()
            .route("/terminal", get(terminal_page))
            .route("/terminal/{sandbox_id}", get(terminal_websocket))
            .with_state(terminal_state)
    };

    // Create VNC proxy routes using the SandboxManager backend
    let _vnc_start = std::time::Instant::now();
    let vnc_routes = {
        tracing::info!(
            "  ✓ VNC proxy using SandboxManager backend ({:.2}s)",
            _vnc_start.elapsed().as_secs_f64()
        );
        let vnc_api_key = config
            .server
            .vnc_api_key
            .clone()
            .or(config.server.web_terminal_api_key.clone())
            .or(config.server.api_key.clone());

        // Create VNC state with optional database pool for session token support
        let vnc_state = if has_db_config {
            // Create a new pool for VNC session token validation
            match crate::db::pool::create_pool_from_config(config).await {
                Ok(pool) => {
                    VncProxyState::new(backend.clone(), vnc_api_key, Arc::new(config.clone()))
                        .with_db_pool(pool)
                }
                Err(e) => {
                    tracing::warn!("Failed to create pool for VNC session tokens: {}, using VNC without session token support", e);
                    VncProxyState::new(backend.clone(), vnc_api_key, Arc::new(config.clone()))
                }
            }
        } else {
            VncProxyState::new(backend.clone(), vnc_api_key, Arc::new(config.clone()))
        };

        Router::new()
            .route("/vnc/{sandbox_id}", get(vnc_websocket))
            .with_state(vnc_state)
    };

    // Create static file service
    // In K8s mode, the backend is provided so file operations are proxied
    // through exec commands into sandbox pods. In Docker mode, the backend
    // is still provided but static files are read from the shared bind mount.
    let static_file_service = Arc::new(StaticFileService::new_with_backend(
        Arc::new(config.clone()),
        service.clone(),
    ));
    let static_routes = Router::new()
        .route("/static/{sandbox_id}/{*file_path}", get(serve_static_file))
        .route("/static/files/{sandbox_id}", get(list_static_files))
        .route(
            "/static/tree/{sandbox_id}",
            get(list_sandbox_directory_tree),
        )
        .route(
            "/static/file/{sandbox_id}/{file_path}",
            delete(delete_static_file),
        )
        .route(
            "/static/sandbox/{sandbox_id}",
            delete(delete_sandbox_static_files),
        )
        .route(
            "/static/download/{sandbox_id}",
            get(download_sandbox_files_as_zip),
        )
        .with_state((static_file_service, service.clone()));

    // Create image routes with SandboxManager as state
    let image_routes = Router::new()
        .route("/images", get(list_images))
        .route("/images/pull", post(pull_image))
        .route("/images/pull-stream", post(pull_image_stream))
        .route("/images/{id}", get(inspect_image).delete(delete_image))
        .with_state(backend.clone());

    // Create authentication state for main API
    // Supports three authentication sources:
    // 1. Admin API key (for admin operations)
    // 2. Database API keys (if PostgreSQL enabled)
    // 3. Legacy config API key (backward compatibility)
    let cookie_key = if let Some(secret) = &config.server.cookie_secret {
        axum_extra::extract::cookie::Key::try_from(secret.as_bytes())
            .unwrap_or_else(|_| axum_extra::extract::cookie::Key::generate())
    } else {
        axum_extra::extract::cookie::Key::generate()
    };

    let auth_state = AuthState {
        config_api_key: config.server.api_key.clone(),
        admin_api_key: config.server.admin_api_key.clone(),
        require_auth: config.server.require_auth,
        static_server_require_auth: config.static_server.require_auth,
        vnc_require_auth: config.server.vnc_require_auth,
        api_key_store: api_key_store.clone(),
        cookie_key,
    };

    // Create admin state for admin API endpoints
    // Admin endpoints require the admin API key (not database or config keys)
    let admin_state = if let Some(ref key_store) = api_key_store {
        Some(AdminState {
            api_key_store: key_store.clone(),
            admin_api_key: config.server.admin_api_key.clone(),
        })
    } else {
        info!("⚠️  Admin API endpoints disabled (requires PostgreSQL)");
        None
    };

    // Create admin routes with admin-only authentication middleware
    let admin_routes = if let Some(state) = admin_state {
        info!("✓ Admin API endpoints enabled");
        Some(
            Router::new()
                .route("/admin/api-keys", post(create_api_key).get(list_api_keys))
                .route(
                    "/admin/api-keys/{id}",
                    get(get_api_key).delete(delete_api_key),
                )
                .route("/admin/api-keys/{id}/rotate", post(rotate_api_key))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    admin_auth_middleware,
                ))
                .with_state(state),
        )
    } else {
        None
    };

    // Build main application router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/config", get(get_config))
        .route("/sandboxes", get(list_sandboxes).post(create_sandbox))
        .route("/sandboxes/create-stream", post(create_sandbox_stream))
        .route("/sandboxes/{id}", get(get_sandbox).delete(delete_sandbox))
        .route("/sandboxes/{id}/restore", post(restore_sandbox))
        .route("/sandboxes/{id}/stop", post(stop_sandbox))
        .route("/sandboxes/{id}/exec", post(exec_sandbox))
        .route("/sandboxes/{id}/tools", post(execute_tool))
        .route("/sandboxes/{id}/upload", post(upload_file))
        .route("/sandboxes/{id}/download", get(download_file))
        .route("/sandboxes/{id}/stats", get(get_sandbox_stats))
        .route("/sandboxes/{id}/stats-stream", get(stream_sandbox_stats))
        .route("/sandboxes/{id}/cleanup", post(cleanup_sandbox))
        .route("/activities", get(list_activities))
        .route("/activities/cleanup-all", post(cleanup_inactive_sandboxes))
        .route("/activities/{id}", get(get_activity))
        .route("/sandboxes/{id}/activities", get(list_sandbox_activities))
        .merge(ssh_routes); // Merge SSH routes with their own state

    // Merge terminal routes
    let app = app.merge(terminal_routes);

    // Merge VNC routes
    let app = app.merge(vnc_routes);

    let app = app
        .merge(static_routes) // Merge static file routes
        .merge(image_routes); // Merge image routes with their own state

    // Merge session token routes (if enabled)
    let app = if let Some(routes) = session_token_routes {
        app.merge(routes)
    } else {
        app
    };

    // Merge admin routes (if enabled)
    let mut app = if let Some(routes) = admin_routes {
        app.merge(routes)
    } else {
        app
    };

    // Apply API key authentication middleware to all routes
    // The middleware will:
    // - Skip /health endpoint (always accessible)
    // - Allow all requests if require_auth=false (dev mode)
    // - Check API keys against: admin key → database keys → legacy config key
    app = app.layer(middleware::from_fn_with_state(
        auth_state.clone(),
        api_key_auth,
    ));

    // Apply HTTP request logging middleware AFTER auth (runs FIRST due to middleware reversal)
    // This logs ALL requests including auth failures, because it wraps the auth middleware
    // Middleware order on request: Logging → Auth → Handler
    // Middleware order on response: Handler → Auth → Logging
    app = app.layer(middleware::from_fn(request_logging_middleware));

    // Register auth routes
    let auth_routes = Router::new()
        .route("/api/auth/login", post(crate::api::auth::login))
        .route("/api/auth/logout", post(crate::api::auth::logout))
        .route("/api/auth/me", get(crate::api::auth::me))
        .layer(Extension(auth_state.clone()));

    // Merge auth routes INTO main app BEFORE applying CORS so they get the global CORS config
    // The auth routes do NOT get the `api_key_auth` middleware because it was applied to `app`
    // before the merge. (Actually, `api_key_auth` skips `/api/auth/login` manually anyway).
    app = app.merge(auth_routes);

    // Apply CORS middleware to allow cross-origin requests from dashboard
    // Must be applied AFTER auth middleware so it runs FIRST (middleware is applied in reverse order)
    let cors = CorsLayer::new()
        .allow_origin(Any) // In production, specify exact origins
        .allow_methods(Any)
        .allow_headers(Any);

    app = app.layer(cors);

    // Serve dashboard static files
    // This serves the compiled dashboard SPA from dashboard/dist
    // - /dashboard/* routes serve the dashboard
    // - Fallback to index.html for SPA routing (handles non-existent paths)
    let dashboard_service = ServeDir::new("dashboard/dist").fallback(
        tower_http::services::ServeFile::new("dashboard/dist/index.html"),
    );
    app = app.nest_service("/dashboard", dashboard_service);

    // Increase default body limit from 2MB to 100MB so large file uploads
    // via multipart forms work correctly (matches DSB_STATIC_SERVER__MAX_FILE_SIZE_MB).
    app = app.layer(axum::extract::DefaultBodyLimit::max(100 * 1024 * 1024));

    // Apply error handler layer for pretty error pages
    // HTML for dashboard/static routes, JSON for API routes
    use crate::api::error_handler::ErrorHandlerLayer;
    app = app.layer(ErrorHandlerLayer::new());

    // Attach the sandbox service state
    let app = app.with_state(service);

    let addr = {
        let host: std::net::IpAddr = config.server.host.parse().expect(
            "Invalid DSB_SERVER__HOST: must be valid IP address (e.g., 0.0.0.0 or 127.0.0.1)",
        );
        SocketAddr::from((host, config.server.port))
    };

    let _bind_start = std::time::Instant::now();
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(
        "  ✓ TCP listener bound to {} ({:.2}s)",
        addr,
        _bind_start.elapsed().as_secs_f64()
    );

    tracing::info!(
        "✅ DSB server initialization completed in {:.2}s",
        _start.elapsed().as_secs_f64()
    );
    info!("API server listening on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Reconciles DB state with K8s cluster state at startup.
///
/// This is DIFFERENT from the operator (which watches CRDs for changes).
/// Startup reconciliation verifies that database records match actual K8s resources:
///
/// 1. For each DB sandbox in "Running" state: verify the corresponding CRD/Pod exists
/// 2. If CRD/Pod is missing: mark sandbox as "Stopped" with reason "Pod not found during startup"
/// 3. For each K8s CRD without a DB record: log as orphan
///
/// This handles scenarios like:
/// - K8s cluster was reset while DSB server was down
/// - Pods were manually deleted by a K8s admin
/// - DSB server restarted after a crash
#[cfg(feature = "kubernetes")]
async fn reconcile_k8s_state(
    backend: Arc<dyn SandboxManager>,
    db: Arc<dyn StateStoreTrait + Send + Sync>,
    namespace: &str,
) {
    use crate::core::types::SandboxState;

    tracing::info!(
        "Running K8s state reconciliation in namespace '{}'...",
        namespace
    );

    // List all sandboxes from DB
    let db_sandboxes = db.list_sandboxes().await;

    let mut reconciled = 0u64;
    let mut stopped_orphans = 0u64;
    let mut errors = 0u64;

    for sandbox in &db_sandboxes {
        // Only check sandboxes that are in Running state
        if sandbox.state != SandboxState::Running {
            continue;
        }

        let sandbox_id = sandbox.id.to_string();

        // Check if the K8s pod is still running
        match backend.is_running(&sandbox_id).await {
            Ok(true) => {
                // Pod is running, no action needed
                reconciled += 1;
            }
            Ok(false) => {
                // Pod exists but is not running -- mark as Stopped
                tracing::warn!(
                    sandbox_id = %sandbox_id,
                    "Sandbox marked as Running in DB but pod is not running, updating to Stopped"
                );
                let mut updated = sandbox.clone();
                updated.state = SandboxState::Stopped;
                updated.error_message =
                    Some("Pod not found during startup reconciliation".to_string());
                if let Err(e) = db.update_sandbox(&updated).await {
                    tracing::error!(
                        sandbox_id = %sandbox_id,
                        error = %e,
                        "Failed to update sandbox state during reconciliation"
                    );
                    errors += 1;
                } else {
                    stopped_orphans += 1;
                }
            }
            Err(e) => {
                // Could not determine pod status -- log and continue
                tracing::warn!(
                    sandbox_id = %sandbox_id,
                    error = %e,
                    "Failed to check pod status during reconciliation"
                );
                errors += 1;
            }
        }
    }

    tracing::info!(
        "K8s state reconciliation completed: {} verified, {} marked Stopped, {} errors",
        reconciled,
        stopped_orphans,
        errors
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_db_config_detection_with_url() {
        let config = Config {
            database: crate::config::DatabaseConfig {
                url: Some("postgresql://localhost/test".to_string()),
                host: "localhost".to_string(),
                port: 5432,
                name: "test".to_string(),
                user: "postgres".to_string(),
                password: None,
                pool_max_size: None,
            },
            ..Default::default()
        };

        let has_db_config = config.database.url.is_some() || config.database.password.is_some();
        assert!(has_db_config);
    }

    #[test]
    fn test_has_db_config_detection_with_password() {
        let config = Config {
            database: crate::config::DatabaseConfig {
                url: None,
                host: "localhost".to_string(),
                port: 5432,
                name: "test".to_string(),
                user: "postgres".to_string(),
                password: Some("test-password".to_string()),
                pool_max_size: None,
            },
            ..Default::default()
        };

        let has_db_config = config.database.url.is_some() || config.database.password.is_some();
        assert!(has_db_config);
    }

    #[test]
    fn test_has_db_config_detection_without_db() {
        let config = Config {
            database: crate::config::DatabaseConfig {
                url: None,
                host: "localhost".to_string(),
                port: 5432,
                name: "test".to_string(),
                user: "postgres".to_string(),
                password: None,
                pool_max_size: None,
            },
            ..Default::default()
        };

        let has_db_config = config.database.url.is_some() || config.database.password.is_some();
        assert!(!has_db_config);
    }

    #[test]
    fn test_database_url_building_with_password() {
        let config = Config {
            database: crate::config::DatabaseConfig {
                url: None,
                host: "localhost".to_string(),
                port: 5432,
                name: "test".to_string(),
                user: "postgres".to_string(),
                password: Some("secret123".to_string()),
                pool_max_size: None,
            },
            ..Default::default()
        };

        // Simulate the URL building logic
        let database_url = if let Some(url) = &config.database.url {
            url.clone()
        } else {
            format!(
                "postgresql://{}:{}@{}:{}/{}",
                config.database.user,
                config.database.password.as_ref().ok_or(
                    "Database password is required (set database.password or database.url)"
                ).unwrap(),
                config.database.host,
                config.database.port,
                config.database.name
            )
        };

        assert_eq!(
            database_url,
            "postgresql://postgres:secret123@localhost:5432/test"
        );
    }

    #[test]
    fn test_database_url_building_without_password_or_url() {
        let config = Config {
            database: crate::config::DatabaseConfig {
                url: None,
                host: "localhost".to_string(),
                port: 5432,
                name: "test".to_string(),
                user: "postgres".to_string(),
                password: None,
                pool_max_size: None,
            },
            ..Default::default()
        };

        // Simulate the URL building logic
        let result: Result<String, Box<dyn std::error::Error + Send + Sync>> =
            (|| {
                let database_url = if let Some(url) = &config.database.url {
                    Ok(url.clone())
                } else {
                    Ok(format!(
                        "postgresql://{}:{}@{}:{}/{}",
                        config.database.user,
                        config.database.password.as_ref().ok_or(
                            "Database password is required (set database.password or database.url)"
                        )?,
                        config.database.host,
                        config.database.port,
                        config.database.name
                    ))
                };
                database_url
            })();

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Database password is required"));
    }
}
