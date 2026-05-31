// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Scenario-based E2E tests
//!
//! These tests validate complete workflows through the MCP tools.

use crate::agents::MonorailAgent;
use crate::tests::test_utils::{
    call_tool_with_retry, create_sandbox_with_retry, extract_output_text, extract_sandbox_id,
    test_image_sandbox, unique_name, wait_for_sandbox_running, TEST_IMAGE_PYTHON,
    TEST_IMAGE_UBUNTU,
};
use anyhow::Context;
use tracing::info;

/// Test: Web scraping pipeline (create -> scrape -> delete)
#[tokio::test]
async fn test_web_scraping_pipeline() -> anyhow::Result<()> {
    info!("Testing web scraping pipeline...");
    let agent = MonorailAgent::new().await?;
    let sandbox_image = test_image_sandbox();

    // Create sandbox with a web-capable image
    info!("Creating sandbox...");
    let create_result = create_sandbox_with_retry(&agent, unique_name("web-scrape"), sandbox_image)
        .await
        .context("create_sandbox failed")?;

    let sandbox_id = extract_sandbox_id(&create_result)?;
    info!("Created sandbox: {}", sandbox_id);

    // Wait for sandbox to be fully running before scraping (browser needed)
    wait_for_sandbox_running(&agent, &sandbox_id, true)
        .await
        .context("sandbox readiness wait failed")?;

    // Scrape web page
    info!("Scraping example.com...");
    let scrape_result = call_tool_with_retry(
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

    info!("Scrape result: {:?}", scrape_result.is_error);

    // Delete sandbox
    info!("Deleting sandbox...");
    agent
        .call_tool(
            "delete_sandbox",
            serde_json::json!({
                "sandbox_id": sandbox_id
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("delete_sandbox failed")?;

    info!("Web scraping pipeline completed successfully");
    Ok(())
}

/// Test: Parallel sandbox operations
#[tokio::test]
async fn test_parallel_sandbox_operations() -> anyhow::Result<()> {
    info!("Testing parallel sandbox operations...");
    let agent = MonorailAgent::new().await?;

    // Create 3 sandboxes in parallel
    let sandboxes = tokio::join!(
        create_sandbox_with_retry(
            &agent,
            unique_name("parallel-1"),
            TEST_IMAGE_UBUNTU.to_string()
        ),
        create_sandbox_with_retry(
            &agent,
            unique_name("parallel-2"),
            TEST_IMAGE_UBUNTU.to_string()
        ),
        create_sandbox_with_retry(
            &agent,
            unique_name("parallel-3"),
            TEST_IMAGE_UBUNTU.to_string()
        )
    );

    let ids = [
        extract_sandbox_id(&sandboxes.0?)?,
        extract_sandbox_id(&sandboxes.1?)?,
        extract_sandbox_id(&sandboxes.2?)?,
    ];

    info!("Created {} sandboxes in parallel", ids.len());

    // Delete all in parallel
    let _ = tokio::join!(
        agent.call_tool(
            "delete_sandbox",
            serde_json::json!({"sandbox_id": ids[0]})
                .as_object()
                .cloned()
        ),
        agent.call_tool(
            "delete_sandbox",
            serde_json::json!({"sandbox_id": ids[1]})
                .as_object()
                .cloned()
        ),
        agent.call_tool(
            "delete_sandbox",
            serde_json::json!({"sandbox_id": ids[2]})
                .as_object()
                .cloned()
        )
    );

    info!("Deleted all sandboxes in parallel");
    Ok(())
}

/// Test: Code execution workflow (create -> execute -> delete)
#[tokio::test]
async fn test_code_execution_workflow() -> anyhow::Result<()> {
    info!("Testing code execution workflow...");
    let agent = MonorailAgent::new().await?;

    // Create sandbox with Python image
    info!("Creating sandbox...");
    let create_result = create_sandbox_with_retry(
        &agent,
        unique_name("code-exec"),
        TEST_IMAGE_PYTHON.to_string(),
    )
    .await
    .context("create_sandbox failed")?;

    let sandbox_id = extract_sandbox_id(&create_result)?;
    info!("Created sandbox: {}", sandbox_id);

    // Wait for sandbox exec readiness only (Python image has no browser)
    wait_for_sandbox_running(&agent, &sandbox_id, false)
        .await
        .context("sandbox readiness wait failed")?;

    // Execute Python code
    info!("Executing Python code...");
    let exec_result = call_tool_with_retry(
        &agent,
        "execute_code",
        serde_json::json!({
            "sandbox_id": sandbox_id,
            "code": "import sys; print(f'Python {sys.version}')"
        })
        .as_object()
        .cloned(),
    )
    .await
    .context("execute_code failed")?;

    let output = extract_output_text(&exec_result)?;
    info!("Code execution output: {}", output);

    // Delete sandbox
    info!("Deleting sandbox...");
    agent
        .call_tool(
            "delete_sandbox",
            serde_json::json!({
                "sandbox_id": sandbox_id
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("delete_sandbox failed")?;

    info!("Code execution workflow completed successfully");
    Ok(())
}

/// Test: Health check workflow using bash_execute with curl
#[tokio::test]
async fn test_health_check_workflow() -> anyhow::Result<()> {
    info!("Testing health check workflow via execute_bash...");
    let agent = MonorailAgent::new().await?;
    let sandbox_image = test_image_sandbox();

    // Create sandbox
    info!("Creating sandbox...");
    let create_result =
        create_sandbox_with_retry(&agent, unique_name("health-check"), sandbox_image)
            .await
            .context("create_sandbox failed")?;

    let sandbox_id = extract_sandbox_id(&create_result)?;
    info!("Created sandbox: {}", sandbox_id);

    // Wait for sandbox to be fully running (browser needed for curl)
    wait_for_sandbox_running(&agent, &sandbox_id, true)
        .await
        .context("sandbox readiness wait failed")?;

    // Run health check via execute_bash + curl
    info!("Running health check via execute_bash...");
    let health_result = call_tool_with_retry(
        &agent,
        "execute_bash",
        serde_json::json!({
            "sandbox_id": sandbox_id,
            "command": "curl -sI https://example.com | head -5"
        })
        .as_object()
        .cloned(),
    )
    .await
    .context("execute_bash health check failed")?;

    let output = extract_output_text(&health_result)?;
    info!("Health check output: {}", output);

    // Delete sandbox
    info!("Deleting sandbox...");
    agent
        .call_tool(
            "delete_sandbox",
            serde_json::json!({
                "sandbox_id": sandbox_id
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("delete_sandbox failed")?;

    info!("Health check workflow completed successfully");
    Ok(())
}
