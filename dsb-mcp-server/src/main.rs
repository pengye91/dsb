// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use clap::Parser;
use dsb_mcp_server::server::MCPServer;
use dsb_mcp_server::settings::Settings;
use std::env;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "dsb-mcp-server")]
#[command(about = "MCP server for DSB (Distributed Sandboxes)", long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(long)]
    port: Option<u16>,

    /// DSB API URL
    #[arg(long)]
    dsb_api_url: Option<String>,

    /// SearXNG search API URL (also reads from DSB_SEARXNG_API_URL env var)
    #[arg(long)]
    searxng_api_url: Option<String>,

    /// API key for authenticating with the DSB server (also reads from DSB_API_KEY env var)
    #[arg(long)]
    api_key: Option<String>,

    /// Log level
    #[arg(long)]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Load settings from config files and environment variables
    let mut settings = Settings::load()?;

    // Override with CLI arguments if provided
    if let Some(port) = args.port {
        settings.server.port = port;
    }
    if let Some(url) = args.dsb_api_url {
        settings.dsb.api_url = url;
    }
    if let Some(url) = args.searxng_api_url {
        settings.web.searxng_url = url;
    }
    if let Some(api_key) = args.api_key {
        settings.dsb.api_key = Some(api_key);
    }
    if let Some(level) = args.log_level {
        settings.system.log_level = level;
    }

    // Handle legacy env vars for backward compatibility
    if let Ok(api_key) = env::var("DSB_API_KEY") {
        if settings.dsb.api_key.is_none() {
            settings.dsb.api_key = Some(api_key);
        }
    }
    if let Ok(searxng_url) = env::var("DSB_SEARXNG_API_URL") {
        // If it was the default value in settings, override it
        if settings.web.searxng_url == "http://localhost:8888/search" {
            settings.web.searxng_url = searxng_url;
        }
    }

    info!("Starting DSB MCP Server on port {}", settings.server.port);
    info!("Connecting to DSB API at {}", settings.dsb.api_url);
    info!("Connecting to SearXNG API at {}", settings.web.searxng_url);
    if settings.dsb.api_key.is_some() {
        info!("API key authentication enabled");
    }

    let server = MCPServer::new(settings).await?;

    info!("MCP server running, listening for connections...");

    server.run().await?;

    Ok(())
}
