// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::runner::CliContext;
use crate::cli::commands::types::Commands;

pub(crate) async fn run(
    ctx: &CliContext,
    cmd: Commands,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = &ctx.client;
    let base_url = ctx.base_url.clone();
    let api_key = ctx.api_key.clone();
    match cmd {
        Commands::Tools {
            id,
            interpreter,
            script,
            action,
            args,
            timeout,
        } => {
            let mut body = serde_json::json!({
                "interpreter": interpreter,
                "script_path": script,
                "action": action,
            });

            if let Some(args_str) = args {
                match serde_json::from_str::<serde_json::Value>(&args_str) {
                    Ok(args_val) => {
                        body["args"] = args_val;
                    }
                    Err(e) => {
                        eprintln!("Invalid JSON args: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            if let Some(t) = timeout {
                body["timeout"] = serde_json::Value::Number(t.into());
            }

            let mut request = client.post(format!("{}/sandboxes/{}/tools", base_url, id));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }
            let response = request.json(&body).send().await?;

            if response.status().is_success() {
                let result: serde_json::Value = response.json().await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                let status = response.status();
                let error = response.text().await.unwrap_or_default();
                eprintln!("Failed to execute tool: {} - {}", status, error);
                std::process::exit(1);
            }
        }

        _ => unreachable!(),
    }
    Ok(())
}
