// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::runner::CliContext;
use crate::cli::commands::types::SessionTokenCommands;
use crate::cli::commands::types::{Commands, OutputFormat};

pub(crate) async fn run(
    ctx: &CliContext,
    cmd: Commands,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = &ctx.client;
    let base_url = ctx.base_url.clone();
    let api_key = ctx.api_key.clone();
    let output_format = ctx.output_format;
    match cmd {
        Commands::SessionTokens { action } => match action {
            SessionTokenCommands::Create {
                sandbox_id,
                service,
                ttl_secs,
            } => {
                let mut body = serde_json::json!({
                    "sandbox_id": sandbox_id,
                    "service": service,
                });
                if let Some(ttl) = ttl_secs {
                    body["ttl_secs"] = serde_json::Value::Number(ttl.into());
                }

                let mut request = client.post(format!("{}/session-tokens", base_url));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.json(&body).send().await?;

                if response.status().is_success()
                    || response.status() == reqwest::StatusCode::CREATED
                {
                    let result: serde_json::Value = response.json().await?;
                    if output_format == OutputFormat::Json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!("✓ Session token created");
                        if let Some(token) = result.get("token").and_then(|v| v.as_str()) {
                            println!("  Token:      {}", token);
                        }
                        if let Some(expires) = result.get("expires_at").and_then(|v| v.as_str()) {
                            println!("  Expires At: {}", expires);
                        }
                    }
                } else {
                    let status = response.status();
                    let error = response.text().await.unwrap_or_default();
                    eprintln!("Failed to create session token: {} - {}", status, error);
                    std::process::exit(1);
                }
            }

            SessionTokenCommands::Validate { token } => {
                let request = client.get(format!("{}/session-tokens/{}/validate", base_url, token));
                let response = request.send().await?;

                if response.status().is_success() {
                    let result: serde_json::Value = response.json().await?;
                    if output_format == OutputFormat::Json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        let valid = result
                            .get("valid")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if valid {
                            println!("✓ Token is valid");
                            if let Some(sandbox_id) =
                                result.get("sandbox_id").and_then(|v| v.as_str())
                            {
                                println!("  Sandbox ID: {}", sandbox_id);
                            }
                            if let Some(service) = result.get("service").and_then(|v| v.as_str()) {
                                println!("  Service:    {}", service);
                            }
                        } else {
                            println!("✗ Token is invalid or expired");
                        }
                    }
                } else {
                    eprintln!("Failed to validate token: {}", response.status());
                    std::process::exit(1);
                }
            }
        },

        _ => unreachable!(),
    }
    Ok(())
}
