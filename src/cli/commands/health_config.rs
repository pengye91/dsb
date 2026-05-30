// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::runner::CliContext;
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
        Commands::Health => {
            let mut request = client.get(format!("{}/health", base_url));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }

            match request.send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let health: serde_json::Value = response.json().await?;
                        if output_format == OutputFormat::Json {
                            println!("{}", serde_json::to_string_pretty(&health)?);
                        } else {
                            println!("Server Health:");
                            println!(
                                "  Status:    {}",
                                health["status"].as_str().unwrap_or("unknown")
                            );
                            if let Some(version) = health.get("version").and_then(|v| v.as_str()) {
                                println!("  Version:   {}", version);
                            }
                            if let Some(ts) = health.get("timestamp").and_then(|v| v.as_str()) {
                                println!("  Timestamp: {}", ts);
                            }
                        }
                    } else {
                        eprintln!("Server returned: {}", response.status());
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to connect to server at {}: {}", base_url, e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Config => {
            let mut request = client.get(format!("{}/config", base_url));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }
            let response = request.send().await?;

            if response.status().is_success() {
                let config_val: serde_json::Value = response.json().await?;
                if output_format == OutputFormat::Json {
                    println!("{}", serde_json::to_string_pretty(&config_val)?);
                } else {
                    println!("Server Configuration:");
                    if let Some(img) = config_val
                        .get("default_sandbox_image")
                        .and_then(|v| v.as_str())
                    {
                        println!("  Default Image:          {}", img);
                    }
                    if let Some(timeout) = config_val
                        .get("default_inactivity_timeout")
                        .and_then(|v| v.as_u64())
                    {
                        println!("  Inactivity Timeout:     {} minutes", timeout);
                    }
                    if let Some(auth) = config_val
                        .get("authentication_required")
                        .and_then(|v| v.as_bool())
                    {
                        println!("  Auth Required:          {}", auth);
                    }
                }
            } else {
                eprintln!("Failed to get config: {}", response.status());
                std::process::exit(1);
            }
        }

        _ => unreachable!(),
    }
    Ok(())
}
