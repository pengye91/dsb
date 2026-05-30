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
        Commands::Upload {
            id,
            file,
            destination,
        } => {
            use reqwest::multipart;

            let file_path = std::path::Path::new(&file);
            if !file_path.exists() {
                eprintln!("File not found: {}", file);
                std::process::exit(1);
            }

            let file_name = file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string());

            let file_bytes = tokio::fs::read(&file).await?;

            let mut form = multipart::Form::new().part(
                "file",
                multipart::Part::bytes(file_bytes).file_name(file_name.clone()),
            );

            if let Some(dest) = &destination {
                form = form.text("destination", dest.clone());
            }

            let mut request = client.post(format!("{}/sandboxes/{}/upload", base_url, id));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }
            let response = request.multipart(form).send().await?;

            if response.status().is_success() {
                let result: serde_json::Value = response.json().await?;
                if output_format == OutputFormat::Json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!("✓ File uploaded: {}", file_name);
                    if let Some(file_info) = result.get("file") {
                        if let Some(path) = file_info.get("path").and_then(|v| v.as_str()) {
                            println!("  Path: {}", path);
                        }
                        if let Some(size) = file_info.get("size").and_then(|v| v.as_u64()) {
                            println!("  Size: {} bytes", size);
                        }
                    }
                }
            } else {
                let status = response.status();
                let error = response.text().await.unwrap_or_default();
                eprintln!("Failed to upload file: {} - {}", status, error);
                std::process::exit(1);
            }
        }

        Commands::Download { id, path, output } => {
            let mut request = client.get(format!("{}/sandboxes/{}/download", base_url, id));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }
            request = request.query(&[("path", &path)]);

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
                let status = response.status();
                let error = response.text().await.unwrap_or_default();
                eprintln!("Failed to download file: {} - {}", status, error);
                std::process::exit(1);
            }
        }

        _ => unreachable!(),
    }
    Ok(())
}
