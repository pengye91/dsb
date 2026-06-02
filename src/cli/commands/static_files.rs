// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::runner::CliContext;
use crate::cli::commands::types::StaticCommands;
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
        Commands::Static { action } => match action {
            StaticCommands::List { sandbox_id } => {
                let mut request = client.get(format!("{}/static/files/{}", base_url, sandbox_id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    let result: serde_json::Value = response.json().await?;
                    if output_format == OutputFormat::Json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        let files = result.get("files").and_then(|f| f.as_array());
                        let total = result
                            .get("total_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let total_size = result
                            .get("total_size_bytes")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        println!(
                            "Static Files ({} files, {} bytes total):",
                            total, total_size
                        );
                        if let Some(files) = files {
                            for f in files {
                                println!(
                                    "  {} ({} bytes) [{}]",
                                    f["file_path"].as_str().unwrap_or(""),
                                    f["file_size_bytes"].as_u64().unwrap_or(0),
                                    f["content_type"].as_str().unwrap_or("unknown"),
                                );
                            }
                        }
                    }
                } else {
                    eprintln!("Failed to list static files: {}", response.status());
                    std::process::exit(1);
                }
            }

            StaticCommands::Tree { sandbox_id } => {
                let mut request = client.get(format!("{}/static/tree/{}", base_url, sandbox_id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    let result: serde_json::Value = response.json().await?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    eprintln!("Failed to get directory tree: {}", response.status());
                    std::process::exit(1);
                }
            }

            StaticCommands::Get {
                sandbox_id,
                file_path,
                output,
            } => {
                let mut request =
                    client.get(format!("{}/static/{}/{}", base_url, sandbox_id, file_path));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    let bytes = response.bytes().await?;
                    if let Some(output_path) = output {
                        tokio::fs::write(&output_path, &bytes).await?;
                        println!("✓ Downloaded to: {}", output_path);
                    } else {
                        use tokio::io::AsyncWriteExt;
                        let mut stdout = tokio::io::stdout();
                        stdout.write_all(&bytes).await?;
                    }
                } else {
                    eprintln!("Failed to get static file: {}", response.status());
                    std::process::exit(1);
                }
            }

            StaticCommands::Delete {
                sandbox_id,
                file_path,
            } => {
                let mut request = client.delete(format!(
                    "{}/static/file/{}/{}",
                    base_url, sandbox_id, file_path
                ));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    let result: serde_json::Value = response.json().await?;
                    if output_format == OutputFormat::Json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!("✓ Static file deleted: {}", file_path);
                    }
                } else {
                    eprintln!("Failed to delete static file: {}", response.status());
                    std::process::exit(1);
                }
            }

            StaticCommands::DeleteAll { sandbox_id, force } => {
                if !force {
                    print!(
                        "Delete ALL static files for sandbox {}? (y/N): ",
                        sandbox_id
                    );
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).unwrap();
                    if !input.trim().to_lowercase().starts_with('y') {
                        println!("Cancelled");
                        return Ok(());
                    }
                }

                let mut request =
                    client.delete(format!("{}/static/sandbox/{}", base_url, sandbox_id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    println!("✓ All static files deleted for sandbox: {}", sandbox_id);
                } else {
                    eprintln!("Failed to delete static files: {}", response.status());
                    std::process::exit(1);
                }
            }

            StaticCommands::Download { sandbox_id, output } => {
                let output_path = output.unwrap_or_else(|| format!("{}.zip", sandbox_id));

                let mut request =
                    client.get(format!("{}/static/download/{}", base_url, sandbox_id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    let bytes = response.bytes().await?;
                    tokio::fs::write(&output_path, &bytes).await?;
                    println!("✓ Downloaded to: {} ({} bytes)", output_path, bytes.len());
                } else {
                    eprintln!("Failed to download static files: {}", response.status());
                    std::process::exit(1);
                }
            }
        },

        _ => unreachable!(),
    }
    Ok(())
}
