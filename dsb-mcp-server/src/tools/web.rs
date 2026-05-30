// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Web scraping tools via the DSB HTTP tool execution endpoint.

use crate::dsb_client::DSBClient;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;

const WEB_TOOLS_PATH: &str = "/opt/tools/web_tools.py";
const DEFAULT_SEARCH_RESULTS: usize = 10;
const MAX_SEARCH_RESULTS: usize = 100;
const SUPPORTED_SEARCH_ENGINES: [&str; 4] = ["google", "duckduckgo", "bing", "baidu"];

/// Helper: Convert search results JSON to Markdown list format
fn json_search_to_markdown(json: &serde_json::Value) -> String {
    let empty_vec = vec![];
    let results = json["results"].as_array().unwrap_or(&empty_vec);

    let mut md = String::new();
    for (i, result) in results.iter().enumerate() {
        let title = result["title"].as_str().unwrap_or("Untitled result");
        let url = result["url"].as_str().unwrap_or("");
        let snippet = result["snippet"]
            .as_str()
            .or_else(|| result["content"].as_str())
            .or_else(|| result["description"].as_str())
            .unwrap_or("");

        if url.is_empty() {
            md.push_str(&format!("{}. **{}**\n\n", i + 1, title));
        } else {
            md.push_str(&format!("{}. **[{}]({})**\n\n", i + 1, title, url));
        }
        if !snippet.is_empty() {
            md.push_str(&format!("   {}\n\n", snippet));
        }
    }
    md
}

fn normalize_search_query(query: String) -> Result<String, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("query cannot be empty".to_string());
    }

    Ok(query.to_string())
}

fn normalize_search_engine(engine: Option<String>) -> Result<Option<String>, String> {
    let Some(engine) = engine else {
        return Ok(None);
    };

    let engine = engine.trim().to_lowercase();
    if engine.is_empty() {
        return Err("engine cannot be empty".to_string());
    }

    if !SUPPORTED_SEARCH_ENGINES.contains(&engine.as_str()) {
        return Err(format!(
            "Unsupported search engine '{}'. Supported engines: {}",
            engine,
            SUPPORTED_SEARCH_ENGINES.join(", ")
        ));
    }

    Ok(Some(engine))
}

fn normalize_num_results(num_results: Option<usize>) -> Result<usize, String> {
    match num_results.unwrap_or(DEFAULT_SEARCH_RESULTS) {
        0 => Err("num_results must be between 1 and 100".to_string()),
        value if value > MAX_SEARCH_RESULTS => Err(format!(
            "num_results must be between 1 and {}",
            MAX_SEARCH_RESULTS
        )),
        value => Ok(value),
    }
}

fn truncate_for_error(message: &str) -> &str {
    const MAX_ERROR_CHARS: usize = 300;
    if message.len() <= MAX_ERROR_CHARS {
        message
    } else {
        &message[..MAX_ERROR_CHARS]
    }
}

/// Validate a URL for SSRF prevention.
///
/// Blocks:
/// - Non-HTTP/HTTPS schemes (file://, data:, ftp://, etc.)
/// - Private/reserved IP ranges (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 127.0.0.0/8, 169.254.0.0/16)
/// - localhost and loopback hostnames
/// - Empty URLs
pub fn validate_url_secure(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    // Parse URL to extract scheme and host
    let url_parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return Err("Invalid URL format".to_string()),
    };

    // Only allow http and https schemes
    let scheme = url_parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "URL scheme '{}' is not allowed. Only http:// and https:// are permitted.",
            scheme
        ));
    }

    // Extract host
    let host = match url_parsed.host_str() {
        Some(h) => h.to_lowercase(),
        None => return Err("Invalid URL - missing host".to_string()),
    };

    // Block localhost and loopback hostnames
    if host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host.starts_with("127.")
        || host.starts_with("0.")
    {
        return Err("URL points to localhost/loopback which is not allowed".to_string());
    }

    // Block link-local addresses
    if host.starts_with("169.254.") {
        return Err("URL points to link-local address which is not allowed".to_string());
    }

    // Block private IP ranges by prefix
    if host.starts_with("10.") || host.starts_with("192.168.") || host.starts_with("172.") {
        // More precise check for 172.16.0.0/12
        if host.starts_with("172.") {
            let octets: Vec<&str> = host.split('.').collect();
            if octets.len() >= 2 {
                if let Ok(second) = octets[1].parse::<u8>() {
                    if (16..=31).contains(&second) {
                        return Err(
                            "URL points to private IP range which is not allowed".to_string()
                        );
                    }
                }
            }
        } else {
            return Err("URL points to private IP range which is not allowed".to_string());
        }
    }

    // Block IPv6 loopback and unique local addresses
    if host == "::1" || host.starts_with("fc") || host.starts_with("fd") {
        return Err("URL points to private IPv6 range which is not allowed".to_string());
    }

    Ok(())
}

fn trim_search_results(
    json_response: &mut serde_json::Value,
    result_limit: usize,
) -> Result<(), String> {
    let results = json_response
        .get_mut("results")
        .and_then(|value| value.as_array_mut())
        .ok_or_else(|| "SearXNG response missing results array".to_string())?;

    results.truncate(result_limit);
    Ok(())
}

async fn call_searxng_search(
    dsb_client: &DSBClient,
    query: &str,
    engine: Option<&str>,
    timeout_secs: Option<f64>,
    language: Option<&str>,
    categories: Option<&str>,
    time_range: Option<&str>,
) -> Result<serde_json::Value, String> {
    let timeout = timeout_secs
        .map(Duration::from_secs_f64)
        .unwrap_or_else(|| Duration::from_secs(dsb_client.timeout_secs()));

    let client = Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|error| format!("Failed to build SearXNG client: {}", error))?;

    let mut request = client.get(dsb_client.searxng_api_url()).query(&[
        ("q", query),
        ("format", "json"),
        ("categories", categories.unwrap_or("general")),
    ]);

    if let Some(engine) = engine {
        request = request.query(&[("engines", engine)]);
    }
    if let Some(lang) = language {
        request = request.query(&[("language", lang)]);
    }
    if let Some(tr) = time_range {
        request = request.query(&[("time_range", tr)]);
    }

    let response = request
        .send()
        .await
        .map_err(|error| format!("SearXNG request failed: {}", error))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read SearXNG response: {}", error))?;

    if !status.is_success() {
        let body = truncate_for_error(body.trim());
        return Err(if body.is_empty() {
            format!("SearXNG request failed with status {}", status)
        } else {
            format!("SearXNG request failed with status {}: {}", status, body)
        });
    }

    let json_response: serde_json::Value = serde_json::from_str(&body).map_err(|error| {
        format!(
            "Failed to parse SearXNG response as JSON: {} (body: {})",
            error,
            truncate_for_error(body.trim())
        )
    })?;

    if json_response
        .get("results")
        .and_then(|value| value.as_array())
        .is_none()
    {
        return Err("SearXNG response missing results array".to_string());
    }

    Ok(json_response)
}

/// Helper: Convert links JSON to Markdown list format
fn json_links_to_markdown(json: &serde_json::Value) -> String {
    if let Some(links) = json.as_array() {
        if links.is_empty() {
            return String::new();
        }

        let mut md = String::from("**Links:**\n\n");
        for link in links {
            let url = link.as_str().unwrap_or("");
            if !url.is_empty() {
                md.push_str(&format!("- {}\n", url));
            }
        }
        return md;
    }

    let empty_map = serde_json::Map::new();
    let links_data = json.as_object().unwrap_or(&empty_map);

    let mut md = String::new();

    // Add internal links if present
    if let Some(internal) = links_data.get("internal").and_then(|v| v.as_array()) {
        if !internal.is_empty() {
            md.push_str("**Internal Links:**\n\n");
            for link in internal {
                let url = link.as_str().unwrap_or("");
                md.push_str(&format!("- {}\n", url));
            }
            md.push('\n');
        }
    }

    // Add external links if present
    if let Some(external) = links_data.get("external").and_then(|v| v.as_array()) {
        if !external.is_empty() {
            md.push_str("**External Links:**\n\n");
            for link in external {
                let url = link.as_str().unwrap_or("");
                md.push_str(&format!("- {}\n", url));
            }
        }
    }

    md
}

/// Helper: Call web tools through the DSB HTTP tool execution endpoint.
async fn call_web_tool(
    dsb_client: &DSBClient,
    sandbox_id: &str,
    command: &str,
    args: serde_json::Value,
    allow_exec_fallback: bool,
) -> Result<serde_json::Value, String> {
    let id = uuid::Uuid::parse_str(sandbox_id).map_err(|e| format!("Invalid sandbox ID: {}", e))?;
    let fallback_args = args.clone();

    match dsb_client
        .execute_tool(id, "python", WEB_TOOLS_PATH, command, Some(args), None)
        .await
    {
        Ok(result) => Ok(result),
        Err(error) if allow_exec_fallback && should_fallback_to_exec(&error) => {
            call_web_tool_via_exec(dsb_client, id, command, fallback_args).await
        }
        Err(error) => Err(format!("Tool execution failed: {}", error)),
    }
}

fn should_fallback_to_exec(error: &anyhow::Error) -> bool {
    let error_text = error.to_string();
    error_text.contains("HTTP request failed")
        || error_text.contains("HTTP request to sandbox failed")
        || error_text.contains("error sending request")
        || error_text.contains("Connection refused")
        || error_text.contains("Failed to connect")
}

async fn call_web_tool_via_exec(
    dsb_client: &DSBClient,
    sandbox_id: uuid::Uuid,
    command: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let result = dsb_client
        .exec_command_with_stdin(
            sandbox_id,
            vec![
                "sh".to_string(),
                "-c".to_string(),
                format!(
                    "exec 9>/tmp/dsb-web-tools.lock && flock 9 && PYTHONWARNINGS=ignore python {} {} 2>/dev/null",
                    WEB_TOOLS_PATH, command
                ),
            ],
            Some(args.to_string()),
        )
        .await
        .map_err(|e| format!("Fallback tool execution failed: {}", e))?;

    let output = result.output.trim();
    let json_response = parse_fallback_json_output(output)?;

    if result.exit_code != 0 {
        let error_message = json_response
            .get("error_message")
            .and_then(|value| value.as_str())
            .unwrap_or(output);
        return Err(error_message.to_string());
    }

    Ok(json_response)
}

fn parse_fallback_json_output(output: &str) -> Result<serde_json::Value, String> {
    if output.is_empty() {
        return Err(
            "Failed to parse fallback tool response: EOF while parsing a value at line 1 column 0 (output: )"
                .to_string(),
        );
    }

    if let Ok(value) = serde_json::from_str(output) {
        return Ok(value);
    }

    for line in output.lines().rev() {
        let candidate = line.trim();
        if candidate.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str(candidate) {
            return Ok(value);
        }
    }

    if let Some(index) = output.rfind('{') {
        if let Ok(value) = serde_json::from_str(&output[index..]) {
            return Ok(value);
        }
    }

    Err(format!(
        "Failed to parse fallback tool response: expected JSON output (output: {})",
        output
    ))
}

/// Configuration for the `scrape_web` tool.
#[derive(Debug, Clone)]
pub struct ScrapeWebConfig {
    /// UUID of the sandbox with browser tools
    pub sandbox_id: String,
    /// URL to scrape
    pub url: String,
    /// Output format (markdown, links, cleaned)
    pub format: Option<String>,
    /// Capture screenshot
    pub screenshot: Option<bool>,
    /// CSS selector for targeted scraping
    pub css_selector: Option<String>,
    /// Minimum word count threshold
    pub word_count_threshold: Option<u32>,
    /// Optional search query to highlight relevant sections
    pub search_query: Option<String>,
    /// Enable content pruning
    pub use_pruning: Option<bool>,
    /// Pruning threshold
    pub pruning_threshold: Option<f64>,
    /// BM25 threshold
    pub bm25_threshold: Option<f64>,
    /// Wait until condition for page load
    pub wait_until: Option<String>,
    /// Cache mode
    pub cache_mode: Option<String>,
    /// Page timeout in seconds
    pub page_timeout: Option<u32>,
    /// Maximum length of returned content
    pub max_length: Option<usize>,
    /// Proxy configuration
    pub proxy_config: Option<serde_json::Value>,
    /// Allow fallback to exec if tool execution fails
    pub allow_exec_fallback: bool,
}

/// Scrape web page using the sandbox web tool endpoint.
pub async fn scrape_web(dsb_client: &DSBClient, config: ScrapeWebConfig) -> Result<String, String> {
    let fmt_value = config
        .format
        .as_ref()
        .unwrap_or(&"markdown".to_string())
        .clone();
    let mut args = json!({
        "url": config.url,
        "format": fmt_value,
    });

    // Add optional parameters
    if let Some(should_capture) = config.screenshot {
        args["screenshot"] = json!(should_capture);
    }
    if let Some(selector) = config.css_selector {
        args["css_selector"] = json!(selector);
    }
    if let Some(threshold) = config.word_count_threshold {
        args["word_count_threshold"] = json!(threshold);
    }
    if let Some(sq) = config.search_query {
        args["search_query"] = json!(sq);
    }
    if let Some(ml) = config.max_length {
        args["max_length"] = json!(ml);
    }
    if let Some(up) = config.use_pruning {
        args["use_pruning"] = json!(up);
    }
    if let Some(pt) = config.pruning_threshold {
        args["pruning_threshold"] = json!(pt);
    }
    if let Some(bt) = config.bm25_threshold {
        args["bm25_threshold"] = json!(bt);
    }
    if let Some(wu) = config.wait_until {
        args["wait_until"] = json!(wu);
    }
    if let Some(cm) = config.cache_mode {
        args["cache_mode"] = json!(cm);
    }
    if let Some(pt) = config.page_timeout {
        args["page_timeout"] = json!(pt);
    }
    if let Some(pc) = config.proxy_config {
        args["proxy_config"] = json!(pc);
    }

    let json_response = call_web_tool(
        dsb_client,
        &config.sandbox_id,
        "web_scrape",
        args,
        config.allow_exec_fallback,
    )
    .await?;

    let fmt = config.format.unwrap_or_else(|| "markdown".to_string());
    if fmt == "links" {
        if let Some(links) = json_response.get("links") {
            Ok(json_links_to_markdown(links))
        } else {
            Ok("No links found".to_string())
        }
    } else {
        Ok(json_response["content"]
            .as_str()
            .unwrap_or("No content")
            .to_string())
    }
}

/// Configuration for the `search_web` tool.
#[derive(Debug, Clone)]
pub struct SearchWebConfig {
    /// Search query string
    pub query: String,
    /// Search engine (google, duckduckgo, bing, baidu)
    pub engine: Option<String>,
    /// Number of results to return (default: 10, max: 100)
    pub num_results: Option<usize>,
    /// Request timeout in seconds
    pub timeout: Option<f64>,
    /// Language for search results
    pub language: Option<String>,
    /// Search categories
    pub categories: Option<String>,
    /// Time range filter
    pub time_range: Option<String>,
}

/// Search web using the configured SearXNG instance.
pub async fn search_web(dsb_client: &DSBClient, config: SearchWebConfig) -> Result<String, String> {
    let query = normalize_search_query(config.query)?;
    let engine = normalize_search_engine(config.engine)?;
    let result_limit = normalize_num_results(config.num_results)?;
    let mut json_response = call_searxng_search(
        dsb_client,
        &query,
        engine.as_deref(),
        config.timeout,
        config.language.as_deref(),
        config.categories.as_deref(),
        config.time_range.as_deref(),
    )
    .await?;
    trim_search_results(&mut json_response, result_limit)?;

    let output = json_search_to_markdown(&json_response);
    if output.trim().is_empty() {
        Ok("No search results found.".to_string())
    } else {
        Ok(output)
    }
}

/// Basic URL validation (legacy, delegates to `validate_url_secure`).
pub fn validate_url(url: &str) -> Result<(), String> {
    validate_url_secure(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // Removing unused test_client_with_searxng

    #[test]
    fn test_validate_url_valid() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("https://example.com/path?query=value").is_ok());
    }

    #[test]
    fn test_validate_url_rejects_localhost() {
        assert!(validate_url("http://localhost:8080").is_err());
        assert!(validate_url("http://127.0.0.1:8080").is_err());
        assert!(validate_url("http://::1").is_err());
    }

    #[test]
    fn test_validate_url_rejects_private_ips() {
        assert!(validate_url("http://10.0.0.1").is_err());
        assert!(validate_url("http://192.168.1.1").is_err());
        assert!(validate_url("http://172.16.0.1").is_err());
        assert!(validate_url("http://172.31.255.255").is_err());
        assert!(validate_url("http://169.254.1.1").is_err());
    }

    #[test]
    fn test_validate_url_rejects_dangerous_schemes() {
        assert!(validate_url("ftp://example.com").is_err());
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("data:text/html,test").is_err());
    }

    #[test]
    fn test_validate_url_invalid() {
        assert!(validate_url("").is_err());
        assert!(validate_url("not-a-url").is_err());
        assert!(validate_url("https://").is_err()); // Incomplete
    }

    #[test]
    fn test_validate_url_with_spaces() {
        assert!(validate_url("https://example.com/path%20with%20spaces").is_ok());
    }

    #[test]
    fn test_validate_url_with_fragment() {
        assert!(validate_url("https://example.com#section").is_ok());
    }

    #[test]
    fn test_validate_url_with_port() {
        assert!(validate_url("https://example.com:8443").is_ok());
    }

    #[test]
    fn test_parse_fallback_json_output_with_log_preamble() {
        let output =
            "[ANTIBOT] noisy preamble\n{\"error_message\":\"Search failed\",\"status_code\":500}";
        let value = parse_fallback_json_output(output).unwrap();
        assert_eq!(value["error_message"], "Search failed");
        assert_eq!(value["status_code"], 500);
    }

    // ===== TESTS FOR PRIVATE HELPER FUNCTIONS =====

    #[test]
    fn test_json_search_to_markdown() {
        let json = json!({
            "results": [
                {"title": "Result 1", "url": "https://example.com/1", "snippet": "Description 1"},
                {"title": "Result 2", "url": "https://example.com/2", "content": "Description 2"}
            ]
        });

        let result = json_search_to_markdown(&json);
        assert!(result.contains("Result 1"));
        assert!(result.contains("Result 2"));
        assert!(result.contains("https://example.com/1"));
        assert!(result.contains("Description 2"));
    }

    #[test]
    fn test_json_links_to_markdown() {
        // agent_browser_tools.py returns data directly
        let json = json!({
            "internal": ["https://example.com/internal1", "https://example.com/internal2"],
            "external": ["https://example.com/external1", "https://example.com/external2"]
        });

        let result = json_links_to_markdown(&json);
        assert!(result.contains("https://example.com/internal1"));
        assert!(result.contains("Internal Links"));
        assert!(result.contains("External Links"));
        assert!(result.contains("https://example.com/external1"));
    }

    #[test]
    fn test_normalize_search_engine_rejects_unsupported_value() {
        let error = normalize_search_engine(Some("askjeeves".to_string())).unwrap_err();
        assert!(error.contains("Unsupported search engine"));
    }

    #[test]
    fn test_normalize_num_results_rejects_zero() {
        let error = normalize_num_results(Some(0)).unwrap_err();
        assert!(error.contains("num_results"));
    }

    #[tokio::test]
    async fn test_search_web_queries_searxng_without_engine_filter() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("q", "rust programming language"))
            .and(query_param("format", "json"))
            .and(query_param("categories", "general"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [
                    {
                        "title": "Rust Programming Language",
                        "url": "https://www.rust-lang.org",
                        "content": "A language empowering everyone to build reliable and efficient software."
                    }
                ]
            })))
            .mount(&server)
            .await;

        let mut settings = Settings::default();
        settings.web.searxng_url = format!("{}/search", server.uri());
        let client = DSBClient::new(settings).unwrap();
        let config = SearchWebConfig {
            query: "rust programming language".to_string(),
            engine: None,
            num_results: None,
            timeout: None,
            language: None,
            categories: None,
            time_range: None,
        };
        let output = search_web(&client, config).await.unwrap();

        assert!(output.contains("Rust Programming Language"));
        assert!(output.contains("https://www.rust-lang.org"));
    }

    #[tokio::test]
    async fn test_search_web_honors_engine_filter_and_result_limit() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .and(query_param("q", "web application security"))
            .and(query_param("format", "json"))
            .and(query_param("categories", "general"))
            .and(query_param("engines", "bing"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": [
                    {
                        "title": "Security result 1",
                        "url": "https://example.com/security-1",
                        "content": "first"
                    },
                    {
                        "title": "Security result 2",
                        "url": "https://example.com/security-2",
                        "content": "second"
                    }
                ]
            })))
            .mount(&server)
            .await;

        let mut settings = Settings::default();
        settings.web.searxng_url = format!("{}/search", server.uri());
        let client = DSBClient::new(settings).unwrap();
        let config = SearchWebConfig {
            query: "web application security".to_string(),
            engine: Some("bing".to_string()),
            num_results: Some(1),
            timeout: None,
            language: None,
            categories: None,
            time_range: None,
        };
        let output = search_web(&client, config).await.unwrap();

        assert!(output.contains("Security result 1"));
        assert!(!output.contains("Security result 2"));
    }

    #[tokio::test]
    async fn test_search_web_returns_friendly_message_for_empty_results() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "results": []
            })))
            .mount(&server)
            .await;

        let mut settings = Settings::default();
        settings.web.searxng_url = format!("{}/search", server.uri());
        let client = DSBClient::new(settings).unwrap();
        let config = SearchWebConfig {
            query: "no results query".to_string(),
            engine: None,
            num_results: None,
            timeout: None,
            language: None,
            categories: None,
            time_range: None,
        };
        let output = search_web(&client, config).await.unwrap();

        assert_eq!(output, "No search results found.");
    }

    #[tokio::test]
    async fn test_search_web_surfaces_http_errors() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/search"))
            .respond_with(ResponseTemplate::new(500).set_body_string("upstream failure"))
            .mount(&server)
            .await;

        let mut settings = Settings::default();
        settings.web.searxng_url = format!("{}/search", server.uri());
        let client = DSBClient::new(settings).unwrap();
        let config = SearchWebConfig {
            query: "error case".to_string(),
            engine: None,
            num_results: None,
            timeout: None,
            language: None,
            categories: None,
            time_range: None,
        };
        let error = search_web(&client, config).await.unwrap_err();

        assert!(error.contains("status 500"));
        assert!(error.contains("upstream failure"));
    }
}
