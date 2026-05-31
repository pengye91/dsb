// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Comprehensive E2E tests for web search and web fetch tools
//!
//! These tests validate web scraping and search capabilities through the MCP protocol,
//! connecting a real MCP client to the real dsb-mcp-server which executes commands
//! in real Docker sandboxes with browser capabilities.
//!
//! All tests require the DSB stack to be running with dsb-mcp-server available.

use crate::agents::MonorailAgent;
use crate::tests::test_utils::{
    call_tool_with_retry, create_sandbox_with_retry, extract_output_text, extract_sandbox_id,
    test_image_sandbox, unique_name, wait_for_sandbox_running,
};
use anyhow::Context;
use std::future::Future;
use tracing::info;

// ============================================================================
// Helper: create a sandbox with browser capabilities and return its ID
// ============================================================================

/// Creates a sandbox using the full sandbox image (with Chromium/CDP support)
/// and returns the sandbox ID. Waits for the sandbox to reach "running" state
/// before returning, ensuring it's ready to accept tool executions.
async fn create_web_sandbox(agent: &MonorailAgent, test_name: &str) -> anyhow::Result<String> {
    let name = unique_name(test_name);
    let sandbox_image = test_image_sandbox();
    info!("Creating web sandbox: {} (image: {})", name, sandbox_image);
    let result = create_sandbox_with_retry(agent, name.clone(), sandbox_image)
        .await
        .context("create_sandbox failed")?;

    let sandbox_id = extract_sandbox_id(&result)?;
    info!("Created web sandbox: {} (id: {})", name, sandbox_id);

    // Wait for the sandbox to actually be running before returning.
    // The DSB server's create_sandbox returns before the Docker container has
    // fully started, so we must poll until the state transitions to "running".
    // Browser is needed since all web tools tests use scrape_web/automate_browser.
    wait_for_sandbox_running(agent, &sandbox_id, true)
        .await
        .context("sandbox readiness wait failed")?;

    Ok(sandbox_id)
}

/// Deletes a sandbox, logging but not failing on errors (for cleanup).
async fn cleanup_sandbox(agent: &MonorailAgent, sandbox_id: &str) {
    info!("Cleaning up sandbox: {}", sandbox_id);
    match agent
        .call_tool(
            "delete_sandbox",
            serde_json::json!({ "sandbox_id": sandbox_id })
                .as_object()
                .cloned(),
        )
        .await
    {
        Ok(_) => info!("Deleted sandbox: {}", sandbox_id),
        Err(e) => tracing::warn!("Failed to delete sandbox {}: {}", sandbox_id, e),
    }
}

/// Runs a test body with a sandbox, guaranteeing cleanup even on failure.
///
/// Creates a web sandbox, passes the agent and sandbox_id to the test closure,
/// and always deletes the sandbox afterward—whether the test passes, fails
/// via `?` early return, or panics from `assert!` macros.
///
/// Uses `tokio::task::spawn` to catch panics from assertion failures, ensuring
/// cleanup runs before re-propagating the panic.
async fn run_with_sandbox<F, Fut>(test_name: &str, test_body: F) -> anyhow::Result<()>
where
    F: FnOnce(MonorailAgent, String) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let agent = MonorailAgent::new().await?;
    let sandbox_id = create_web_sandbox(&agent, test_name).await?;

    // Spawn the test body as a task so panics are caught by the JoinHandle
    let sandbox_id_clone = sandbox_id.clone();
    let join_result = tokio::task::spawn(test_body(agent, sandbox_id_clone)).await;

    // Always clean up, regardless of how the test body terminated
    let cleanup_agent = MonorailAgent::new().await.unwrap_or_else(|e| {
        tracing::error!("Failed to create cleanup agent: {}", e);
        panic!(
            "Cannot clean up sandbox {} — failed to reconnect to MCP server",
            sandbox_id
        );
    });
    cleanup_sandbox(&cleanup_agent, &sandbox_id).await;

    // Propagate the original result: re-panic if panicked, return error if errored
    match join_result {
        Ok(test_result) => test_result,
        Err(join_err) => {
            // The task panicked — resume the panic after cleanup
            std::panic::resume_unwind(join_err.into_panic());
        }
    }
}

/// Runs a test body with multiple web sandboxes, guaranteeing cleanup even on failure.
async fn run_with_sandboxes<F, Fut>(
    test_name: &str,
    sandbox_count: usize,
    test_body: F,
) -> anyhow::Result<()>
where
    F: FnOnce(MonorailAgent, Vec<String>) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let agent = MonorailAgent::new().await?;
    let mut sandbox_ids = Vec::with_capacity(sandbox_count);

    for index in 0..sandbox_count {
        match create_web_sandbox(&agent, &format!("{test_name}-{}", index + 1)).await {
            Ok(sandbox_id) => sandbox_ids.push(sandbox_id),
            Err(error) => {
                let cleanup_agent = MonorailAgent::new().await.unwrap_or_else(|cleanup_error| {
                    tracing::error!("Failed to create cleanup agent: {}", cleanup_error);
                    panic!(
                        "Cannot clean up partially created sandboxes {:?} — failed to reconnect to MCP server",
                        sandbox_ids
                    );
                });

                for sandbox_id in &sandbox_ids {
                    cleanup_sandbox(&cleanup_agent, sandbox_id).await;
                }

                return Err(error);
            }
        }
    }

    let sandbox_ids_for_test = sandbox_ids.clone();
    let join_result = tokio::task::spawn(test_body(agent, sandbox_ids_for_test)).await;

    let cleanup_agent = MonorailAgent::new().await.unwrap_or_else(|e| {
        tracing::error!("Failed to create cleanup agent: {}", e);
        panic!(
            "Cannot clean up sandboxes {:?} — failed to reconnect to MCP server",
            sandbox_ids
        );
    });

    for sandbox_id in &sandbox_ids {
        cleanup_sandbox(&cleanup_agent, sandbox_id).await;
    }

    match join_result {
        Ok(test_result) => test_result,
        Err(join_err) => std::panic::resume_unwind(join_err.into_panic()),
    }
}

/// Counts numbered markdown search results in the MCP tool output.
fn count_markdown_search_results(output: &str) -> usize {
    output
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            let Some((index, rest)) = trimmed.split_once(". **[") else {
                return false;
            };
            !index.is_empty() && index.chars().all(|ch| ch.is_ascii_digit()) && rest.contains("](")
        })
        .count()
}

fn search_results_unavailable(output: &str) -> bool {
    output
        .trim()
        .to_lowercase()
        .contains("no search results found")
}

// ============================================================================
// Category 1: Web Fetch — Different Content Types
// ============================================================================

/// Test: Fetch a simple static HTML page and verify content is returned
#[tokio::test]
async fn test_web_fetch_simple_html_page() -> anyhow::Result<()> {
    run_with_sandbox("fetch-simple", |agent, sandbox_id| async move {
        let result = call_tool_with_retry(
            &agent,
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://example.com"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("scrape_web failed")?;

        assert!(
            result.is_error != Some(true),
            "scrape_web returned error: {:?}",
            result.content
        );

        let output = extract_output_text(&result)?;
        assert!(!output.is_empty(), "scrape_web returned empty output");
        info!("Fetched {} chars from example.com", output.len());

        // example.com should contain its well-known heading
        let output_lower = output.to_lowercase();
        assert!(
            output_lower.contains("example") || output_lower.contains("domain"),
            "Expected 'example' or 'domain' in output, got: {}",
            &output[..output.len().min(200)]
        );

        info!("test_web_fetch_simple_html_page passed");
        Ok(())
    })
    .await
}

/// Test: Verify that timeouts correctly tear down zombie processes using os.killpg
#[tokio::test]
async fn test_web_fetch_timeout_teardown() -> anyhow::Result<()> {
    run_with_sandbox("fetch-timeout", |agent, sandbox_id| async move {
        // Run a tool with a very short timeout so it predictably times out
        let _call_result = call_tool_with_retry(
            &agent,
            "execute_bash",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "command": "python3 -c 'import time; time.sleep(10)'",
                "timeout": 2
            })
            .as_object()
            .cloned(),
        )
        .await;

        // Since it timed out, verify no python3 sleep processes are left
        let check_result = call_tool_with_retry(
            &agent,
            "execute_bash",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "command": "ps aux | grep -v grep | grep 'time.sleep' || true"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("process check failed")?;

        let output = match extract_output_text(&check_result) {
            Ok(text) => text,
            Err(e) if e.to_string().contains("No text output found") => String::new(),
            Err(e) => return Err(e),
        };
        assert!(
            output.is_empty(),
            "Found lingering process after timeout: {}",
            output
        );

        info!("test_web_fetch_timeout_teardown passed");
        Ok(())
    })
    .await
}

/// Test: Fetch the same page in different formats (markdown, text, links)
#[tokio::test]
async fn test_web_fetch_with_different_formats() -> anyhow::Result<()> {
    run_with_sandbox("fetch-formats", |agent, sandbox_id| async move {
        // Fetch as markdown
        let md_result = call_tool_with_retry(
            &agent,
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://example.com",
                "format": "markdown"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("scrape_web markdown failed")?;

        assert!(
            md_result.is_error != Some(true),
            "scrape_web markdown returned error"
        );
        let md_output = extract_output_text(&md_result)?;
        assert!(!md_output.is_empty(), "markdown output is empty");
        info!("Markdown format: {} chars", md_output.len());

        // Fetch as text
        let text_result = call_tool_with_retry(
            &agent,
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://example.com",
                "format": "text"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("scrape_web text failed")?;

        assert!(
            text_result.is_error != Some(true),
            "scrape_web text returned error"
        );
        let text_output = extract_output_text(&text_result)?;
        assert!(!text_output.is_empty(), "text output is empty");
        info!("Text format: {} chars", text_output.len());

        // Fetch as links
        let links_result = call_tool_with_retry(
            &agent,
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://example.com",
                "format": "links"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("scrape_web links failed")?;

        assert!(
            links_result.is_error != Some(true),
            "scrape_web links returned error"
        );
        let links_output = extract_output_text(&links_result)?;
        assert!(!links_output.is_empty(), "links output is empty");
        info!("Links format: {} chars", links_output.len());

        info!(
            "All 3 formats returned content: markdown={}, text={}, links={}",
            md_output.len(),
            text_output.len(),
            links_output.len()
        );

        info!("test_web_fetch_with_different_formats passed");
        Ok(())
    })
    .await
}

/// Test: Fetch content using a CSS selector to target specific elements
#[tokio::test]
async fn test_web_fetch_with_css_selector() -> anyhow::Result<()> {
    run_with_sandbox("fetch-css", |agent, sandbox_id| async move {
        let result = call_tool_with_retry(
            &agent,
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://example.com",
                "css_selector": "h1"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("scrape_web with css_selector failed")?;

        assert!(
            result.is_error != Some(true),
            "scrape_web with css_selector returned error: {:?}",
            result.content
        );

        let output = extract_output_text(&result)?;
        assert!(!output.is_empty(), "CSS selector output is empty");
        info!("CSS selector output: {}", output);

        // The h1 on example.com is "Example Domain"
        let output_lower = output.to_lowercase();
        assert!(
            output_lower.contains("example"),
            "Expected 'example' in h1 selector output, got: {}",
            output
        );

        info!("test_web_fetch_with_css_selector passed");
        Ok(())
    })
    .await
}

/// Test: Fetch a page requesting raw HTML format output
#[tokio::test]
async fn test_web_fetch_html_format() -> anyhow::Result<()> {
    run_with_sandbox("fetch-html", |agent, sandbox_id| async move {
        let result = call_tool_with_retry(
            &agent,
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://example.com",
                "format": "html"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("scrape_web html format failed")?;

        assert!(
            result.is_error != Some(true),
            "scrape_web html returned error: {:?}",
            result.content
        );

        let output = extract_output_text(&result)?;
        assert!(!output.is_empty(), "html output is empty");

        // HTML format should contain raw HTML tags
        let output_lower = output.to_lowercase();
        assert!(
            output_lower.contains("<")
                || output_lower.contains("html")
                || output_lower.contains("example"),
            "Expected HTML-like content, got: {}",
            &output[..output.len().min(300)]
        );

        info!("HTML format output: {} chars", output.len());
        info!("test_web_fetch_html_format passed");
        Ok(())
    })
    .await
}

/// Test: Fetch a page known to have dynamic/JS-rendered content
#[tokio::test]
async fn test_web_fetch_javascript_rendered_page() -> anyhow::Result<()> {
    run_with_sandbox("fetch-js-rendered", |agent, sandbox_id| async move {
        // httpbin.org/html serves a simple HTML page that the browser must render
        let result = call_tool_with_retry(
            &agent,
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://httpbin.org/html"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("scrape_web JS-rendered page failed")?;

        assert!(
            result.is_error != Some(true),
            "scrape_web JS-rendered returned error: {:?}",
            result.content
        );

        let output = extract_output_text(&result)?;
        assert!(!output.is_empty(), "JS-rendered page output is empty");
        info!("JS-rendered page: {} chars", output.len());

        // httpbin.org/html contains "Herman Melville" text
        let output_lower = output.to_lowercase();
        assert!(
            output_lower.contains("moby")
                || output_lower.contains("melville")
                || output_lower.contains("herman"),
            "Expected Moby Dick content from httpbin.org/html, got: {}",
            &output[..output.len().min(300)]
        );

        info!("test_web_fetch_javascript_rendered_page passed");
        Ok(())
    })
    .await
}

/// Test: Fetch a large documentation page and verify substantial content returned
#[tokio::test]
async fn test_web_fetch_large_documentation_page() -> anyhow::Result<()> {
    run_with_sandbox("fetch-large-page", |agent, sandbox_id| async move {
        // MDN Web Docs — a content-rich documentation page
        let result = call_tool_with_retry(
            &agent,
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://developer.mozilla.org/en-US/docs/Web/HTTP/Status"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("scrape_web large page failed")?;

        assert!(
            result.is_error != Some(true),
            "scrape_web large page returned error: {:?}",
            result.content
        );

        let output = extract_output_text(&result)?;
        assert!(!output.is_empty(), "large page output is empty");

        // A real documentation page should return substantial content
        assert!(
            output.len() > 500,
            "Expected >500 chars from documentation page, got {} chars",
            output.len()
        );

        let output_lower = output.to_lowercase();
        assert!(
            output_lower.contains("http")
                || output_lower.contains("status")
                || output_lower.contains("200"),
            "Expected HTTP status documentation content, got: {}",
            &output[..output.len().min(300)]
        );

        info!("Large documentation page: {} chars", output.len());
        info!("test_web_fetch_large_documentation_page passed");
        Ok(())
    })
    .await
}

/// Test: Fetch a page with screenshot capture enabled
#[tokio::test]
async fn test_web_fetch_with_screenshot() -> anyhow::Result<()> {
    run_with_sandbox("fetch-screenshot", |agent, sandbox_id| async move {
        let result = call_tool_with_retry(
            &agent,
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://example.com",
                "screenshot": true
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("scrape_web with screenshot failed")?;

        assert!(
            result.is_error != Some(true),
            "scrape_web with screenshot returned error: {:?}",
            result.content
        );

        let output = extract_output_text(&result)?;
        assert!(!output.is_empty(), "screenshot output is empty");

        // The response should contain base64-encoded image data or at minimum content
        // Screenshot data may be embedded as base64 or referenced
        info!("Screenshot output: {} chars", output.len());

        // Even with screenshot=true, the text content should still be present
        let output_lower = output.to_lowercase();
        assert!(
            output_lower.contains("example")
                || output_lower.contains("base64")
                || output_lower.contains("screenshot")
                || output_lower.contains("image")
                || output_lower.contains("data:image"),
            "Expected content or screenshot data, got: {}",
            &output[..output.len().min(300)]
        );

        info!("test_web_fetch_with_screenshot passed");
        Ok(())
    })
    .await
}

/// Test: Fetch a page with word_count_threshold to filter small content blocks
#[tokio::test]
async fn test_web_fetch_with_word_count_threshold() -> anyhow::Result<()> {
    run_with_sandbox("fetch-threshold", |agent, sandbox_id| async move {
        // Fetch with a high word_count_threshold to filter short blocks
        let result = call_tool_with_retry(
            &agent,
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://example.com",
                "word_count_threshold": 50
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("scrape_web with word_count_threshold failed")?;

        assert!(
            result.is_error != Some(true),
            "scrape_web with word_count_threshold returned error: {:?}",
            result.content
        );

        let output = extract_output_text(&result)?;
        // With a high threshold, we may get less content or empty (filtered out)
        // The key assertion is that the tool executed without error
        info!(
            "Word count threshold output: {} chars (threshold=50)",
            output.len()
        );

        info!("test_web_fetch_with_word_count_threshold passed");
        Ok(())
    })
    .await
}

/// Test: Fetching an invalid/unreachable URL returns an error
#[tokio::test]
async fn test_web_fetch_invalid_url_returns_error() -> anyhow::Result<()> {
    run_with_sandbox("fetch-invalid-url", |agent, sandbox_id| async move {
        let call_result = call_tool_with_retry(
            &agent,
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://this-domain-does-not-exist-dsb-test-12345.invalid"
            })
            .as_object()
            .cloned(),
        )
        .await;

        // Either the RPC call fails or the tool returns an error result — both are valid
        match call_result {
            Err(e) => {
                info!("Invalid URL correctly returned RPC-level error: {}", e);
            }
            Ok(result) => {
                let output = extract_output_text(&result).unwrap_or_default();
                let is_error = result.is_error == Some(true);
                let output_lower = output.to_lowercase();
                let has_error_content = output_lower.contains("error")
                    || output_lower.contains("fail")
                    || output_lower.contains("not")
                    || output_lower.contains("timeout")
                    || output_lower.contains("resolve");

                assert!(
                    is_error || has_error_content,
                    "Expected error for invalid URL, got is_error={:?}, output: {}",
                    result.is_error,
                    &output[..output.len().min(300)]
                );
                info!("Invalid URL correctly returned error/error content");
            }
        }

        info!("test_web_fetch_invalid_url_returns_error passed");
        Ok(())
    })
    .await
}

// ============================================================================
// Category 2: Concurrent Web Fetch (3 parallel tool calls)
// ============================================================================

/// Test: Fetch 3 different URLs concurrently using tokio::join!
#[tokio::test]
async fn test_web_fetch_concurrent_three_pages() -> anyhow::Result<()> {
    run_with_sandboxes("fetch-concurrent", 3, |agent, sandbox_ids| async move {
        let sandbox_id_1 = sandbox_ids[0].clone();
        let sandbox_id_2 = sandbox_ids[1].clone();
        let sandbox_id_3 = sandbox_ids[2].clone();

        // Use separate sandboxes so this test exercises 3 simultaneous MCP tool
        // calls without sharing one browser session. Same-sandbox concurrency is
        // already covered by test_web_fetch_concurrent_different_formats.
        let (result1, result2, result3) = tokio::join!(
            call_tool_with_retry(
                &agent,
                "scrape_web",
                serde_json::json!({
                    "sandbox_id": sandbox_id_1,
                    "url": "https://example.com"
                })
                .as_object()
                .cloned()
            ),
            call_tool_with_retry(
                &agent,
                "scrape_web",
                serde_json::json!({
                    "sandbox_id": sandbox_id_2,
                    "url": "https://www.python.org/"
                })
                .as_object()
                .cloned()
            ),
            call_tool_with_retry(
                &agent,
                "scrape_web",
                serde_json::json!({
                    "sandbox_id": sandbox_id_3,
                    "url": "https://www.rust-lang.org/"
                })
                .as_object()
                .cloned()
            )
        );

        // Verify all 3 succeeded
        let r1 = result1.context("concurrent fetch 1 (example.com) failed")?;
        let r2 = result2.context("concurrent fetch 2 (python.org) failed")?;
        let r3 = result3.context("concurrent fetch 3 (rust-lang.org) failed")?;

        assert!(
            r1.is_error != Some(true),
            "fetch example.com returned error"
        );
        assert!(r2.is_error != Some(true), "fetch python.org returned error");
        assert!(
            r3.is_error != Some(true),
            "fetch rust-lang.org returned error"
        );

        let output1 = extract_output_text(&r1)?;
        let output2 = extract_output_text(&r2)?;
        let output3 = extract_output_text(&r3)?;

        assert!(!output1.is_empty(), "example.com output empty");
        assert!(!output2.is_empty(), "python.org output empty");
        assert!(!output3.is_empty(), "rust-lang.org output empty");

        // Verify results are distinct content from different pages
        assert_ne!(
            output1, output2,
            "example.com and python.org returned identical content"
        );
        assert_ne!(
            output1, output3,
            "example.com and rust-lang.org returned identical content"
        );
        assert_ne!(
            output2, output3,
            "python.org and rust-lang.org returned identical content"
        );

        info!(
            "All 3 concurrent fetches succeeded: {} chars, {} chars, {} chars",
            output1.len(),
            output2.len(),
            output3.len()
        );

        info!("test_web_fetch_concurrent_three_pages passed");
        Ok(())
    })
    .await
}

/// Test: Fetch the same URL in 3 different formats concurrently
#[tokio::test]
async fn test_web_fetch_concurrent_different_formats() -> anyhow::Result<()> {
    run_with_sandbox("fetch-concurrent-fmt", |agent, sandbox_id| async move {
        let (md_result, text_result, links_result) = tokio::join!(
            call_tool_with_retry(
                &agent,
                "scrape_web",
                serde_json::json!({
                    "sandbox_id": sandbox_id,
                    "url": "https://example.com",
                    "format": "markdown"
                })
                .as_object()
                .cloned()
            ),
            call_tool_with_retry(
                &agent,
                "scrape_web",
                serde_json::json!({
                    "sandbox_id": sandbox_id,
                    "url": "https://example.com",
                    "format": "text"
                })
                .as_object()
                .cloned()
            ),
            call_tool_with_retry(
                &agent,
                "scrape_web",
                serde_json::json!({
                    "sandbox_id": sandbox_id,
                    "url": "https://example.com",
                    "format": "links"
                })
                .as_object()
                .cloned()
            )
        );

        let r_md = md_result.context("concurrent markdown fetch failed")?;
        let r_text = text_result.context("concurrent text fetch failed")?;
        let r_links = links_result.context("concurrent links fetch failed")?;

        assert!(r_md.is_error != Some(true), "markdown fetch returned error");
        assert!(r_text.is_error != Some(true), "text fetch returned error");
        assert!(r_links.is_error != Some(true), "links fetch returned error");

        let md_output = extract_output_text(&r_md)?;
        let text_output = extract_output_text(&r_text)?;
        let links_output = extract_output_text(&r_links)?;

        assert!(!md_output.is_empty(), "markdown output empty");
        assert!(!text_output.is_empty(), "text output empty");
        assert!(!links_output.is_empty(), "links output empty");

        info!(
            "All 3 concurrent format fetches succeeded: markdown={} chars, text={} chars, links={} chars",
            md_output.len(),
            text_output.len(),
            links_output.len()
        );

        info!("test_web_fetch_concurrent_different_formats passed");
        Ok(())
    })
    .await
}

/// Test: Fetch the same page with 3 different CSS selectors concurrently
#[tokio::test]
async fn test_web_fetch_concurrent_same_page_different_selectors() -> anyhow::Result<()> {
    run_with_sandbox(
        "fetch-concurrent-selectors",
        |agent, sandbox_id| async move {
            let sid1 = sandbox_id.clone();
            let sid2 = sandbox_id.clone();
            let sid3 = sandbox_id.clone();

            let (h1_result, p_result, a_result) = tokio::join!(
                call_tool_with_retry(
                    &agent,
                    "scrape_web",
                    serde_json::json!({
                        "sandbox_id": sid1,
                        "url": "https://example.com",
                        "css_selector": "h1"
                    })
                    .as_object()
                    .cloned()
                ),
                call_tool_with_retry(
                    &agent,
                    "scrape_web",
                    serde_json::json!({
                        "sandbox_id": sid2,
                        "url": "https://example.com",
                        "css_selector": "p"
                    })
                    .as_object()
                    .cloned()
                ),
                call_tool_with_retry(
                    &agent,
                    "scrape_web",
                    serde_json::json!({
                        "sandbox_id": sid3,
                        "url": "https://example.com",
                        "css_selector": "a"
                    })
                    .as_object()
                    .cloned()
                )
            );

            let r_h1 = h1_result.context("concurrent h1 selector failed")?;
            let r_p = p_result.context("concurrent p selector failed")?;
            let r_a = a_result.context("concurrent a selector failed")?;

            assert!(r_h1.is_error != Some(true), "h1 selector returned error");
            assert!(r_p.is_error != Some(true), "p selector returned error");
            assert!(r_a.is_error != Some(true), "a selector returned error");

            let h1_output = extract_output_text(&r_h1)?;
            let p_output = extract_output_text(&r_p)?;
            let a_output = extract_output_text(&r_a)?;

            assert!(!h1_output.is_empty(), "h1 output empty");
            assert!(!p_output.is_empty(), "p output empty");
            assert!(!a_output.is_empty(), "a output empty");

            // h1 should contain "Example Domain"
            let h1_lower = h1_output.to_lowercase();
            assert!(
                h1_lower.contains("example"),
                "Expected 'example' in h1 output, got: {}",
                h1_output
            );

            info!(
                "Concurrent selectors: h1={} chars, p={} chars, a={} chars",
                h1_output.len(),
                p_output.len(),
                a_output.len()
            );

            info!("test_web_fetch_concurrent_same_page_different_selectors passed");
            Ok(())
        },
    )
    .await
}

// ============================================================================
// Category 3: Web Search — Different Query Types
// ============================================================================

/// Test: Basic web search query
#[tokio::test]
async fn test_web_search_basic_query() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "Rust programming language"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("search_web failed")?;

    assert!(
        result.is_error != Some(true),
        "search_web returned error: {:?}",
        result.content
    );

    let output = extract_output_text(&result)?;
    assert!(!output.is_empty(), "search_web returned empty output");
    info!("Search results: {} chars", output.len());

    if search_results_unavailable(&output) {
        info!("Skipping basic search assertions because the search backend returned no results");
        return Ok(());
    }

    let output_lower = output.to_lowercase();
    assert!(
        output_lower.contains("rust") || output_lower.contains("http"),
        "Expected search results about 'Rust', got: {}",
        &output[..output.len().min(500)]
    );

    info!("test_web_search_basic_query passed");
    Ok(())
}

/// Test: Technical web search query
#[tokio::test]
async fn test_web_search_technical_query() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "web application security",
                "engine": "bing"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("search_web technical failed")?;

    assert!(
        result.is_error != Some(true),
        "search_web returned error: {:?}",
        result.content
    );

    let output = extract_output_text(&result)?;
    assert!(!output.is_empty(), "search_web returned empty output");
    info!("Technical search results: {} chars", output.len());

    if search_results_unavailable(&output) {
        info!(
            "Skipping technical search assertions because the search backend returned no results"
        );
        return Ok(());
    }

    let output_lower = output.to_lowercase();
    assert!(
        output_lower.contains("web")
            || output_lower.contains("application")
            || output_lower.contains("security"),
        "Expected technical search results, got: {}",
        &output[..output.len().min(500)]
    );

    info!("test_web_search_technical_query passed");
    Ok(())
}

/// Test: Web search with explicit num_results parameter
#[tokio::test]
async fn test_web_search_with_num_results() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "Python machine learning",
                "num_results": 5
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("search_web with num_results failed")?;

    assert!(
        result.is_error != Some(true),
        "search_web returned error: {:?}",
        result.content
    );

    let output = extract_output_text(&result)?;
    assert!(!output.is_empty(), "search_web returned empty output");

    if search_results_unavailable(&output) {
        info!(
            "Skipping num_results search assertions because the search backend returned no results"
        );
        return Ok(());
    }

    let result_count = count_markdown_search_results(&output);
    assert!(
        result_count > 0 && result_count <= 5,
        "Expected 1-5 search results, got {result_count}: {}",
        &output[..output.len().min(500)]
    );
    info!("Search with num_results=5: {} chars", output.len());

    info!("test_web_search_with_num_results passed");
    Ok(())
}

/// Test: Web search with a multi-word phrase query
#[tokio::test]
async fn test_web_search_phrase_query() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "cloud computing platforms",
                "engine": "bing"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("search_web phrase failed")?;

    assert!(
        result.is_error != Some(true),
        "search_web returned error: {:?}",
        result.content
    );

    let output = extract_output_text(&result)?;
    assert!(!output.is_empty(), "search_web returned empty output");

    if search_results_unavailable(&output) {
        info!("Skipping phrase search assertions because the search backend returned no results");
        return Ok(());
    }

    let output_lower = output.to_lowercase();
    assert!(
        output_lower.contains("cloud")
            || output_lower.contains("computing")
            || output_lower.contains("platform"),
        "Expected phrase search results, got: {}",
        &output[..output.len().min(500)]
    );
    info!("Phrase search results: {} chars", output.len());

    info!("test_web_search_phrase_query passed");
    Ok(())
}

/// Test: Web search using the Google engine explicitly
#[tokio::test]
async fn test_web_search_with_google_engine() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "Linux kernel development",
                "engine": "google"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("search_web google engine failed")?;

    assert!(
        result.is_error != Some(true),
        "search_web google returned error: {:?}",
        result.content
    );

    let output = extract_output_text(&result)?;
    assert!(!output.is_empty(), "google search returned empty output");

    if search_results_unavailable(&output) {
        info!("Skipping Google search assertions because the search backend returned no results");
        return Ok(());
    }

    let output_lower = output.to_lowercase();
    assert!(
        output_lower.contains("linux")
            || output_lower.contains("kernel")
            || output_lower.contains("http"),
        "Expected Linux-related search results, got: {}",
        &output[..output.len().min(500)]
    );
    info!("Google engine search: {} chars", output.len());

    info!("test_web_search_with_google_engine passed");
    Ok(())
}

/// Test: Web search using the DuckDuckGo engine explicitly
#[tokio::test]
async fn test_web_search_with_duckduckgo_engine() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "open source software",
                "engine": "duckduckgo"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("search_web duckduckgo engine failed")?;

    assert!(
        result.is_error != Some(true),
        "search_web duckduckgo returned error: {:?}",
        result.content
    );

    let output = extract_output_text(&result)?;
    assert!(
        !output.is_empty(),
        "duckduckgo search returned empty output"
    );

    if search_results_unavailable(&output) {
        info!(
            "Skipping DuckDuckGo search assertions because the search backend returned no results"
        );
        return Ok(());
    }

    let output_lower = output.to_lowercase();
    assert!(
        output_lower.contains("open")
            || output_lower.contains("source")
            || output_lower.contains("software")
            || output_lower.contains("http"),
        "Expected open source search results, got: {}",
        &output[..output.len().min(500)]
    );
    info!("DuckDuckGo engine search: {} chars", output.len());

    info!("test_web_search_with_duckduckgo_engine passed");
    Ok(())
}

/// Test: Web search with a single-word query
#[tokio::test]
async fn test_web_search_single_word_query() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "kubernetes"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("search_web single word failed")?;

    assert!(
        result.is_error != Some(true),
        "search_web single word returned error: {:?}",
        result.content
    );

    let output = extract_output_text(&result)?;
    assert!(
        !output.is_empty(),
        "single word search returned empty output"
    );

    if search_results_unavailable(&output) {
        info!(
            "Skipping single-word search assertions because the search backend returned no results"
        );
        return Ok(());
    }

    let result_count = count_markdown_search_results(&output);
    assert!(
        result_count > 0,
        "Expected at least 1 search result for 'kubernetes', got {}: {}",
        result_count,
        &output[..output.len().min(500)]
    );
    info!(
        "Single word search: {} results, {} chars",
        result_count,
        output.len()
    );

    info!("test_web_search_single_word_query passed");
    Ok(())
}

/// Test: Web search with a long multi-word query
#[tokio::test]
async fn test_web_search_long_query() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "how to build a distributed system with microservices architecture in production"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("search_web long query failed")?;

    assert!(
        result.is_error != Some(true),
        "search_web long query returned error: {:?}",
        result.content
    );

    let output = extract_output_text(&result)?;
    assert!(
        !output.is_empty(),
        "long query search returned empty output"
    );

    if search_results_unavailable(&output) {
        info!(
            "Skipping long-query search assertions because the search backend returned no results"
        );
        return Ok(());
    }

    let result_count = count_markdown_search_results(&output);
    assert!(
        result_count > 0,
        "Expected at least 1 search result for long query, got {}: {}",
        result_count,
        &output[..output.len().min(500)]
    );
    info!(
        "Long query search: {} results, {} chars",
        result_count,
        output.len()
    );

    info!("test_web_search_long_query passed");
    Ok(())
}

/// Test: Web search with special characters in the query
#[tokio::test]
async fn test_web_search_special_characters_query() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "C++ vs C# programming"
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("search_web special chars failed")?;

    assert!(
        result.is_error != Some(true),
        "search_web special chars returned error: {:?}",
        result.content
    );

    let output = extract_output_text(&result)?;
    assert!(
        !output.is_empty(),
        "special chars search returned empty output"
    );

    if search_results_unavailable(&output) {
        info!("Skipping special-character search assertions because the search backend returned no results");
        return Ok(());
    }

    let output_lower = output.to_lowercase();
    assert!(
        output_lower.contains("c++")
            || output_lower.contains("c#")
            || output_lower.contains("programming")
            || output_lower.contains("http"),
        "Expected C++/C# search results, got: {}",
        &output[..output.len().min(500)]
    );
    info!("Special characters search: {} chars", output.len());

    info!("test_web_search_special_characters_query passed");
    Ok(())
}

/// Test: Validate that search results follow the expected markdown structure
/// (numbered list with title, URL, and snippet)
#[tokio::test]
async fn test_web_search_result_structure_validation() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "Rust programming language",
                "num_results": 5
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("search_web for structure validation failed")?;

    assert!(
        result.is_error != Some(true),
        "search_web returned error: {:?}",
        result.content
    );

    let output = extract_output_text(&result)?;
    assert!(!output.is_empty(), "search output is empty");

    if search_results_unavailable(&output) {
        info!(
            "Skipping search structure assertions because the search backend returned no results"
        );
        return Ok(());
    }

    let result_count = count_markdown_search_results(&output);
    assert!(
        result_count > 0,
        "Expected numbered markdown results, got none in: {}",
        &output[..output.len().min(500)]
    );

    // Validate structure: each result should have a numbered entry with **[title](url)** format
    let mut found_url = false;
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some((idx, rest)) = trimmed.split_once(". **[") {
            if idx.chars().all(|c| c.is_ascii_digit()) && rest.contains("](") && rest.contains(")")
            {
                // Extract URL and verify it looks like a valid URL
                if let Some(url_start) = rest.find("](") {
                    let url_part = &rest[url_start + 2..];
                    if let Some(url_end) = url_part.find(')') {
                        let url = &url_part[..url_end];
                        assert!(
                            url.starts_with("http://") || url.starts_with("https://"),
                            "Expected URL to start with http(s)://, got: {}",
                            url
                        );
                        found_url = true;
                    }
                }
            }
        }
    }

    assert!(
        found_url,
        "Expected at least one result with a valid URL in markdown format"
    );

    info!(
        "Structure validation: {} results with valid markdown format",
        result_count
    );

    info!("test_web_search_result_structure_validation passed");
    Ok(())
}

/// Test: Using an unsupported search engine returns an error
#[tokio::test]
async fn test_web_search_invalid_engine_returns_error() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let call_result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "test query",
                "engine": "askjeeves"
            })
            .as_object()
            .cloned(),
        )
        .await;

    // Either the RPC call fails or the tool returns an error result — both are valid
    match call_result {
        Err(e) => {
            let err_msg = e.to_string().to_lowercase();
            assert!(
                err_msg.contains("unsupported")
                    || err_msg.contains("error")
                    || err_msg.contains("invalid"),
                "Expected error about unsupported engine, got: {}",
                e
            );
            info!("Invalid engine correctly returned RPC-level error: {}", e);
        }
        Ok(result) => {
            let is_error = result.is_error == Some(true);
            let output = extract_output_text(&result).unwrap_or_default();
            let output_lower = output.to_lowercase();
            let has_error_content = output_lower.contains("unsupported")
                || output_lower.contains("error")
                || output_lower.contains("invalid");

            assert!(
                is_error || has_error_content,
                "Expected error for unsupported engine 'askjeeves', got is_error={:?}, output: {}",
                result.is_error,
                &output[..output.len().min(500)]
            );
            info!("Invalid engine correctly returned error in tool result");
        }
    }

    info!("test_web_search_invalid_engine_returns_error passed");
    Ok(())
}

/// Test: Web search with num_results=1 (minimum boundary)
#[tokio::test]
async fn test_web_search_num_results_boundary() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool(
            "search_web",
            serde_json::json!({
                "query": "GitHub",
                "num_results": 1
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("search_web num_results=1 failed")?;

    assert!(
        result.is_error != Some(true),
        "search_web num_results=1 returned error: {:?}",
        result.content
    );

    let output = extract_output_text(&result)?;
    assert!(!output.is_empty(), "num_results=1 returned empty output");

    if search_results_unavailable(&output) {
        info!("Skipping num_results boundary assertions because the search backend returned no results");
        return Ok(());
    }

    let result_count = count_markdown_search_results(&output);
    assert!(
        result_count == 1,
        "Expected exactly 1 result with num_results=1, got {}: {}",
        result_count,
        &output[..output.len().min(500)]
    );

    info!(
        "num_results=1 boundary: {} result, {} chars",
        result_count,
        output.len()
    );

    info!("test_web_search_num_results_boundary passed");
    Ok(())
}

// ============================================================================
// Category 4: Concurrent Web Search (3 parallel tool calls)
// ============================================================================

/// Test: Search 3 different queries concurrently using tokio::join!
#[tokio::test]
async fn test_web_search_concurrent_three_queries() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let (result1, result2, result3) = tokio::join!(
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "Rust programming language"
            })
            .as_object()
            .cloned()
        ),
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "Python machine learning"
            })
            .as_object()
            .cloned()
        ),
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "JavaScript frameworks"
            })
            .as_object()
            .cloned()
        )
    );

    let r1 = result1.context("concurrent search 1 (Rust) failed")?;
    let r2 = result2.context("concurrent search 2 (Python) failed")?;
    let r3 = result3.context("concurrent search 3 (JavaScript) failed")?;

    assert!(r1.is_error != Some(true), "Rust search returned error");
    assert!(r2.is_error != Some(true), "Python search returned error");
    assert!(
        r3.is_error != Some(true),
        "JavaScript search returned error"
    );

    let output1 = extract_output_text(&r1)?;
    let output2 = extract_output_text(&r2)?;
    let output3 = extract_output_text(&r3)?;

    assert!(!output1.is_empty(), "Rust search output empty");
    assert!(!output2.is_empty(), "Python search output empty");
    assert!(!output3.is_empty(), "JavaScript search output empty");

    if [output1.as_str(), output2.as_str(), output3.as_str()]
        .iter()
        .any(|output| search_results_unavailable(output))
    {
        info!(
            "Skipping concurrent three-query search assertions because at least one search returned no results"
        );
        return Ok(());
    }

    assert_ne!(
        output1, output2,
        "Rust and Python search returned identical results"
    );
    assert_ne!(
        output1, output3,
        "Rust and JavaScript search returned identical results"
    );
    assert_ne!(
        output2, output3,
        "Python and JavaScript search returned identical results"
    );

    info!(
        "All 3 concurrent searches succeeded: {} chars, {} chars, {} chars",
        output1.len(),
        output2.len(),
        output3.len()
    );

    info!("test_web_search_concurrent_three_queries passed");
    Ok(())
}

/// Test: Search 3 different queries concurrently, all using the same engine
#[tokio::test]
async fn test_web_search_concurrent_same_engine() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let (result1, result2, result3) = tokio::join!(
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "cloud computing platforms",
                "engine": "bing"
            })
            .as_object()
            .cloned()
        ),
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "database management systems",
                "engine": "bing"
            })
            .as_object()
            .cloned()
        ),
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "web application security",
                "engine": "bing"
            })
            .as_object()
            .cloned()
        )
    );

    let r1 = result1.context("concurrent Bing search 1 failed")?;
    let r2 = result2.context("concurrent Bing search 2 failed")?;
    let r3 = result3.context("concurrent Bing search 3 failed")?;

    assert!(
        r1.is_error != Some(true),
        "cloud computing Bing search returned error"
    );
    assert!(
        r2.is_error != Some(true),
        "database Bing search returned error"
    );
    assert!(
        r3.is_error != Some(true),
        "security Bing search returned error"
    );

    let output1 = extract_output_text(&r1)?;
    let output2 = extract_output_text(&r2)?;
    let output3 = extract_output_text(&r3)?;

    assert!(!output1.is_empty(), "cloud computing search output empty");
    assert!(!output2.is_empty(), "database search output empty");
    assert!(!output3.is_empty(), "security search output empty");

    if [output1.as_str(), output2.as_str(), output3.as_str()]
        .iter()
        .any(|output| search_results_unavailable(output))
    {
        info!(
            "Skipping concurrent same-engine search assertions because at least one search returned no results"
        );
        return Ok(());
    }

    assert_ne!(
        output1, output2,
        "cloud computing and database search returned identical results"
    );
    assert_ne!(
        output1, output3,
        "cloud computing and security search returned identical results"
    );

    info!(
        "All 3 concurrent Bing searches succeeded: {} chars, {} chars, {} chars",
        output1.len(),
        output2.len(),
        output3.len()
    );

    info!("test_web_search_concurrent_same_engine passed");
    Ok(())
}

/// Test: Search 3 different queries concurrently, each using a different engine
/// (google, bing, duckduckgo) via tokio::join!
#[tokio::test]
async fn test_web_search_concurrent_different_engines() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let (result1, result2, result3) = tokio::join!(
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "artificial intelligence",
                "engine": "google"
            })
            .as_object()
            .cloned()
        ),
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "machine learning frameworks",
                "engine": "bing"
            })
            .as_object()
            .cloned()
        ),
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "deep learning tutorials",
                "engine": "duckduckgo"
            })
            .as_object()
            .cloned()
        )
    );

    let r1 = result1.context("concurrent google search failed")?;
    let r2 = result2.context("concurrent bing search failed")?;
    let r3 = result3.context("concurrent duckduckgo search failed")?;

    assert!(r1.is_error != Some(true), "google search returned error");
    assert!(r2.is_error != Some(true), "bing search returned error");
    assert!(
        r3.is_error != Some(true),
        "duckduckgo search returned error"
    );

    let output1 = extract_output_text(&r1)?;
    let output2 = extract_output_text(&r2)?;
    let output3 = extract_output_text(&r3)?;

    assert!(!output1.is_empty(), "google search output empty");
    assert!(!output2.is_empty(), "bing search output empty");
    assert!(!output3.is_empty(), "duckduckgo search output empty");

    if [output1.as_str(), output2.as_str(), output3.as_str()]
        .iter()
        .any(|output| search_results_unavailable(output))
    {
        info!(
            "Skipping concurrent multi-engine search assertions because at least one search returned no results"
        );
        return Ok(());
    }

    // All results should have numbered markdown entries
    let count1 = count_markdown_search_results(&output1);
    let count2 = count_markdown_search_results(&output2);
    let count3 = count_markdown_search_results(&output3);

    assert!(count1 > 0, "google returned 0 results");
    assert!(count2 > 0, "bing returned 0 results");
    assert!(count3 > 0, "duckduckgo returned 0 results");

    info!(
        "Concurrent different engines: google={} results ({} chars), bing={} results ({} chars), duckduckgo={} results ({} chars)",
        count1, output1.len(),
        count2, output2.len(),
        count3, output3.len()
    );

    info!("test_web_search_concurrent_different_engines passed");
    Ok(())
}

/// Test: Search 3 queries concurrently with different num_results (3, 5, 10)
/// and verify the result counts respect the requested limits
#[tokio::test]
async fn test_web_search_concurrent_with_different_num_results() -> anyhow::Result<()> {
    let agent = MonorailAgent::new().await?;
    let (result1, result2, result3) = tokio::join!(
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "containerization tools",
                "num_results": 3
            })
            .as_object()
            .cloned()
        ),
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "continuous integration",
                "num_results": 5
            })
            .as_object()
            .cloned()
        ),
        agent.call_tool(
            "search_web",
            serde_json::json!({
                "query": "infrastructure as code",
                "num_results": 10
            })
            .as_object()
            .cloned()
        )
    );

    let r1 = result1.context("concurrent search (num=3) failed")?;
    let r2 = result2.context("concurrent search (num=5) failed")?;
    let r3 = result3.context("concurrent search (num=10) failed")?;

    assert!(r1.is_error != Some(true), "search num=3 returned error");
    assert!(r2.is_error != Some(true), "search num=5 returned error");
    assert!(r3.is_error != Some(true), "search num=10 returned error");

    let output1 = extract_output_text(&r1)?;
    let output2 = extract_output_text(&r2)?;
    let output3 = extract_output_text(&r3)?;

    assert!(!output1.is_empty(), "num=3 search output empty");
    assert!(!output2.is_empty(), "num=5 search output empty");
    assert!(!output3.is_empty(), "num=10 search output empty");

    if [output1.as_str(), output2.as_str(), output3.as_str()]
        .iter()
        .any(|output| search_results_unavailable(output))
    {
        info!(
            "Skipping concurrent num_results search assertions because at least one search returned no results"
        );
        return Ok(());
    }

    let count1 = count_markdown_search_results(&output1);
    let count2 = count_markdown_search_results(&output2);
    let count3 = count_markdown_search_results(&output3);

    // Each result count should respect its requested limit
    assert!(
        count1 > 0 && count1 <= 3,
        "Expected 1-3 results for num_results=3, got {}: {}",
        count1,
        &output1[..output1.len().min(500)]
    );
    assert!(
        count2 > 0 && count2 <= 5,
        "Expected 1-5 results for num_results=5, got {}: {}",
        count2,
        &output2[..output2.len().min(500)]
    );
    assert!(
        count3 > 0 && count3 <= 10,
        "Expected 1-10 results for num_results=10, got {}: {}",
        count3,
        &output3[..output3.len().min(500)]
    );

    info!(
        "Concurrent different num_results: 3→{} results, 5→{} results, 10→{} results",
        count1, count2, count3
    );

    info!("test_web_search_concurrent_with_different_num_results passed");
    Ok(())
}
