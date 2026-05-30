// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use anyhow::Result;
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;
use serde_json::json;

pub async fn run_browser_tools_test(api_url: &str, api_key: &str) -> Result<()> {
    let client = Client::builder().timeout(Duration::from_secs(120)).build()?;
    
    // 1. Create a sandbox
    let res = client.post(&format!("{}/sandboxes", api_url))
        .header("X-API-Key", api_key)
        .json(&json!({
            "sandbox_name": "test-k8s-browser-1",
            "image": "ghcr.io/dsb/sandbox:k8s-v0.1.0"
        }))
        .send().await?;
        
    let status = res.status();
    let text = res.text().await?;
    println!("Create sandbox response ({}): {}", status, text);
    assert!(status.is_success());
    
    let sandbox_id = serde_json::from_str::<serde_json::Value>(&text)?["id"].as_str().unwrap().to_string();
    println!("Sandbox created: {}", sandbox_id);
    
    // Wait for it to be ready
    println!("Waiting for sandbox to be ready...");
    sleep(Duration::from_secs(5)).await;
    
    // 2. Launch browser to dashboard (requires starting VNC/Browser process inside first usually, but the tool does it)
    println!("Running browser tool to navigate to Wikipedia...");
    let req_body = json!({
        "interpreter": "python",
        "script_path": "/opt/tools/browser_tools.py",
        "action": "browser_navigate",
        "args": {
            "url": "https://en.wikipedia.org/wiki/Kubernetes"
        }
    });
    
    let res = client.post(&format!("{}/sandboxes/{}/tools", api_url, sandbox_id))
        .header("X-API-Key", api_key)
        .json(&req_body)
        .send().await?;
        
    let status = res.status();
    let text = res.text().await?;
    println!("Browser navigate response ({}): {}", status, text);
    
    // Test static files server
    println!("Testing static file upload...");
    let req_body = json!({
        "interpreter": "bash",
        "script_path": "",
        "action": "upload",
        "args": {
            "source_path": "/home/dsb/test_file.txt",
            "content": "Hello from K8s backend!"
        }
    });
    
    let _ = client.post(&format!("{}/sandboxes/{}/tools", api_url, sandbox_id))
        .header("X-API-Key", api_key)
        .json(&req_body)
        .send().await?;

    println!("All k8s browser tests completed");
    Ok(())
}
