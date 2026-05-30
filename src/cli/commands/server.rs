// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::runner::CliContext;
use crate::cli::commands::types::Commands;

pub(crate) async fn run(
    _ctx: &CliContext,
    cmd: Commands,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match cmd {
        Commands::Server {
            port,
            postgres,
            env_file,
            config_file,
        } => {
            // Load configuration with custom file paths if provided
            let config = if env_file.is_some() || config_file.is_some() {
                match crate::config::load_with_files(env_file.as_deref(), config_file.as_deref()) {
                    Ok(config) => config,
                    Err(e) => {
                        eprintln!("❌ Configuration error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                match crate::config::load() {
                    Ok(config) => config,
                    Err(e) => {
                        eprintln!("❌ Configuration error: {}", e);
                        std::process::exit(1);
                    }
                }
            };

            // Override port if specified via CLI (highest priority)
            let config = if port != 8080 {
                // CLI port override takes precedence
                let mut cli_args = std::collections::HashMap::new();
                cli_args.insert("server.port".to_string(), port.to_string());
                match crate::config::load_with_cli_args(Some(cli_args)) {
                    Ok(config) => config,
                    Err(e) => {
                        eprintln!("❌ Configuration error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                config
            };

            // Check for PostgreSQL configuration if --postgres flag is set
            if postgres {
                let has_db_config =
                    config.database.url.is_some() || config.database.password.is_some();
                if !has_db_config {
                    eprintln!(
                        "❌ Error: --postgres flag set but no PostgreSQL configuration found"
                    );
                    eprintln!();
                    eprintln!("To use PostgreSQL storage, set one of the following:");
                    eprintln!("  Option 1: DSB_DATABASE__URL environment variable");
                    eprintln!("    export DSB_DATABASE__URL=\"postgresql://user:pass@localhost:5432/dsb\"");
                    eprintln!();
                    eprintln!("  Option 2: Individual DSB_DATABASE__* variables");
                    eprintln!("    export DSB_DATABASE__HOST=\"localhost\"");
                    eprintln!("    export DSB_DATABASE__PORT=\"5432\"");
                    eprintln!("    export DSB_DATABASE__NAME=\"dsb\"");
                    eprintln!("    export DSB_DATABASE__USER=\"postgres\"");
                    eprintln!("    export DSB_DATABASE__PASSWORD=\"your-password\"");
                    eprintln!();
                    eprintln!("Or configure in dsb.yaml:");
                    eprintln!("  database:");
                    eprintln!("    url: \"postgresql://user:pass@localhost:5432/dsb\"");
                    eprintln!();
                    eprintln!("For more information, see: https://github.com/your-org/dsb#storage-backends");
                    std::process::exit(1);
                }
            }

            // Start API server
            crate::api::start_server(&config).await?;
        }

        _ => unreachable!(),
    }
    Ok(())
}
