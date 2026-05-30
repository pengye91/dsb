// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # DSB SSH Gateway Server
//!
//! This server provides SSH access to DSB sandboxes.
//!
//! ## Overview
//!
//! The SSH gateway:
//! 1. Accepts SSH connections on a configured port
//! 2. Authenticates users via public key
//! 3. Authorizes sandbox access via DSB API
//! 4. Creates Docker exec instances with PTY
//! 5. Forwards data bidirectionally between SSH client and container
//!
//! ## Usage
//!
//! ```bash
//! # Start the server with default configuration
//! ssh-gateway
//!
//! # Start the server with custom port
//! ssh-gateway --port 2222
//!
//! # With explicit API URL
//! ssh-gateway --api-url http://localhost:8080
//!
//! # Connect to a sandbox
//! ssh -p 2222 <sandbox-id>@localhost
//! ```
//!
//! ## SSH Host Key Management
//!
//! The SSH gateway automatically generates and uses a persistent Ed25519 host key:
//!
//! - **Default behavior**: Auto-generates persistent key at `~/.dsb/ssh_host_key` on first run
//! - **Key persistence**: The same key is used across all restarts (no more host key warnings!)
//! - **Custom key**: Use `--host-key-path` to specify a custom key file
//! - **Format**: Accepts standard SSH private key files (PEM, OpenSSH formats)
//!
//! ### First Run
//!
//! On the first run, the SSH gateway will automatically generate a persistent host key:
//!
//! ```bash
//! $ ssh-gateway
//! INFO SSH host key not found at: ~/.dsb/ssh_host_key
//! INFO Auto-generating persistent SSH host key...
//! INFO Generated persistent SSH host key: ~/.dsb/ssh_host_key
//! ```
//!
//! ### Connecting to Sandboxes
//!
//! Since the host key is persistent, you only need to accept it once:
//!
//! ```bash
//! # First connection: Accept the host key prompt
//! ssh -p 2222 <sandbox-id>@localhost
//! # The host 'localhost' will be added to your known_hosts
//!
//! # Subsequent connections: No prompt needed!
//! ssh -p 2222 <sandbox-id>@localhost
//! ```
//!
//! ### Using a Custom Host Key
//!
//! If you want to use a specific key file:
//!
//! ```bash
//! # Generate a custom key
//! ssh-keygen -t ed25519 -f /path/to/my_key -N ""
//!
//! # Use it with ssh-gateway
//! ssh-gateway --host-key-path /path/to/my_key
//! ```
//!
//! ## Configuration
//!
//! The SSH gateway uses the same centralized configuration system as the main DSB server.
//! Configuration is loaded from (in priority order):
//! 1. Default values
//! 2. dsb.yaml file
//! 3. .env file
//! 4. Environment variables (DSB_SSH__*)
//! 5. Command-line arguments (highest priority)

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{debug, info};

mod docker;
mod k8s;
mod session;
mod ssh;

use dsb::config::{self, Config};
use ssh::SshServer;

/// DSB SSH Gateway Server
#[derive(Parser, Debug)]
#[command(name = "ssh-gateway")]
#[command(author = "DSB Team")]
#[command(version = "0.1.0")]
#[command(about = "SSH gateway server for DSB sandboxes", long_about = None)]
struct Args {
    /// SSH server port (overrides config file and env vars)
    ///
    /// Default from config: 2222
    /// Environment variable: DSB_SSH__PORT
    #[arg(short, long)]
    port: Option<u16>,

    /// DSB API base URL (overrides config file and env vars)
    ///
    /// Default from config: http://localhost:8080
    /// Environment variable: DSB_SSH__API_URL
    #[arg(long)]
    api_url: Option<String>,

    /// API key for DSB authentication (overrides config file and env vars)
    ///
    /// Environment variable: DSB_SSH__API_KEY or DSB_SERVER__SSH_GATEWAY_API_KEY
    #[arg(long)]
    api_key: Option<String>,

    /// Host key file path (overrides config file and env vars)
    ///
    /// Environment variable: DSB_SSH__HOST_KEY_PATH
    /// If not specified, uses default persistent key at ~/.dsb/ssh_host_key
    /// (auto-generated on first run if it doesn't exist)
    #[arg(long)]
    host_key_path: Option<String>,

    /// Log level (overrides config file and env vars)
    ///
    /// Default from config: info
    /// Environment variable: DSB_LOGGING__LEVEL
    /// Valid values: trace, debug, info, warn, error
    #[arg(long)]
    log_level: Option<String>,

    /// Sandbox backend for exec operations (overrides config)
    ///
    /// Environment variable: DSB_SSH__BACKEND
    /// Valid values: docker, kubernetes
    #[arg(long)]
    backend: Option<String>,

    /// Kubernetes namespace for sandbox pods (overrides config)
    ///
    /// Environment variable: DSB_SSH__KUBERNETES_NAMESPACE
    #[arg(long)]
    kubernetes_namespace: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install rustls crypto provider early — required before any HTTPS/TLS operations.
    // Both reqwest (DSB API calls) and kube (K8s API) use rustls under the hood.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls ring crypto provider");

    // Parse command line arguments
    let cli_args = Args::parse();

    // Load configuration from all sources (defaults, YAML, .env, env vars)
    let config = match config::load() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("❌ Configuration error: {}", e);
            std::process::exit(1);
        }
    };

    // Apply CLI argument overrides (highest priority)
    let config = apply_cli_overrides(config, cli_args);

    // Initialize logging with configuration (uses the same logging system as main DSB)
    dsb::logging::init_logging(&config)
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging: {}", e))?;

    info!(
        "Starting DSB SSH Gateway Server v{}",
        env!("CARGO_PKG_VERSION")
    );
    debug!(
        "SSH configuration: port={}, api_url={}",
        config.ssh.port, config.ssh.api_url
    );
    debug!("Docker configuration: host={:?}", config.docker.host);

    // Create and run the SSH server with full config
    let server = SshServer::new(config).context("Failed to create SSH server")?;

    info!("SSH server initialized successfully");

    // Run the server (blocks indefinitely)
    server.run().await.context("SSH server error")?;

    Ok(())
}

/// Applies CLI argument overrides to the configuration
///
/// CLI arguments have the highest priority and override values from
/// config files, environment variables, and defaults.
fn apply_cli_overrides(mut config: Config, cli_args: Args) -> Config {
    let mut overrides = std::collections::HashMap::new();

    // Build CLI override map
    if let Some(port) = cli_args.port {
        overrides.insert("ssh.port".to_string(), port.to_string());
        debug!("CLI override: ssh.port = {}", port);
    }

    if let Some(api_url) = cli_args.api_url {
        overrides.insert("ssh.api_url".to_string(), api_url.clone());
        debug!("CLI override: ssh.api_url = {}", api_url);
    }

    if let Some(log_level) = cli_args.log_level {
        overrides.insert("logging.level".to_string(), log_level.clone());
        debug!("CLI override: logging.level = {}", log_level);
    }

    // For api_key and host_key_path, we need special handling since they're Option<String>
    // We'll apply these directly instead of using the override map
    if let Some(api_key) = cli_args.api_key {
        config.ssh.api_key = Some(api_key);
        debug!("CLI override: ssh.api_key = ***");
    }

    if let Some(host_key_path) = cli_args.host_key_path {
        debug!("CLI override: ssh.host_key_path = {}", host_key_path);
        config.ssh.host_key_path = Some(host_key_path);
    }

    if let Some(backend) = cli_args.backend {
        config.ssh.backend = backend;
        debug!("CLI override: ssh.backend = {}", config.ssh.backend);
    }

    if let Some(kubernetes_namespace) = cli_args.kubernetes_namespace {
        config.ssh.kubernetes_namespace = kubernetes_namespace;
        debug!(
            "CLI override: ssh.kubernetes_namespace = {}",
            config.ssh.kubernetes_namespace
        );
    }

    // Apply the numeric/string overrides
    if !overrides.is_empty() {
        match config::load_with_cli_args(Some(overrides)) {
            Ok(merged_config) => {
                // Preserve the Option<String> fields we already set
                config.ssh.api_key = config.ssh.api_key.or(merged_config.ssh.api_key);
                config.ssh.host_key_path =
                    config.ssh.host_key_path.or(merged_config.ssh.host_key_path);
                // Apply the merged values for other fields
                config.ssh.port = merged_config.ssh.port;
                config.ssh.api_url = merged_config.ssh.api_url;
                config.logging.level = merged_config.logging.level;
            }
            Err(e) => {
                eprintln!("Warning: Failed to apply CLI overrides: {}", e);
                // Continue with original config
            }
        }
    }

    config
}
