// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Shared test utilities for dsb-agent-tester

use crate::agents::MonorailAgent;
use std::time::Duration;
use tracing::{info, warn};

const CREATE_SANDBOX_RETRY_ATTEMPTS: usize = 5;

pub fn setup_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

/// Maximum time to wait for a sandbox to reach "running" state after creation.
/// Full `make test` runs many sandboxes in sequence; Docker-on-desktop can exceed 120s under load.
const SANDBOX_READINESS_TIMEOUT_SECS: u64 = 300;
/// Interval between readiness polls.
const SANDBOX_READINESS_POLL_INTERVAL_SECS: u64 = 2;

fn is_transient_sandbox_creation_failure(message: &str) -> bool {
    message.contains("docker.raw.sock: connect: connection refused")
        || message.contains("error sending request for url")
        || message.contains("connect: connection refused")
        || message.contains("Internal server error")
}

/// Creates a sandbox and retries narrow transient Docker daemon connection failures.
pub async fn create_sandbox_with_retry(
    agent: &MonorailAgent,
    name: String,
    image: String,
) -> anyhow::Result<rmcp::model::CallToolResult> {
    for attempt in 1..=CREATE_SANDBOX_RETRY_ATTEMPTS {
        let result = agent
            .call_tool(
                "create_sandbox",
                serde_json::json!({
                    "name": name.clone(),
                    "image": image.clone()
                })
                .as_object()
                .cloned(),
            )
            .await;

        match result {
            Ok(tool_result) if tool_result.is_error != Some(true) => return Ok(tool_result),
            Ok(tool_result) => {
                let message = format!("{:?}", tool_result.content);
                if attempt == CREATE_SANDBOX_RETRY_ATTEMPTS
                    || !is_transient_sandbox_creation_failure(&message)
                {
                    anyhow::bail!(
                        "create_sandbox returned an error: {:?}",
                        tool_result.content
                    );
                }

                warn!(
                    "Transient create_sandbox error on attempt {attempt}/{}: {}",
                    CREATE_SANDBOX_RETRY_ATTEMPTS, message
                );
            }
            Err(error) => {
                let message = error.to_string();
                if attempt == CREATE_SANDBOX_RETRY_ATTEMPTS
                    || !is_transient_sandbox_creation_failure(&message)
                {
                    return Err(error);
                }

                warn!(
                    "Transient create_sandbox transport error on attempt {attempt}/{}: {}",
                    CREATE_SANDBOX_RETRY_ATTEMPTS, message
                );
            }
        }

        tokio::time::sleep(Duration::from_secs(attempt as u64)).await;
    }

    unreachable!("sandbox creation retry loop returns or errors on the final attempt")
}

/// Calls an MCP tool and retries on transient connection or sandbox-not-ready errors.
pub async fn call_tool_with_retry(
    agent: &MonorailAgent,
    name: &str,
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
) -> anyhow::Result<rmcp::model::CallToolResult> {
    const MAX_RETRIES: usize = 20;
    const RETRY_DELAY: Duration = Duration::from_secs(3);

    for attempt in 1..=MAX_RETRIES {
        let result = agent.call_tool(name, arguments.clone()).await;

        match result {
            Ok(tool_result) if tool_result.is_error != Some(true) => return Ok(tool_result),
            Ok(tool_result) => {
                let message = extract_execution_output(&tool_result)
                    .unwrap_or_else(|_| format!("{:?}", tool_result.content));
                if attempt == MAX_RETRIES || !is_transient_tool_error(&message) {
                    return Ok(tool_result); // Return the tool error if not retryable or exhausted
                }
                eprintln!(
                    "Transient tool error on attempt {attempt}/{}: {}. Retrying in {:?}...",
                    MAX_RETRIES,
                    truncate_str(&message, 100),
                    RETRY_DELAY
                );
            }
            Err(error) => {
                let message = format!("{:?}", error);
                if attempt == MAX_RETRIES || !is_transient_tool_error(&message) {
                    eprintln!(
                        "Non-retryable error or retries exhausted: {}",
                        truncate_str(&message, 200)
                    );
                    return Err(error);
                }
                eprintln!(
                    "Transient transport error on attempt {attempt}/{}: {}. Retrying in {:?}...",
                    MAX_RETRIES,
                    truncate_str(&message, 100),
                    RETRY_DELAY
                );
            }
        }
        tokio::time::sleep(RETRY_DELAY).await;
    }
    unreachable!()
}

fn is_transient_tool_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    // Only retry on actual connection/timing issues
    lower.contains("sandbox is not running")
        || lower.contains("error sending request for url")
        || lower.contains("connection refused")
        || lower.contains("failed to connect")
        || lower.contains("connection reset")
        || lower.contains("broken pipe")
}

/// Waits for a sandbox to be fully ready by verifying container state and,
/// optionally, tool_proxy/browser accessibility.
///
/// The DSB server returns from `create_sandbox` before the Docker container has
/// fully started. This function first waits for the sandbox to reach "Running"
/// state (Phase 1), then — if `wait_for_browser` is true — verifies that the
/// internal `tool_proxy` is accepting connections by attempting a browser
/// screenshot (Phase 2).
///
/// # Arguments
/// * `agent` - The MCP agent used to call tools.
/// * `sandbox_id` - The ID of the sandbox to wait for.
/// * `wait_for_browser` - When `true`, also waits for the browser/tool_proxy
///   to become accessible (Phase 2). Pass `false` for plain images (e.g.
///   `python:3.12`, `ubuntu:22.04`) that do not include a browser.
pub async fn wait_for_sandbox_running(
    agent: &MonorailAgent,
    sandbox_id: &str,
    wait_for_browser: bool,
) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(SANDBOX_READINESS_TIMEOUT_SECS);
    let poll_interval = Duration::from_secs(SANDBOX_READINESS_POLL_INTERVAL_SECS);

    info!(
        "Waiting for sandbox {} to be fully ready (timeout: {}s, wait_for_browser: {})",
        sandbox_id, SANDBOX_READINESS_TIMEOUT_SECS, wait_for_browser
    );

    // Phase 1: Wait for "Running" state via execute_bash (Docker exec path)
    loop {
        match agent
            .call_tool(
                "execute_bash",
                serde_json::json!({
                    "sandbox_id": sandbox_id,
                    "command": "echo ready"
                })
                .as_object()
                .cloned(),
            )
            .await
        {
            Ok(result) if result.is_error != Some(true) => {
                info!(
                    "Sandbox {} reached Running state (Docker exec OK)",
                    sandbox_id
                );
                break;
            }
            _ => {
                if start.elapsed() > timeout {
                    anyhow::bail!(
                        "Sandbox {} failed to reach Running state within {}s",
                        sandbox_id,
                        SANDBOX_READINESS_TIMEOUT_SECS
                    );
                }
                tokio::time::sleep(poll_interval).await;
            }
        }
    }

    if !wait_for_browser {
        let elapsed = start.elapsed();
        info!(
            "Sandbox {} is ready (exec only) after {:.1}s",
            sandbox_id,
            elapsed.as_secs_f64()
        );
        return Ok(());
    }

    // Phase 2: Wait for tool_proxy accessibility (HTTP path)
    // We use automate_browser with a simple "screenshot" action as it's a
    // relatively lightweight tool that requires tool_proxy + browser connection.
    loop {
        match agent
            .call_tool(
                "automate_browser",
                serde_json::json!({
                    "sandbox_id": sandbox_id,
                    "action": "screenshot"
                })
                .as_object()
                .cloned(),
            )
            .await
        {
            Ok(result) if result.is_error != Some(true) => {
                let elapsed = start.elapsed();
                info!(
                    "Sandbox {} is fully ready after {:.1}s",
                    sandbox_id,
                    elapsed.as_secs_f64()
                );
                return Ok(());
            }
            Ok(result) => {
                let message = format!("{:?}", result.content);
                // If it's a browser error (like "browser not connected"), it means
                // tool_proxy is UP but still initializing. We keep waiting.
                eprintln!(
                    "Sandbox {} tool_proxy responding but not ready: {} ({:.0}s elapsed)",
                    sandbox_id,
                    truncate_str(&message, 100),
                    start.elapsed().as_secs_f64()
                );
            }
            Err(error) => {
                // Connection error - tool_proxy port not accessible yet.
                eprintln!(
                    "Sandbox {} tool_proxy connection refused: {} ({:.0}s elapsed)",
                    sandbox_id,
                    error,
                    start.elapsed().as_secs_f64()
                );
            }
        }

        if start.elapsed() > timeout {
            anyhow::bail!(
                "Sandbox {} tool_proxy did not become ready within {}s",
                sandbox_id,
                SANDBOX_READINESS_TIMEOUT_SECS
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Truncates a string to a maximum length, appending "..." if truncated.
fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}

/// Returns a unique sandbox name suffix to avoid Docker container name conflicts across runs
pub fn unique_name(base: &str) -> String {
    let rand: u32 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{}-{:08x}", base, rand)
}

/// Extracts the sandbox ID from a create_sandbox tool result.
/// Handles format: "Created sandbox: {uuid} (image: ..., state: ...)"
/// Falls back to JSON parsing if text format is not recognized.
pub fn extract_sandbox_id(result: &rmcp::model::CallToolResult) -> anyhow::Result<String> {
    if result.is_error == Some(true) {
        anyhow::bail!("create_sandbox returned an error: {:?}", result.content);
    }

    for content in &result.content {
        if let rmcp::model::RawContent::Text(text_content) = &content.raw {
            let text = &text_content.text;
            if let Some(rest) = text.strip_prefix("Created sandbox: ") {
                if let Some(id) = rest.split_whitespace().next().filter(|id| !id.is_empty()) {
                    return Ok(id.to_string());
                }
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
                if let Some(id) = json.get("id").and_then(|i| i.as_str()) {
                    return Ok(id.to_string());
                }
            }
        }
    }

    anyhow::bail!(
        "Could not find sandbox ID in result content: {:?}",
        result.content
    )
}

/// Extracts text output from a tool result, returning an error if no text is found.
pub fn extract_output_text(result: &rmcp::model::CallToolResult) -> anyhow::Result<String> {
    let mut output = String::new();
    for content in &result.content {
        if let rmcp::model::RawContent::Text(text_content) = &content.raw {
            output.push_str(&text_content.text);
        }
    }

    if output.is_empty() {
        anyhow::bail!(
            "No text output found in result content: {:?}",
            result.content
        )
    }

    Ok(output)
}

/// Extracts execution output from a tool result, returning error message text if the result is an error.
/// Otherwise delegates to extract_output_text.
pub fn extract_execution_output(result: &rmcp::model::CallToolResult) -> anyhow::Result<String> {
    if result.is_error == Some(true) {
        let mut err_msg = String::new();
        for content in &result.content {
            if let rmcp::model::RawContent::Text(text_content) = &content.raw {
                err_msg.push_str(&text_content.text);
            }
        }
        if err_msg.is_empty() {
            anyhow::bail!("Tool returned an error with no text content");
        }
        return Ok(err_msg);
    }

    extract_output_text(result)
}

/// Standard test images
pub const TEST_IMAGE_UBUNTU: &str = "ubuntu:22.04";
pub const TEST_IMAGE_PYTHON: &str = "python:3.12";

/// Full **dsb/sandbox** image for scenario tests (daemon + browser tooling; matches Helm `config.docker.defaultImage`).
/// Override with `DSB_TEST_SANDBOX_IMAGE` (e.g. `dsb/sandbox-minimal:latest`) only when trimming image size.
pub const DEFAULT_TEST_IMAGE_SANDBOX: &str = "dsb/sandbox:latest";

/// Returns the sandbox image to use for browser-capable E2E tests.
///
/// `make test-agent` can override this with `DSB_TEST_SANDBOX_IMAGE` so the
/// suite exercises the freshly built sandbox image for the current branch
/// instead of a potentially stale local `latest` tag.
pub fn test_image_sandbox() -> String {
    match std::env::var("DSB_TEST_SANDBOX_IMAGE") {
        Ok(image) if !image.trim().is_empty() => image,
        _ => DEFAULT_TEST_IMAGE_SANDBOX.to_string(),
    }
}
