// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Sandbox management tools

use crate::dsb_client::CreateSandboxConfig;
use uuid::Uuid;

/// Create a new sandbox
pub async fn create_sandbox(
    dsb_client: &crate::dsb_client::DSBClient,
    image: String,
    name: Option<String>,
) -> Result<String, String> {
    match dsb_client.create_sandbox(image, name).await {
        Ok(sandbox) => Ok(format!(
            "Created sandbox: {} (image: {}, state: {})",
            sandbox.id, sandbox.config.image, sandbox.state
        )),
        Err(e) => Err(format!("Failed to create sandbox: {}", e)),
    }
}

/// Create a new sandbox with full configuration
pub async fn create_sandbox_with_full_config(
    dsb_client: &crate::dsb_client::DSBClient,
    config: CreateSandboxConfig,
) -> Result<String, String> {
    // Check configuration before moving values
    let has_environment = config.environment.is_some();
    let has_port_mappings = config.port_mappings.is_some();
    let has_resource_limits = config.resource_limits.is_some();
    let has_volumes = config.volumes.is_some();
    let has_command = config.command.is_some();
    let timeout_value = config.inactivity_timeout_minutes;
    let policy_value = config.pull_policy.clone();

    match dsb_client.create_sandbox_full(config).await {
        Ok(sandbox) => {
            let mut details = format!(
                "Created sandbox: {} (image: {}, state: {})",
                sandbox.id, sandbox.config.image, sandbox.state
            );

            // Add configuration details if provided
            if let Some(sandbox_name) = &sandbox.config.name {
                details.push_str(&format!(", name: {}", sandbox_name));
            }

            if has_environment {
                details.push_str(", environment: configured");
            }

            if has_port_mappings {
                details.push_str(", ports: mapped");
            }

            if has_resource_limits {
                details.push_str(", resources: limited");
            }

            if has_volumes {
                details.push_str(", volumes: mounted");
            }

            if has_command {
                details.push_str(", command: set");
            }

            if let Some(timeout) = timeout_value {
                details.push_str(&format!(", timeout: {} minutes", timeout));
            }

            if let Some(policy) = policy_value {
                details.push_str(&format!(", pull_policy: {}", policy));
            }

            Ok(details)
        }
        Err(e) => Err(format!("Failed to create sandbox: {}", e)),
    }
}

/// List all sandboxes
pub async fn list_sandboxes(dsb_client: &crate::dsb_client::DSBClient) -> Result<String, String> {
    match dsb_client.list_sandboxes().await {
        Ok(sandboxes) => {
            if sandboxes.is_empty() {
                Ok("No sandboxes found".to_string())
            } else {
                let mut result = String::from("Sandboxes:\n");
                for sandbox in sandboxes {
                    result.push_str(&format!(
                        "  - {}: {} ({})\n",
                        sandbox.id,
                        sandbox.config.name.as_deref().unwrap_or("unnamed"),
                        sandbox.state
                    ));
                }
                Ok(result)
            }
        }
        Err(e) => Err(format!("Failed to list sandboxes: {}", e)),
    }
}

/// Delete a sandbox
pub async fn delete_sandbox(
    dsb_client: &crate::dsb_client::DSBClient,
    sandbox_id: String,
) -> Result<String, String> {
    let id = Uuid::parse_str(&sandbox_id).map_err(|e| format!("Invalid sandbox ID: {}", e))?;

    match dsb_client.delete_sandbox(id).await {
        Ok(_) => Ok(format!("Deleted sandbox: {}", sandbox_id)),
        Err(e) => Err(format!("Failed to delete sandbox: {}", e)),
    }
}

/// Validate a sandbox name for alphanumeric characters, hyphens, and underscores.
///
/// Returns `Ok(())` if valid, or `Err(message)` with a descriptive error.
pub fn validate_sandbox_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Sandbox name cannot be empty".to_string());
    }
    if name.len() > 64 {
        return Err("Sandbox name too long (max 64 characters)".to_string());
    }
    // Check for invalid characters
    if name
        .chars()
        .any(|c| !c.is_alphanumeric() && c != '-' && c != '_')
    {
        return Err(
            "Sandbox name can only contain alphanumeric characters, hyphens, and underscores"
                .to_string(),
        );
    }
    Ok(())
}

/// Validate a Docker image name for basic format correctness.
///
/// Returns `Ok(())` if valid, or `Err(message)` with a descriptive error.
pub fn validate_image_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Image name cannot be empty".to_string());
    }
    // Basic validation for Docker image names
    if name.contains('@') || name.contains('$') || name.contains('!') {
        return Err("Image name contains invalid characters".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn test_validate_sandbox_name_valid() {
        assert!(validate_sandbox_name("test").is_ok());
        assert!(validate_sandbox_name("test-123").is_ok());
        assert!(validate_sandbox_name("test_sandbox").is_ok());
    }

    #[test]
    fn test_validate_sandbox_name_invalid() {
        assert!(validate_sandbox_name("").is_err());
        assert!(validate_sandbox_name("a".repeat(100).as_str()).is_err());
        assert!(validate_sandbox_name("invalid@name").is_err());
    }

    #[test]
    fn test_validate_image_name_valid() {
        assert!(validate_image_name("python:3.12").is_ok());
        assert!(validate_image_name("nginx").is_ok());
        assert!(validate_image_name("myregistry/myimage:v1").is_ok());
    }

    #[test]
    fn test_validate_image_name_invalid() {
        assert!(validate_image_name("").is_err());
        assert!(validate_image_name("image@test").is_err());
    }

    #[tokio::test]
    async fn test_delete_sandbox_invalid_uuid() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result = delete_sandbox(&client, "not-a-uuid".to_string()).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid sandbox ID"));
    }

    #[tokio::test]
    async fn test_delete_sandbox_valid_uuid_format() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result =
            delete_sandbox(&client, "123e4567-e89b-12d3-a456-426614174000".to_string()).await;

        // Should pass UUID validation (may fail on connection)
        assert!(result.is_ok() || result.unwrap_err().contains("Failed to delete"));
    }
}
