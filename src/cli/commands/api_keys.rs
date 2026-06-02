// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::runner::CliContext;
use crate::cli::commands::types::ApiKeyCommands;
use crate::cli::commands::types::Commands;

pub(crate) async fn run(
    ctx: &CliContext,
    cmd: Commands,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = &ctx.client;
    let base_url = ctx.base_url.clone();
    let admin_api_key = ctx.admin_api_key.clone();
    match cmd {
        Commands::ApiKey { action } => match action {
            ApiKeyCommands::List { reveal } => {
                // API key management commands require admin API key (not regular api_key)
                let admin_key = admin_api_key.as_ref();
                if admin_key.is_none() {
                    eprintln!("❌ Admin API key required");
                    eprintln!();
                    eprintln!("API key management commands require the admin API key.");
                    eprintln!();
                    eprintln!("Set the environment variable:");
                    eprintln!("  export DSB_SERVER__ADMIN_API_KEY=\"your-admin-key\"");
                    eprintln!();
                    eprintln!("Or configure in dsb.yaml:");
                    eprintln!("  server:");
                    eprintln!("    admin_api_key: \"your-admin-key\"");
                    std::process::exit(1);
                }

                let endpoint = format!("{}/admin/api-keys", base_url);
                let mut request = client.get(&endpoint);
                request = request.header("X-API-Key", admin_key.unwrap());

                let response = request.send().await?;

                if response.status().is_success() {
                    let keys: Vec<serde_json::Value> = response.json().await?;
                    if keys.is_empty() {
                        println!("No API keys found");
                    } else {
                        println!("API Keys ({} total):", keys.len());
                        for key in keys {
                            println!();
                            println!("  ID:        {}", key["id"]);
                            println!("  Name:      {}", key["name"]);
                            if let Some(desc) = key.get("description") {
                                if !desc.is_null() {
                                    println!("  Description: {}", desc);
                                }
                            }
                            println!("  Prefix:    {}", key["key_prefix"]);
                            println!("  Active:    {}", key["is_active"]);
                            println!("  Created:   {}", key["created_at"]);
                            if let Some(expires) = key.get("expires_at") {
                                if !expires.is_null() {
                                    println!("  Expires:   {}", expires);
                                }
                            }
                            if let Some(last_used) = key.get("last_used_at") {
                                if !last_used.is_null() {
                                    println!("  Last Used: {}", last_used);
                                } else {
                                    println!("  Last Used: Never");
                                }
                            }
                            if reveal {
                                // Note: The list endpoint doesn't return full keys
                                println!("  WARNING: Full keys are not shown in list. Use 'dsb api-key show <id>' to reveal.");
                            }
                        }
                    }
                } else if response.status() == 401 {
                    eprintln!("❌ Unauthorized: Invalid admin API key");
                    eprintln!("   The DSB_SERVER__ADMIN_API_KEY is set but appears to be invalid");
                    std::process::exit(1);
                } else {
                    eprintln!("❌ Failed to list API keys: {}", response.status());
                    std::process::exit(1);
                }
            }

            ApiKeyCommands::Create {
                name,
                description,
                scopes,
                expires_in_days,
            } => {
                // API key management commands require admin API key (not regular api_key)
                let admin_key = admin_api_key.as_ref();
                if admin_key.is_none() {
                    eprintln!("❌ Admin API key required");
                    eprintln!();
                    eprintln!("API key management commands require the admin API key.");
                    eprintln!();
                    eprintln!("Set the environment variable:");
                    eprintln!("  export DSB_SERVER__ADMIN_API_KEY=\"your-admin-key\"");
                    eprintln!();
                    eprintln!("Or configure in dsb.yaml:");
                    eprintln!("  server:");
                    eprintln!("    admin_api_key: \"your-admin-key\"");
                    std::process::exit(1);
                }

                let endpoint = format!("{}/admin/api-keys", base_url);

                // Build request body
                let mut body = serde_json::json!({
                    "name": name,
                });

                if let Some(desc) = description {
                    body["description"] = serde_json::Value::String(desc);
                }

                if let Some(scopes_str) = scopes {
                    let scopes_list: Vec<&str> = scopes_str.split(',').map(|s| s.trim()).collect();
                    body["scopes"] = serde_json::Value::Array(
                        scopes_list
                            .into_iter()
                            .map(|s| serde_json::Value::String(s.to_string()))
                            .collect(),
                    );
                }

                if let Some(days) = expires_in_days {
                    body["expires_in_days"] = serde_json::Value::Number(days.into());
                }

                let mut request = client.post(&endpoint).json(&body);
                request = request.header("X-API-Key", admin_key.unwrap());

                let response = request.send().await?;

                if response.status().is_success() {
                    let result: serde_json::Value = response.json().await?;
                    if let Some(api_key) = result.get("api_key") {
                        println!("✅ API key created successfully!");
                        println!();
                        println!("⚠️  IMPORTANT: Save this API key now - you won't be able to see it again!");
                        println!();
                        println!("API Key: {}", api_key);
                        println!();
                        if let Some(key_info) = result.get("key") {
                            println!("Key Details:");
                            println!("  ID:        {}", key_info["id"]);
                            println!("  Name:      {}", key_info["name"]);
                            println!("  Prefix:    {}", key_info["key_prefix"]);
                            println!("  Active:    {}", key_info["is_active"]);
                            if let Some(expires) = key_info.get("expires_at") {
                                if !expires.is_null() {
                                    println!("  Expires:   {}", expires);
                                }
                            }
                        }
                        println!();
                        println!("You can now use this key with:");
                        println!("  export DSB_API_KEY=\"{}\"", api_key.as_str().unwrap());
                    }
                } else if response.status() == 401 {
                    eprintln!("❌ Unauthorized: Invalid admin API key");
                    eprintln!("   The DSB_SERVER__ADMIN_API_KEY is set but appears to be invalid");
                    std::process::exit(1);
                } else {
                    let status = response.status();
                    let error: serde_json::Value = response.json().await?;
                    eprintln!("❌ Failed to create API key: {}", status);
                    if let Some(msg) = error.get("message") {
                        eprintln!("   {}", msg);
                    }
                    std::process::exit(1);
                }
            }

            ApiKeyCommands::Show { id } => {
                // API key management commands require admin API key (not regular api_key)
                let admin_key = admin_api_key.as_ref();
                if admin_key.is_none() {
                    eprintln!("❌ Admin API key required");
                    eprintln!();
                    eprintln!("API key management commands require the admin API key.");
                    eprintln!();
                    eprintln!("Set the environment variable:");
                    eprintln!("  export DSB_SERVER__ADMIN_API_KEY=\"your-admin-key\"");
                    eprintln!();
                    eprintln!("Or configure in dsb.yaml:");
                    eprintln!("  server:");
                    eprintln!("    admin_api_key: \"your-admin-key\"");
                    std::process::exit(1);
                }

                let endpoint = format!("{}/admin/api-keys/{}", base_url, id);
                let mut request = client.get(&endpoint);
                request = request.header("X-API-Key", admin_key.unwrap());

                let response = request.send().await?;

                if response.status().is_success() {
                    let key_info: serde_json::Value = response.json().await?;
                    println!("API Key Details:");
                    println!("  ID:        {}", key_info["id"]);
                    println!("  Name:      {}", key_info["name"]);
                    if let Some(desc) = key_info.get("description") {
                        if !desc.is_null() {
                            println!("  Description: {}", desc);
                        }
                    }
                    println!("  Prefix:    {}", key_info["key_prefix"]);
                    println!("  Active:    {}", key_info["is_active"]);
                    println!("  Created:   {}", key_info["created_at"]);
                    if let Some(expires) = key_info.get("expires_at") {
                        if !expires.is_null() {
                            println!("  Expires:   {}", expires);
                        }
                    }
                    if let Some(last_used) = key_info.get("last_used_at") {
                        if !last_used.is_null() {
                            println!("  Last Used: {}", last_used);
                        } else {
                            println!("  Last Used: Never");
                        }
                    }
                    if let Some(scopes) = key_info.get("scopes") {
                        if !scopes.is_null()
                            && !scopes.as_array().map(|a| a.is_empty()).unwrap_or(true)
                        {
                            println!("  Scopes:    {}", scopes);
                        }
                    }
                } else if response.status() == 404 {
                    eprintln!("❌ API key not found: {}", id);
                    std::process::exit(1);
                } else if response.status() == 401 {
                    eprintln!("❌ Unauthorized: Admin API key required");
                    eprintln!("   Set DSB_SERVER__ADMIN_API_KEY or use 'dsb server --help' for configuration");
                    std::process::exit(1);
                } else {
                    eprintln!("❌ Failed to get API key: {}", response.status());
                    std::process::exit(1);
                }
            }

            ApiKeyCommands::Delete { id, force } => {
                // API key management commands require admin API key (not regular api_key)
                let admin_key = admin_api_key.as_ref();
                if admin_key.is_none() {
                    eprintln!("❌ Admin API key required");
                    eprintln!();
                    eprintln!("API key management commands require the admin API key.");
                    eprintln!();
                    eprintln!("Set the environment variable:");
                    eprintln!("  export DSB_SERVER__ADMIN_API_KEY=\"your-admin-key\"");
                    eprintln!();
                    eprintln!("Or configure in dsb.yaml:");
                    eprintln!("  server:");
                    eprintln!("    admin_api_key: \"your-admin-key\"");
                    std::process::exit(1);
                }

                if !force {
                    print!("Are you sure you want to delete API key {}? (y/N): ", id);
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).unwrap();
                    if !input.trim().to_lowercase().starts_with('y') {
                        println!("Cancelled");
                        return Ok(());
                    }
                }

                let endpoint = format!("{}/admin/api-keys/{}", base_url, id);
                let mut request = client.delete(&endpoint);
                request = request.header("X-API-Key", admin_key.unwrap());

                let response = request.send().await?;

                if response.status().is_success() {
                    println!("✅ API key deleted successfully");
                } else if response.status() == 404 {
                    eprintln!("❌ API key not found: {}", id);
                    std::process::exit(1);
                } else if response.status() == 401 {
                    eprintln!("❌ Unauthorized: Admin API key required");
                    eprintln!("   Set DSB_SERVER__ADMIN_API_KEY or use 'dsb server --help' for configuration");
                    std::process::exit(1);
                } else {
                    eprintln!("❌ Failed to delete API key: {}", response.status());
                    std::process::exit(1);
                }
            }

            ApiKeyCommands::Rotate { id } => {
                // API key management commands require admin API key (not regular api_key)
                let admin_key = admin_api_key.as_ref();
                if admin_key.is_none() {
                    eprintln!("❌ Admin API key required");
                    eprintln!();
                    eprintln!("API key management commands require the admin API key.");
                    eprintln!();
                    eprintln!("Set the environment variable:");
                    eprintln!("  export DSB_SERVER__ADMIN_API_KEY=\"your-admin-key\"");
                    eprintln!();
                    eprintln!("Or configure in dsb.yaml:");
                    eprintln!("  server:");
                    eprintln!("    admin_api_key: \"your-admin-key\"");
                    std::process::exit(1);
                }

                let endpoint = format!("{}/admin/api-keys/{}/rotate", base_url, id);
                let mut request = client.post(&endpoint);
                request = request.header("X-API-Key", admin_key.unwrap());

                let response = request.send().await?;

                if response.status().is_success() {
                    let result: serde_json::Value = response.json().await?;
                    if let Some(api_key) = result.get("api_key") {
                        println!("✅ API key rotated successfully!");
                        println!();
                        println!("⚠️  IMPORTANT: Save this new API key now - the old key is no longer valid!");
                        println!();
                        println!("New API Key: {}", api_key);
                        println!();
                        if let Some(key_info) = result.get("key") {
                            println!("Key Details:");
                            println!("  ID:        {}", key_info["id"]);
                            println!("  Name:      {}", key_info["name"]);
                            println!("  Prefix:    {}", key_info["key_prefix"]);
                            println!("  Active:    {}", key_info["is_active"]);
                            if let Some(expires) = key_info.get("expires_at") {
                                if !expires.is_null() {
                                    println!("  Expires:   {}", expires);
                                }
                            }
                        }
                        println!();
                        println!("You can now use this key with:");
                        println!("  export DSB_API_KEY=\"{}\"", api_key.as_str().unwrap());
                    }
                } else if response.status() == 404 {
                    eprintln!("❌ API key not found: {}", id);
                    std::process::exit(1);
                } else if response.status() == 401 {
                    eprintln!("❌ Unauthorized: Admin API key required");
                    eprintln!("   Set DSB_SERVER__ADMIN_API_KEY or use 'dsb server --help' for configuration");
                    std::process::exit(1);
                } else {
                    eprintln!("❌ Failed to rotate API key: {}", response.status());
                    std::process::exit(1);
                }
            }
        },

        _ => unreachable!(),
    }
    Ok(())
}
