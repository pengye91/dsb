// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
#[tokio::test]
#[ignore = "requires a running DSB server with DSB_MCP_DSB__API_URL configured"]
async fn test_dsb_client_create_sandbox() {
    let mut settings = dsb_mcp_server::settings::Settings::load().unwrap();
    settings.sandbox.default_image = "ghcr.io/dsb/sandbox:k8s-v0.0.5".to_string();

    println!("DSB API URL: {}", settings.dsb.api_url);
    println!("DSB API Key present: {}", settings.dsb.api_key.is_some());
    if let Some(ref key) = settings.dsb.api_key {
        println!("DSB API Key prefix: {}", &key[..10.min(key.len())]);
    }

    let client = dsb_mcp_server::dsb_client::DSBClient::new(settings.clone()).unwrap();
    let result = client
        .create_sandbox_full(dsb_mcp_server::dsb_client::CreateSandboxConfig {
            image: "ghcr.io/dsb/sandbox:k8s-v0.0.5".to_string(),
            name: None,
            environment: Some(std::collections::HashMap::new()),
            port_mappings: None,
            resource_limits: None,
            volumes: None,
            command: None,
            inactivity_timeout_minutes: None,
            pull_policy: None,
        })
        .await;

    match &result {
        Ok(s) => println!("Success: id={}, state={}", s.id, s.state),
        Err(e) => println!("Error: {}", e),
    }

    assert!(result.is_ok(), "Failed to create sandbox: {:?}", result);
}
