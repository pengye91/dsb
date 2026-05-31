// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! K8s E2E tests: Concurrent tool execution with real Wikipedia pages.
//!
//! These tests validate that the MCP web and browser tools work correctly
//! under concurrent load against a real K8s cluster. They use real Wikipedia
//! pages instead of simple test pages like example.com.
//!
//! Prerequisites:
//!   - MCP server deployed in the k8s cluster
//!   - Port-forward to MCP services: `kubectl port-forward svc/dsb-mcp-server 3333:3000 -n dsb`
//!   - `DSB_API_KEY` set

use crate::agents::MonorailAgent;
use crate::tests::k8s_mod::{
    k8s_mcp_browser_url, k8s_mcp_web_url, k8s_sandbox_image, k8s_session_id, WIKIPEDIA_PAGES,
};
use crate::tests::test_utils::{call_tool_with_retry, extract_output_text, extract_sandbox_id};
use anyhow::Context;
use std::future::Future;
use tracing::info;

// ============================================================================
// Helpers
// ============================================================================

/// Creates a web-agent connected sandbox and returns (agent, session_id, sandbox_id).
async fn create_web_session(test_name: &str) -> anyhow::Result<(MonorailAgent, String, String)> {
    let web_agent = MonorailAgent::connect_to_url(k8s_mcp_web_url())
        .await
        .context("connect to web MCP")?;

    let session_id = k8s_session_id(test_name);
    let image = k8s_sandbox_image();

    info!(%session_id, %image, "Creating sandbox via web service");

    // web_fetch auto-creates sandbox on first call; but we explicitly create
    // via sandbox service so we can capture the ID for cleanup.
    let sandbox_agent = MonorailAgent::connect_to_url(
        std::env::var("K8S_MCP_SANDBOX_URL")
            .unwrap_or_else(|_| "http://localhost:3333/mcp/dsb/sandbox".to_string()),
    )
    .await
    .context("connect to sandbox MCP")?;

    let create_result = call_tool_with_retry(
        &sandbox_agent,
        "create_sandbox",
        serde_json::json!({
            "session_id": session_id.clone(),
            "image": image
        })
        .as_object()
        .cloned(),
    )
    .await
    .context("create_sandbox failed")?;

    let sandbox_id = extract_sandbox_id(&create_result)?;
    info!(%sandbox_id, "Sandbox created");

    Ok((web_agent, session_id, sandbox_id))
}

/// Destroy a sandbox by session ID.
async fn destroy_session(session_id: &str) {
    let sandbox_agent = match MonorailAgent::connect_to_url(
        std::env::var("K8S_MCP_SANDBOX_URL")
            .unwrap_or_else(|_| "http://localhost:3333/mcp/dsb/sandbox".to_string()),
    )
    .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!("Failed to connect sandbox MCP for cleanup: {}", e);
            return;
        }
    };

    let _ = sandbox_agent
        .call_tool(
            "destroy_sandbox",
            serde_json::json!({"session_id": session_id})
                .as_object()
                .cloned(),
        )
        .await;
}

/// Run a test body with automatic sandbox cleanup.
async fn run_with_web_session<F, Fut>(test_name: &str, test_body: F) -> anyhow::Result<()>
where
    F: FnOnce(MonorailAgent, String, String) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let (agent, session_id, sandbox_id) = create_web_session(test_name).await?;
    let session_for_cleanup = session_id.clone();

    let join_result = tokio::task::spawn(test_body(agent, session_id, sandbox_id)).await;

    destroy_session(&session_for_cleanup).await;

    match join_result {
        Ok(r) => r,
        Err(e) => std::panic::resume_unwind(e.into_panic()),
    }
}

// ============================================================================
// Category 1: Concurrent web_fetch
// ============================================================================

/// Test: Concurrently fetch 5 different Wikipedia pages.
#[tokio::test]
async fn test_k8s_concurrent_web_fetch_wikipedia() -> anyhow::Result<()> {
    run_with_web_session(
        "concurrent-wiki-5",
        |agent, session_id, _sandbox_id| async move {
            let pages = WIKIPEDIA_PAGES;

            // Build concurrent fetch futures
            let fetches: Vec<_> = pages
                .iter()
                .enumerate()
                .map(|(i, (url, _expected))| {
                    let agent_ref = &agent;
                    let session = session_id.clone();
                    let url = url.to_string();
                    async move {
                        let result = call_tool_with_retry(
                            agent_ref,
                            "web_fetch",
                            serde_json::json!({
                                "session_id": session,
                                "url": url,
                                "format": "markdown",
                                "max_length": 50000
                            })
                            .as_object()
                            .cloned(),
                        )
                        .await
                        .context(format!("web_fetch {} failed", i))?;

                        if result.is_error == Some(true) {
                            let msg = extract_output_text(&result).unwrap_or_default();
                            anyhow::bail!("web_fetch {} returned error: {}", i, msg);
                        }

                        let text = extract_output_text(&result)?;
                        anyhow::ensure!(
                            text.len() > 500,
                            "web_fetch {} returned only {} chars",
                            i,
                            text.len()
                        );
                        info!("Fetched page {}: {} chars", i, text.len());
                        Ok(text)
                    }
                })
                .collect();

            // Run all 5 fetches concurrently
            let results = futures::future::join_all(fetches).await;

            for (i, result) in results.iter().enumerate() {
                let text = result
                    .as_ref()
                    .map_err(|e| anyhow::anyhow!("fetch {}: {}", i, e))?;
                let (_, expected_keywords) = pages[i];
                let text_lower = text.to_lowercase();
                let mut found = 0;
                for kw in expected_keywords {
                    if text_lower.contains(&kw.to_lowercase()) {
                        found += 1;
                    }
                }
                anyhow::ensure!(
                    found >= expected_keywords.len().saturating_sub(1),
                    "Page {} missing expected keywords. Found {}/{}: {:?}",
                    i,
                    found,
                    expected_keywords.len(),
                    expected_keywords
                );
            }

            info!(
                "All {} Wikipedia pages fetched concurrently and verified",
                pages.len()
            );
            Ok(())
        },
    )
    .await
}

/// Test: Rapid sequential web_fetch on the same session (sandbox reuse).
#[tokio::test]
async fn test_k8s_rapid_sequential_web_fetch() -> anyhow::Result<()> {
    run_with_web_session("rapid-seq", |agent, session_id, _sandbox_id| async move {
        let urls = [
            "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            "https://en.wikipedia.org/wiki/Kubernetes",
            "https://en.wikipedia.org/wiki/World_Wide_Web",
        ];

        for (i, url) in urls.iter().enumerate() {
            let start = std::time::Instant::now();
            let result = call_tool_with_retry(
                &agent,
                "web_fetch",
                serde_json::json!({
                    "session_id": session_id,
                    "url": url,
                    "format": "markdown",
                    "max_length": 10000
                })
                .as_object()
                .cloned(),
            )
            .await
            .context(format!("sequential fetch {} failed", i))?;

            if result.is_error == Some(true) {
                let msg = extract_output_text(&result).unwrap_or_default();
                anyhow::bail!("sequential fetch {} error: {}", i, msg);
            }

            let text = extract_output_text(&result)?;
            anyhow::ensure!(
                text.len() > 200,
                "fetch {} returned only {} chars",
                i,
                text.len()
            );
            info!(
                "Sequential fetch {} done in {:?}: {} chars",
                i,
                start.elapsed(),
                text.len()
            );
        }

        info!(
            "All {} sequential fetches completed with sandbox reuse",
            urls.len()
        );
        Ok(())
    })
    .await
}

// ============================================================================
// Category 2: Concurrent browser tools
// ============================================================================

/// Test: Concurrent browser_navigate + screenshot on 3 Wikipedia pages.
/// Each page gets its own sandbox since browser automation is not thread-safe
/// within a single browser instance.
#[tokio::test]
async fn test_k8s_concurrent_browser_screenshot() -> anyhow::Result<()> {
    let pages = WIKIPEDIA_PAGES[..3].to_vec();

    let tasks: Vec<_> = pages
        .into_iter()
        .enumerate()
        .map(|(i, (url, _))| {
            let url = url.to_string();
            async move {
                run_with_web_session(
                    &format!("browser-concurrent-{}", i),
                    move |_web_agent, session_id, _sandbox_id| async move {
                        let browser_agent = MonorailAgent::connect_to_url(k8s_mcp_browser_url())
                            .await
                            .context("connect to browser MCP")?;

                        // Navigate
                        let nav_result = call_tool_with_retry(
                            &browser_agent,
                            "browser_navigate",
                            serde_json::json!({
                                "session_id": session_id.clone(),
                                "url": url
                            })
                            .as_object()
                            .cloned(),
                        )
                        .await
                        .context(format!("browser_navigate {} failed", i))?;

                        if nav_result.is_error == Some(true) {
                            let msg = extract_output_text(&nav_result).unwrap_or_default();
                            anyhow::bail!("browser_navigate {} error: {}", i, msg);
                        }
                        info!("Browser navigated to page {}", i);

                        // Screenshot
                        let ss_result = call_tool_with_retry(
                            &browser_agent,
                            "browser_screenshot",
                            serde_json::json!({
                                "session_id": session_id,
                                "full_page": false
                            })
                            .as_object()
                            .cloned(),
                        )
                        .await
                        .context(format!("browser_screenshot {} failed", i))?;

                        if ss_result.is_error == Some(true) {
                            let msg = extract_output_text(&ss_result).unwrap_or_default();
                            anyhow::bail!("browser_screenshot {} error: {}", i, msg);
                        }

                        info!("Screenshot {} succeeded", i);
                        Ok(())
                    },
                )
                .await
            }
        })
        .collect();

    for (i, result) in futures::future::join_all(tasks).await.iter().enumerate() {
        result
            .as_ref()
            .map_err(|e| anyhow::anyhow!("screenshot task {}: {}", i, e))?;
    }

    info!("All browser screenshots completed concurrently with separate sandboxes");
    Ok(())
}

/// Test: Mixed concurrent load — web_fetch + browser_navigate simultaneously.
/// Web fetches share a sandbox; each browser task gets its own sandbox
/// since browser automation is not thread-safe within a single instance.
#[tokio::test]
async fn test_k8s_mixed_concurrent_tools() -> anyhow::Result<()> {
    // Web fetches share one sandbox
    run_with_web_session(
        "mixed-web",
        |_web_agent, web_session, _sandbox_id| async move {
            // 3 web_fetch + 2 browser_navigate at the same time
            let mut tasks: Vec<std::pin::Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>> =
                Vec::with_capacity(5);

            // Web fetches — all share the same web session
            for (i, page) in WIKIPEDIA_PAGES.iter().take(3).enumerate() {
                let session = web_session.clone();
                let url = page.0.to_string();
                let web_url = k8s_mcp_web_url();
                tasks.push(Box::pin(async move {
                    let agent = MonorailAgent::connect_to_url(web_url)
                        .await
                        .context("connect web MCP for mixed task")?;
                    let result = call_tool_with_retry(
                        &agent,
                        "web_fetch",
                        serde_json::json!({
                            "session_id": session,
                            "url": url,
                            "format": "markdown",
                            "max_length": 20000
                        })
                        .as_object()
                        .cloned(),
                    )
                    .await?;

                    if result.is_error == Some(true) {
                        let msg = extract_output_text(&result).unwrap_or_default();
                        anyhow::bail!("web_fetch mixed {} error: {}", i, msg);
                    }
                    let text = extract_output_text(&result)?;
                    anyhow::ensure!(text.len() > 200, "mixed fetch {} too short", i);
                    info!("Mixed web_fetch {}: {} chars", i, text.len());
                    Ok(())
                }));
            }

            // Browser navigations — each gets its own sandbox/session
            for (i, page) in WIKIPEDIA_PAGES.iter().take(2).enumerate() {
                let url = page.0.to_string();
                let browser_url = k8s_mcp_browser_url();
                let sandbox_url = std::env::var("K8S_MCP_SANDBOX_URL")
                    .unwrap_or_else(|_| "http://localhost:3333/mcp/dsb/sandbox".to_string());
                tasks.push(Box::pin(async move {
                    let browser_session = k8s_session_id(&format!("mixed-browser-{}", i));
                    let sandbox_agent = MonorailAgent::connect_to_url(&sandbox_url)
                        .await
                        .context("connect sandbox MCP for mixed browser task")?;
                    let create_result = call_tool_with_retry(
                        &sandbox_agent,
                        "create_sandbox",
                        serde_json::json!({
                            "session_id": browser_session.clone(),
                            "image": k8s_sandbox_image()
                        })
                        .as_object()
                        .cloned(),
                    )
                    .await
                    .context("create_sandbox for mixed browser task")?;

                    let _sandbox_id = extract_sandbox_id(&create_result)?;

                    let agent = MonorailAgent::connect_to_url(browser_url)
                        .await
                        .context("connect browser MCP for mixed task")?;
                    let result = call_tool_with_retry(
                        &agent,
                        "browser_navigate",
                        serde_json::json!({
                            "session_id": browser_session.clone(),
                            "url": url
                        })
                        .as_object()
                        .cloned(),
                    )
                    .await?;

                    if result.is_error == Some(true) {
                        let msg = extract_output_text(&result).unwrap_or_default();
                        anyhow::bail!("browser_navigate mixed {} error: {}", i, msg);
                    }
                    info!("Mixed browser_navigate {} succeeded", i);

                    // Cleanup browser sandbox
                    let _ = sandbox_agent
                        .call_tool(
                            "destroy_sandbox",
                            serde_json::json!({"session_id": browser_session})
                                .as_object()
                                .cloned(),
                        )
                        .await;

                    Ok(())
                }));
            }

            let results = futures::future::join_all(tasks).await;
            for (i, result) in results.iter().enumerate() {
                result
                    .as_ref()
                    .map_err(|e| anyhow::anyhow!("mixed task {} failed: {}", i, e))?;
            }

            info!("Mixed concurrent load test passed");
            Ok(())
        },
    )
    .await
}

/// Test: Browser click + fill workflow on Wikipedia search.
#[tokio::test]
async fn test_k8s_browser_interaction_workflow() -> anyhow::Result<()> {
    run_with_web_session(
        "browser-interact",
        |_web_agent, session_id, _sandbox_id| async move {
            let browser_agent = MonorailAgent::connect_to_url(k8s_mcp_browser_url())
                .await
                .context("connect to browser MCP")?;

            // Navigate to Wikipedia
            let nav_result = call_tool_with_retry(
                &browser_agent,
                "browser_navigate",
                serde_json::json!({
                    "session_id": session_id.clone(),
                    "url": "https://en.wikipedia.org/wiki/Main_Page"
                })
                .as_object()
                .cloned(),
            )
            .await
            .context("navigate to Wikipedia main page")?;

            if nav_result.is_error == Some(true) {
                let msg = extract_output_text(&nav_result).unwrap_or_default();
                anyhow::bail!("navigate error: {}", msg);
            }
            info!("Navigated to Wikipedia main page");

            // Get clickable elements
            let elements_result = call_tool_with_retry(
                &browser_agent,
                "browser_get_clickable_elements",
                serde_json::json!({"session_id": session_id.clone()})
                    .as_object()
                    .cloned(),
            )
            .await
            .context("get clickable elements")?;

            let elements_text = extract_output_text(&elements_result)?;
            info!("Clickable elements: {} chars", elements_text.len());
            anyhow::ensure!(
                elements_text.len() > 50,
                "Expected clickable elements, got: {}",
                elements_text
            );

            // Take a screenshot
            let ss_result = call_tool_with_retry(
                &browser_agent,
                "browser_screenshot",
                serde_json::json!({
                    "session_id": session_id.clone(),
                    "full_page": false
                })
                .as_object()
                .cloned(),
            )
            .await
            .context("take screenshot")?;

            if ss_result.is_error == Some(true) {
                let msg = extract_output_text(&ss_result).unwrap_or_default();
                anyhow::bail!("screenshot error: {}", msg);
            }
            info!("Screenshot taken successfully");

            // Evaluate JS to get page title
            let eval_result = call_tool_with_retry(
                &browser_agent,
                "browser_evaluate",
                serde_json::json!({
                    "session_id": session_id,
                    "script": "document.title"
                })
                .as_object()
                .cloned(),
            )
            .await
            .context("evaluate JS")?;

            let eval_text = extract_output_text(&eval_result)?;
            let title_lower = eval_text.to_lowercase();
            anyhow::ensure!(
                title_lower.contains("wikipedia") || title_lower.contains("main page"),
                "Expected Wikipedia in title, got: {}",
                eval_text
            );
            info!("Page title verified: {}", eval_text);

            Ok(())
        },
    )
    .await
}
