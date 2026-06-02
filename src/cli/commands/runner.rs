// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::types::{Cli, Commands, OutputFormat};
use clap::Parser;
use reqwest::Client;

pub(crate) struct CliContext {
    pub client: Client,
    pub base_url: String,
    pub api_key: Option<String>,
    pub admin_api_key: Option<String>,
    pub searxng_api_url: String,
    pub output_format: OutputFormat,
    pub config: crate::config::Config,
}

/// Main entry point for CLI commands.
///
/// Parses command-line arguments and executes the appropriate command.
pub async fn run_cli() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();
    let client = Client::new();

    let config =
        crate::config::load().map_err(|e| format!("Failed to load configuration: {}", e))?;

    let base_url = cli
        .api_url
        .clone()
        .unwrap_or_else(|| config.ssh.api_url.clone());
    let api_key = cli
        .api_key
        .clone()
        .or_else(|| config.server.api_key.clone())
        .or_else(|| config.server.admin_api_key.clone());
    let admin_api_key = cli
        .admin_api_key
        .clone()
        .or_else(|| config.server.admin_api_key.clone());
    let searxng_api_url = cli
        .searxng_api_url
        .clone()
        .unwrap_or_else(|| crate::cli::commands::types::DEFAULT_SEARXNG_API_URL.to_string());
    let output_format = cli.output;

    let ctx = CliContext {
        client,
        base_url,
        api_key,
        admin_api_key,
        searxng_api_url,
        output_format,
        config,
    };

    let cmd = cli.command;
    match cmd {
        Commands::Create { .. } => super::sandbox::run(&ctx, cmd).await,
        Commands::List { .. } => super::sandbox::run(&ctx, cmd).await,
        Commands::Info { .. } => super::sandbox::run(&ctx, cmd).await,
        Commands::Exec { .. } => super::sandbox::run(&ctx, cmd).await,
        Commands::Ssh { .. } => super::sandbox::run(&ctx, cmd).await,
        Commands::Stop { .. } => super::sandbox::run(&ctx, cmd).await,
        Commands::Delete { .. } => super::sandbox::run(&ctx, cmd).await,
        Commands::Restore { .. } => super::sandbox::run(&ctx, cmd).await,
        Commands::Stats { .. } => super::sandbox::run(&ctx, cmd).await,
        Commands::Cleanup { .. } => super::sandbox::run(&ctx, cmd).await,

        Commands::Activities { .. } => super::activities::run(&ctx, cmd).await,
        Commands::ApiKey { .. } => super::api_keys::run(&ctx, cmd).await,
        Commands::Health => super::health_config::run(&ctx, cmd).await,
        Commands::Config => super::health_config::run(&ctx, cmd).await,
        Commands::Upload { .. } => super::file_transfer::run(&ctx, cmd).await,
        Commands::Download { .. } => super::file_transfer::run(&ctx, cmd).await,
        Commands::Tools { .. } => super::tools::run(&ctx, cmd).await,
        Commands::Web { .. } => super::web::run(&ctx, cmd).await,
        Commands::Images { .. } => super::images::run(&ctx, cmd).await,
        Commands::Static { .. } => super::static_files::run(&ctx, cmd).await,
        Commands::SessionTokens { .. } => super::session_tokens::run(&ctx, cmd).await,
        Commands::SshSessions { .. } => super::ssh_sessions::run(&ctx, cmd).await,
        Commands::Server { .. } => super::server::run(&ctx, cmd).await,
    }
}
