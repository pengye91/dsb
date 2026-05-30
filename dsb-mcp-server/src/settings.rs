// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Settings for DSB MCP Server
//!
//! This module provides a robust, hierarchical settings system that supports
//! defaults, configuration files, and environment variable overrides.
//! It is designed to match the Python implementation for compatibility.

use anyhow::Context;
use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};

/// Top-level settings for the DSB MCP server.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    /// PagerDuty integration key for incident alerting
    pub pagerduty_key: String,
    /// HTTP server configuration
    pub server: ServerSettings,
    /// Redis storage configuration
    pub storage: StorageSettings,
    /// DSB API connection settings
    pub dsb: DSBSettings,
    /// Web search and scraping settings
    pub web: WebSettings,
    /// Search engine name mappings
    pub web_engine_mapping: WebEngineMapping,
    /// Output format mappings for web content
    pub web_format_mapping: WebFormatMapping,
    /// Sandbox creation and management defaults
    pub sandbox: SandboxSettings,
    /// Interactive terminal settings
    pub terminal: TerminalSettings,
    /// Browser automation settings
    pub browser: BrowserSettings,
    /// SSH gateway settings
    pub ssh: SSHSettings,
    /// System-wide logging and formatting
    pub system: SystemSettings,
    /// Value retrieval (entity search) settings
    pub value_retrieval: ValueRetrievalSettings,
    /// Knowledge retrieval (passage search) settings
    pub knowledge_retrieval: KnowledgeRetrievalSettings,
}

/// HTTP server configuration for the MCP server.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerSettings {
    /// Port number the MCP server will listen on
    pub port: u16,
    /// Host interface the MCP server will bind to
    pub host: String,
    /// Display name for the system tools group
    pub mcp_server_system_name: String,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            port: 3000,
            host: "0.0.0.0".to_string(),
            mcp_server_system_name: "System Tools Group".to_string(),
        }
    }
}

/// Redis storage configuration.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageSettings {
    /// Redis server hostname or IP address
    pub redis_host: String,
    /// Redis server port
    pub redis_port: u16,
    /// Optional Redis username
    pub redis_username: Option<String>,
    /// Optional Redis password
    pub redis_password: Option<String>,
    /// Whether to use SSL for Redis connection
    pub redis_ssl: bool,
    /// Whether to decode Redis responses as UTF-8 strings
    pub redis_decode_responses: bool,
    /// Prefix for all Redis keys to avoid collisions
    pub redis_key_prefix: String,
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            redis_host: "127.0.0.1".to_string(),
            redis_port: 6379,
            redis_username: None,
            redis_password: None,
            redis_ssl: false,
            redis_decode_responses: true,
            redis_key_prefix: "dms:local:dev:".to_string(),
        }
    }
}

/// DSB API connection settings.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DSBSettings {
    /// Base URL for the DSB API
    pub api_url: String,
    /// Optional API key for authenticating with the DSB service
    pub api_key: Option<String>,
    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl Default for DSBSettings {
    fn default() -> Self {
        Self {
            api_url: "http://localhost:8080".to_string(),
            api_key: None,
            // Must exceed DSB → sandbox tool HTTP timeout (tool_timeouts.max_allowed_secs + http_buffer_secs + margin).
            // Default DSB web path is ~120s; custom tools can be ~330s. MCP aborting early surfaces as flaky 500s.
            timeout_secs: 600,
        }
    }
}

/// Web search and scraping settings.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebSettings {
    /// URL of the SearXNG instance for web searches
    pub searxng_url: String,
    /// Timeout for SearXNG search requests in seconds
    pub searxng_timeout: f64,
    /// Number of search results to request from SearXNG
    pub searxng_result_num: u32,
    /// Comma-separated list of default search engines to use
    pub searxng_default_engines: String,
    /// Comma-separated list of default categories to search in
    pub searxng_default_categories: String,
    /// Safe search level (0=off, 1=moderate, 2=strict)
    pub searxng_safesearch: String,
    /// Minimum word count for a content block to be considered significant
    pub word_count_threshold: u32,
    /// Similarity threshold for pruning redundant content blocks
    pub pruning_threshold: f64,
    /// BM25 relevance threshold for keyword search
    pub bm25_threshold: f64,
    /// Playwright wait condition for page loading
    pub wait_until: String,
    /// Cache behavior for web pages (e.g., "bypass", "refresh")
    pub cache_mode: String,
    /// Maximum time to wait for a page to load in milliseconds
    pub page_timeout: u32,
    /// Default output format for web content (e.g., "markdown")
    pub default_format: String,
    /// Number of times to retry health checks
    pub health_check_retry_times: u32,
    /// Interval between health check retries in seconds
    pub health_check_retry_interval: f64,
    /// When true, transient failures on the sandbox HTTP tool path fall back to kube exec / stdin.
    /// Default false: on Kubernetes, exec often lacks RBAC parity with the HTTP path; prefer fixing HTTP readiness.
    pub allow_exec_fallback: bool,
}

impl Default for WebSettings {
    fn default() -> Self {
        Self {
            searxng_url: "http://localhost:8888/search".to_string(),
            searxng_timeout: 60.0,
            searxng_result_num: 20,
            searxng_default_engines: "google,bing".to_string(),
            searxng_default_categories: "general,news,it".to_string(),
            searxng_safesearch: "0".to_string(),
            word_count_threshold: 10,
            pruning_threshold: 0.48,
            bm25_threshold: 1.0,
            wait_until: "domcontentloaded".to_string(),
            cache_mode: "bypass".to_string(),
            page_timeout: 180_000,
            default_format: "markdown".to_string(),
            health_check_retry_times: 3,
            health_check_retry_interval: 5.0,
            allow_exec_fallback: false,
        }
    }
}

/// Search engine name mappings.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebEngineMapping {
    /// Mapping for Google search engine
    pub google: String,
    /// Mapping for Brave search engine
    pub brave: String,
    /// Mapping for Bing search engine
    pub bing: String,
    /// Mapping for Baidu search engine
    pub baidu: String,
    /// Mapping for DuckDuckGo search engine
    pub duckduckgo: String,
    /// Alternative mapping for DuckDuckGo search engine
    pub ddg: String,
}

impl Default for WebEngineMapping {
    fn default() -> Self {
        Self {
            google: "google".to_string(),
            brave: "brave".to_string(),
            bing: "bing".to_string(),
            baidu: "baidu".to_string(),
            duckduckgo: "duckduckgo".to_string(),
            ddg: "duckduckgo".to_string(),
        }
    }
}

/// Output format mappings for web content.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WebFormatMapping {
    /// Format identifier for Markdown output
    pub markdown: String,
    /// Format identifier for HTML output
    pub html: String,
    /// Format identifier for plain text output
    pub text: String,
    /// Format identifier for link list output
    pub links: String,
}

impl Default for WebFormatMapping {
    fn default() -> Self {
        Self {
            markdown: "MARKDOWN".to_string(),
            html: "HTML".to_string(),
            text: "TEXT".to_string(),
            links: "LINKS".to_string(),
        }
    }
}

/// Sandbox creation and management defaults.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SandboxSettings {
    /// Default Docker image to use for sandboxes
    pub default_image: String,
    /// Whether to enable all optional sandbox features by default
    pub enable_all_features: bool,
    /// Default file encoding for sandbox operations
    pub file_encoding: String,
    /// Timeout for static file operations in seconds
    pub static_file_timeout: f64,
    /// API endpoint prefix for single static files
    pub static_file_endpoint: String,
    /// API endpoint prefix for static file directories
    pub static_files_endpoint: String,
    /// Default file name to serve if none specified
    pub default_static_file: String,
    /// Root directory for sandbox files
    pub base_directory: String,
    /// Redis key prefix for sandbox-related data
    pub cache_prefix_sandbox: String,
    /// Redis key prefix for session-related data
    pub cache_prefix_session: String,
}

impl Default for SandboxSettings {
    fn default() -> Self {
        Self {
            default_image: "docker.io/dsb/sandbox:dev".to_string(),
            enable_all_features: true,
            file_encoding: "utf-8".to_string(),
            static_file_timeout: 30.0,
            static_file_endpoint: "/static/".to_string(),
            static_files_endpoint: "/static/files/".to_string(),
            default_static_file: "index.html".to_string(),
            base_directory: "/workspace".to_string(),
            cache_prefix_sandbox: "sandbox:".to_string(),
            cache_prefix_session: "session:".to_string(),
        }
    }
}

/// Interactive terminal settings.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TerminalSettings {
    /// Timeout for establishing a terminal connection in seconds
    pub connection_timeout: f64,
    /// Timeout for executing a terminal command in seconds
    pub command_timeout: f64,
    /// Default number of rows for terminal window
    pub default_rows: u32,
    /// Default number of columns for terminal window
    pub default_cols: u32,
    /// WebSocket protocol for HTTP connections
    pub websocket_protocol_http: String,
    /// WebSocket protocol for HTTPS connections
    pub websocket_protocol_https: String,
    /// Instructional text for terminal usage
    pub websocket_instructions: String,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            connection_timeout: 30.0,
            command_timeout: 5.0,
            default_rows: 24,
            default_cols: 80,
            websocket_protocol_http: "ws://".to_string(),
            websocket_protocol_https: "wss://".to_string(),
            websocket_instructions: "Connect to the WebSocket URL to access the interactive terminal. Send commands and receive output through the WebSocket connection.".to_string(),
        }
    }
}

/// Browser automation settings.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BrowserSettings {
    /// String indicating successful browser operation
    pub success_status: String,
    /// Whether to take full-page screenshots by default
    pub screenshot_full_page: bool,
    /// Default filename for browser screenshots
    pub screenshot_default_name: String,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            success_status: "success".to_string(),
            screenshot_full_page: false,
            screenshot_default_name: "screenshot".to_string(),
        }
    }
}

/// SSH gateway settings.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SSHSettings {
    /// Optional default SSH public key for sandbox access
    pub default_public_key: Option<String>,
}

/// System-wide logging and formatting settings.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemSettings {
    /// Format string for dates and times
    pub date_format: String,
    /// Global logging level (e.g., "DEBUG", "INFO", "WARN", "ERROR")
    pub log_level: String,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            date_format: "%Y/%m/%d %H:%M:%S".to_string(),
            log_level: "INFO".to_string(),
        }
    }
}

/// Knowledge retrieval (passage search) settings using Milvus.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KnowledgeRetrievalSettings {
    /// Milvus server URL
    pub milvus_url: String,
    /// Name of the Milvus collection for knowledge retrieval
    pub collection_name: String,
    /// Maximum number of documents to retrieve
    pub search_limit: u32,
    /// Minimum similarity score ratio for search results (0.0 to 1.0)
    pub drop_ratio_search: f64,
    /// Minimum threshold score for reranked results (0.0 to 1.0)
    pub rerank_threshold: f64,
    /// Model name used for text embedding
    pub embedding_model: String,
    /// API endpoint for the embedding service
    pub embedding_api_url: String,
    /// Optional API key for the embedding service
    pub embedding_api_key: Option<String>,
    /// Model name used for reranking search results
    pub rerank_model: String,
    /// API endpoint for the reranking service
    pub rerank_api_url: String,
    /// Optional API key for the reranking service
    pub rerank_api_key: Option<String>,
    /// Default instruction to prepend to search queries
    pub search_instruction: String,
    /// List of document fields to include in the output
    pub output_fields: Vec<String>,
    /// Primary content field name
    pub doc: String,
}

impl Default for KnowledgeRetrievalSettings {
    fn default() -> Self {
        Self {
            milvus_url: "http://localhost:19530".to_string(),
            collection_name: "deep_research_knowledge".to_string(),
            search_limit: 10,
            drop_ratio_search: 0.2,
            rerank_threshold: 0.75,
            embedding_model: "qwen-embedding-0.6".to_string(),
            embedding_api_url: "http://localhost:8000/v1/embedding".to_string(),
            embedding_api_key: None,
            rerank_model: "qwen-reranker-0.6".to_string(),
            rerank_api_url: "http://localhost:8000/v1/rerank".to_string(),
            rerank_api_key: None,
            search_instruction:
                "Given a web search query, retrieve relevant passages that answer the query"
                    .to_string(),
            output_fields: vec!["doc".to_string()],
            doc: "doc".to_string(),
        }
    }
}

/// Value retrieval (entity search) settings using Milvus.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ValueRetrievalSettings {
    /// Milvus server URL
    pub milvus_url: String,
    /// Name of the Milvus collection for value retrieval
    pub collection_name: String,
    /// Maximum number of results to retrieve
    pub search_limit: u32,
    /// Minimum similarity score ratio for search results (0.0 to 1.0)
    pub drop_ratio_search: f64,
    /// Minimum threshold score for reranked results (0.0 to 1.0)
    pub rerank_threshold: f64,
    /// Model name used for text embedding
    pub embedding_model: String,
    /// API endpoint for the embedding service
    pub embedding_api_url: String,
    /// Optional API key for the embedding service
    pub embedding_api_key: Option<String>,
    /// Model name used for reranking search results
    pub rerank_model: String,
    /// API endpoint for the reranking service
    pub rerank_api_url: String,
    /// Optional API key for the reranking service
    pub rerank_api_key: Option<String>,
    /// Default instruction to prepend to search queries
    pub search_instruction: String,
    /// List of document fields to include in the output
    pub output_fields: Vec<String>,
    /// Primary content field name
    pub doc: String,
}

impl Default for ValueRetrievalSettings {
    fn default() -> Self {
        Self {
            milvus_url: "http://localhost:19530".to_string(),
            collection_name: "nl2sql_value_retrieval_ft".to_string(),
            search_limit: 10,
            drop_ratio_search: 0.2,
            rerank_threshold: 0.95,
            embedding_model: "qwen-embedding-0.6".to_string(),
            embedding_api_url: "http://localhost:8000/v1/embedding".to_string(),
            embedding_api_key: None,
            rerank_model: "qwen-reranker-0.6".to_string(),
            rerank_api_url: "http://localhost:8000/v1/rerank".to_string(),
            rerank_api_key: None,
            search_instruction: "Given a web search query, retrieve the most relevant entity name that mentioned in the query".to_string(),
            output_fields: vec!["hit_value".to_string(), "evidence".to_string()],
            doc: "hit_value".to_string(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            pagerduty_key: "".to_string(),
            server: ServerSettings::default(),
            storage: StorageSettings::default(),
            dsb: DSBSettings::default(),
            web: WebSettings::default(),
            web_engine_mapping: WebEngineMapping::default(),
            web_format_mapping: WebFormatMapping::default(),
            sandbox: SandboxSettings::default(),
            terminal: TerminalSettings::default(),
            browser: BrowserSettings::default(),
            ssh: SSHSettings::default(),
            system: SystemSettings::default(),
            value_retrieval: ValueRetrievalSettings::default(),
            knowledge_retrieval: KnowledgeRetrievalSettings::default(),
        }
    }
}

impl Settings {
    /// Load settings from defaults, config files, and environment variables.
    ///
    /// The order of precedence is:
    /// 1. Environment variables prefixed with `DSB_MCP_` (e.g. `DSB_MCP_SERVER__PORT=8080`)
    /// 2. Local config file `dsb-mcp-settings.toml`
    /// 3. Default values
    pub fn load() -> anyhow::Result<Self> {
        let s = Config::builder()
            // Start with defaults from the Default implementation
            .add_source(config::Config::try_from(&Settings::default())?)
            // Add optional local config file
            .add_source(File::with_name("dsb-mcp-settings").required(false))
            // Add environment variables
            .add_source(
                Environment::with_prefix("DSB_MCP")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()
            .context("Failed to build configuration")?;

        s.try_deserialize()
            .context("Failed to deserialize settings")
    }

    /// Load settings for tests, skipping local configuration files.
    ///
    /// This ensures tests are reproducible and don't depend on the developer's local setup.
    pub fn load_for_tests() -> anyhow::Result<Self> {
        let s = Config::builder()
            // Start with defaults from the Default implementation
            .add_source(config::Config::try_from(&Settings::default())?)
            // Add environment variables (optional for tests)
            .add_source(
                Environment::with_prefix("DSB_MCP")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()
            .context("Failed to build test configuration")?;

        s.try_deserialize()
            .context("Failed to deserialize test settings")
    }

    /// Returns a map of standard environment variables for sandbox tools
    /// based on the current settings.
    pub fn get_sandbox_env(&self) -> std::collections::HashMap<String, String> {
        let mut env = std::collections::HashMap::new();

        // Redis settings
        env.insert("REDIS_HOST".to_string(), self.storage.redis_host.clone());
        env.insert(
            "REDIS_PORT".to_string(),
            self.storage.redis_port.to_string(),
        );
        if let Some(user) = &self.storage.redis_username {
            env.insert("REDIS_USER".to_string(), user.clone());
        }
        if let Some(pass) = &self.storage.redis_password {
            env.insert("REDIS_PASSWORD".to_string(), pass.clone());
        }
        env.insert("REDIS_SSL".to_string(), self.storage.redis_ssl.to_string());
        env.insert(
            "REDIS_KEY_PREFIX".to_string(),
            self.storage.redis_key_prefix.clone(),
        );

        env
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    /// Helper to serialize settings tests that use env vars.
    /// Rust tests run in parallel by default, and `config` reads env vars at
    /// call time, so concurrent set/remove of the same var causes flaky failures.
    static SETTINGS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_settings_load_defaults() {
        let _guard = SETTINGS_TEST_LOCK.lock().unwrap();
        env::remove_var("DSB_MCP_SERVER__PORT");
        env::remove_var("DSB_MCP_DSB__API_KEY");
        env::remove_var("DSB_MCP_WEB__ALLOW_EXEC_FALLBACK");

        let settings = Settings::load_for_tests().expect("Failed to load settings");
        assert_eq!(settings.server.port, 3000);
        assert_eq!(settings.server.host, "0.0.0.0");
        assert!(!settings.web.allow_exec_fallback);
    }

    #[test]
    fn test_settings_env_override() {
        let _guard = SETTINGS_TEST_LOCK.lock().unwrap();
        env::set_var("DSB_MCP_SERVER__PORT", "9999");
        env::set_var("DSB_MCP_DSB__API_KEY", "test-api-key");

        let settings = Settings::load_for_tests().expect("Failed to load settings");

        assert_eq!(settings.server.port, 9999);
        assert_eq!(settings.dsb.api_key, Some("test-api-key".to_string()));

        // Cleanup
        env::remove_var("DSB_MCP_SERVER__PORT");
        env::remove_var("DSB_MCP_DSB__API_KEY");
    }

    #[test]
    fn test_settings_web_allow_exec_fallback_env() {
        let _guard = SETTINGS_TEST_LOCK.lock().unwrap();
        env::remove_var("DSB_MCP_WEB__ALLOW_EXEC_FALLBACK");
        env::set_var("DSB_MCP_WEB__ALLOW_EXEC_FALLBACK", "true");
        let settings = Settings::load_for_tests().expect("Failed to load settings");
        assert!(settings.web.allow_exec_fallback);
        env::remove_var("DSB_MCP_WEB__ALLOW_EXEC_FALLBACK");
    }
}
