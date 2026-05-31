// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::types::ImagesCommands;
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
        Commands::Images { action } => match action {
            ImagesCommands::List => {
                let mut request = client.get(format!("{}/images", base_url));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    let images: serde_json::Value = response.json().await?;
                    if output_format == OutputFormat::Json {
                        println!("{}", serde_json::to_string_pretty(&images)?);
                    } else {
                        let empty = vec![];
                        let images_arr = images.as_array().unwrap_or(&empty);
                        println!("Images ({} total):", images_arr.len());
                        for img in images_arr {
                            let tags = img
                                .get("repo_tags")
                                .and_then(|t| t.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_else(|| "<none>".to_string());
                            let size_mb =
                                img.get("size").and_then(|s| s.as_i64()).unwrap_or(0) / 1024 / 1024;
                            println!(
                                "  {} | {} MB | {}",
                                tags,
                                size_mb,
                                img["id"]
                                    .as_str()
                                    .unwrap_or("")
                                    .chars()
                                    .take(12)
                                    .collect::<String>()
                            );
                        }
                    }
                } else {
                    eprintln!("Failed to list images: {}", response.status());
                    std::process::exit(1);
                }
            }

            ImagesCommands::Inspect { id } => {
                let mut request = client.get(format!("{}/images/{}", base_url, id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    let image: serde_json::Value = response.json().await?;
                    if output_format == OutputFormat::Json {
                        println!("{}", serde_json::to_string_pretty(&image)?);
                    } else {
                        println!("Image Details:");
                        println!("  ID:           {}", image["id"].as_str().unwrap_or(""));
                        if let Some(tags) = image.get("repo_tags").and_then(|t| t.as_array()) {
                            let tag_strs: Vec<&str> =
                                tags.iter().filter_map(|v| v.as_str()).collect();
                            println!("  Tags:         {}", tag_strs.join(", "));
                        }
                        if let Some(size) = image.get("size").and_then(|s| s.as_i64()) {
                            println!("  Size:         {} MB", size / 1024 / 1024);
                        }
                        if let Some(arch) = image.get("architecture").and_then(|v| v.as_str()) {
                            println!("  Architecture: {}", arch);
                        }
                        if let Some(os) = image.get("os").and_then(|v| v.as_str()) {
                            println!("  OS:           {}", os);
                        }
                        if let Some(features) = image.get("features").and_then(|f| f.as_array()) {
                            if !features.is_empty() {
                                let feat_strs: Vec<&str> =
                                    features.iter().filter_map(|v| v.as_str()).collect();
                                println!("  Features:     {}", feat_strs.join(", "));
                            }
                        }
                    }
                } else if response.status() == 404 {
                    eprintln!("Image not found: {}", id);
                    std::process::exit(1);
                } else {
                    eprintln!("Failed to inspect image: {}", response.status());
                    std::process::exit(1);
                }
            }

            ImagesCommands::Pull { image, tag, stream } => {
                let mut body = serde_json::json!({ "image": image });
                if let Some(t) = &tag {
                    body["tag"] = serde_json::Value::String(t.clone());
                }

                if stream {
                    println!(
                        "Pulling image: {}:{}",
                        image,
                        tag.as_deref().unwrap_or("latest")
                    );

                    let mut request = client.post(format!("{}/images/pull-stream", base_url));
                    if let Some(key) = &api_key {
                        request = request.header("X-API-Key", key);
                    }

                    match request.json(&body).send().await {
                        Ok(mut response) => {
                            if response.status().is_success() {
                                loop {
                                    match response.chunk().await {
                                        Ok(Some(chunk)) => {
                                            let data = String::from_utf8_lossy(&chunk);
                                            for line in data.lines() {
                                                if line.starts_with("data:") {
                                                    let json_str =
                                                        line.trim_start_matches("data:").trim();
                                                    if let Ok(event) =
                                                        serde_json::from_str::<serde_json::Value>(
                                                            json_str,
                                                        )
                                                    {
                                                        let status = event
                                                            .get("status")
                                                            .and_then(|s| s.as_str())
                                                            .unwrap_or("");
                                                        if let Some(progress) = event
                                                            .get("progress")
                                                            .and_then(|p| p.as_str())
                                                        {
                                                            println!("  {} {}", status, progress);
                                                        } else {
                                                            println!("  {}", status);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Ok(None) => break,
                                        Err(e) => {
                                            eprintln!("Stream error: {}", e);
                                            std::process::exit(1);
                                        }
                                    }
                                }
                                println!("✓ Pull complete");
                            } else {
                                let status = response.status();
                                let error = response.text().await.unwrap_or_default();
                                eprintln!("Failed to pull image: {} - {}", status, error);
                                std::process::exit(1);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to connect to server: {}", e);
                            std::process::exit(1);
                        }
                    }
                } else {
                    let mut request = client.post(format!("{}/images/pull", base_url));
                    if let Some(key) = &api_key {
                        request = request.header("X-API-Key", key);
                    }
                    let response = request.json(&body).send().await?;

                    if response.status().is_success()
                        || response.status() == reqwest::StatusCode::ACCEPTED
                    {
                        println!(
                            "✓ Image pull initiated: {}:{}",
                            image,
                            tag.as_deref().unwrap_or("latest")
                        );
                    } else {
                        let status = response.status();
                        let error = response.text().await.unwrap_or_default();
                        eprintln!("Failed to pull image: {} - {}", status, error);
                        std::process::exit(1);
                    }
                }
            }

            ImagesCommands::Delete { id } => {
                let mut request = client.delete(format!("{}/images/{}", base_url, id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success()
                    || response.status() == reqwest::StatusCode::NO_CONTENT
                {
                    println!("✓ Image deleted: {}", id);
                } else {
                    let status = response.status();
                    let error = response.text().await.unwrap_or_default();
                    eprintln!("Failed to delete image: {} - {}", status, error);
                    std::process::exit(1);
                }
            }
        }

        _ => unreachable!(),
    }
    Ok(())
}
