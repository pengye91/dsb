// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # DSB Static File Server - Binary Entry Point
//!
//! This is the binary entry point for the DSB static file server.
//! Currently implements only argument parsing and configuration validation.
//!
//! ⚠️ **STATUS**: Architecture backbone only - not yet functional.

use anyhow::Result;
use clap::Parser;
use std::str::FromStr;
use tracing::info;

/// DSB Static File Server
///
/// Serves static files published by DSB sandboxes.
/// Currently in architecture backbone phase - not yet functional.
#[derive(Parser, Debug)]
#[command(name = "static-server")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "DSB Static File Server", long_about = None)]
struct Args {
    /// Port to listen on (default: 8081)
    #[arg(short, long)]
    port: Option<u16>,

    /// Base path for static file storage
    #[arg(long)]
    base_path: Option<String>,

    /// DSB API URL for authentication
    #[arg(long)]
    dsb_api_url: Option<String>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long)]
    log_level: Option<String>,

    /// Dry run - validate configuration and exit
    #[arg(long, default_value = "false")]
    dry_run: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = args.log_level.as_deref().unwrap_or("info");
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::from_str(log_level)?)
        .init();

    info!("🚧 DSB Static File Server (Architecture Backbone)");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    // Load DSB configuration
    let config = match dsb::config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            info!("Note: DSB configuration not found: {}", e);
            info!("This is expected if running standalone without full DSB setup.");
            info!("Using default configuration for validation purposes.");
            dsb::config::Config::default()
        }
    };

    print_config_summary(&config, &args);

    if args.dry_run {
        info!("");
        info!("✅ Dry-run: Configuration validated successfully");
        return Ok(());
    }

    // Print message and exit
    info!("");
    info!("⚠️  Static file server is not yet implemented.");
    info!("");
    info!("This binary is currently an architecture backbone only.");
    info!("See README.md for the development roadmap.");
    info!("");
    info!("When implemented, this server will:");
    info!("  - Serve static files from /var/lib/dsb/static-files/");
    info!("  - Support configurable cache-control headers");
    info!("  - Provide optional authentication via DSB API");
    info!("  - Run independently on port 8081");
    info!("");

    std::process::exit(0);
}

fn print_config_summary(config: &dsb::config::Config, args: &Args) {
    info!("");
    info!("Configuration:");
    info!("  base_path: {}", config.static_server.base_path);
    info!("  cache_control: {}", config.static_server.cache_control);
    info!(
        "  cache_control_by_type: {} entries",
        config.static_server.cache_control_by_type.len()
    );
    info!(
        "  max_file_size_mb: {}",
        config.static_server.max_file_size_mb
    );
    info!(
        "  enable_directory_browsing: {}",
        config.static_server.enable_directory_browsing
    );

    if let Some(port) = args.port {
        info!("");
        info!("Command-line overrides:");
        info!("  port: {}", port);
    }

    if let Some(base_path) = &args.base_path {
        info!("  base_path: {}", base_path);
    }

    if let Some(dsb_url) = &args.dsb_api_url {
        info!("  dsb_api_url: {}", dsb_url);
    }
}

/// Convert string log level to tracing::Level
#[allow(dead_code)]
fn tracing_level_from_str(s: &str) -> Result<tracing::Level, anyhow::Error> {
    match s.to_lowercase().as_str() {
        "trace" => Ok(tracing::Level::TRACE),
        "debug" => Ok(tracing::Level::DEBUG),
        "info" => Ok(tracing::Level::INFO),
        "warn" => Ok(tracing::Level::WARN),
        "error" => Ok(tracing::Level::ERROR),
        _ => Err(anyhow::anyhow!("Invalid log level: {}", s)),
    }
}
