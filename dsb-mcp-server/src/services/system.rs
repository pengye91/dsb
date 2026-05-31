// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! System Service for basic system-level MCP tools.
//!
//! This service provides simple utility tools such as retrieving the current
//! system time. It follows the same `ServerHandler` pattern as other services
//! but does not require sandbox or session functionality for its tools.

use crate::dsb_client::DSBClient;
use crate::session::SessionManager;
use crate::settings::Settings;
use chrono::Local;
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
use tracing::info;

// ========== Service Definition ==========

/// System service providing utility MCP tools.
///
/// Offers lightweight system-level tools like time retrieval.
#[derive(Debug, Clone)]
pub struct SystemService {
    settings: Arc<Settings>,
    tool_router: ToolRouter<SystemService>,
}

impl SystemService {
    /// Create a new system service.
    pub fn new(
        _dsb_client: Arc<DSBClient>,
        _session_manager: Arc<SessionManager>,
        settings: Arc<Settings>,
    ) -> Self {
        Self {
            settings,
            tool_router: Self::tool_router(),
        }
    }
}

// ========== Tool Argument Schemas ==========

/// Arguments for getting the current system time.
///
/// This is a zero-field struct since the tool takes no parameters,
/// but it is required by the `Parameters<T>` wrapper for schema generation.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSystemTimeArgs {}

// ========== Tool Router ==========

#[tool_router]
impl SystemService {
    #[tool(
        description = "Get the current system date and time. Returns the current time formatted according to the server's configured date format, along with timezone information."
    )]
    async fn get_system_time(
        &self,
        Parameters(_args): Parameters<GetSystemTimeArgs>,
    ) -> Result<String, ErrorData> {
        info!("Getting system time");

        let now = Local::now();
        let date_format = &self.settings.system.date_format;
        let formatted_time = now.format(date_format).to_string();
        let timezone = now.offset().to_string();

        let result = serde_json::json!({
            "current_time": formatted_time,
            "format": date_format,
            "timezone": timezone,
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "get_system_time",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }
}

// ========== ServerHandler Implementation ==========

impl ServerHandler for SystemService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::V_2025_06_18,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "dsb-system-service".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "DSB System Service - Provides system-level utility tools such as time retrieval."
                    .to_string(),
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
        tracing::info!("list_tools: returning {} system tools", tools.len());
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

    fn create_test_service() -> SystemService {
        let settings = Settings::load_for_tests().unwrap();
        let dsb_client = Arc::new(DSBClient::new(settings.clone()).unwrap());
        let session_manager = Arc::new(SessionManager::new());
        let settings = Arc::new(settings);
        SystemService::new(dsb_client, session_manager, settings)
    }

    #[test]
    fn test_get_system_time_args_deserialization() {
        let json = r#"{}"#;
        let args: GetSystemTimeArgs = serde_json::from_str(json).unwrap();
        // Zero-field struct, just verify it deserializes without error
        assert!(format!("{:?}", args).contains("GetSystemTimeArgs"));
    }

    #[tokio::test]
    async fn test_get_system_time_returns_valid_json() {
        let service = create_test_service();
        let args = Parameters(GetSystemTimeArgs {});
        let result = service.get_system_time(args).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.get("current_time").is_some());
        assert!(parsed.get("format").is_some());
        assert!(parsed.get("timezone").is_some());

        // Verify the format field matches settings
        let format = parsed.get("format").unwrap().as_str().unwrap();
        assert_eq!(format, service.settings.system.date_format);
    }

    #[tokio::test]
    async fn test_1_tool_registered() {
        let service = create_test_service();
        let tools = service.tool_router.list_all();
        assert_eq!(tools.len(), 1, "Should have exactly 1 tool registered");

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
        assert!(tool_names.contains(&"get_system_time".to_string()));
    }

    #[tokio::test]
    async fn test_server_handler_info() {
        let service = create_test_service();
        let info = service.get_info();
        assert_eq!(info.server_info.name, "dsb-system-service");
    }
}
