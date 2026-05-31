// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Monorail MCP Client Agent
//!
//! This module provides the MonorailAgent which connects to the dsb-mcp-server
//! via the MCP protocol over HTTP and provides access to all available tools.

use anyhow::Context;
use rmcp::model::{CallToolRequestParams, ClientInfo, JsonObject, Tool};
use rmcp::service::{RoleClient, ServiceExt};
use rmcp::transport::StreamableHttpClientTransport;
use std::env;
use tracing::info;

/// Default MCP server URL constant
const DEFAULT_MCP_SERVER_URL: &str = "http://localhost:3223/mcp";

/// Expected number of tools that should be available
const EXPECTED_TOOL_COUNT: usize = 8;

/// The 8 tool names expected from the MCP server (DSBService at /mcp)
pub const EXPECTED_TOOL_NAMES: &[&str] = &[
    "create_sandbox",
    "list_sandboxes",
    "delete_sandbox",
    "execute_code",
    "execute_bash",
    "scrape_web",
    "search_web",
    "automate_browser",
];

/// MonorailAgent connects to the dsb-mcp-server and provides access to MCP tools.
///
/// This client uses the rmcp crate to communicate with the MCP server over
/// HTTP using the streamable HTTP transport.
#[derive(Debug)]
pub struct MonorailAgent {
    /// The running MCP client service
    running_service: rmcp::service::RunningService<RoleClient, ClientInfo>,
    /// Whether the agent is connected
    connected: bool,
}

impl MonorailAgent {
    /// Gets the configured MCP server URL
    fn get_mcp_url() -> String {
        if let Ok(url) = env::var("DSB_MCP_URL") {
            return url;
        }
        if let Ok(url) = env::var("DSB_API_URL") {
            // Check if the API URL is set but an API key is also needed?
            return format!("{}/mcp", url);
        }
        DEFAULT_MCP_SERVER_URL.to_string()
    }

    /// Creates a new MonorailAgent by connecting to the MCP server.
    ///
    /// This establishes the MCP client connection and performs the initialization
    /// handshake with the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection fails or the server rejects initialization.
    pub async fn new() -> anyhow::Result<Self> {
        let mcp_url = Self::get_mcp_url();
        Self::connect_to_url(mcp_url).await
    }

    /// Connect to a specific MCP service endpoint.
    ///
    /// Constructs the service URL from the base MCP URL by appending `/dsb/{service}`.
    /// For example, if `DSB_MCP_URL` is `http://localhost:3000/mcp` and service is `"web"`,
    /// connects to `http://localhost:3000/mcp/dsb/web`.
    ///
    /// If `DSB_MCP_URL` already contains a service path (e.g. `/mcp/dsb/web`),
    /// replaces the service segment with the requested one.
    pub async fn connect_to_service(service: &str) -> anyhow::Result<Self> {
        let base_url = Self::get_mcp_url();
        let service_url = if base_url.contains("/mcp/dsb/") {
            // Replace existing service segment
            let prefix = base_url.rsplit_once('/').map(|x| x.0).unwrap_or(&base_url);
            format!("{}/{}", prefix, service)
        } else if base_url.ends_with("/mcp") {
            format!("{}/dsb/{}", base_url, service)
        } else {
            format!("{}/mcp/dsb/{}", base_url, service)
        };
        Self::connect_to_url(service_url).await
    }

    /// Connect to an explicit MCP HTTP endpoint (e.g. `/mcp/dsb/web` vs `/mcp/dsb/sandbox`).
    ///
    /// Uses `DSB_API_KEY` the same way as [`Self::new`].
    pub async fn connect_to_url(mcp_url: impl Into<String>) -> anyhow::Result<Self> {
        let mcp_url = mcp_url.into();
        info!("Connecting to MCP server at {}", mcp_url);

        let mcp_url_with_auth = if let Ok(api_key) = env::var("DSB_API_KEY") {
            if mcp_url.contains('?') {
                format!("{}&api_key={}", mcp_url, api_key)
            } else {
                format!("{}?api_key={}", mcp_url, api_key)
            }
        } else {
            mcp_url.clone()
        };

        let mut config =
            rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
                mcp_url_with_auth,
            );

        if let Ok(api_key) = env::var("DSB_API_KEY") {
            config = config.auth_header(api_key);
        }

        let mut headers = reqwest::header::HeaderMap::new();
        if let Ok(api_key) = env::var("DSB_API_KEY") {
            let mut auth_value = reqwest::header::HeaderValue::from_str(&api_key)
                .context("Invalid API key format")?;
            auth_value.set_sensitive(true);
            headers.insert("x-api-key", auth_value);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to build HTTP client")?;

        let transport = StreamableHttpClientTransport::with_client(client, config);

        // Create client info for initialization
        let client_info = ClientInfo::default();

        // Start the client service - this performs the MCP handshake
        let running_service = client_info
            .serve(transport)
            .await
            .context("Failed to connect to MCP server")?;

        info!("Successfully connected to MCP server");

        Ok(Self {
            running_service,
            connected: true,
        })
    }

    /// Lists all available tools from the MCP server.
    ///
    /// This calls the `tools/list` endpoint to retrieve all tools that the
    /// server has registered.
    ///
    /// # Errors
    ///
    /// Returns an error if the RPC call fails.
    pub async fn list_tools(&self) -> anyhow::Result<Vec<Tool>> {
        let peer = &*self.running_service;

        // Call list_tools - None for paginated request gets all tools
        let result = peer.list_tools(None).await?;

        info!("Listed {} tools from server", result.tools.len());
        Ok(result.tools)
    }

    /// Lists all available tools, following pagination if needed.
    ///
    /// This is a convenience method that wraps `list_tools()` and ensures
    /// all tools are retrieved even if the server uses pagination.
    ///
    /// # Errors
    ///
    /// Returns an error if the RPC call fails.
    pub async fn list_all_tools(&self) -> anyhow::Result<Vec<Tool>> {
        let peer = &*self.running_service;
        let tools = peer.list_all_tools().await?;
        info!("Listed {} total tools from server", tools.len());
        Ok(tools)
    }

    /// Calls a tool on the MCP server with the given name and arguments.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the tool to call (e.g., "create_sandbox")
    /// * `arguments` - A JSON object containing the tool arguments, or None for no arguments
    ///
    /// # Errors
    ///
    /// Returns an error if the RPC call fails.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
    ) -> anyhow::Result<rmcp::model::CallToolResult> {
        let peer = &*self.running_service;

        let mut request = CallToolRequestParams::new(name.to_string());
        if let Some(args) = arguments {
            request = request.with_arguments(args);
        }

        info!("Calling tool: {}", name);
        let result = peer.call_tool(request).await?;

        info!("Tool {} returned successfully", name);
        Ok(result)
    }

    /// Verifies that all 13 expected tools are available.
    ///
    /// # Errors
    ///
    /// Returns an error if any expected tool is missing.
    pub async fn verify_tools(&self) -> anyhow::Result<()> {
        let tools = self.list_all_tools().await?;
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

        for expected in EXPECTED_TOOL_NAMES {
            if !tool_names.contains(expected) {
                anyhow::bail!(
                    "Missing expected tool: {}. Found tools: {:?}",
                    expected,
                    tool_names
                );
            }
        }

        if tools.len() != EXPECTED_TOOL_COUNT {
            anyhow::bail!(
                "Expected {} tools but found {}. Tools: {:?}",
                EXPECTED_TOOL_COUNT,
                tools.len(),
                tool_names
            );
        }

        info!("All {} expected tools verified", EXPECTED_TOOL_COUNT);
        Ok(())
    }

    /// Returns whether the agent is connected to the MCP server.
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Gets the list of tool names from the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the RPC call fails.
    pub async fn get_tool_names(&self) -> anyhow::Result<Vec<String>> {
        let tools = self.list_all_tools().await?;
        Ok(tools.into_iter().map(|t| t.name.into_owned()).collect())
    }

    /// Checks if the MCP server is reachable at the configured URL.
    pub async fn is_server_reachable() -> bool {
        let mcp_url = Self::get_mcp_url();
        let mut client_builder = reqwest::Client::builder();
        if let Ok(api_key) = env::var("DSB_API_KEY") {
            let mut headers = reqwest::header::HeaderMap::new();
            if let Ok(mut auth_value) = reqwest::header::HeaderValue::from_str(&api_key) {
                auth_value.set_sensitive(true);
                headers.insert("x-api-key", auth_value);
                client_builder = client_builder.default_headers(headers);
            }
        }

        let client = match client_builder.build() {
            Ok(c) => c,
            Err(_) => return false,
        };

        client
            .get(&mcp_url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_utils::{extract_sandbox_id, unique_name, TEST_IMAGE_UBUNTU};

    #[tokio::test]
    async fn test_monorail_agent_connection() -> anyhow::Result<()> {
        // This test requires the DSB stack to be running
        let agent = MonorailAgent::new().await?;
        assert!(agent.is_connected());
        Ok(())
    }

    #[tokio::test]
    async fn test_list_tools() -> anyhow::Result<()> {
        // This test requires the DSB stack to be running
        let agent = MonorailAgent::new().await?;
        let tools = agent.list_tools().await?;
        assert!(!tools.is_empty(), "Expected at least one tool");
        info!("Found {} tools", tools.len());
        Ok(())
    }

    #[tokio::test]
    async fn test_verify_tools() -> anyhow::Result<()> {
        // This test requires the DSB stack to be running
        let agent = MonorailAgent::new().await?;
        agent.verify_tools().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_call_create_sandbox() -> anyhow::Result<()> {
        // This test requires the DSB stack to be running
        let agent = MonorailAgent::new().await?;

        // Create minimal sandbox config
        let mut arguments = serde_json::Map::new();
        arguments.insert(
            "name".to_string(),
            serde_json::Value::String(unique_name("test-sandbox")),
        );
        arguments.insert(
            "image".to_string(),
            serde_json::Value::String(TEST_IMAGE_UBUNTU.to_string()),
        );

        let result = agent.call_tool("create_sandbox", Some(arguments)).await?;
        assert_ne!(
            result.is_error,
            Some(true),
            "Tool returned error: {:?}",
            result.content
        );

        let sandbox_id = extract_sandbox_id(&result)?;
        agent
            .call_tool(
                "delete_sandbox",
                serde_json::json!({ "sandbox_id": sandbox_id })
                    .as_object()
                    .cloned(),
            )
            .await?;

        Ok(())
    }
}
