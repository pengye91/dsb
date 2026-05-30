// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use clap::{Parser, Subcommand, ValueEnum};

/// Output format for CLI commands
#[derive(Debug, Clone, Copy, ValueEnum, Default, PartialEq)]
pub enum OutputFormat {
    /// Human-readable table output (default)
    #[default]
    Table,
    /// JSON output for scripting
    Json,
}

/// Output formats supported by `dsb web fetch`
#[derive(Debug, Clone, Copy, ValueEnum, Default, PartialEq, Eq)]
pub enum WebFetchFormat {
    /// Markdown optimized for reading/LLM usage
    #[default]
    Markdown,
    /// Raw HTML
    Html,
    /// Plain text
    Text,
    /// Extracted links
    Links,
}

impl WebFetchFormat {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Html => "html",
            Self::Text => "text",
            Self::Links => "links",
        }
    }
}

/// Search engines supported by the SearXNG-backed web search command.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum WebSearchEngine {
    /// Google search engine
    Google,
    /// DuckDuckGo search engine
    Duckduckgo,
    /// Bing search engine
    Bing,
    /// Baidu search engine
    Baidu,
}

impl WebSearchEngine {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Duckduckgo => "duckduckgo",
            Self::Bing => "bing",
            Self::Baidu => "baidu",
        }
    }
}

pub(crate) const DEFAULT_SEARXNG_API_URL: &str = "http://localhost:8888/search";
pub(crate) const AGENT_BROWSER_TOOLS_PATH: &str = "/opt/tools/agent_browser_tools.py";

/// DSB command-line interface.
#[derive(Parser)]
#[command(name = "dsb")]
#[command(about = "Distributed Sandboxes - A fast sandbox manager", long_about = None)]
pub struct Cli {
    /// DSB server API URL (overrides config and DSB_API_URL env var)
    #[arg(long, global = true, env = "DSB_API_URL")]
    pub api_url: Option<String>,

    /// API key for authentication (overrides config and DSB_API_KEY env var)
    #[arg(long, global = true, env = "DSB_API_KEY")]
    pub api_key: Option<String>,

    /// Admin API key for admin operations (overrides config and DSB_ADMIN_API_KEY env var)
    #[arg(long, global = true, env = "DSB_ADMIN_API_KEY")]
    pub admin_api_key: Option<String>,

    /// SearXNG API URL for web search (overrides DSB_SEARXNG_API_URL env var)
    #[arg(long, global = true, env = "DSB_SEARXNG_API_URL")]
    pub searxng_api_url: Option<String>,

    /// Output format (table or json)
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Table)]
    pub output: OutputFormat,

    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI subcommands.
#[derive(Subcommand)]
#[allow(missing_docs)]
pub enum Commands {
    #[command(
        about = "Create a new sandbox",
        long_about = "Create a new sandbox container from a specified Docker image. Allows configuring ports, volumes, resources, and running commands.",
        after_help = "EXAMPLES:\n  dsb create -i ubuntu\n  dsb create -i python:3 -p 8080:80 -v /local:/remote -e FOO=bar\n  dsb create -i node:18 --command 'npm start'"
    )]
    Create {
        /// Docker image to use
        #[arg(short, long)]
        image: String,

        /// Sandbox name
        #[arg(short, long)]
        name: Option<String>,

        /// Image pull policy: always, missing (default), or never
        #[arg(long, value_parser = crate::cli::commands::parsers::parse_pull_policy)]
        pull: Option<crate::core::types::PullPolicy>,

        /// Port mappings (e.g., 8080:80)
        #[arg(short = 'p', long, value_parser = crate::cli::commands::parsers::parse_port_mapping)]
        ports: Vec<(u16, u16)>,

        /// CPU shares (relative weight, default 1024)
        #[arg(short = 'c', long)]
        cpu_shares: Option<u64>,

        /// Memory limit in MB
        #[arg(short = 'm', long)]
        memory_mb: Option<u64>,

        /// Volume mounts (e.g., /host/path:/container/path or vol_name:/container/path:ro)
        #[arg(short = 'v', long, value_parser = crate::cli::commands::parsers::parse_volume_string)]
        volumes: Vec<crate::core::types::VolumeMount>,

        /// Command to run in the container (e.g., "sleep infinity" or "tail -f /dev/null")
        ///
        /// Can be specified in two ways:
        ///   - Multiple arguments: --command sudo /usr/bin/supervisord -c /etc/supervisor/conf.d/supervisord.conf
        ///   - Single quoted string (shell word splitting applied): --command 'sudo /usr/bin/supervisord -c /etc/supervisor/conf.d/supervisord.conf'
        ///
        /// If not specified, defaults to "tail -f /dev/null" to keep the container running.
        #[arg(long, num_args = 1..)]
        command: Option<Vec<String>>,

        /// Auto-cleanup timeout in minutes (0 to disable)
        #[arg(short = 't', long)]
        timeout: Option<u64>,

        /// Environment variables (e.g., FOO=bar)
        #[arg(short = 'e', long, value_parser = crate::cli::commands::parsers::parse_env_var)]
        env: Vec<(String, String)>,

        /// Feature profiles to enable from image metadata (e.g., vnc,browser)
        #[arg(long, value_delimiter = ',')]
        features: Vec<String>,

        /// Enable all available features from image metadata
        #[arg(long)]
        enable_all_features: bool,
    },

    /// List all sandboxes
    List {
        /// Show activity information for each sandbox
        #[arg(long)]
        activity: bool,

        /// Filter by sandbox state (e.g., running, stopped, error)
        #[arg(long)]
        state: Option<String>,

        /// Filter by image name (partial match)
        #[arg(long)]
        image: Option<String>,

        /// Include deleted sandboxes
        #[arg(long)]
        include_deleted: bool,

        /// Filter by creation date (ISO 8601 format, e.g., 2024-01-01T00:00:00Z)
        #[arg(long)]
        created_after: Option<String>,

        /// Filter by creation date (ISO 8601 format, e.g., 2024-12-31T23:59:59Z)
        #[arg(long)]
        created_before: Option<String>,

        /// Page number for pagination (default: 1)
        #[arg(long)]
        page: Option<u32>,

        /// Items per page (default: 50, max: 200)
        #[arg(long)]
        per_page: Option<u32>,
    },

    /// Get sandbox details
    Info {
        /// Sandbox ID
        id: String,
    },

    #[command(
        about = "Execute a command in a sandbox",
        long_about = "Execute a command in a running sandbox container. Commands with shell operators (&&, ||, |, ;, >, <) are automatically wrapped with 'sh -c'.",
        after_help = "EXAMPLES:\n  dsb exec 123 ls -la\n  dsb exec 123 'mkdir -p /tmp && touch /tmp/test.txt'"
    )]
    Exec {
        /// Sandbox ID
        id: String,

        /// Command to execute (e.g., "ls -la" or "mkdir -p /tmp && cd /tmp && touch test.txt")
        ///
        /// Commands with shell operators (&&, ||, |, ;, >, <, etc.) are automatically
        /// wrapped with 'sh -c' for proper execution.
        #[arg(trailing_var_arg = true)]
        command: Vec<String>,
    },

    #[command(
        about = "SSH into a sandbox (interactive shell)",
        long_about = "Start an interactive shell in a running sandbox. Note: This connects directly via the local Docker daemon, not the API server, so local Docker access is required.",
        after_help = "EXAMPLES:\n  dsb ssh 12345"
    )]
    Ssh {
        /// Sandbox ID
        id: String,
    },

    /// Stop a sandbox
    Stop {
        /// Sandbox ID
        id: String,
    },

    /// Delete a sandbox
    Delete {
        /// Sandbox ID
        id: String,
    },

    /// Restore a deleted sandbox
    Restore {
        /// Sandbox ID
        id: String,
    },

    /// Get sandbox resource statistics
    Stats {
        /// Sandbox ID
        id: String,

        /// Stream stats in real-time (SSE)
        #[arg(short, long)]
        stream: bool,
    },

    /// Force cleanup sandbox and release all resources
    Cleanup {
        /// Sandbox ID
        id: String,
    },

    /// Start the API server
    Server {
        /// Port to listen on
        #[arg(short, long, default_value_t = 8080)]
        port: u16,

        /// Use PostgreSQL for persistent storage (requires DATABASE_URL or DB_* env vars)
        #[arg(long)]
        postgres: bool,

        /// Path to .env file for configuration (default: .env)
        #[arg(long, value_name = "FILE")]
        env_file: Option<String>,

        /// Path to YAML configuration file (default: dsb.yaml)
        #[arg(long, value_name = "FILE")]
        config_file: Option<String>,
    },

    /// Activity tracking commands
    Activities {
        #[command(subcommand)]
        action: ActivitiesCommands,
    },

    /// API key management commands
    ApiKey {
        #[command(subcommand)]
        action: ApiKeyCommands,
    },

    /// Check server health
    Health,

    /// Get server configuration
    Config,

    #[command(
        about = "Upload a file to a sandbox",
        long_about = "Upload a local file to a specified path inside the sandbox.",
        after_help = "EXAMPLES:\n  dsb upload 123 ./script.py\n  dsb upload 123 ./script.py -d /app/script.py"
    )]
    Upload {
        /// Sandbox ID
        id: String,

        /// Local file path to upload
        file: String,

        /// Destination path inside the sandbox (optional)
        #[arg(short, long)]
        destination: Option<String>,
    },

    #[command(
        about = "Download a file from a sandbox",
        long_about = "Download a file from the sandbox to your local machine or stdout.",
        after_help = "EXAMPLES:\n  dsb download 123 -p /app/output.json -o ./local_output.json\n  dsb download 123 -p /app/logs.txt"
    )]
    Download {
        /// Sandbox ID
        id: String,

        /// File path inside the sandbox to download
        #[arg(short, long)]
        path: String,

        /// Local output file path (defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },

    #[command(
        about = "Execute a tool/script in a sandbox",
        long_about = "Execute a pre-defined tool or script inside the sandbox environment.",
        after_help = "EXAMPLES:\n  dsb tools 123 --interpreter python --script /opt/tools/my_tool.py --action run --args '{\"key\": \"value\"}'"
    )]
    Tools {
        /// Sandbox ID
        id: String,

        /// Interpreter to use (e.g., python, node)
        #[arg(long)]
        interpreter: String,

        /// Script path inside the sandbox
        #[arg(long)]
        script: String,

        /// Tool action to execute
        #[arg(long)]
        action: String,

        /// Arguments as JSON string (e.g., '{"url": "https://example.com"}')
        #[arg(long)]
        args: Option<String>,

        /// Timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
    },

    /// Web tooling commands
    Web {
        #[command(subcommand)]
        action: WebCommands,
    },

    /// Docker image management commands
    Images {
        #[command(subcommand)]
        action: ImagesCommands,
    },

    /// Static file management commands
    Static {
        #[command(subcommand)]
        action: StaticCommands,
    },

    /// Session token management commands
    SessionTokens {
        #[command(subcommand)]
        action: SessionTokenCommands,
    },

    /// SSH session management commands
    SshSessions {
        #[command(subcommand)]
        action: SshSessionCommands,
    },
}

/// Activity tracking subcommands.
#[derive(Subcommand)]
pub enum ActivitiesCommands {
    /// List sandbox activities
    List {
        /// Filter by sandbox ID (optional)
        #[arg(short, long)]
        sandbox: Option<String>,

        /// Maximum number of activities to show
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,

        /// Filter by activity type (e.g., create, exec, ssh, stop, delete)
        #[arg(long)]
        activity_type: Option<String>,
    },

    /// Get details of a specific activity
    Show {
        /// Activity ID
        id: String,
    },

    /// Clean up inactive sandboxes
    CleanupAll {
        /// Dry-run mode (show what would be cleaned without actually deleting)
        #[arg(long)]
        dry_run: bool,

        /// Inactivity threshold in minutes
        #[arg(short, long, default_value_t = 30)]
        timeout: u64,
    },
}

/// API key management subcommands.
#[derive(Subcommand)]
pub enum ApiKeyCommands {
    /// List all API keys
    List {
        /// Show full key (WARNING: this exposes sensitive credentials)
        #[arg(long)]
        reveal: bool,
    },

    /// Create a new API key
    Create {
        /// Name for the API key
        #[arg(short, long)]
        name: String,

        /// Description for the API key
        #[arg(short, long)]
        description: Option<String>,

        /// Comma-separated list of scopes (e.g., sandbox:read,sandbox:write)
        #[arg(short, long)]
        scopes: Option<String>,

        /// Expiration time in days
        #[arg(short, long)]
        expires_in_days: Option<u64>,
    },

    /// Get details of a specific API key
    Show {
        /// API Key ID (UUID)
        #[arg(short, long)]
        id: String,
    },

    /// Delete an API key
    Delete {
        /// API Key ID (UUID)
        #[arg(short, long)]
        id: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Rotate an API key (generate new key, invalidate old)
    Rotate {
        /// API Key ID (UUID)
        #[arg(short, long)]
        id: String,
    },
}

/// Docker image management subcommands.
#[derive(Subcommand)]
pub enum ImagesCommands {
    /// List all local Docker images
    List,

    /// Inspect a Docker image
    Inspect {
        /// Image ID or tag
        id: String,
    },

    /// Pull a Docker image from registry
    Pull {
        /// Image name (e.g., python:3.12)
        image: String,

        /// Image tag (default: latest)
        #[arg(short, long)]
        tag: Option<String>,

        /// Stream pull progress in real-time
        #[arg(short, long)]
        stream: bool,
    },

    /// Delete a Docker image
    Delete {
        /// Image ID or tag
        id: String,
    },
}

/// Static file management subcommands.
#[derive(Subcommand)]
pub enum StaticCommands {
    /// List static files for a sandbox
    List {
        /// Sandbox ID
        sandbox_id: String,
    },

    /// Show directory tree for a sandbox
    Tree {
        /// Sandbox ID
        sandbox_id: String,
    },

    /// Get/download a static file
    Get {
        /// Sandbox ID
        sandbox_id: String,

        /// File path
        file_path: String,

        /// Local output file path (defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Delete a specific static file
    Delete {
        /// Sandbox ID
        sandbox_id: String,

        /// File path to delete
        file_path: String,
    },

    /// Delete all static files for a sandbox
    DeleteAll {
        /// Sandbox ID
        sandbox_id: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },

    /// Download all static files as a zip archive
    Download {
        /// Sandbox ID
        sandbox_id: String,

        /// Output file path (default: {sandbox_id}.zip)
        #[arg(short, long)]
        output: Option<String>,
    },
}

/// Session token management subcommands.
#[derive(Subcommand)]
pub enum SessionTokenCommands {
    /// Create a session token
    Create {
        /// Sandbox ID
        #[arg(long)]
        sandbox_id: String,

        /// Service type (e.g., terminal, vnc)
        #[arg(long)]
        service: String,

        /// Token TTL in seconds
        #[arg(long)]
        ttl_secs: Option<u64>,
    },

    /// Validate a session token
    Validate {
        /// Token to validate
        token: String,
    },
}

/// SSH session management subcommands.
#[derive(Subcommand)]
pub enum SshSessionCommands {
    /// List SSH sessions
    List {
        /// Filter by sandbox ID
        #[arg(long)]
        sandbox_id: Option<String>,

        /// Filter by state (connecting, active, disconnected, terminated, error)
        #[arg(long)]
        state: Option<String>,

        /// Maximum number of results
        #[arg(short = 'n', long)]
        limit: Option<u32>,
    },

    /// Get SSH session details
    Show {
        /// SSH session ID
        id: String,
    },

    /// Terminate an SSH session
    Terminate {
        /// SSH session ID
        id: String,

        /// Reason for termination
        #[arg(long)]
        reason: Option<String>,
    },

    /// Get SSH session statistics
    Stats,

    /// Create a new SSH session
    #[command(
        about = "Create a new SSH session",
        long_about = "Create a new SSH session for a sandbox. This registers a public key for SSH access.",
        after_help = "EXAMPLES:\n  dsb ssh-sessions create --sandbox-id 123 --username dev --public-key \"ssh-rsa AAA...\""
    )]
    Create {
        /// Sandbox ID
        #[arg(long)]
        sandbox_id: String,

        /// SSH Username (default: root)
        #[arg(long, default_value = "root")]
        username: String,

        /// SSH Public Key
        #[arg(long)]
        public_key: String,
    },

    /// Update SSH session activity (heartbeat)
    #[command(
        about = "Update SSH session activity",
        long_about = "Send a heartbeat to keep an SSH session active.",
        after_help = "EXAMPLES:\n  dsb ssh-sessions heartbeat 12345"
    )]
    Heartbeat {
        /// SSH session ID
        id: String,
    },
}

/// Web tooling subcommands.
#[derive(Subcommand)]
pub enum WebCommands {
    /// Fetch webpage content inside a sandbox
    Fetch {
        /// Sandbox ID
        sandbox_id: String,

        /// URL to fetch
        url: String,

        /// Content format
        #[arg(long, value_enum, default_value_t = WebFetchFormat::Markdown)]
        format: WebFetchFormat,

        /// Capture a screenshot alongside the fetched content
        #[arg(long)]
        screenshot: bool,

        /// Restrict extraction to a CSS selector
        #[arg(long)]
        css_selector: Option<String>,

        /// Filter out short content blocks
        #[arg(long, default_value_t = 10)]
        word_count_threshold: u32,

        /// Optional BM25 query used to focus extracted content
        #[arg(long)]
        search_query: Option<String>,

        /// Maximum returned content length
        #[arg(long)]
        max_length: Option<usize>,

        /// Keep the browser tab open for later VNC inspection
        #[arg(long)]
        keep_open: bool,

        /// Tool timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
    },

    /// Search the web through the configured SearXNG service
    Search {
        /// Search query
        query: String,

        /// Restrict search to a specific engine
        #[arg(long, value_enum)]
        engine: Option<WebSearchEngine>,

        /// Maximum number of results to show
        #[arg(short = 'n', long, default_value_t = 10)]
        num_results: usize,
    },
}
