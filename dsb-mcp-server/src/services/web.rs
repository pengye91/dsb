// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Web Service for web scraping and search via session-based sandbox management.
//!
//! This service provides MCP tools for web fetching and web search, mirroring
//! the Python implementation at `tools/dsb_tools/web.py`. Web fetch uses a
//! session-bound sandbox to execute the web scraping tool, while web search
//! queries the configured SearXNG instance directly.

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
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

// ========== Service Definition ==========

/// Web service with session-based web scraping and search tool routing.
///
/// Manages web operations via session IDs, providing automatic sandbox creation
/// and reuse for web fetch operations. Web search operates independently of
/// sandboxes, querying the SearXNG instance directly.
#[derive(Debug, Clone)]
pub struct WebService {
    dsb_client: Arc<DSBClient>,
    session_manager: Arc<SessionManager>,
    settings: Arc<Settings>,
    tool_router: ToolRouter<WebService>,
}

impl WebService {
    /// Create a new web service.
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

/// Arguments for fetching (scraping) a web page.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WebFetchArgs {
    /// Session ID for sandbox resolution. A sandbox is created automatically if none exists.
    pub session_id: String,
    /// URL of the web page to fetch.
    pub url: String,
    /// Output format: "markdown" (default), "html", "text", or "links".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Optional search query to highlight relevant sections within the page content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_query: Option<String>,
    /// Maximum length of the returned content in characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
}

/// Arguments for searching the web via SearXNG.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WebSearchArgs {
    /// Search query string.
    pub query: String,
    /// Search engine to use (e.g., "google", "duckduckgo", "bing", "baidu").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engines: Option<String>,
    /// Language for search results (e.g., "en", "zh"). Not yet supported by Rust integration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Search categories (e.g., "general", "news", "it"). Not yet supported by Rust integration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<String>,
    /// Time range filter (e.g., "day", "week", "month", "year"). Not yet supported by Rust integration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_range: Option<String>,
    /// Number of search results to return (1-100, default 10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_num: Option<usize>,
    /// Request timeout in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<f64>,
}

// ========== Tool Router ==========

#[tool_router]
impl WebService {
    #[tool(
        description = "Fetch and extract content from a web page. Uses a sandbox-bound web scraper that converts pages to markdown, HTML, or text. Supports content filtering by search query."
    )]
    async fn web_fetch(
        &self,
        Parameters(WebFetchArgs {
            session_id,
            url,
            format,
            search_query,
            max_length,
        }): Parameters<WebFetchArgs>,
    ) -> Result<String, ErrorData> {
        info!(
            session_id = %session_id,
            url = %url,
            "Fetching web page"
        );

        // Validate URL for SSRF prevention
        if let Err(e) = crate::tools::web::validate_url_secure(&url) {
            return Err(ErrorData::invalid_params(
                "web_fetch",
                Some(serde_json::json!(e)),
            ));
        }

        // Resolve or create sandbox for this session
        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        // Delegate to the existing scrape_web helper with all settings wired through
        let config = crate::tools::web::ScrapeWebConfig {
            sandbox_id: sandbox_id.to_string(),
            url: url.clone(),
            format: format.clone(),
            screenshot: None,
            css_selector: None,
            word_count_threshold: Some(self.settings.web.word_count_threshold),
            search_query: search_query.clone(),
            use_pruning: None, // not yet exposed via MCP args
            pruning_threshold: Some(self.settings.web.pruning_threshold),
            bm25_threshold: Some(self.settings.web.bm25_threshold),
            wait_until: Some(self.settings.web.wait_until.clone()),
            cache_mode: Some(self.settings.web.cache_mode.clone()),
            page_timeout: Some(self.settings.web.page_timeout),
            max_length,
            proxy_config: None, // not yet exposed via MCP args
            allow_exec_fallback: self.settings.web.allow_exec_fallback,
        };
        let content = crate::tools::web::scrape_web(&self.dsb_client, config)
            .await
            .map_err(|e| {
                ErrorData::internal_error("web_fetch", Some(serde_json::json!(e.to_string())))
            })?;

        // Wrap the scraped content in a JSON response
        let result = serde_json::json!({
            "url": url,
            "title": "",
            "content": content,
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "web_fetch",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Search the web using the configured SearXNG instance. Returns formatted search results with titles, URLs, and snippets."
    )]
    async fn web_search(
        &self,
        Parameters(WebSearchArgs {
            query,
            engines,
            language,
            categories,
            time_range,
            result_num,
            timeout,
        }): Parameters<WebSearchArgs>,
    ) -> Result<String, ErrorData> {
        info!(query = %query, "Searching the web");

        // Delegate to the existing search_web helper with all parameters wired through
        let config = crate::tools::web::SearchWebConfig {
            query,
            engine: engines,
            num_results: result_num,
            timeout,
            language,
            categories,
            time_range,
        };
        let results = crate::tools::web::search_web(&self.dsb_client, config)
            .await
            .map_err(|e| {
                ErrorData::internal_error("web_search", Some(serde_json::json!(e.to_string())))
            })?;

        Ok(results)
    }
}

// ========== ServerHandler Implementation ==========

impl ServerHandler for WebService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::V_2025_06_18,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "dsb-web-service".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "DSB Web Service - Web scraping and search tools via session-based sandbox management.".to_string(),
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
        tracing::info!("list_tools: returning {} web tools", tools.len());
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

    fn create_test_service() -> WebService {
        let settings = Settings::load_for_tests().unwrap();
        let dsb_client = Arc::new(DSBClient::new(settings.clone()).unwrap());
        let session_manager = Arc::new(SessionManager::new());
        let settings = Arc::new(settings);
        WebService::new(dsb_client, session_manager, settings)
    }

    // --- Argument deserialization tests ---

    #[test]
    fn test_web_fetch_args_full() {
        let json = r#"{
            "session_id": "test-session-1",
            "url": "https://example.com",
            "format": "markdown",
            "search_query": "rust programming",
            "max_length": 5000
        }"#;
        let args: WebFetchArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
        assert_eq!(args.url, "https://example.com");
        assert_eq!(args.format, Some("markdown".to_string()));
        assert_eq!(args.search_query, Some("rust programming".to_string()));
        assert_eq!(args.max_length, Some(5000));
    }

    #[test]
    fn test_web_fetch_args_minimal() {
        let json = r#"{
            "session_id": "test-session-1",
            "url": "https://example.com"
        }"#;
        let args: WebFetchArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
        assert_eq!(args.url, "https://example.com");
        assert!(args.format.is_none());
        assert!(args.search_query.is_none());
        assert!(args.max_length.is_none());
    }

    #[test]
    fn test_web_search_args_full() {
        let json = r#"{
            "query": "rust programming language",
            "engines": "google",
            "language": "en",
            "categories": "general",
            "time_range": "month",
            "result_num": 20,
            "timeout": 30.0
        }"#;
        let args: WebSearchArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.query, "rust programming language");
        assert_eq!(args.engines, Some("google".to_string()));
        assert_eq!(args.language, Some("en".to_string()));
        assert_eq!(args.categories, Some("general".to_string()));
        assert_eq!(args.time_range, Some("month".to_string()));
        assert_eq!(args.result_num, Some(20));
        assert_eq!(args.timeout, Some(30.0));
    }

    #[test]
    fn test_web_search_args_minimal() {
        let json = r#"{"query": "hello world"}"#;
        let args: WebSearchArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.query, "hello world");
        assert!(args.engines.is_none());
        assert!(args.language.is_none());
        assert!(args.categories.is_none());
        assert!(args.time_range.is_none());
        assert!(args.result_num.is_none());
        assert!(args.timeout.is_none());
    }

    #[test]
    fn test_web_search_args_with_multiple_engines() {
        let json = r#"{
            "query": "test query",
            "engines": "google,bing"
        }"#;
        let args: WebSearchArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.engines, Some("google,bing".to_string()));
    }

    // --- Tool registration tests ---

    #[tokio::test]
    async fn test_all_2_tools_registered() {
        let service = create_test_service();
        let tools = service.tool_router.list_all();
        assert_eq!(tools.len(), 2, "Should have exactly 2 tools registered");

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();

        assert!(tool_names.contains(&"web_fetch".to_string()));
        assert!(tool_names.contains(&"web_search".to_string()));
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
        assert_eq!(info.server_info.name, "dsb-web-service");
    }

    // --- resolve_or_create_session test ---

    #[test]
    fn test_resolve_session_missing_will_create() {
        // We can't test the full creation flow without a DSB server,
        // but we can verify the session manager starts empty.
        let service = create_test_service();
        assert!(service.session_manager.is_empty());
    }

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

    // --- web_search with invalid query ---

    #[tokio::test]
    async fn test_web_search_empty_query_returns_error() {
        let service = create_test_service();

        let args = Parameters(WebSearchArgs {
            query: "".to_string(),
            engines: None,
            language: None,
            categories: None,
            time_range: None,
            result_num: None,
            timeout: None,
        });

        let result = service.web_search(args).await;
        assert!(result.is_err(), "Empty query should return an error");
    }

    #[tokio::test]
    async fn test_web_search_whitespace_query_returns_error() {
        let service = create_test_service();

        let args = Parameters(WebSearchArgs {
            query: "   ".to_string(),
            engines: None,
            language: None,
            categories: None,
            time_range: None,
            result_num: None,
            timeout: None,
        });

        let result = service.web_search(args).await;
        assert!(
            result.is_err(),
            "Whitespace-only query should return an error"
        );
    }

    #[tokio::test]
    async fn test_web_search_unsupported_engine_returns_error() {
        let service = create_test_service();

        let args = Parameters(WebSearchArgs {
            query: "test query".to_string(),
            engines: Some("altavista".to_string()),
            language: None,
            categories: None,
            time_range: None,
            result_num: None,
            timeout: None,
        });

        let result = service.web_search(args).await;
        assert!(result.is_err(), "Unsupported engine should return an error");
    }
}
