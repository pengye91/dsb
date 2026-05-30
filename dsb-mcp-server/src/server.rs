// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! MCP server implementation
//!
//! Implements the MCP protocol using rmcp SDK's Streamable HTTP transport.
//! Mounts 6 separate services under different Axum paths, matching the
//! Python implementation's Starlette mounting pattern:
//!
//! - `/mcp/dsb/sandbox`     -> SandboxService      (8 tools)
//! - `/mcp/dsb/web`         -> WebService           (2 tools)
//! - `/mcp/dsb/browser`     -> BrowserService       (14 tools)
//! - `/mcp/dsb/terminal`    -> TerminalService       (3 tools)
//! - `/mcp/system`          -> SystemService         (1 tool)
//! - `/mcp/value_retrieval` -> ValueRetrievalService (2 tools)

use crate::dsb_client::DSBClient;
use crate::dsb_service::DSBService;
use crate::services::{
    browser::BrowserService, sandbox::SandboxService, system::SystemService,
    terminal::TerminalService, value_retrieval::ValueRetrievalService, web::WebService,
};
use crate::session::SessionManager;
use crate::settings::Settings;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;

/// MCP server
///
/// Creates and runs an Axum HTTP server that mounts 6 MCP services
/// on separate paths. Each service has its own `LocalSessionManager` for
/// MCP protocol session management, while the custom `SessionManager`
/// (session-to-sandbox mapping) is shared across all services.
pub struct MCPServer {
    settings: Settings,
    dsb_client: DSBClient,
}

impl MCPServer {
    /// Create a new MCP server
    pub async fn new(settings: Settings) -> anyhow::Result<Self> {
        let dsb_client = DSBClient::new(settings.clone()).map_err(|e| anyhow::anyhow!(e))?;

        Ok(Self {
            settings,
            dsb_client,
        })
    }

    /// Run the server
    ///
    /// Binds to the configured host:port and serves all 6 MCP service endpoints.
    pub async fn run(&self) -> anyhow::Result<()> {
        // Initialize tracing
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".to_string().into()),
            )
            .init();

        info!("Starting DSB MCP Server (Multi-Service)");

        // Create shared resources
        let dsb_client = Arc::new(self.dsb_client.clone());
        let session_manager = Arc::new(SessionManager::new());
        let settings = Arc::new(self.settings.clone());

        // Start background session cleanup to evict stale mappings
        session_manager.start_cleanup_task().await;

        // Create all 6 services with shared state.
        // Each StreamableHttpService gets its own LocalSessionManager for MCP
        // protocol session management, while our custom SessionManager (session-to-sandbox
        // mapping) is shared across all services via Arc.

        let sandbox_service = StreamableHttpService::new(
            {
                let dsb_client = dsb_client.clone();
                let session_manager = session_manager.clone();
                let settings = settings.clone();
                move || {
                    Ok(SandboxService::new(
                        dsb_client.clone(),
                        session_manager.clone(),
                        settings.clone(),
                    ))
                }
            },
            LocalSessionManager::default().into(),
            Default::default(),
        );

        let web_service = StreamableHttpService::new(
            {
                let dsb_client = dsb_client.clone();
                let session_manager = session_manager.clone();
                let settings = settings.clone();
                move || {
                    Ok(WebService::new(
                        dsb_client.clone(),
                        session_manager.clone(),
                        settings.clone(),
                    ))
                }
            },
            LocalSessionManager::default().into(),
            Default::default(),
        );

        let browser_service = StreamableHttpService::new(
            {
                let dsb_client = dsb_client.clone();
                let session_manager = session_manager.clone();
                let settings = settings.clone();
                move || {
                    Ok(BrowserService::new(
                        dsb_client.clone(),
                        session_manager.clone(),
                        settings.clone(),
                    ))
                }
            },
            LocalSessionManager::default().into(),
            Default::default(),
        );

        let terminal_service = StreamableHttpService::new(
            {
                let dsb_client = dsb_client.clone();
                let session_manager = session_manager.clone();
                let settings = settings.clone();
                move || {
                    Ok(TerminalService::new(
                        dsb_client.clone(),
                        session_manager.clone(),
                        settings.clone(),
                    ))
                }
            },
            LocalSessionManager::default().into(),
            Default::default(),
        );

        let system_service = StreamableHttpService::new(
            {
                let dsb_client = dsb_client.clone();
                let session_manager = session_manager.clone();
                let settings = settings.clone();
                move || {
                    Ok(SystemService::new(
                        dsb_client.clone(),
                        session_manager.clone(),
                        settings.clone(),
                    ))
                }
            },
            LocalSessionManager::default().into(),
            Default::default(),
        );

        let value_retrieval_service = StreamableHttpService::new(
            {
                let dsb_client = dsb_client.clone();
                let session_manager = session_manager.clone();
                let settings = settings.clone();
                move || {
                    Ok(ValueRetrievalService::new(
                        dsb_client.clone(),
                        session_manager.clone(),
                        settings.clone(),
                    ))
                }
            },
            LocalSessionManager::default().into(),
            Default::default(),
        );

        // Create the backward-compatible DSBService for /mcp endpoint.
        // This keeps existing tests and clients working while the new
        // multi-service architecture is being adopted.
        let legacy_dsbservice = StreamableHttpService::new(
            {
                let dsb_client = dsb_client.clone();
                move || Ok(DSBService::new((*(dsb_client.as_ref())).clone()))
            },
            LocalSessionManager::default().into(),
            Default::default(),
        );

        // Build Axum router with all 7 service mounts (6 new + 1 legacy)
        // Increase body limit to 20MB so file_upload can handle large files
        let router = axum::Router::new()
            .nest_service("/mcp", legacy_dsbservice)
            .nest_service("/mcp/dsb/sandbox", sandbox_service)
            .nest_service("/mcp/dsb/web", web_service)
            .nest_service("/mcp/dsb/browser", browser_service)
            .nest_service("/mcp/dsb/terminal", terminal_service)
            .nest_service("/mcp/system", system_service)
            .nest_service("/mcp/value_retrieval", value_retrieval_service)
            .layer(axum::extract::DefaultBodyLimit::max(20 * 1024 * 1024));

        // Parse bind address
        let bind: SocketAddr = format!(
            "{}:{}",
            self.settings.server.host, self.settings.server.port
        )
        .parse()?;

        // Bind TCP listener
        let tcp_listener = tokio::net::TcpListener::bind(&bind).await?;

        info!("DSB MCP Server listening on http://{}", bind);
        info!("Service endpoints:");
        info!("  - http://{}/mcp                 (legacy, 8 tools)", bind);
        info!("  - http://{}/mcp/dsb/sandbox     (8 tools)", bind);
        info!("  - http://{}/mcp/dsb/web          (2 tools)", bind);
        info!("  - http://{}/mcp/dsb/browser      (14 tools)", bind);
        info!("  - http://{}/mcp/dsb/terminal     (3 tools)", bind);
        info!("  - http://{}/mcp/system           (1 tool)", bind);
        info!("  - http://{}/mcp/value_retrieval  (2 tools)", bind);
        info!("Ready to accept connections...");

        // Start server with graceful shutdown
        let _ = axum::serve(tcp_listener, router)
            .with_graceful_shutdown(async move {
                tokio::signal::ctrl_c()
                    .await
                    .expect("Failed to wait for Ctrl+C");
                info!("Shutting down server...");
            })
            .await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that all 6 service types can be constructed with shared state.
    #[test]
    fn test_all_services_constructable() {
        let settings = Settings::load_for_tests().unwrap();
        let dsb_client = Arc::new(DSBClient::new(settings.clone()).unwrap());
        let session_manager = Arc::new(SessionManager::new());
        let settings = Arc::new(settings);

        let _sandbox = SandboxService::new(
            dsb_client.clone(),
            session_manager.clone(),
            settings.clone(),
        );
        let _web = WebService::new(
            dsb_client.clone(),
            session_manager.clone(),
            settings.clone(),
        );
        let _browser = BrowserService::new(
            dsb_client.clone(),
            session_manager.clone(),
            settings.clone(),
        );
        let _terminal = TerminalService::new(
            dsb_client.clone(),
            session_manager.clone(),
            settings.clone(),
        );
        let _system = SystemService::new(
            dsb_client.clone(),
            session_manager.clone(),
            settings.clone(),
        );
        let _value_retrieval = ValueRetrievalService::new(
            dsb_client.clone(),
            session_manager.clone(),
            settings.clone(),
        );
    }

    /// Verify that each service can be constructed and has a unique service name
    /// with the expected tool count (validated in per-service test modules).
    #[tokio::test]
    async fn test_all_services_have_unique_names() {
        use rmcp::ServerHandler;

        let settings = Settings::load_for_tests().unwrap();
        let dsb_client = Arc::new(DSBClient::new(settings.clone()).unwrap());
        let session_manager = Arc::new(SessionManager::new());
        let settings_arc = Arc::new(settings);

        let names = [
            SandboxService::new(
                dsb_client.clone(),
                session_manager.clone(),
                settings_arc.clone(),
            )
            .get_info()
            .server_info
            .name
            .to_string(),
            WebService::new(
                dsb_client.clone(),
                session_manager.clone(),
                settings_arc.clone(),
            )
            .get_info()
            .server_info
            .name
            .to_string(),
            BrowserService::new(
                dsb_client.clone(),
                session_manager.clone(),
                settings_arc.clone(),
            )
            .get_info()
            .server_info
            .name
            .to_string(),
            TerminalService::new(
                dsb_client.clone(),
                session_manager.clone(),
                settings_arc.clone(),
            )
            .get_info()
            .server_info
            .name
            .to_string(),
            SystemService::new(
                dsb_client.clone(),
                session_manager.clone(),
                settings_arc.clone(),
            )
            .get_info()
            .server_info
            .name
            .to_string(),
            ValueRetrievalService::new(
                dsb_client.clone(),
                session_manager.clone(),
                settings_arc.clone(),
            )
            .get_info()
            .server_info
            .name
            .to_string(),
        ];

        // All 6 names must be unique
        let unique_count = names.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(unique_count, 6, "All 6 service names should be unique");

        // Verify expected names
        assert!(names.contains(&"dsb-sandbox-service".to_string()));
        assert!(names.contains(&"dsb-web-service".to_string()));
        assert!(names.contains(&"dsb-browser-service".to_string()));
        assert!(names.contains(&"dsb-terminal-service".to_string()));
        assert!(names.contains(&"dsb-system-service".to_string()));
        assert!(names.contains(&"dsb-value-retrieval-service".to_string()));
    }
}
