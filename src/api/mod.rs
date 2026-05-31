// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # HTTP API Server
//!
//! This module provides the RESTful HTTP API for managing sandboxes.
//!
//! ## Modules
//!
//! - [`server`](crate::api::server) - API server initialization and routing
//! - [`handlers`](crate::api::handlers) - HTTP request handlers
//! - [`auth`](crate::api::auth) - Authentication middleware
//! - [`logging`](crate::api::logging) - HTTP request logging middleware
//! - [`error_pages`](crate::api::error_pages) - HTML error page templates
//! - [`error_handler`](crate::api::error_handler) - Error handler middleware
//! - [`errors`](crate::api::errors) - RFC 9457 error types and conversions
//! - [`middleware`](crate::api::middleware) - Request ID middleware
//! - [`session_tokens`](crate::api::session_tokens) - Session token creation and validation for service auth

pub mod auth;
pub mod error_handler;
pub mod error_pages;
pub mod errors;
pub mod handlers;
pub mod logging;
pub mod middleware;
/// API server initialization and routing.
pub mod server;
/// Session token creation and validation for service authentication.
pub mod session_tokens;

pub use server::start_server;

// Re-export commonly used types
pub use errors::{ApiError, ErrorCode, ProblemDetails};
pub use middleware::RequestId;

use crate::api::handlers::*;
use crate::core::{SandboxService, SshSessionService};
use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

/// Build a router for testing purposes with pre-configured services.
///
/// This function creates the complete API router with all routes and handlers,
/// using the provided services. It's intended for integration testing.
///
/// # Arguments
///
/// * `sandbox_service` - Arc-wrapped SandboxService
/// * `ssh_service` - Arc-wrapped SshSessionService
///
/// # Returns
///
/// A configured Axum Router ready for testing
///
/// # Example
///
/// ```rust,no_run,ignore
/// # use dsb::api::build_test_router;
/// # use dsb::core::{SandboxService, SshSessionService};
/// # use std::sync::Arc;
/// # async fn example() {
/// let sandbox_service = Arc::new(/* ... */);
/// let ssh_service = Arc::new(/* ... */);
/// let app = build_test_router(sandbox_service, ssh_service);
/// // Use app for testing
/// # }
/// ```
pub fn build_test_router(
    sandbox_service: Arc<SandboxService>,
    ssh_service: Arc<SshSessionService>,
) -> Router {
    // Create SSH session routes
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

    // SSH auth routes - for testing, no API key required
    let ssh_auth_state = crate::api::handlers::ssh::SshAuthState {
        service: sandbox_service.clone(),
        api_key: None, // No auth in tests
    };
    let ssh_auth_routes = Router::new()
        .route("/ssh/authorize/{sandbox_id}", get(authorize_ssh_access))
        .with_state(ssh_auth_state);

    // Merge SSH routes
    let ssh_routes = ssh_session_routes.merge(ssh_auth_routes);

    // Create web terminal routes using the SandboxManager backend
    let test_config = crate::config::load_for_tests().expect("Failed to load test config");
    let docker_manager = crate::docker::DockerManager::new_with_config(&test_config)
        .expect("Failed to create Docker manager for web terminal");
    let backend: std::sync::Arc<dyn crate::core::manager::SandboxManager> =
        std::sync::Arc::new(docker_manager);
    let terminal_state = crate::web_terminal::WebTerminalState::new(
        backend,
        None, // No auth in tests
        std::sync::Arc::new(test_config),
    );
    let terminal_routes = Router::new()
        .route("/terminal", get(crate::web_terminal::terminal_page))
        .route(
            "/terminal/{sandbox_id}",
            get(crate::web_terminal::terminal_websocket),
        )
        .with_state(terminal_state);

    // Build main app router
    Router::new()
        .route("/health", get(health_check))
        .route("/sandboxes", get(list_sandboxes).post(create_sandbox))
        .route("/sandboxes/create-stream", post(create_sandbox_stream))
        .route("/sandboxes/{id}", get(get_sandbox).delete(delete_sandbox))
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
        .merge(ssh_routes)
        .merge(terminal_routes)
        .with_state(sandbox_service)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_test_router_exists() {
        // Compile-time test to ensure the function exists
        let _ = build_test_router;
    }
}
