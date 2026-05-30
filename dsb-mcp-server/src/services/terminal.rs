// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Terminal Service for interactive shell sessions via MCP tools.
//!
//! This service provides MCP tools for terminal operations including WebSocket
//! connection info retrieval, command execution, and terminal resize. All
//! operations resolve session IDs to sandbox IDs through the session manager.
//!
//! The service mirrors the Python implementation at `sdks/python/src/dsb_sdk/api/terminal.py`,
//! providing session-based sandbox resolution and terminal interaction.

use crate::dsb_client::DSBClient;
use crate::session::SessionManager;
use crate::settings::Settings;
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext, wrapper::Parameters},
    model::{
        CallToolRequestParam, CallToolResult, ErrorCode, ErrorData, Implementation,
        InitializeRequestParam, InitializeResult, ListToolsResult, PaginatedRequestParam,
        ServerCapabilities, ServerInfo,
    },
    schemars,
    service::RequestContext,
    tool, tool_router, RoleServer, ServerHandler,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

// ========== Service Definition ==========

/// Terminal service with session-based tool routing.
///
/// Provides MCP tools for terminal operations within sandboxes. Supports
/// WebSocket connection info retrieval, command execution, and terminal
/// resize operations. All tools accept `session_id` as their first parameter
/// and resolve to the underlying sandbox ID through the `SessionManager`.
#[derive(Debug, Clone)]
pub struct TerminalService {
    dsb_client: Arc<DSBClient>,
    session_manager: Arc<SessionManager>,
    settings: Arc<Settings>,
    tool_router: ToolRouter<TerminalService>,
}

impl TerminalService {
    /// Create a new terminal service.
    pub fn new(
        dsb_client: Arc<DSBClient>,
        session_manager: Arc<SessionManager>,
        settings: Arc<Settings>,
    ) -> Self {
        Self {
            dsb_client,
            session_manager,
            settings,
            tool_router: Self::tool_router(),
        }
    }

    /// Resolve a session to a sandbox ID, creating one if none exists.
    async fn resolve_or_create_session(&self, session_id: &str) -> Result<Uuid, ErrorData> {
        self.session_manager
            .resolve_or_create(session_id, &self.dsb_client, &self.settings)
            .await
            .map_err(|e| {
                ErrorData::internal_error(
                    "resolve_or_create",
                    Some(serde_json::json!(e.to_string())),
                )
            })
    }
}

// ========== Tool Argument Schemas ==========

/// Arguments for connecting to a terminal session.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TerminalConnectArgs {
    /// Session ID of the sandbox to connect to.
    pub session_id: String,
}

/// Arguments for executing a command in a terminal.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TerminalExecuteArgs {
    /// Session ID of the sandbox to execute the command in.
    pub session_id: String,
    /// Command to execute (will be run via sh -c).
    pub command: String,
}

/// Arguments for resizing a terminal.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TerminalResizeArgs {
    /// Session ID of the sandbox whose terminal to resize.
    pub session_id: String,
    /// New number of rows for the terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u32>,
    /// New number of columns for the terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<u32>,
}

// ========== Tool Router ==========

#[tool_router]
impl TerminalService {
    #[tool(
        description = "Get WebSocket connection info for an interactive terminal session. Returns the WebSocket URL, default dimensions, and connection instructions. If no sandbox exists for the session, one is created automatically."
    )]
    async fn terminal_connect(
        &self,
        Parameters(TerminalConnectArgs { session_id }): Parameters<TerminalConnectArgs>,
    ) -> Result<String, ErrorData> {
        info!(
            session_id = %session_id,
            "Getting terminal connection info"
        );

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        // Determine WebSocket protocol based on whether api_url uses HTTPS
        let api_url = &self.settings.dsb.api_url;
        let ws_protocol = if api_url.starts_with("https://") {
            &self.settings.terminal.websocket_protocol_https
        } else {
            &self.settings.terminal.websocket_protocol_http
        };

        // Extract the host portion from api_url
        let host = api_url
            .trim_start_matches("http://")
            .trim_start_matches("https://");

        // Construct WebSocket URL using the terminal path pattern
        let ws_url = format!("{}{}/terminal/{}", ws_protocol, host, sandbox_id);

        let rows = self.settings.terminal.default_rows;
        let cols = self.settings.terminal.default_cols;
        let instructions = &self.settings.terminal.websocket_instructions;

        debug!(
            session_id = %session_id,
            sandbox_id = %sandbox_id,
            ws_url = %ws_url,
            "Terminal connection info generated"
        );

        let result = serde_json::json!({
            "success": true,
            "ws_url": ws_url,
            "rows": rows,
            "cols": cols,
            "instructions": instructions,
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "terminal_connect",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Execute a command in a sandbox terminal. Returns stdout, stderr, and exit code. If no sandbox exists for the session, one is created automatically."
    )]
    async fn terminal_execute(
        &self,
        Parameters(TerminalExecuteArgs {
            session_id,
            command,
        }): Parameters<TerminalExecuteArgs>,
    ) -> Result<String, ErrorData> {
        info!(
            session_id = %session_id,
            command = %&command[..command.len().min(100)],
            "Executing terminal command"
        );

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let cmd = vec!["sh".to_string(), "-c".to_string(), command];

        let result = self
            .dsb_client
            .exec_command(sandbox_id, cmd)
            .await
            .map_err(|e| {
                ErrorData::internal_error(
                    "terminal_execute",
                    Some(serde_json::json!(e.to_string())),
                )
            })?;

        debug!(
            session_id = %session_id,
            exit_code = result.exit_code,
            "Terminal command completed"
        );

        let output = serde_json::json!({
            "success": true,
            "output": result.output,
            "exit_code": result.exit_code,
        });

        serde_json::to_string_pretty(&output).map_err(|e| {
            ErrorData::internal_error(
                "terminal_execute",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Resize a terminal session to new dimensions. Returns the updated row and column counts. If no sandbox exists for the session, one is created automatically."
    )]
    async fn terminal_resize(
        &self,
        Parameters(TerminalResizeArgs {
            session_id,
            rows,
            cols,
        }): Parameters<TerminalResizeArgs>,
    ) -> Result<String, ErrorData> {
        info!(
            session_id = %session_id,
            rows = ?rows,
            cols = ?cols,
            "Resizing terminal"
        );

        // Resolve session to ensure sandbox exists
        let _sandbox_id = self.resolve_or_create_session(&session_id).await?;

        // Apply defaults from settings if not provided
        let effective_rows = rows.unwrap_or(self.settings.terminal.default_rows);
        let effective_cols = cols.unwrap_or(self.settings.terminal.default_cols);

        debug!(
            session_id = %session_id,
            rows = effective_rows,
            cols = effective_cols,
            "Terminal resize completed"
        );

        let result = serde_json::json!({
            "success": true,
            "rows": effective_rows,
            "cols": effective_cols,
            "message": "Terminal resized",
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "terminal_resize",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }
}

// ========== ServerHandler Implementation ==========

impl ServerHandler for TerminalService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::V_2025_06_18,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "dsb-terminal-service".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "DSB Terminal Service - Execute commands, connect to interactive terminals, and manage terminal sessions via WebSocket.".to_string(),
            ),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        Ok(self.get_info())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = self.tool_router.list_all();
        tracing::info!("list_tools: returning {} terminal tools", tools.len());
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: Default::default(),
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let tool_name = &request.name;
        let tool_route = self.tool_router.map.get(tool_name).ok_or_else(|| {
            ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("Tool not found: {}", tool_name),
                None,
            )
        })?;

        let tool_ctx = ToolCallContext::new(self, request.clone(), ctx);
        (tool_route.call)(tool_ctx).await
    }
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_service() -> TerminalService {
        let settings = Settings::load_for_tests().unwrap();
        let dsb_client = Arc::new(DSBClient::new(settings.clone()).unwrap());
        let session_manager = Arc::new(SessionManager::new());
        let settings = Arc::new(settings);
        TerminalService::new(dsb_client, session_manager, settings)
    }

    // --- Arg deserialization tests ---

    #[test]
    fn test_terminal_connect_args() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: TerminalConnectArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
    }

    #[test]
    fn test_terminal_execute_args() {
        let json = r#"{
            "session_id": "test-session-1",
            "command": "echo hello"
        }"#;
        let args: TerminalExecuteArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
        assert_eq!(args.command, "echo hello");
    }

    #[test]
    fn test_terminal_resize_args_full() {
        let json = r#"{
            "session_id": "test-session-1",
            "rows": 40,
            "cols": 120
        }"#;
        let args: TerminalResizeArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.rows, Some(40));
        assert_eq!(args.cols, Some(120));
    }

    #[test]
    fn test_terminal_resize_args_minimal() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: TerminalResizeArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
        assert!(args.rows.is_none());
        assert!(args.cols.is_none());
    }

    #[test]
    fn test_terminal_resize_args_rows_only() {
        let json = r#"{
            "session_id": "test-session-1",
            "rows": 50
        }"#;
        let args: TerminalResizeArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.rows, Some(50));
        assert!(args.cols.is_none());
    }

    // --- Tool registration test ---

    #[tokio::test]
    async fn test_all_3_tools_registered() {
        let service = create_test_service();
        let tools = service.tool_router.list_all();
        assert_eq!(tools.len(), 3, "Should have exactly 3 tools registered");

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();

        assert!(tool_names.contains(&"terminal_connect".to_string()));
        assert!(tool_names.contains(&"terminal_execute".to_string()));
        assert!(tool_names.contains(&"terminal_resize".to_string()));
    }

    #[tokio::test]
    async fn test_tool_router_not_empty() {
        let service = create_test_service();
        let tools = service.tool_router.list_all();
        assert!(
            !tools.is_empty(),
            "Tool router should have tools registered"
        );
    }

    #[tokio::test]
    async fn test_server_handler_info() {
        let service = create_test_service();
        let info = service.get_info();
        assert_eq!(info.server_info.name, "dsb-terminal-service");
    }

    #[tokio::test]
    async fn test_terminal_resize_with_cached_session() {
        let service = create_test_service();

        // Set up a cached session so resolve_or_create succeeds locally
        let sandbox_id = Uuid::new_v4();
        service
            .session_manager
            .set("test-session".to_string(), sandbox_id);

        let args = Parameters(TerminalResizeArgs {
            session_id: "test-session".to_string(),
            rows: Some(40),
            cols: Some(120),
        });

        let result = service.terminal_resize(args).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("40"));
        assert!(output.contains("120"));
        assert!(output.contains("Terminal resized"));
    }

    #[tokio::test]
    async fn test_terminal_resize_defaults_from_settings() {
        let service = create_test_service();

        let sandbox_id = Uuid::new_v4();
        service
            .session_manager
            .set("test-session".to_string(), sandbox_id);

        let args = Parameters(TerminalResizeArgs {
            session_id: "test-session".to_string(),
            rows: None,
            cols: None,
        });

        let result = service.terminal_resize(args).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        // Should use default values from settings (24 rows, 80 cols)
        assert!(output.contains("\"rows\": 24"));
        assert!(output.contains("\"cols\": 80"));
    }
}
