// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! MonorailAgent MCP client tests
//!
//! These tests verify the MCP client connection and tool discovery capabilities.
//! They require the DSB stack (dsb-server + dsb-mcp-server) to be running.

use crate::agents::MonorailAgent;
use crate::tests::test_utils::{extract_execution_output, extract_sandbox_id, unique_name};
use anyhow::Context;
use tracing::info;

// --- Tests ---

/// Tests that the MCP client can establish a connection to the server.
/// This test requires the DSB stack to be running (dsb-server + dsb-mcp-server).
#[tokio::test]
async fn test_mcp_client_connection() -> anyhow::Result<()> {
    info!("Testing MCP client connection...");

    let agent = MonorailAgent::new().await?;
    assert!(
        agent.is_connected(),
        "Agent should be connected after creation"
    );

    info!("MCP client connection test passed");
    Ok(())
}

/// Tests that the MCP client can list tools from the server.
/// This test requires the DSB stack to be running.
#[tokio::test]
async fn test_list_tools() -> anyhow::Result<()> {
    info!("Testing list_tools...");

    let agent = MonorailAgent::new().await?;
    let tools = agent.list_tools().await?;

    assert!(
        !tools.is_empty(),
        "Expected at least one tool to be available"
    );
    info!("Found {} tools", tools.len());

    // Log tool names for debugging
    for tool in &tools {
        info!("  - {}", tool.name);
    }

    Ok(())
}

/// Tests that all 13 expected tools are present on the server.
/// This test requires the DSB stack to be running.
#[tokio::test]
async fn test_verify_all_8_tools_present() -> anyhow::Result<()> {
    info!("Testing that all 8 expected tools are present...");

    let agent = MonorailAgent::new().await?;
    agent.verify_tools().await?;

    info!("All 8 tools verified successfully");
    Ok(())
}

/// Tests that the client can call the create_sandbox tool.
/// This test requires the DSB stack to be running.
#[tokio::test]
async fn test_create_sandbox_tool_call() -> anyhow::Result<()> {
    info!("Testing create_sandbox tool call...");

    let agent = MonorailAgent::new().await?;

    let mut create_args = serde_json::Map::new();
    create_args.insert(
        "name".to_string(),
        serde_json::Value::String(unique_name("test-sb")),
    );
    create_args.insert(
        "image".to_string(),
        serde_json::Value::String("ubuntu:22.04".to_string()),
    );

    let result = agent
        .call_tool("create_sandbox", Some(create_args))
        .await
        .context("Failed to call create_sandbox")?;

    info!("create_sandbox returned: is_error={:?}", result.is_error);

    // The result structure should be valid even if it's an error
    assert!(
        !result.content.is_empty() || result.structured_content.is_some(),
        "Expected some content in the result"
    );

    Ok(())
}

/// Tests that the client can call list_sandboxes tool.
/// This test requires the DSB stack to be running.
#[tokio::test]
async fn test_list_sandboxes_tool_call() -> anyhow::Result<()> {
    info!("Testing list_sandboxes tool call...");

    let agent = MonorailAgent::new().await?;
    let result = agent
        .call_tool("list_sandboxes", None)
        .await
        .context("Failed to call list_sandboxes")?;

    info!("list_sandboxes returned: is_error={:?}", result.is_error);

    // Verify response structure
    assert!(
        !result.content.is_empty() || result.structured_content.is_some(),
        "Expected some content in the list_sandboxes result"
    );

    Ok(())
}

/// Tests that calling an unknown tool returns an appropriate error.
/// This test requires the DSB stack to be running.
#[tokio::test]
async fn test_unknown_tool_returns_error() -> anyhow::Result<()> {
    info!("Testing unknown tool call...");

    let agent = MonorailAgent::new().await?;

    let result = agent.call_tool("nonexistent_tool", None).await;

    // The call might fail at the RPC level or return an error in the result
    // Both are acceptable behaviors
    match result {
        Ok(call_result) => {
            info!(
                "Unknown tool returned result: is_error={:?}",
                call_result.is_error
            );
            assert_eq!(
                call_result.is_error,
                Some(true),
                "Expected is_error to be true for unknown tool"
            );
        }
        Err(e) => {
            info!("Unknown tool returned error (expected): {}", e);
        }
    }

    Ok(())
}

/// Tests a full sandbox lifecycle: create -> code_execute -> delete
/// This test requires the DSB stack to be running.
#[tokio::test]
async fn test_sandbox_lifecycle() -> anyhow::Result<()> {
    info!("Testing full sandbox lifecycle...");

    let agent = MonorailAgent::new().await?;

    // 1. Create a sandbox
    info!("Creating sandbox...");
    let mut create_args = serde_json::Map::new();
    create_args.insert(
        "name".to_string(),
        serde_json::Value::String(unique_name("lifecycle")),
    );
    create_args.insert(
        "image".to_string(),
        serde_json::Value::String("python:3.12".to_string()),
    );
    let create_result = agent
        .call_tool("create_sandbox", Some(create_args))
        .await
        .context("Failed to call create_sandbox")?;

    let sandbox_id = extract_sandbox_id(&create_result)?;
    info!("Created sandbox with ID: {}", sandbox_id);

    // 2. Execute code in the sandbox
    info!("Executing code in sandbox {}...", sandbox_id);
    let mut exec_args = serde_json::Map::new();
    exec_args.insert(
        "sandbox_id".to_string(),
        serde_json::Value::String(sandbox_id.clone()),
    );
    exec_args.insert(
        "code".to_string(),
        serde_json::Value::String("print('hello from sandbox')".to_string()),
    );
    let exec_result = agent
        .call_tool("execute_code", Some(exec_args))
        .await
        .context("Failed to call execute_code")?;

    let output = extract_execution_output(&exec_result)?;
    info!("Code execution output: {}", output);
    assert!(
        output.contains("hello from sandbox"),
        "Output did not contain expected string"
    );

    // 3. Delete the sandbox
    info!("Deleting sandbox {}...", sandbox_id);
    let mut delete_args = serde_json::Map::new();
    delete_args.insert(
        "sandbox_id".to_string(),
        serde_json::Value::String(sandbox_id),
    );
    let delete_result = agent
        .call_tool("delete_sandbox", Some(delete_args))
        .await
        .context("Failed to call delete_sandbox")?;

    assert_ne!(
        delete_result.is_error,
        Some(true),
        "Failed to delete sandbox"
    );
    info!("Sandbox deleted successfully");

    Ok(())
}

/// Tests error handling when attempting to execute code on a nonexistent sandbox
/// This test requires the DSB stack to be running.
#[tokio::test]
async fn test_error_handling_nonexistent_sandbox() -> anyhow::Result<()> {
    info!("Testing error handling for nonexistent sandbox...");

    let agent = MonorailAgent::new().await?;
    let fake_id = "fake-sandbox-id-12345";

    // Attempt to execute code on the fake sandbox
    let mut exec_args = serde_json::Map::new();
    exec_args.insert(
        "sandbox_id".to_string(),
        serde_json::Value::String(fake_id.to_string()),
    );
    exec_args.insert(
        "code".to_string(),
        serde_json::Value::String("print('this should fail')".to_string()),
    );

    let exec_result = agent.call_tool("execute_code", Some(exec_args)).await;

    // In actual MCP implementation, calling with an invalid sandbox ID might fail the tool call entirely
    // or return a tool result with is_error=true. Handle both cases.
    if let Ok(result) = exec_result {
        info!(
            "Execution result on fake sandbox: is_error={:?}",
            result.is_error
        );
        assert_eq!(
            result.is_error,
            Some(true),
            "Expected is_error=true when executing on nonexistent sandbox"
        );
        // Extract the error message to verify it's correctly formatted
        if let Ok(error_msg) = extract_execution_output(&result) {
            info!("Error message: {}", error_msg);
        }
    } else {
        info!(
            "Execution on fake sandbox failed at RPC level (expected): {:?}",
            exec_result.err()
        );
    }

    // Verify we can still do normal operations (create a sandbox)
    info!("Verifying normal operations still work...");
    let mut create_args = serde_json::Map::new();
    create_args.insert(
        "name".to_string(),
        serde_json::Value::String(unique_name("post-err")),
    );
    create_args.insert(
        "image".to_string(),
        serde_json::Value::String("ubuntu:22.04".to_string()),
    );

    let create_result = agent
        .call_tool("create_sandbox", Some(create_args))
        .await
        .context("Failed to call create_sandbox after error")?;

    assert_ne!(
        create_result.is_error,
        Some(true),
        "Failed to create sandbox after error"
    );

    // Clean up
    if let Ok(sandbox_id) = extract_sandbox_id(&create_result) {
        let mut delete_args = serde_json::Map::new();
        delete_args.insert(
            "sandbox_id".to_string(),
            serde_json::Value::String(sandbox_id),
        );
        let _ = agent.call_tool("delete_sandbox", Some(delete_args)).await;
    }

    Ok(())
}

/// Tests the bash_execute tool: create -> bash_execute -> delete
/// This test requires the DSB stack to be running.
#[tokio::test]
async fn test_bash_execute_tool_call() -> anyhow::Result<()> {
    info!("Testing bash_execute tool call...");

    let agent = MonorailAgent::new().await?;

    // 1. Create a sandbox
    let mut create_args = serde_json::Map::new();
    create_args.insert(
        "name".to_string(),
        serde_json::Value::String(unique_name("bash-test")),
    );
    create_args.insert(
        "image".to_string(),
        serde_json::Value::String("ubuntu:22.04".to_string()),
    );
    let create_result = agent.call_tool("create_sandbox", Some(create_args)).await?;

    let sandbox_id = extract_sandbox_id(&create_result)?;

    // 2. Execute bash command
    let mut exec_args = serde_json::Map::new();
    exec_args.insert(
        "sandbox_id".to_string(),
        serde_json::Value::String(sandbox_id.clone()),
    );
    exec_args.insert(
        "command".to_string(),
        serde_json::Value::String("echo 'hello from bash'".to_string()),
    );

    let exec_result = agent.call_tool("execute_bash", Some(exec_args)).await?;

    assert_ne!(
        exec_result.is_error,
        Some(true),
        "bash_execute returned an error"
    );

    let output = extract_execution_output(&exec_result)?;
    assert!(
        output.contains("hello from bash"),
        "Output did not contain expected string"
    );

    // 3. Delete the sandbox
    let mut delete_args = serde_json::Map::new();
    delete_args.insert(
        "sandbox_id".to_string(),
        serde_json::Value::String(sandbox_id),
    );
    let _ = agent.call_tool("delete_sandbox", Some(delete_args)).await;

    Ok(())
}
