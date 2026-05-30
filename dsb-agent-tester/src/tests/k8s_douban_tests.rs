// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::agents::monorail::MonorailAgent;
use crate::tests::test_utils::{
    create_sandbox_with_retry, extract_execution_output, extract_sandbox_id, setup_test_tracing,
};
use tracing::info;

#[tokio::test]
async fn test_douban_fetch_k8s() {
    setup_test_tracing();

    // Create MCP agent
    let agent = MonorailAgent::new()
        .await
        .expect("Failed to create Monorail agent");

    info!("Agent created successfully");

    // Create a new sandbox using the agent's MCP tool
    let image = crate::tests::test_utils::test_image_sandbox();
    info!("Creating sandbox with image: {}", image);

    let result = create_sandbox_with_retry(&agent, "k8s-douban-test".to_string(), image)
        .await
        .expect("Failed to create sandbox");

    let sandbox_id = extract_sandbox_id(&result).expect("Failed to extract sandbox ID");
    info!("Created sandbox: {}", sandbox_id);

    // Wait for the sandbox to be fully running
    crate::tests::test_utils::wait_for_sandbox_running(&agent, &sandbox_id)
        .await
        .expect("Sandbox failed to reach running state");

    // Test that the sandbox has web fetch tool
    info!("Starting web fetch task using MCP scrape_web tool...");

    let result = agent
        .call_tool(
            "scrape_web",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "url": "https://movie.douban.com/top250",
                "format": "markdown"
            })
            .as_object()
            .cloned(),
        )
        .await
        .expect("Failed to scrape web");

    let output = extract_execution_output(&result).unwrap_or_default();

    info!("Writing output to douban_top250_k8s.html");

    // Save to the container
    let write_result = agent.call_tool(
        "execute_bash",
        serde_json::json!({
            "sandbox_id": sandbox_id,
            "command": "cat << 'EOF' > douban_top250.md\n" .to_string() + &output.replace("'", "'\\''") + "\nEOF\nls -la douban_top250.md"
        }).as_object().cloned()
    ).await.expect("Failed to write file");

    let check_output = extract_execution_output(&write_result).unwrap_or_default();
    info!("File check result: {}", check_output);
    assert!(
        check_output.contains("douban_top250.md"),
        "File creation verification failed"
    );

    // Download to local for user verification
    let _ = agent
        .call_tool(
            "execute_bash",
            serde_json::json!({
                "sandbox_id": sandbox_id,
                "command": "cat douban_top250.md"
            })
            .as_object()
            .cloned(),
        )
        .await;

    std::fs::write("douban_top250_k8s.md", &output).expect("Failed to save local copy");

    // Cleanup
    info!("Cleaning up sandbox...");
    let _ = agent
        .call_tool(
            "delete_sandbox",
            serde_json::json!({ "id": sandbox_id }).as_object().cloned(),
        )
        .await;
}
