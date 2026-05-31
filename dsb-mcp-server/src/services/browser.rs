// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Browser Automation Service for web browser control via session-based sandbox management.
//!
//! This service provides 14 individual MCP tools for browser automation, replacing the
//! old monolithic `automate_browser` approach. Each tool maps to a specific browser
//! action (navigate, click, fill, scroll, screenshot, etc.) and uses the sandbox-bound
//! `agent_browser_tools.py` under the hood.
//!
//! The service mirrors the Python implementation at `tools/dsb_tools/browser.py`,
//! providing session-based sandbox resolution and per-action tool handlers.

use crate::dsb_client::DSBClient;
use crate::session::SessionManager;
use crate::settings::Settings;
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext, wrapper::Parameters},
    model::{
        CallToolRequestParam, CallToolResult, ErrorData, Implementation, InitializeRequestParam,
        InitializeResult, ListToolsResult, PaginatedRequestParam, ServerCapabilities, ServerInfo,
    },
    schemars,
    service::RequestContext,
    tool, tool_router, RoleServer, ServerHandler,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

// ========== Service Definition ==========

/// Browser automation service with session-based tool routing.
///
/// Provides 14 individual browser tools for granular web automation control.
/// All tools accept `session_id` as their first parameter and resolve to the
/// underlying sandbox ID through the `SessionManager`.
#[derive(Debug, Clone)]
pub struct BrowserService {
    dsb_client: Arc<DSBClient>,
    session_manager: Arc<SessionManager>,
    settings: Arc<Settings>,
    tool_router: ToolRouter<BrowserService>,
}

impl BrowserService {
    /// Create a new browser service.
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

/// Arguments for navigating to a URL.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserNavigateArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
    /// URL to navigate to.
    pub url: String,
}

/// Arguments for getting clickable/interactive elements.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserGetClickableElementsArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
}

/// Arguments for clicking an element.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserClickArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
    /// Element index from the interactive snapshot (e.g., 1, 2, 3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<i32>,
    /// CSS selector for the element to click.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
}

/// Arguments for filling a form field.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserFillArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
    /// CSS selector for the form field.
    pub selector: String,
    /// Value to fill in.
    pub value: String,
    /// Whether to clear the field before filling (default: true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clear: Option<bool>,
}

/// Arguments for scrolling the page.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserScrollArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
    /// Number of pixels to scroll (positive = down, negative = up).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<i32>,
}

/// Arguments for taking a screenshot.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserScreenshotArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
    /// Optional name for the screenshot file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether to capture the full page (default: false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_page: Option<bool>,
    /// Optional CSS selector to screenshot a specific element.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
}

/// Arguments for opening a new browser tab.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserNewTabArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
    /// Optional URL to open in the new tab.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Arguments for listing browser tabs.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserTabListArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
}

/// Arguments for switching to a browser tab by index.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserSwitchTabArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
    /// Tab index to switch to (0-based).
    pub index: i32,
}

/// Arguments for checking browser health.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserHealthCheckArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
}

/// Arguments for navigating back in browser history.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserGoBackArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
}

/// Arguments for navigating forward in browser history.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserGoForwardArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
}

/// Arguments for evaluating JavaScript in the browser.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserEvaluateArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
    /// JavaScript code to evaluate.
    pub script: String,
}

/// Arguments for closing the browser.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BrowserCloseArgs {
    /// Session ID for sandbox resolution.
    pub session_id: String,
}

// ========== Tool Router ==========

#[tool_router]
impl BrowserService {
    #[tool(
        description = "Navigate the browser to a specified URL. Waits for the page to load before returning."
    )]
    async fn browser_navigate(
        &self,
        Parameters(BrowserNavigateArgs { session_id, url }): Parameters<BrowserNavigateArgs>,
    ) -> Result<String, ErrorData> {
        info!(session_id = %session_id, url = %url, "Navigating browser");

        // Validate URL for SSRF prevention before creating sandbox
        if let Err(e) = crate::tools::web::validate_url_secure(&url) {
            return Err(ErrorData::invalid_params(
                "browser_navigate",
                Some(serde_json::json!(e)),
            ));
        }

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;
        let result = crate::tools::browser::navigate(&self.dsb_client, sandbox_id.to_string(), url)
            .await
            .map_err(|e| {
                ErrorData::internal_error(
                    "browser_navigate",
                    Some(serde_json::json!(e.to_string())),
                )
            })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_navigate",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Get a list of clickable and interactive elements on the current page. Returns elements with refs (@e1, @e2) for use with browser_click and browser_fill."
    )]
    async fn browser_get_clickable_elements(
        &self,
        Parameters(BrowserGetClickableElementsArgs { session_id }): Parameters<
            BrowserGetClickableElementsArgs,
        >,
    ) -> Result<String, ErrorData> {
        info!(session_id = %session_id, "Getting clickable elements");

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;
        let result = crate::tools::browser::snapshot(
            &self.dsb_client,
            sandbox_id.to_string(),
            true, // interactive = true to get clickable elements
        )
        .await
        .map_err(|e| {
            ErrorData::internal_error(
                "browser_get_clickable_elements",
                Some(serde_json::json!(e.to_string())),
            )
        })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_get_clickable_elements",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Click an element on the page by its ref index from get_clickable_elements or by CSS selector."
    )]
    async fn browser_click(
        &self,
        Parameters(BrowserClickArgs {
            session_id,
            index,
            selector,
        }): Parameters<BrowserClickArgs>,
    ) -> Result<String, ErrorData> {
        info!(
            session_id = %session_id,
            index = ?index,
            selector = ?selector,
            "Clicking element"
        );

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        // Convert index to ref format (@e1, @e2, etc.)
        let r#ref = index.map(|i| format!("@e{}", i));

        let result =
            crate::tools::browser::click(&self.dsb_client, sandbox_id.to_string(), r#ref, selector)
                .await
                .map_err(|e| {
                    ErrorData::internal_error(
                        "browser_click",
                        Some(serde_json::json!(e.to_string())),
                    )
                })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_click",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Fill a form field with a value. Supports clearing the field before filling."
    )]
    async fn browser_fill(
        &self,
        Parameters(BrowserFillArgs {
            session_id,
            selector,
            value,
            clear,
        }): Parameters<BrowserFillArgs>,
    ) -> Result<String, ErrorData> {
        info!(
            session_id = %session_id,
            selector = %selector,
            clear = ?clear,
            "Filling form field"
        );

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let result = crate::tools::browser::fill(
            &self.dsb_client,
            sandbox_id.to_string(),
            None, // ref
            Some(selector),
            value,
            clear,
        )
        .await
        .map_err(|e| {
            ErrorData::internal_error("browser_fill", Some(serde_json::json!(e.to_string())))
        })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_fill",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Scroll the page by a number of pixels. Positive values scroll down, negative values scroll up."
    )]
    async fn browser_scroll(
        &self,
        Parameters(BrowserScrollArgs { session_id, amount }): Parameters<BrowserScrollArgs>,
    ) -> Result<String, ErrorData> {
        let scroll_amount = amount.unwrap_or(300);
        info!(
            session_id = %session_id,
            amount = scroll_amount,
            "Scrolling page"
        );

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let result = crate::tools::browser::scroll(
            &self.dsb_client,
            sandbox_id.to_string(),
            "down".to_string(),
            scroll_amount,
        )
        .await
        .map_err(|e| {
            ErrorData::internal_error("browser_scroll", Some(serde_json::json!(e.to_string())))
        })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_scroll",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Take a screenshot of the current page or a specific element. Returns the path to the saved screenshot."
    )]
    async fn browser_screenshot(
        &self,
        Parameters(BrowserScreenshotArgs {
            session_id,
            name,
            full_page,
            selector,
        }): Parameters<BrowserScreenshotArgs>,
    ) -> Result<String, ErrorData> {
        let is_full_page = full_page.unwrap_or(self.settings.browser.screenshot_full_page);
        info!(
            session_id = %session_id,
            full_page = is_full_page,
            name = ?name,
            selector = ?selector,
            "Taking screenshot"
        );

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let result = crate::tools::browser::screenshot(
            &self.dsb_client,
            sandbox_id.to_string(),
            is_full_page,
            name,
            selector,
        )
        .await
        .map_err(|e| {
            ErrorData::internal_error("browser_screenshot", Some(serde_json::json!(e.to_string())))
        })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_screenshot",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(description = "Open a new browser tab, optionally navigating to a URL.")]
    async fn browser_new_tab(
        &self,
        Parameters(BrowserNewTabArgs { session_id, url }): Parameters<BrowserNewTabArgs>,
    ) -> Result<String, ErrorData> {
        info!(session_id = %session_id, "Opening new tab");

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let result = crate::tools::browser::tabs(
            &self.dsb_client,
            sandbox_id.to_string(),
            "new".to_string(),
            None,
            url,
        )
        .await
        .map_err(|e| {
            ErrorData::internal_error("browser_new_tab", Some(serde_json::json!(e.to_string())))
        })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_new_tab",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(description = "List all open browser tabs with their titles and URLs.")]
    async fn browser_tab_list(
        &self,
        Parameters(BrowserTabListArgs { session_id }): Parameters<BrowserTabListArgs>,
    ) -> Result<String, ErrorData> {
        info!(session_id = %session_id, "Listing browser tabs");

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let result = crate::tools::browser::tabs(
            &self.dsb_client,
            sandbox_id.to_string(),
            "list".to_string(),
            None,
            None,
        )
        .await
        .map_err(|e| {
            ErrorData::internal_error("browser_tab_list", Some(serde_json::json!(e.to_string())))
        })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_tab_list",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(description = "Switch to a browser tab by its index (0-based).")]
    async fn browser_switch_tab(
        &self,
        Parameters(BrowserSwitchTabArgs { session_id, index }): Parameters<BrowserSwitchTabArgs>,
    ) -> Result<String, ErrorData> {
        info!(
            session_id = %session_id,
            index = index,
            "Switching browser tab"
        );

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let result = crate::tools::browser::tabs(
            &self.dsb_client,
            sandbox_id.to_string(),
            "select".to_string(),
            Some(index),
            None,
        )
        .await
        .map_err(|e| {
            ErrorData::internal_error("browser_switch_tab", Some(serde_json::json!(e.to_string())))
        })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_switch_tab",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Check if the browser is running and responsive. Returns the health status of the browser instance."
    )]
    async fn browser_health_check(
        &self,
        Parameters(BrowserHealthCheckArgs { session_id }): Parameters<BrowserHealthCheckArgs>,
    ) -> Result<String, ErrorData> {
        info!(session_id = %session_id, "Checking browser health");

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let args = json!({});
        let result = crate::tools::browser::call_browser_tool(
            &self.dsb_client,
            &sandbox_id.to_string(),
            "browser_health_check",
            args,
        )
        .await
        .map_err(|e| {
            ErrorData::internal_error(
                "browser_health_check",
                Some(serde_json::json!(e.to_string())),
            )
        })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_health_check",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(description = "Navigate back in browser history.")]
    async fn browser_go_back(
        &self,
        Parameters(BrowserGoBackArgs { session_id }): Parameters<BrowserGoBackArgs>,
    ) -> Result<String, ErrorData> {
        info!(session_id = %session_id, "Going back in browser history");

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let result = crate::tools::browser::go_back(&self.dsb_client, sandbox_id.to_string())
            .await
            .map_err(|e| {
                ErrorData::internal_error("browser_go_back", Some(serde_json::json!(e.to_string())))
            })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_go_back",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(description = "Navigate forward in browser history.")]
    async fn browser_go_forward(
        &self,
        Parameters(BrowserGoForwardArgs { session_id }): Parameters<BrowserGoForwardArgs>,
    ) -> Result<String, ErrorData> {
        info!(session_id = %session_id, "Going forward in browser history");

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let result = crate::tools::browser::go_forward(&self.dsb_client, sandbox_id.to_string())
            .await
            .map_err(|e| {
                ErrorData::internal_error(
                    "browser_go_forward",
                    Some(serde_json::json!(e.to_string())),
                )
            })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_go_forward",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(description = "Evaluate JavaScript code in the browser and return the result.")]
    async fn browser_evaluate(
        &self,
        Parameters(BrowserEvaluateArgs { session_id, script }): Parameters<BrowserEvaluateArgs>,
    ) -> Result<String, ErrorData> {
        info!(
            session_id = %session_id,
            script_len = script.len(),
            "Evaluating JavaScript"
        );

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let result =
            crate::tools::browser::evaluate(&self.dsb_client, sandbox_id.to_string(), script)
                .await
                .map_err(|e| {
                    ErrorData::internal_error(
                        "browser_evaluate",
                        Some(serde_json::json!(e.to_string())),
                    )
                })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_evaluate",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(description = "Close the browser instance and release resources.")]
    async fn browser_close(
        &self,
        Parameters(BrowserCloseArgs { session_id }): Parameters<BrowserCloseArgs>,
    ) -> Result<String, ErrorData> {
        info!(session_id = %session_id, "Closing browser");

        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let args = json!({});
        let result = crate::tools::browser::call_browser_tool(
            &self.dsb_client,
            &sandbox_id.to_string(),
            "browser_close",
            args,
        )
        .await
        .map_err(|e| {
            ErrorData::internal_error("browser_close", Some(serde_json::json!(e.to_string())))
        })?;

        let response = serde_json::json!({
            "success": true,
            "status": self.settings.browser.success_status,
            "output": result,
        });
        serde_json::to_string_pretty(&response).map_err(|e| {
            ErrorData::internal_error(
                "browser_close",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }
}

// ========== ServerHandler Implementation ==========

impl ServerHandler for BrowserService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::V_2025_06_18,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "dsb-browser-service".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "DSB Browser Service - Browser automation tools via session-based sandbox management.".to_string(),
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
        tracing::info!("list_tools: returning {} browser tools", tools.len());
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
                rmcp::model::ErrorCode::METHOD_NOT_FOUND,
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

    fn create_test_service() -> BrowserService {
        let settings = Settings::load_for_tests().unwrap();
        let dsb_client = Arc::new(DSBClient::new(settings.clone()).unwrap());
        let session_manager = Arc::new(SessionManager::new());
        let settings = Arc::new(settings);
        BrowserService::new(dsb_client, session_manager, settings)
    }

    // --- Argument deserialization tests ---

    #[test]
    fn test_browser_navigate_args() {
        let json = r#"{
            "session_id": "test-session-1",
            "url": "https://example.com"
        }"#;
        let args: BrowserNavigateArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
        assert_eq!(args.url, "https://example.com");
    }

    #[test]
    fn test_browser_get_clickable_elements_args() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: BrowserGetClickableElementsArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
    }

    #[test]
    fn test_browser_click_args_with_index() {
        let json = r#"{
            "session_id": "test-session-1",
            "index": 3
        }"#;
        let args: BrowserClickArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
        assert_eq!(args.index, Some(3));
        assert!(args.selector.is_none());
    }

    #[test]
    fn test_browser_click_args_with_selector() {
        let json = r##"{
            "session_id": "test-session-1",
            "selector": "#submit-button"
        }"##;
        let args: BrowserClickArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.selector, Some("#submit-button".to_string()));
        assert!(args.index.is_none());
    }

    #[test]
    fn test_browser_click_args_minimal() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: BrowserClickArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
        assert!(args.index.is_none());
        assert!(args.selector.is_none());
    }

    #[test]
    fn test_browser_fill_args_full() {
        let json = r##"{
            "session_id": "test-session-1",
            "selector": "#search-input",
            "value": "hello world",
            "clear": true
        }"##;
        let args: BrowserFillArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.selector, "#search-input");
        assert_eq!(args.value, "hello world");
        assert_eq!(args.clear, Some(true));
    }

    #[test]
    fn test_browser_fill_args_minimal() {
        let json = r##"{
            "session_id": "test-session-1",
            "selector": "#search-input",
            "value": "test"
        }"##;
        let args: BrowserFillArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.value, "test");
        assert!(args.clear.is_none());
    }

    #[test]
    fn test_browser_scroll_args_with_amount() {
        let json = r#"{
            "session_id": "test-session-1",
            "amount": 500
        }"#;
        let args: BrowserScrollArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.amount, Some(500));
    }

    #[test]
    fn test_browser_scroll_args_negative() {
        let json = r#"{
            "session_id": "test-session-1",
            "amount": -200
        }"#;
        let args: BrowserScrollArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.amount, Some(-200));
    }

    #[test]
    fn test_browser_scroll_args_minimal() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: BrowserScrollArgs = serde_json::from_str(json).unwrap();
        assert!(args.amount.is_none());
    }

    #[test]
    fn test_browser_screenshot_args_full() {
        let json = r##"{
            "session_id": "test-session-1",
            "name": "homepage",
            "full_page": true,
            "selector": "#main-content"
        }"##;
        let args: BrowserScreenshotArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.name, Some("homepage".to_string()));
        assert_eq!(args.full_page, Some(true));
        assert_eq!(args.selector, Some("#main-content".to_string()));
    }

    #[test]
    fn test_browser_screenshot_args_minimal() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: BrowserScreenshotArgs = serde_json::from_str(json).unwrap();
        assert!(args.name.is_none());
        assert!(args.full_page.is_none());
        assert!(args.selector.is_none());
    }

    #[test]
    fn test_browser_new_tab_args_with_url() {
        let json = r#"{
            "session_id": "test-session-1",
            "url": "https://example.com"
        }"#;
        let args: BrowserNewTabArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.url, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_browser_new_tab_args_minimal() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: BrowserNewTabArgs = serde_json::from_str(json).unwrap();
        assert!(args.url.is_none());
    }

    #[test]
    fn test_browser_tab_list_args() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: BrowserTabListArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
    }

    #[test]
    fn test_browser_switch_tab_args() {
        let json = r#"{
            "session_id": "test-session-1",
            "index": 2
        }"#;
        let args: BrowserSwitchTabArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.index, 2);
    }

    #[test]
    fn test_browser_health_check_args() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: BrowserHealthCheckArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
    }

    #[test]
    fn test_browser_go_back_args() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: BrowserGoBackArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
    }

    #[test]
    fn test_browser_go_forward_args() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: BrowserGoForwardArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
    }

    #[test]
    fn test_browser_evaluate_args() {
        let json = r#"{
            "session_id": "test-session-1",
            "script": "document.title"
        }"#;
        let args: BrowserEvaluateArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.script, "document.title");
    }

    #[test]
    fn test_browser_evaluate_args_complex_script() {
        let json = r#"{
            "session_id": "test-session-1",
            "script": "const els = document.querySelectorAll('a'); return els.length;"
        }"#;
        let args: BrowserEvaluateArgs = serde_json::from_str(json).unwrap();
        assert!(args.script.contains("querySelectorAll"));
    }

    #[test]
    fn test_browser_close_args() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: BrowserCloseArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
    }

    // --- Tool registration tests ---

    #[tokio::test]
    async fn test_all_14_tools_registered() {
        let service = create_test_service();
        let tools = service.tool_router.list_all();
        assert_eq!(tools.len(), 14, "Should have exactly 14 tools registered");

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();

        let expected = [
            "browser_navigate",
            "browser_get_clickable_elements",
            "browser_click",
            "browser_fill",
            "browser_scroll",
            "browser_screenshot",
            "browser_new_tab",
            "browser_tab_list",
            "browser_switch_tab",
            "browser_health_check",
            "browser_go_back",
            "browser_go_forward",
            "browser_evaluate",
            "browser_close",
        ];

        for name in &expected {
            assert!(
                tool_names.contains(&name.to_string()),
                "Missing tool: {}",
                name
            );
        }
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

    // --- ServerHandler tests ---

    #[tokio::test]
    async fn test_server_handler_info() {
        let service = create_test_service();
        let info = service.get_info();
        assert_eq!(info.server_info.name, "dsb-browser-service");
    }

    // --- Session resolution tests ---

    #[test]
    fn test_resolve_session_found() {
        let service = create_test_service();
        let sandbox_id = Uuid::new_v4();
        service
            .session_manager
            .set("test-session".to_string(), sandbox_id);

        let result = service.session_manager.get("test-session");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), sandbox_id);
    }

    #[test]
    fn test_session_manager_initially_empty() {
        let service = create_test_service();
        assert!(service.session_manager.is_empty());
    }
}
