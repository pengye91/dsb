// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! DSB Agent Tester - CLI Entry Point
//!
//! This binary provides a CLI for running the DSB agent tester.
//! It manages the DSB stack lifecycle (docker-compose + MCP server).

use anyhow::Context;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .init();

    info!("DSB Agent Tester starting...");

    // Start the DSB stack (docker-compose + MCP server)
    info!("Starting DSB stack...");
    let stack = dsb_agent_tester::DSBStack::start()
        .await
        .context("Failed to start DSB stack")?;

    info!("DSB stack is ready!");

    // Phase 2: MCP client connection and tool testing
    info!("Connecting to MCP server as client...");
    let agent = dsb_agent_tester::agents::MonorailAgent::new()
        .await
        .context("Failed to connect to MCP server")?;

    // List all available tools
    info!("Listing available tools...");
    let tool_names = agent.get_tool_names().await?;
    info!("Found {} tools: {:?}", tool_names.len(), tool_names);

    // Verify all 14 expected tools are present
    info!("Verifying all 14 expected tools are present...");
    agent
        .verify_tools()
        .await
        .context("Tool verification failed")?;
    info!("All 14 tools verified successfully!");

    // Test basic tool invocation: create_sandbox
    info!("Testing create_sandbox tool invocation...");
    let sandbox_args = serde_json::json!({
        "name": "test-sandbox-from-agent",
        "template": "ubuntu"
    });

    let result = agent
        .call_tool("create_sandbox", sandbox_args.as_object().cloned())
        .await
        .context("create_sandbox tool call failed")?;

    info!("create_sandbox result: is_error={:?}", result.is_error);

    // Stop the DSB stack
    info!("Stopping DSB stack...");
    if let Err(e) = stack.stop().await {
        error!("Failed to stop DSB stack cleanly: {}", e);
    }

    info!("DSB Agent Tester finished successfully");
    Ok(())
}
