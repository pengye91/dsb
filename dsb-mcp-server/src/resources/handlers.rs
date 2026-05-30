// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! MCP resource handlers
//!
//! This module handles incoming MCP resource requests and routes them to the appropriate implementation.

use crate::dsb_client::DSBClient;
use serde_json::json;

/// List available resources
pub async fn handle_resources_list() -> Result<serde_json::Value, String> {
    Ok(json!({
        "resources": [
            {
                "uri": "dsb://server/info",
                "name": "server_info",
                "description": "DSB server information and statistics",
                "mimeType": "application/json"
            },
            {
                "uri": "dsb://sandbox/{id}/status",
                "name": "sandbox_status",
                "description": "Sandbox status and resource usage. Replace {id} with actual sandbox UUID.",
                "mimeType": "application/json"
            }
        ]
    }))
}

/// Read a resource by URI
pub async fn handle_resources_read(
    _client: &DSBClient,
    uri: &str,
) -> Result<serde_json::Value, String> {
    match uri {
        "dsb://server/info" => handle_server_info(_client).await,
        uri if uri.starts_with("dsb://sandbox/") && uri.ends_with("/status") => {
            let sandbox_id = uri
                .strip_prefix("dsb://sandbox/")
                .and_then(|s| s.strip_suffix("/status"))
                .ok_or_else(|| format!("Invalid sandbox status URI: {}", uri))?;
            handle_sandbox_status(_client, sandbox_id).await
        }
        _ => Err(format!("Unknown resource URI: {}", uri)),
    }
}

/// Handle server info resource
async fn handle_server_info(client: &DSBClient) -> Result<serde_json::Value, String> {
    // Call DSB API to get actual server info
    let health = client.get_health().await.map_err(|e| e.to_string())?;
    let sandboxes = client.list_sandboxes().await.map_err(|e| e.to_string())?;

    let running_count = sandboxes.iter().filter(|s| s.state == "running").count();

    Ok(json!({
        "contents": [
            {
                "uri": "dsb://server/info",
                "mimeType": "application/json",
                "text": json!({
                    "status": health.status,
                    "api_url": client.api_url(),
                    "total_sandboxes": sandboxes.len(),
                    "running_sandboxes": running_count,
                    "capabilities": ["web_scraping", "browser_automation", "code_execution"]
                }).to_string()
            }
        ]
    }))
}

/// Handle sandbox status resource
async fn handle_sandbox_status(
    client: &DSBClient,
    sandbox_id: &str,
) -> Result<serde_json::Value, String> {
    // Parse sandbox ID as UUID
    let id = uuid::Uuid::parse_str(sandbox_id)
        .map_err(|_| format!("Invalid sandbox ID: {}", sandbox_id))?;

    // Call DSB API to get actual sandbox status
    let sandbox = client.get_sandbox(id).await.map_err(|e| e.to_string())?;

    Ok(json!({
        "contents": [
            {
                "uri": format!("dsb://sandbox/{}/status", sandbox_id),
                "mimeType": "application/json",
                "text": json!({
                    "id": sandbox.id,
                    "state": sandbox.state,
                    "image": sandbox.config.image,
                    "name": sandbox.config.name,
                    "container_id": sandbox.container_id,
                    "created_at": sandbox.created_at,
                    "updated_at": sandbox.updated_at,
                    "environment": sandbox.config.environment
                }).to_string()
            }
        ]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;

    fn create_test_client() -> DSBClient {
        let config = ServerConfig::default();
        DSBClient::new(config).unwrap()
    }

    #[tokio::test]
    async fn test_resources_list() {
        let result = handle_resources_list().await;
        assert!(result.is_ok());

        let resources = result.unwrap();
        let resources_array = resources
            .get("resources")
            .and_then(|v| v.as_array())
            .expect("resources should be an array");

        assert_eq!(resources_array.len(), 2);
    }

    #[tokio::test]
    async fn test_resources_read_server_info() {
        let client = create_test_client();
        let result = handle_resources_read(&client, "dsb://server/info").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resources_read_unknown_uri() {
        let client = create_test_client();
        let result = handle_resources_read(&client, "dsb://unknown/resource").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown resource URI"));
    }

    #[tokio::test]
    async fn test_resources_read_sandbox_status() {
        let client = create_test_client();
        let result = handle_resources_read(&client, "dsb://sandbox/123-456/status").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resources_read_sandbox_status_invalid_uuid() {
        let client = create_test_client();
        let result = handle_resources_read(&client, "dsb://sandbox/not-a-uuid/status").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid sandbox ID"));
    }
}
