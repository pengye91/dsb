// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::runner::CliContext;
use crate::cli::commands::types::ActivitiesCommands;
use crate::cli::commands::types::Commands;

pub(crate) async fn run(
    ctx: &CliContext,
    cmd: Commands,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = &ctx.client;
    let base_url = ctx.base_url.clone();
    let api_key = ctx.api_key.clone();
    match cmd {
        Commands::Activities { action } => match action {
            ActivitiesCommands::List {
                sandbox,
                limit,
                activity_type,
            } => {
                let endpoint = if let Some(sandbox_id) = sandbox {
                    format!("{}/sandboxes/{}/activities", base_url, sandbox_id)
                } else {
                    format!("{}/activities", base_url)
                };

                let mut request = client.get(&endpoint);
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                request = request.query(&[("limit", limit)]);
                if let Some(ref at) = activity_type {
                    request = request.query(&[("activity_type", at)]);
                }

                let response = request.send().await?;

                if response.status().is_success() {
                    let activities: Vec<serde_json::Value> = response.json().await?;
                    if activities.is_empty() {
                        println!("No activities found");
                    } else {
                        println!("Activities ({} most recent):", limit);
                        for act in activities {
                            println!(
                                "  - {} | {} | {}",
                                act["id"], act["activity_type"], act["timestamp"]
                            );
                            if let Some(details) = act.get("details") {
                                if !details.is_null()
                                    && !details.as_object().map(|o| o.is_empty()).unwrap_or(true)
                                {
                                    println!("    Details: {}", details);
                                }
                            }
                        }
                    }
                } else {
                    eprintln!("Failed to list activities: {}", response.status());
                    std::process::exit(1);
                }
            }

            ActivitiesCommands::Show { id } => {
                let mut request = client.get(format!("{}/activities/{}", base_url, id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    let activity: serde_json::Value = response.json().await?;
                    println!("Activity Details:");
                    println!("  ID: {}", activity["id"]);
                    println!("  Sandbox ID: {}", activity["sandbox_id"]);
                    println!("  Type: {}", activity["activity_type"]);
                    println!("  Timestamp: {}", activity["timestamp"]);
                    println!("  Details: {}", activity["details"]);
                    println!("  Sandbox Deleted: {}", activity["sandbox_is_deleted"]);
                } else if response.status() == reqwest::StatusCode::NOT_FOUND {
                    println!("Activity not found: {}", id);
                } else {
                    eprintln!("Failed to get activity: {}", response.status());
                    std::process::exit(1);
                }
            }

            ActivitiesCommands::CleanupAll { dry_run, timeout } => {
                println!(
                    "{} inactive sandboxes (timeout: {} minutes)",
                    if dry_run {
                        "[DRY RUN] Would clean"
                    } else {
                        "Cleaning"
                    },
                    timeout
                );

                let url = format!(
                    "{}/activities/cleanup-all?dry_run={}&timeout={}",
                    base_url, dry_run, timeout
                );

                let mut request = client.post(&url);
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }

                let response = request.send().await?;

                if response.status().is_success() {
                    let result: serde_json::Value = response.json().await?;
                    if let Some(cleaned) = result.get("cleaned").and_then(|v| v.as_u64()) {
                        if let Some(message) = result.get("message") {
                            println!("{}", message);
                        }
                        println!("Sandboxes cleaned: {}", cleaned);
                    } else {
                        println!("{}", result);
                    }
                } else {
                    eprintln!("Failed to cleanup: {}", response.status());
                    std::process::exit(1);
                }
            }
        },

        _ => unreachable!(),
    }
    Ok(())
}
