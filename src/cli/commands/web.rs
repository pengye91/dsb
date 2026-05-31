// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::types::WebCommands;
use crate::cli::commands::types::AGENT_BROWSER_TOOLS_PATH;
use crate::cli::commands::parsers::{render_web_fetch_table, render_web_search_results, truncate_search_results};
use crate::cli::commands::runner::CliContext;
use crate::cli::commands::types::{Commands, OutputFormat};

pub(crate) async fn run(
    ctx: &CliContext,
    cmd: Commands,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = &ctx.client;
    let base_url = ctx.base_url.clone();
    let api_key = ctx.api_key.clone();
    let searxng_api_url = ctx.searxng_api_url.clone();
    let output_format = ctx.output_format;
    match cmd {
        Commands::Web { action } => match action {
            WebCommands::Fetch {
                sandbox_id,
                url,
                format,
                screenshot,
                css_selector,
                word_count_threshold,
                search_query,
                max_length,
                keep_open,
                timeout,
            } => {
                let mut body = serde_json::json!({
                    "interpreter": "python",
                    "script_path": AGENT_BROWSER_TOOLS_PATH,
                    "action": "web_scrape",
                    "args": {
                        "url": url,
                        "format": format.as_str(),
                        "screenshot": screenshot,
                        "word_count_threshold": word_count_threshold,
                        "keep_open": keep_open,
                    }
                });

                if let Some(selector) = css_selector {
                    body["args"]["css_selector"] = serde_json::Value::String(selector);
                }
                if let Some(query) = search_query {
                    body["args"]["search_query"] = serde_json::Value::String(query);
                }
                if let Some(length) = max_length {
                    body["args"]["max_length"] =
                        serde_json::Value::Number(serde_json::Number::from(length as u64));
                }
                if let Some(seconds) = timeout {
                    body["timeout"] = serde_json::Value::Number(seconds.into());
                }

                let mut request =
                    client.post(format!("{}/sandboxes/{}/tools", base_url, sandbox_id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.json(&body).send().await?;

                if response.status().is_success() {
                    let result: serde_json::Value = response.json().await?;
                    if output_format == OutputFormat::Json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!("{}", render_web_fetch_table(&result));
                    }
                } else {
                    let status = response.status();
                    let error = response.text().await.unwrap_or_default();
                    eprintln!("Failed to fetch web content: {} - {}", status, error);
                    std::process::exit(1);
                }
            }

            WebCommands::Search {
                query,
                engine,
                num_results,
            } => {
                let mut request = client.get(&searxng_api_url).query(&[
                    ("q", query.as_str()),
                    ("format", "json"),
                    ("categories", "general"),
                ]);

                if let Some(engine) = engine {
                    request = request.query(&[("engines", engine.as_str())]);
                }

                let response = request.send().await?;
                if response.status().is_success() {
                    let mut result: serde_json::Value = response.json().await?;
                    if let Err(error) = truncate_search_results(&mut result, num_results) {
                        eprintln!("Invalid search response: {}", error);
                        std::process::exit(1);
                    }

                    if output_format == OutputFormat::Json {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    } else {
                        println!("{}", render_web_search_results(&result));
                    }
                } else {
                    let status = response.status();
                    let error = response.text().await.unwrap_or_default();
                    eprintln!("Failed to search web: {} - {}", status, error);
                    std::process::exit(1);
                }
            }
        }

        _ => unreachable!(),
    }
    Ok(())
}
