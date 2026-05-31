// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let api_url = "http://localhost:8080";
    let api_key = "YOUR_API_KEY_HERE";

    println!("Testing K8s browser tools against API: {}", api_url);

    // We'll just run a simple bash command via tool proxy to verify it's working
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    // 1. Create a sandbox
    let res = client
        .post(format!("{}/sandboxes", api_url))
        .header("X-API-Key", api_key)
        .json(&serde_json::json!({
            "sandbox_name": "test-k8s-browser-direct",
            "image": "ghcr.io/dsb/sandbox:k8s-v0.1.0"
        }))
        .send()
        .await?;

    let status = res.status();
    let text = res.text().await?;
    println!("Create sandbox response ({}): {}", status, text);

    let sandbox_id = serde_json::from_str::<serde_json::Value>(&text)?["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Response missing sandbox id"))?
        .to_string();
    println!("Sandbox created: {}", sandbox_id);

    println!("Waiting 15 seconds for pod to be ready...");
    tokio::time::sleep(std::time::Duration::from_secs(15)).await;

    // 2. Fetch douban.com
    println!("Running MCP web fetch equivalent via Python on the sandbox...");
    let req_body = serde_json::json!({
        "interpreter": "python",
        "script_path": "/opt/tools/web_tools.py",
        "action": "web_scrape",
        "args": {
            "url": "https://movie.douban.com/top250"
        }
    });

    let res = client
        .post(format!("{}/sandboxes/{}/tools", api_url, sandbox_id))
        .header("X-API-Key", api_key)
        .json(&req_body)
        .send()
        .await?;

    let status = res.status();
    let text = res.text().await?;
    println!("Web fetch status: {}", status);

    // Save output
    if status.is_success() {
        if let Ok(json_res) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(content) = json_res.get("content") {
                let html = format!(
                    "<html><body><h1>Douban Top 250</h1><pre>{}</pre></body></html>",
                    content.as_str().unwrap_or("")
                );
                std::fs::write("douban_top250_k8s_test.html", html)?;
                println!("Saved douban html to douban_top250_k8s_test.html");
            }
        }
    } else {
        println!("Error output: {}", text);
    }

    Ok(())
}
