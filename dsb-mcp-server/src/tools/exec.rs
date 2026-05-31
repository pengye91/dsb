// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Command execution tools

/// Execute Python code in a sandbox
pub async fn execute_code(
    dsb_client: &crate::dsb_client::DSBClient,
    sandbox_id: String,
    code: String,
) -> Result<String, String> {
    let id =
        uuid::Uuid::parse_str(&sandbox_id).map_err(|e| format!("Invalid sandbox ID: {}", e))?;

    // Wrap code in python -c
    let command = vec!["python".to_string(), "-c".to_string(), code];

    match dsb_client.exec_command(id, command).await {
        Ok(result) => {
            if result.exit_code == 0 {
                Ok(result.output)
            } else {
                Err(format!(
                    "Code execution failed (exit code {}): {}",
                    result.exit_code, result.output
                ))
            }
        }
        Err(e) => Err(format!("Failed to execute code: {}", e)),
    }
}

/// Execute bash command in a sandbox
pub async fn execute_bash(
    dsb_client: &crate::dsb_client::DSBClient,
    sandbox_id: String,
    command: String,
) -> Result<String, String> {
    let id =
        uuid::Uuid::parse_str(&sandbox_id).map_err(|e| format!("Invalid sandbox ID: {}", e))?;

    // Wrap in bash -c
    let cmd = vec!["bash".to_string(), "-c".to_string(), command];

    match dsb_client.exec_command(id, cmd).await {
        Ok(result) => {
            if result.exit_code == 0 {
                Ok(result.output)
            } else {
                Err(format!(
                    "Command failed (exit code {}): {}",
                    result.exit_code, result.output
                ))
            }
        }
        Err(e) => Err(format!("Failed to execute command: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[tokio::test]
    async fn test_execute_code_valid_uuid() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result = execute_code(
            &client,
            "123e4567-e89b-12d3-a456-426614174000".to_string(),
            "print(42)".to_string(),
        )
        .await;

        // Will fail on connection but UUID parsing should work
        if let Err(e) = result {
            assert!(
                e.contains("connection") || e.contains("error") || e.contains("Failed"),
                "Expected connection/failure error, got: {}",
                e
            );
        }
    }

    #[tokio::test]
    async fn test_execute_code_invalid_uuid() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result =
            execute_code(&client, "invalid-uuid".to_string(), "print(42)".to_string()).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid sandbox ID"));
    }

    #[tokio::test]
    async fn test_execute_bash_valid_uuid() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result = execute_bash(
            &client,
            "123e4567-e89b-12d3-a456-426614174000".to_string(),
            "echo test".to_string(),
        )
        .await;

        // Will fail on connection but UUID parsing should work
        if let Err(e) = result {
            assert!(
                e.contains("connection") || e.contains("error") || e.contains("Failed"),
                "Expected connection/failure error, got: {}",
                e
            );
        }
    }

    #[tokio::test]
    async fn test_execute_bash_invalid_uuid() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result =
            execute_bash(&client, "invalid-uuid".to_string(), "echo test".to_string()).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid sandbox ID"));
    }
}
