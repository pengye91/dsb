// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
#[tokio::main]
async fn main() {
    let api_key = "YOUR_API_KEY_HERE";
    let url = "http://localhost:18080/sandboxes";

    println!("Testing with reqwest...");

    let mut headers = reqwest::header::HeaderMap::new();
    let mut val = reqwest::header::HeaderValue::from_str(api_key).unwrap();
    val.set_sensitive(true);
    headers.insert("x-api-key", val);

    // Test 1: default client
    let client = reqwest::Client::builder()
        .default_headers(headers.clone())
        .build()
        .unwrap();

    let body = serde_json::json!({
        "image": "ghcr.io/dsb/sandbox:k8s-v0.0.5",
        "environment": {
            "REDIS_HOST": "127.0.0.1",
            "REDIS_PORT": "6379",
            "REDIS_SSL": "false",
            "REDIS_KEY_PREFIX": "dms:local:dev:"
        }
    });

    println!("Sending POST to {}...", url);
    let start = std::time::Instant::now();
    match client.post(url).json(&body).send().await {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            println!(
                "Success in {:?}: status={}, body_len={}",
                start.elapsed(),
                status,
                text.len()
            );
        }
        Err(e) => {
            println!("Error in {:?}: {}", start.elapsed(), e);
        }
    }
}
