// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let mut settings = dsb_mcp_server::settings::Settings::load().unwrap();
    settings.sandbox.default_image = "ghcr.io/dsb/sandbox:k8s-v0.0.5".to_string();

    println!("API URL: {}", settings.dsb.api_url);
    println!(
        "API Key: {:?}",
        settings.dsb.api_key.as_ref().map(|k| &k[..10])
    );

    let client = Arc::new(dsb_mcp_server::dsb_client::DSBClient::new(settings.clone()).unwrap());
    let session_manager = Arc::new(dsb_mcp_server::session::SessionManager::new());

    println!("Creating sandbox via session manager...");
    let result = session_manager
        .resolve_or_create("test-session", &client, &settings)
        .await;

    match result {
        Ok(id) => println!("Success: {}", id),
        Err(e) => {
            println!("Error (display): {}", e);
            println!("Error (debug): {:?}", e);
            // Print full chain
            let mut cause = e.source();
            while let Some(c) = cause {
                println!("Caused by: {}", c);
                cause = c.source();
            }
        }
    }
}
