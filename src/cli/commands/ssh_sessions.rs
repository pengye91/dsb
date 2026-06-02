// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::runner::CliContext;
use crate::cli::commands::types::SshSessionCommands;
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
        Commands::SshSessions { action } => match action {
            SshSessionCommands::Create {
                sandbox_id,
                username,
                public_key,
            } => {
                let body = serde_json::json!({
                    "sandbox_id": sandbox_id,
                    "username": username,
                    "public_key": public_key,
                });

                let mut request = client.post(format!("{}/ssh-sessions", base_url));
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
                        println!("✓ SSH session created");
                        if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
                            println!("  Session ID: {}", id);
                        }
                    }
                } else {
                    let status = response.status();
                    let error = response.text().await.unwrap_or_default();
                    eprintln!("Failed to create SSH session: {} - {}", status, error);
                    std::process::exit(1);
                }
            }
            SshSessionCommands::Heartbeat { id } => {
                let mut request =
                    client.post(format!("{}/ssh-sessions/{}/heartbeat", base_url, id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    if output_format == OutputFormat::Json {
                        println!(r#"{{"status": "success"}}"#);
                    } else {
                        println!("✓ Heartbeat sent successfully");
                    }
                } else {
                    let status = response.status();
                    let error = response.text().await.unwrap_or_default();
                    eprintln!("Failed to send heartbeat: {} - {}", status, error);
                    std::process::exit(1);
                }
            }
            SshSessionCommands::List {
                sandbox_id,
                state,
                limit,
            } => {
                let mut request = client.get(format!("{}/ssh-sessions", base_url));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                if let Some(sid) = &sandbox_id {
                    request = request.query(&[("sandbox_id", sid.as_str())]);
                }
                if let Some(s) = &state {
                    request = request.query(&[("state", s.as_str())]);
                }
                if let Some(l) = limit {
                    request = request.query(&[("limit", l.to_string().as_str())]);
                }

                let response = request.send().await?;

                if response.status().is_success() {
                    let sessions: serde_json::Value = response.json().await?;
                    if output_format == OutputFormat::Json {
                        println!("{}", serde_json::to_string_pretty(&sessions)?);
                    } else {
                        let empty = vec![];
                        let arr = sessions.as_array().unwrap_or(&empty);
                        println!("SSH Sessions ({} total):", arr.len());
                        for s in arr {
                            println!(
                                "  {} | Sandbox: {} | State: {} | User: {}",
                                s["id"].as_str().unwrap_or(""),
                                s["sandbox_id"].as_str().unwrap_or(""),
                                s["state"].as_str().unwrap_or(""),
                                s["username"].as_str().unwrap_or(""),
                            );
                        }
                    }
                } else {
                    eprintln!("Failed to list SSH sessions: {}", response.status());
                    std::process::exit(1);
                }
            }

            SshSessionCommands::Show { id } => {
                let mut request = client.get(format!("{}/ssh-sessions/{}", base_url, id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    let session: serde_json::Value = response.json().await?;
                    if output_format == OutputFormat::Json {
                        println!("{}", serde_json::to_string_pretty(&session)?);
                    } else {
                        println!("SSH Session Details:");
                        println!("  ID:          {}", session["id"].as_str().unwrap_or(""));
                        println!(
                            "  Sandbox ID:  {}",
                            session["sandbox_id"].as_str().unwrap_or("")
                        );
                        println!(
                            "  Username:    {}",
                            session["username"].as_str().unwrap_or("")
                        );
                        println!("  State:       {}", session["state"].as_str().unwrap_or(""));
                        if let Some(connected) =
                            session.get("connected_at").and_then(|v| v.as_str())
                        {
                            println!("  Connected:   {}", connected);
                        }
                        if let Some(activity) =
                            session.get("last_activity_at").and_then(|v| v.as_str())
                        {
                            println!("  Last Active: {}", activity);
                        }
                        if let Some(sent) = session.get("bytes_sent").and_then(|v| v.as_u64()) {
                            println!("  Bytes Sent:  {}", sent);
                        }
                        if let Some(recv) = session.get("bytes_received").and_then(|v| v.as_u64()) {
                            println!("  Bytes Recv:  {}", recv);
                        }
                    }
                } else if response.status() == 404 {
                    eprintln!("SSH session not found: {}", id);
                    std::process::exit(1);
                } else {
                    eprintln!("Failed to get SSH session: {}", response.status());
                    std::process::exit(1);
                }
            }

            SshSessionCommands::Terminate { id, reason } => {
                let body = if let Some(r) = reason {
                    serde_json::json!({ "reason": r })
                } else {
                    serde_json::json!({})
                };

                let mut request =
                    client.post(format!("{}/ssh-sessions/{}/terminate", base_url, id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.json(&body).send().await?;

                if response.status().is_success() {
                    println!("✓ SSH session terminated: {}", id);
                } else if response.status() == 404 {
                    eprintln!("SSH session not found: {}", id);
                    std::process::exit(1);
                } else {
                    eprintln!("Failed to terminate SSH session: {}", response.status());
                    std::process::exit(1);
                }
            }

            SshSessionCommands::Stats => {
                let mut request = client.get(format!("{}/ssh-sessions/statistics", base_url));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    let stats: serde_json::Value = response.json().await?;
                    if output_format == OutputFormat::Json {
                        println!("{}", serde_json::to_string_pretty(&stats)?);
                    } else {
                        println!("SSH Session Statistics:");
                        if let Some(total) = stats.get("total_sessions").and_then(|v| v.as_u64()) {
                            println!("  Total Sessions:   {}", total);
                        }
                        if let Some(active) = stats.get("active_sessions").and_then(|v| v.as_u64())
                        {
                            println!("  Active Sessions:  {}", active);
                        }
                        if let Some(connecting) =
                            stats.get("connecting_sessions").and_then(|v| v.as_u64())
                        {
                            println!("  Connecting:       {}", connecting);
                        }
                        if let Some(terminated) =
                            stats.get("terminated_sessions").and_then(|v| v.as_u64())
                        {
                            println!("  Terminated:       {}", terminated);
                        }
                        if let Some(sent) = stats.get("total_bytes_sent").and_then(|v| v.as_u64()) {
                            println!("  Total Bytes Sent: {}", sent);
                        }
                        if let Some(recv) =
                            stats.get("total_bytes_received").and_then(|v| v.as_u64())
                        {
                            println!("  Total Bytes Recv: {}", recv);
                        }
                    }
                } else {
                    eprintln!(
                        "Failed to get SSH session statistics: {}",
                        response.status()
                    );
                    std::process::exit(1);
                }
            }
        },

        _ => unreachable!(),
    }
    Ok(())
}
