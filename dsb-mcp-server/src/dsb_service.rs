// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! DSB MCP Service
//!
//! Main service implementation using rmcp SDK v0.12 with declarative macros.

use crate::dsb_client::{DSBClient, PortMapping, ResourceLimits, VolumeMount};
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
use std::collections::HashMap;
use std::sync::Arc;

/// DSB MCP Service
///
/// Main service struct with tool router for declarative tool registration.
#[derive(Debug, Clone)]
pub struct DSBService {
    dsb_client: Arc<DSBClient>,
    tool_router: ToolRouter<DSBService>,
}

impl DSBService {
    /// Create a new DSB service
    pub fn new(dsb_client: DSBClient) -> Self {
        Self {
            dsb_client: Arc::new(dsb_client),
            tool_router: Self::tool_router(),
        }
    }

    /// Get the DSB client
    pub fn client(&self) -> &DSBClient {
        &self.dsb_client
    }
}

// ========== Tool Argument Schemas ==========

/// Arguments for creating a sandbox with full configuration
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateSandboxArgs {
    /// Docker image to use (e.g., 'python:3.12')
    pub image: String,
    /// Optional name for the sandbox
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Command to run in the container
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// Environment variables as key-value pairs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<HashMap<String, String>>,
    /// Port mappings from host to container
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_mappings: Option<Vec<PortMapping>>,
    /// Resource limits for the container
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_limits: Option<ResourceLimits>,
    /// Volume mounts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volumes: Option<Vec<VolumeMount>>,
    /// Auto-delete sandbox after inactivity timeout
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inactivity_timeout_minutes: Option<u64>,
    /// Docker image pull policy (e.g., 'Always', 'IfNotPresent')
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pull_policy: Option<String>,
}

/// Arguments for deleting a sandbox
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteSandboxArgs {
    /// UUID of the sandbox to delete
    pub sandbox_id: String,
}

/// Arguments for executing code
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExecuteCodeArgs {
    /// UUID of the sandbox to execute code in
    pub sandbox_id: String,
    /// Python code to execute
    pub code: String,
}

/// Arguments for executing bash command
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExecuteBashArgs {
    /// UUID of the sandbox to execute command in
    pub sandbox_id: String,
    /// Bash command to execute
    pub command: String,
}

/// Arguments for web scraping
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScrapeWebArgs {
    /// UUID of the sandbox with browser tools
    pub sandbox_id: String,
    /// URL to scrape
    pub url: String,
    /// Output format (markdown, links, cleaned)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// CSS selector for targeted scraping
    #[serde(skip_serializing_if = "Option::is_none")]
    pub css_selector: Option<String>,
    /// Capture screenshot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<bool>,
    /// Minimum word count threshold
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_count_threshold: Option<usize>,
}

/// Arguments for web search
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchWebArgs {
    /// Search query. Supports phrases in quotes and site: filters.
    pub query: String,
    /// Search engine: 'google', 'duckduckgo', 'bing', or 'baidu'. Omit for default aggregated search.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// Number of results to return (default: 10, max: 100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_results: Option<usize>,
}

/// Arguments for browser automation
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AutomateBrowserArgs {
    /// UUID of the sandbox with browser tools
    pub sandbox_id: String,
    /// Browser action (navigate, click, fill, submit, wait, screenshot, etc.)
    pub action: String,
    /// URL to navigate to (for navigate action)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// CSS selector for element interaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    /// Value to input (for fill action)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

// ========== Tool Router ==========
// IMPORTANT: This MUST come before ServerHandler impl for #[tool_handler] to work!

#[tool_router]
impl DSBService {
    // ========== Sandbox Tools ==========

    #[tool(
        description = "Create a new isolated Docker sandbox for code execution, web scraping, or browser automation. Returns the sandbox ID (UUID) required by all other tools. Use image 'dsb/sandbox:latest' for web/browser capabilities, 'python:3.12' for code execution."
    )]
    async fn create_sandbox(
        &self,
        Parameters(CreateSandboxArgs {
            image,
            name,
            command,
            environment,
            port_mappings,
            resource_limits,
            volumes,
            inactivity_timeout_minutes,
            pull_policy,
        }): Parameters<CreateSandboxArgs>,
    ) -> Result<String, ErrorData> {
        // Build the environment, starting with defaults from settings
        // and merging any user-provided overrides.
        // Note: DSBService needs to have settings. For backward compatibility
        // we load it here, but ideally it should be passed in.
        let settings = crate::settings::Settings::load().unwrap_or_default();
        let mut final_env = settings.get_sandbox_env();
        if let Some(user_env) = environment {
            final_env.extend(user_env);
        }

        let config = crate::dsb_client::CreateSandboxConfig {
            image,
            name,
            environment: Some(final_env),
            port_mappings,
            resource_limits,
            volumes,
            command,
            inactivity_timeout_minutes,
            pull_policy,
        };

        self.dsb_client
            .create_sandbox_full(config)
            .await
            .map(|sandbox| {
                format!(
                    "Created sandbox: {} (image: {}, state: {})",
                    sandbox.id, sandbox.config.image, sandbox.state
                )
            })
            .map_err(|e| {
                ErrorData::internal_error("create_sandbox", Some(serde_json::json!(e.to_string())))
            })
    }

    #[tool(
        description = "List all active sandboxes with their IDs, names, states, and images. Use to find sandbox IDs for other tools."
    )]
    async fn list_sandboxes(&self) -> Result<String, ErrorData> {
        self.dsb_client
            .list_sandboxes()
            .await
            .map(|sandboxes| {
                if sandboxes.is_empty() {
                    "No sandboxes found".to_string()
                } else {
                    let mut result = String::from("Sandboxes:\n");
                    for sandbox in sandboxes {
                        result.push_str(&format!(
                            "  - {}: {} ({})\n",
                            sandbox.id,
                            sandbox.config.name.as_deref().unwrap_or("unnamed"),
                            sandbox.state
                        ));
                    }
                    result
                }
            })
            .map_err(|e| {
                ErrorData::internal_error("list_sandboxes", Some(serde_json::json!(e.to_string())))
            })
    }

    #[tool(
        description = "Permanently delete a sandbox and free all its resources. Use when done to prevent resource leaks."
    )]
    async fn delete_sandbox(
        &self,
        Parameters(DeleteSandboxArgs { sandbox_id }): Parameters<DeleteSandboxArgs>,
    ) -> Result<String, ErrorData> {
        let id = uuid::Uuid::parse_str(&sandbox_id).map_err(|e| {
            ErrorData::invalid_params("delete_sandbox", Some(serde_json::json!(e.to_string())))
        })?;

        self.dsb_client
            .delete_sandbox(id)
            .await
            .map(|_| format!("Deleted sandbox: {}", sandbox_id))
            .map_err(|e| {
                ErrorData::internal_error("delete_sandbox", Some(serde_json::json!(e.to_string())))
            })
    }

    // ========== Execution Tools ==========

    #[tool(
        description = "Execute Python code inside a sandbox. Supports multi-line scripts with imports and pip packages. Returns stdout, stderr, and exit code."
    )]
    async fn execute_code(
        &self,
        Parameters(ExecuteCodeArgs { sandbox_id, code }): Parameters<ExecuteCodeArgs>,
    ) -> Result<String, ErrorData> {
        let id = uuid::Uuid::parse_str(&sandbox_id).map_err(|e| {
            ErrorData::invalid_params("execute_code", Some(serde_json::json!(e.to_string())))
        })?;

        // Use sh -c with single-quoted code to avoid shell mis-parsing multi-arg commands.
        let escaped = code.replace('\'', "'\\''");
        let command = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("python3 -c '{}'", escaped),
        ];

        self.dsb_client
            .exec_command(id, command)
            .await
            .map(|result| result.output)
            .map_err(|e| {
                ErrorData::internal_error("execute_code", Some(serde_json::json!(e.to_string())))
            })
    }

    #[tool(
        description = "Execute a shell command inside a sandbox. Supports pipes, redirects, and chaining (&&, ||). Returns stdout, stderr, and exit code."
    )]
    async fn execute_bash(
        &self,
        Parameters(ExecuteBashArgs {
            sandbox_id,
            command,
        }): Parameters<ExecuteBashArgs>,
    ) -> Result<String, ErrorData> {
        let id = uuid::Uuid::parse_str(&sandbox_id).map_err(|e| {
            ErrorData::invalid_params("execute_bash", Some(serde_json::json!(e.to_string())))
        })?;

        let cmd = vec!["sh".to_string(), "-c".to_string(), command];

        self.dsb_client
            .exec_command(id, cmd)
            .await
            .map(|result| result.output)
            .map_err(|e| {
                ErrorData::internal_error("execute_bash", Some(serde_json::json!(e.to_string())))
            })
    }

    // ========== Web Scraping Tools ==========

    #[tool(
        description = "Scrape a web page with JavaScript rendering via headless Chromium. Extracts content as markdown, plain text, HTML, or links. Supports CSS selectors for targeted extraction and optional screenshots. Requires a browser-capable sandbox (image: 'dsb/sandbox:latest')."
    )]
    async fn scrape_web(
        &self,
        Parameters(ScrapeWebArgs {
            sandbox_id,
            url,
            format,
            css_selector,
            screenshot,
            word_count_threshold,
        }): Parameters<ScrapeWebArgs>,
    ) -> Result<String, ErrorData> {
        uuid::Uuid::parse_str(&sandbox_id).map_err(|e| {
            ErrorData::invalid_params("scrape_web", Some(serde_json::json!(e.to_string())))
        })?;

        let word_count_threshold = word_count_threshold
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    ErrorData::invalid_params(
                        "scrape_web",
                        Some(serde_json::json!(
                            "word_count_threshold exceeds supported range"
                        )),
                    )
                })
            })
            .transpose()?;

        let config = crate::tools::web::ScrapeWebConfig {
            sandbox_id,
            url,
            format,
            screenshot,
            css_selector,
            word_count_threshold,
            search_query: None,
            use_pruning: None,
            pruning_threshold: None,
            bm25_threshold: None,
            wait_until: None,
            cache_mode: None,
            page_timeout: None,
            max_length: None,
            proxy_config: None,
            allow_exec_fallback: true, // legacy DSBService: allow exec fallback for Docker-style sandboxes
        };

        crate::tools::web::scrape_web(self.dsb_client.as_ref(), config)
            .await
            .map_err(|e| {
                ErrorData::internal_error("scrape_web", Some(serde_json::json!(e.to_string())))
            })
    }

    #[tool(
        description = "Search the web using the configured SearXNG search engine. Returns results with titles, URLs, and snippets. Does NOT require a sandbox."
    )]
    async fn search_web(
        &self,
        Parameters(SearchWebArgs {
            query,
            engine,
            num_results,
        }): Parameters<SearchWebArgs>,
    ) -> Result<String, ErrorData> {
        let config = crate::tools::web::SearchWebConfig {
            query,
            engine,
            num_results,
            timeout: None,
            language: None,
            categories: None,
            time_range: None,
        };

        crate::tools::web::search_web(self.dsb_client.as_ref(), config)
            .await
            .map_err(|e| {
                ErrorData::internal_error("search_web", Some(serde_json::json!(e.to_string())))
            })
    }

    // ========== Browser Automation ==========

    #[tool(
        description = "Perform interactive browser actions: navigate to URLs, click elements, type text, take screenshots, scroll, and extract page content. Requires a browser-capable sandbox (image: 'dsb/sandbox:latest')."
    )]
    async fn automate_browser(
        &self,
        Parameters(AutomateBrowserArgs {
            sandbox_id,
            action,
            url,
            selector,
            value,
        }): Parameters<AutomateBrowserArgs>,
    ) -> Result<String, ErrorData> {
        // Validate UUID format
        let _id = uuid::Uuid::parse_str(&sandbox_id).map_err(|e| {
            ErrorData::invalid_params("automate_browser", Some(serde_json::json!(e.to_string())))
        })?;

        // Delegate to the unified Python agent_browser_tools.py via tool proxy.
        // This replaces the legacy Node.js browser_tools.js path.
        crate::tools::browser::automate_browser(
            &self.dsb_client,
            sandbox_id,
            action,
            selector,
            value,
            url,
        )
        .await
        .map_err(|e| {
            ErrorData::internal_error("automate_browser", Some(serde_json::json!(e.to_string())))
        })
    }
}

// ========== ServerHandler Implementation ==========
// IMPORTANT: This MUST come AFTER #[tool_router] for manual delegation to work!

impl ServerHandler for DSBService {
    /// Get server information
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::V_2025_06_18, // Latest MCP spec for Streamable HTTP
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "dsb-mcp-server".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "DSB MCP Server for distributed sandboxes with web scraping and browser automation."
                    .to_string(),
            ),
        }
    }

    /// Initialize the session
    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        Ok(self.get_info())
    }

    /// List available tools - manually delegates to tool_router
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = self.tool_router.list_all();
        tracing::info!("list_tools: returning {} tools", tools.len());
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: Default::default(),
        })
    }

    /// Call a tool - manually delegates to tool_router
    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        // Look up the tool by name
        let tool_name = &request.name;
        let tool_route = self.tool_router.map.get(tool_name).ok_or_else(|| {
            ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("Tool not found: {}", tool_name),
                None,
            )
        })?;

        // Create the tool call context and invoke the handler
        let tool_ctx = ToolCallContext::new(self, request.clone(), ctx);
        (tool_route.call)(tool_ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[tokio::test]
    async fn test_tool_router_has_tools() {
        let dsb_client = DSBClient::new(Settings::load_for_tests().unwrap()).unwrap();
        let service = DSBService::new(dsb_client);

        let tools = service.tool_router.list_all();
        println!("Number of tools: {}", tools.len());
        println!(
            "Tools: {:?}",
            tools.iter().map(|t| &t.name).collect::<Vec<_>>()
        );

        assert!(
            !tools.is_empty(),
            "Tool router should have tools registered"
        );
    }

    #[tokio::test]
    async fn test_all_8_tools_registered() {
        let dsb_client = DSBClient::new(Settings::load_for_tests().unwrap()).unwrap();
        let service = DSBService::new(dsb_client);

        let tools = service.tool_router.list_all();
        assert_eq!(tools.len(), 8, "Should have exactly 8 tools registered");

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();

        // Verify all expected tools are present
        assert!(tool_names.contains(&"create_sandbox".to_string()));
        assert!(tool_names.contains(&"list_sandboxes".to_string()));
        assert!(tool_names.contains(&"delete_sandbox".to_string()));
        assert!(tool_names.contains(&"execute_code".to_string()));
        assert!(tool_names.contains(&"execute_bash".to_string()));
        assert!(tool_names.contains(&"scrape_web".to_string()));
        assert!(tool_names.contains(&"search_web".to_string()));
        assert!(tool_names.contains(&"automate_browser".to_string()));
    }

    #[tokio::test]
    async fn test_handler_list_sandboxes_direct() {
        let dsb_client = DSBClient::new(Settings::load_for_tests().unwrap()).unwrap();
        let service = DSBService::new(dsb_client);

        let result = service.list_sandboxes().await;
        match result {
            Ok(text) => {
                println!("✅ list_sandboxes handler executed: {}", text);
            }
            Err(e) => {
                println!("⚠️  list_sandboxes returned error: {}", e.message);
            }
        }
    }

    #[tokio::test]
    async fn test_handler_delete_sandbox_direct() {
        let dsb_client = DSBClient::new(Settings::load_for_tests().unwrap()).unwrap();
        let service = DSBService::new(dsb_client);

        let args = Parameters(DeleteSandboxArgs {
            sandbox_id: "123e4567-e89b-12d3-a456-426614174000".to_string(),
        });

        let result = service.delete_sandbox(args).await;
        match result {
            Ok(text) => {
                println!("✅ delete_sandbox handler executed: {}", text);
            }
            Err(e) => {
                println!("⚠️  delete_sandbox returned expected error: {}", e.message);
            }
        }
    }
}
