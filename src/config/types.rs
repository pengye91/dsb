// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Configuration Type Definitions
//!
//! This module defines all configuration structures used throughout DSB.
//!
//! ## Configuration Hierarchy
//!
//! ```text
//! Config (root)
//! ├── server: ServerConfig
//! ├── database: DatabaseConfig
//! ├── docker: DockerConfig
//! ├── sandbox: SandboxConfig
//! │   ├── backend: BackendType (Docker | Kubernetes)
//! │   └── kubernetes: KubernetesConfig
//! ├── ssh: SshConfig
//! ├── logging: LoggingConfig
//! └── static_server: StaticServerConfig
//! ```

use serde::{Deserialize, Serialize};

/// Root configuration structure for the DSB application.
///
/// This struct is the top-level container for all application settings.
/// It is populated by the configuration loader which merges values from
/// defaults, `.env` files, YAML config files, environment variables, and CLI arguments.
///
/// # Example
///
/// ```yaml
/// server:
///   port: 8080
///   host: "0.0.0.0"
/// database:
///   url: "postgresql://postgres:postgres@localhost:5433/dsb"
/// docker:
///   registry: "docker.io"
///   host: "unix:///var/run/docker.sock"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Config {
    /// HTTP server settings (port, host, API keys, authentication).
    pub server: ServerConfig,

    /// PostgreSQL database connection settings.
    pub database: DatabaseConfig,

    /// Docker daemon and image registry settings.
    pub docker: DockerConfig,

    /// Sandbox backend selection, timeouts, resource limits, and cleanup settings.
    pub sandbox: SandboxConfig,

    /// SSH gateway settings (port, API URL, session timeouts).
    pub ssh: SshConfig,

    /// Logging settings (level, format, file rotation, ANSI colors).
    pub logging: LoggingConfig,

    /// Static file server settings (base path, cache control, ZIP downloads).
    #[serde(default)]
    pub static_server: StaticServerConfig,
}

/// HTTP server configuration.
///
/// Controls the API server binding, authentication requirements, and
/// per-service API keys for web terminal, SSH gateway, and VNC proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Server port (default: 8080)
    pub port: u16,

    /// Server host (default: "0.0.0.0")
    #[serde(default = "default_server_host")]
    pub host: String,

    /// API key for authentication (optional)
    pub api_key: Option<String>,

    /// Web terminal API key (optional, overrides server.api_key)
    pub web_terminal_api_key: Option<String>,

    /// SSH gateway API key (optional, overrides server.api_key)
    pub ssh_gateway_api_key: Option<String>,

    /// VNC proxy API key (optional, overrides server.api_key)
    pub vnc_api_key: Option<String>,

    /// Require VNC authentication via session tokens (default: false)
    ///
    /// When enabled, VNC connections require a short-lived session token
    /// generated via POST /auth/vnc/tokens instead of using static API keys.
    /// This provides better security as tokens:
    /// - Are bound to specific sandboxes
    /// - Have automatic expiration
    /// - Include audit trail
    ///
    /// - false: Use vnc_api_key for authentication (backward compatible)
    /// - true: Require session tokens from /auth/vnc/tokens endpoint
    #[serde(default)]
    pub vnc_require_auth: bool,

    /// VNC token time-to-live in seconds (default: 3600 = 1 hour)
    ///
    /// Only used when vnc_require_auth is true.
    /// Tokens automatically expire after this duration.
    #[serde(default = "default_vnc_token_ttl")]
    pub vnc_token_ttl_secs: u64,

    /// Enable/disable authentication requirement (default: false)
    ///
    /// - false: Allow all requests (development mode)
    /// - true: Require valid API key for all requests except /health
    #[serde(default)]
    pub require_auth: bool,

    /// Admin API key for admin operations (optional)
    ///
    /// This key can access /admin/* endpoints and is used for bootstrapping
    /// the API key management system. If not set, admin endpoints are inaccessible.
    #[serde(default)]
    pub admin_api_key: Option<String>,

    /// Session token time-to-live in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_session_token_ttl")]
    pub session_token_ttl_secs: u64,

    /// Secret key used for signing/encrypting cookies
    ///
    /// If not provided, a random key will be generated on startup,
    /// meaning all sessions will be invalidated when the server restarts.
    #[serde(default)]
    pub cookie_secret: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            host: default_server_host(),
            api_key: None,
            web_terminal_api_key: None,
            ssh_gateway_api_key: None,
            vnc_api_key: None,
            vnc_require_auth: false,
            vnc_token_ttl_secs: default_vnc_token_ttl(),
            require_auth: false,
            admin_api_key: None,
            session_token_ttl_secs: default_session_token_ttl(),
            cookie_secret: None,
        }
    }
}

fn default_server_host() -> String {
    "0.0.0.0".to_string()
}

fn default_vnc_token_ttl() -> u64 {
    3600 // 1 hour
}

fn default_session_token_ttl() -> u64 {
    300 // 5 minutes
}

/// PostgreSQL database connection configuration.
///
/// Supports both a full connection URL and individual connection parameters.
/// When `url` is set, it takes precedence over the individual fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// Full database URL (takes precedence over individual fields)
    pub url: Option<String>,

    /// Database host (default: localhost)
    #[serde(default = "default_db_host")]
    pub host: String,

    /// Database port (default: 5432)
    pub port: u16,

    /// Database name (default: dsb)
    #[serde(default = "default_db_name")]
    pub name: String,

    /// Database user (default: postgres)
    #[serde(default = "default_db_user")]
    pub user: String,

    /// Database password (required)
    pub password: Option<String>,

    /// Maximum pool size (default: 10)
    pub pool_max_size: Option<usize>,
}

impl DatabaseConfig {
    /// Get the database URL, constructing from individual fields if not set
    ///
    /// Returns the URL if explicitly set, otherwise constructs it from
    /// host, port, name, user, and password fields.
    ///
    /// # Returns
    ///
    /// * `Option<String>` - The database URL, or None if password is not configured
    pub fn get_url(&self) -> Option<String> {
        if let Some(ref url) = self.url {
            return Some(url.clone());
        }

        // Construct URL from individual fields
        let password = self.password.as_ref()?;
        Some(format!(
            "postgresql://{}:{}@{}:{}/{}",
            self.user, password, self.host, self.port, self.name
        ))
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: None,
            host: default_db_host(),
            port: 5432,
            name: default_db_name(),
            user: default_db_user(),
            password: None,
            pool_max_size: Some(10),
        }
    }
}

fn default_db_host() -> String {
    "localhost".to_string()
}

fn default_db_name() -> String {
    "dsb".to_string()
}

fn default_db_user() -> String {
    "postgres".to_string()
}

/// Docker daemon and image registry configuration.
///
/// Controls how DSB connects to the Docker daemon, which registry to use,
/// and default/test images for sandbox creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerConfig {
    /// Docker registry for images (default: docker.io)
    #[serde(default = "default_docker_registry")]
    pub registry: String,

    /// Docker daemon connection string
    /// - Auto-detects if not specified
    /// - Respects DOCKER_HOST environment variable
    #[serde(default)]
    pub host: Option<String>,

    /// Default sandbox image
    #[serde(default = "default_sandbox_image")]
    pub default_image: String,

    /// Test image (separate from default)
    #[serde(default = "default_test_image")]
    pub test_image: String,

    /// Docker network for sandbox containers (optional)
    /// When running in Docker, sandboxes should join the same network as DSB
    /// to enable direct container-to-container communication (e.g., VNC proxy)
    #[serde(default)]
    pub network: Option<String>,

    /// Home directory path for tilde expansion (default: $HOME environment variable)
    ///
    /// Used when expanding paths containing `~` to absolute paths.
    /// If not set, defaults to the HOME environment variable.
    #[serde(default)]
    pub home_dir: Option<String>,

    /// HTTP client timeout configuration
    ///
    /// Prevents indefinite hangs when communicating with sandbox containers
    /// and tool_proxy.py HTTP endpoints.
    #[serde(default)]
    pub http_client: HttpClientTimeoutConfig,

    /// Proxy environment variables to inject into sandbox containers
    ///
    /// These are read from the dsb-server's own environment at startup
    /// (populated by docker-compose from the deployment .env file).
    /// When set, they are merged into every sandbox container's environment.
    #[serde(default)]
    pub proxy_env: std::collections::HashMap<String, String>,
}

/// HTTP client timeout configuration for Docker operations
///
/// These timeouts prevent indefinite hangs when the DSB server communicates
/// with sandbox containers, particularly during tool execution and health checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpClientTimeoutConfig {
    /// Connection timeout in seconds (default: 10)
    #[serde(default = "default_http_connect_timeout")]
    pub connect_timeout_secs: u64,

    /// Read timeout in seconds (default: 30)
    /// CRITICAL: Prevents infinite hangs when response body never arrives
    /// This is the most important timeout - without it, `.json().await` can hang forever
    #[serde(default = "default_http_read_timeout")]
    pub read_timeout_secs: u64,

    /// Pool idle timeout in seconds (default: 90)
    #[serde(default = "default_http_pool_idle_timeout")]
    pub pool_idle_timeout_secs: u64,
}

fn default_http_connect_timeout() -> u64 {
    10
}

fn default_http_read_timeout() -> u64 {
    300 // 5 minutes to accommodate long-running operations like web crawling
}

fn default_http_pool_idle_timeout() -> u64 {
    90
}

impl Default for HttpClientTimeoutConfig {
    fn default() -> Self {
        Self {
            connect_timeout_secs: default_http_connect_timeout(),
            read_timeout_secs: default_http_read_timeout(),
            pool_idle_timeout_secs: default_http_pool_idle_timeout(),
        }
    }
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            registry: default_docker_registry(),
            host: None,
            default_image: default_sandbox_image(),
            test_image: default_test_image(),
            network: None,
            home_dir: None,
            http_client: Default::default(),
            proxy_env: std::collections::HashMap::new(),
        }
    }
}

pub(crate) fn default_docker_registry() -> String {
    "docker.io".to_string()
}

fn default_sandbox_image() -> String {
    "docker.io/dsb/sandbox:latest".to_string()
}

fn default_test_image() -> String {
    "python:3.12".to_string()
}

/// Tool execution timeout configuration
///
/// These timeouts control how long different types of tool operations
/// are allowed to run inside sandbox containers before being terminated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolTimeoutConfig {
    /// Default timeout for general tool execution in seconds (default: 60)
    pub default_secs: u64,

    /// Timeout for web scraping operations in seconds (default: 90)
    ///
    /// Web scraping includes: scrape, crawl, extract_css, extract_table, links
    /// Increased from 60 to 90 to accommodate crawl operations
    pub web_tools_secs: u64,

    /// Timeout for browser automation operations in seconds (default: 120)
    ///
    /// Browser operations include: navigate, click, fill, screenshot, etc.
    /// These require longer timeouts due to page load times.
    pub browser_tools_secs: u64,

    /// Timeout for databend database operations in seconds (default: 60)
    ///
    /// SQL query execution, table listing, schema inspection
    pub databend_tools_secs: u64,

    /// HTTP client buffer time in seconds (default: 30)
    ///
    /// Additional time added to HTTP request timeouts to account for
    /// network overhead. Tool timeout + buffer = HTTP timeout
    pub http_buffer_secs: u64,

    /// Maximum allowed timeout for custom operations in seconds (default: 300)
    ///
    /// Upper limit for user-specified timeouts to prevent resource exhaustion
    pub max_allowed_secs: u64,
}

/// Ulimit configuration for process resource limits.
///
/// Ulimits control the resource limits for individual processes within the container.
/// Common ulimits include:
///
/// - **nofile** - Maximum number of open file descriptors
/// - **nproc** - Maximum number of processes/threads
/// - **memlock** - Maximum locked-in-memory address space
///
/// # Example
///
/// ```yaml
/// ulimits:
///   - name: "nofile"
///     soft: 65536
///     hard: 65536
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultUlimit {
    /// Name of the ulimit (e.g., "nofile", "nproc", "memlock")
    pub name: String,

    /// Soft limit (can be increased by the process up to the hard limit)
    pub soft: i64,

    /// Hard limit (absolute maximum that cannot be exceeded)
    pub hard: i64,
}

/// Default resource limits for sandbox creation.
///
/// These limits are applied when creating sandboxes without explicit resource limits.
/// They help prevent resource exhaustion in production environments by setting
/// sensible defaults for memory, CPU, and process limits.
///
/// # Merging Strategy
///
/// When a sandbox is created:
/// 1. Request-level limits take highest priority
/// 2. Config-level defaults are used for any limit not specified in the request
/// 3. If neither is specified, the limit remains unlimited (None)
///
/// # Example Configuration
///
/// ```yaml
/// sandbox:
///   default_resource_limits:
///     memory_mb: 2048        # 2GB default memory limit
///     cpu_quota: 100000      # 100% of one CPU core
///     cpu_period: 100000     # 100ms period
///     pids_limit: 1000       # Max 1000 processes
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct DefaultResourceLimits {
    /// Memory limit in megabytes (default: None/unlimited)
    ///
    /// If set, containers will be limited to this amount of RAM.
    /// Example: 2048 for 2GB, 4096 for 4GB
    pub memory_mb: Option<u64>,

    /// CPU quota in microseconds per period (default: None/unlimited)
    ///
    /// Works together with cpu_period to limit CPU usage.
    /// For example, to limit to 1 CPU core:
    /// - cpu_quota: 100000 (100ms per 100ms period = 100%)
    /// - cpu_period: 100000
    pub cpu_quota: Option<i64>,

    /// CPU period in microseconds (default: None, typically 100000)
    ///
    /// The timeframe for CPU quota enforcement. Usually set to 100000 (100ms).
    pub cpu_period: Option<i64>,

    /// CPU shares (relative weight, default 1024) (default: None)
    ///
    /// Relative CPU weight when containers compete for CPU time.
    /// Higher values get more CPU. Default is 1024.
    pub cpu_shares: Option<u64>,

    /// Maximum number of processes (PIDs) in the container (default: None/unlimited)
    ///
    /// Prevents fork bombs and limits process creation.
    /// Example: 1000 for max 1000 processes
    pub pids_limit: Option<i64>,

    /// Per-process resource limits (ulimits) (default: None)
    ///
    /// Controls resource limits for individual processes.
    /// Common values:
    /// - nofile: max open files (e.g., 65536)
    /// - nproc: max processes per user
    /// - memlock: max locked memory
    pub ulimits: Option<Vec<DefaultUlimit>>,
}

impl Default for ToolTimeoutConfig {
    fn default() -> Self {
        Self {
            default_secs: 60,
            web_tools_secs: 90, // Increased from 60 to 90 for crawl operations
            browser_tools_secs: 120,
            databend_tools_secs: 60,
            http_buffer_secs: 30,
            max_allowed_secs: 300,
        }
    }
}

/// Sandbox backend type selection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    /// Docker backend (default, uses bollard to talk to Docker daemon)
    #[default]
    Docker,
    /// Kubernetes backend (uses kube-rs to manage pods)
    Kubernetes,
}

/// Kubernetes backend configuration
///
/// Only used when `sandbox.backend` is set to `kubernetes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KubernetesConfig {
    /// K8s namespace for sandbox pods (default: "dsb-sandboxes")
    #[serde(default = "default_k8s_namespace")]
    pub namespace: String,

    /// Image pull secrets for private registries
    #[serde(default)]
    pub image_pull_secrets: Vec<String>,

    /// Tolerations for sandbox pods
    /// Each entry is a JSON object with key, operator, value, effect fields
    #[serde(default)]
    pub tolerations: Vec<serde_json::Value>,

    /// Node selector for sandbox pods
    #[serde(default)]
    pub node_selector: std::collections::HashMap<String, String>,

    /// Default resource requests and limits for sandbox pods
    #[serde(default)]
    pub resource_defaults: KubernetesResourceDefaults,

    /// Storage class for PVC (empty = cluster default)
    #[serde(default)]
    pub pvc_storage_class: String,

    /// PVC access mode (default: ReadWriteMany)
    #[serde(default = "default_pvc_access_mode")]
    pub pvc_access_mode: String,

    /// Name of the shared PVC for static files (default: "dsb-static-files-shared")
    #[serde(default = "default_pvc_name")]
    pub pvc_name: String,

    /// Timeout in seconds for pod to become ready (default: 120)
    #[serde(default = "default_pod_ready_timeout")]
    pub pod_ready_timeout_secs: u64,

    /// GPU configuration for sandbox pods
    #[serde(default)]
    pub gpu: GpuConfig,
}

impl Default for KubernetesConfig {
    fn default() -> Self {
        Self {
            namespace: default_k8s_namespace(),
            image_pull_secrets: Vec::new(),
            tolerations: Vec::new(),
            node_selector: std::collections::HashMap::new(),
            resource_defaults: KubernetesResourceDefaults::default(),
            pvc_storage_class: String::new(),
            pvc_access_mode: default_pvc_access_mode(),
            pvc_name: default_pvc_name(),
            pod_ready_timeout_secs: default_pod_ready_timeout(),
            gpu: GpuConfig::default(),
        }
    }
}

fn default_k8s_namespace() -> String {
    "dsb-sandboxes".to_string()
}

fn default_pvc_access_mode() -> String {
    "ReadWriteMany".to_string()
}

fn default_pvc_name() -> String {
    "dsb-static-files-shared".to_string()
}

fn default_pod_ready_timeout() -> u64 {
    120
}

/// Default resource configuration for K8s sandbox pods
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KubernetesResourceDefaults {
    /// CPU request (e.g., "500m")
    #[serde(default = "default_cpu_request")]
    pub cpu_request: String,

    /// Memory request (e.g., "1Gi")
    #[serde(default = "default_memory_request")]
    pub memory_request: String,

    /// CPU limit (e.g., "2000m")
    #[serde(default = "default_cpu_limit")]
    pub cpu_limit: String,

    /// Memory limit (e.g., "4Gi")
    #[serde(default = "default_memory_limit")]
    pub memory_limit: String,
}

fn default_cpu_request() -> String {
    "500m".to_string()
}
fn default_memory_request() -> String {
    "1Gi".to_string()
}
fn default_cpu_limit() -> String {
    "2000m".to_string()
}
fn default_memory_limit() -> String {
    "4Gi".to_string()
}

impl Default for KubernetesResourceDefaults {
    fn default() -> Self {
        Self {
            cpu_request: default_cpu_request(),
            memory_request: default_memory_request(),
            cpu_limit: default_cpu_limit(),
            memory_limit: default_memory_limit(),
        }
    }
}

/// GPU configuration for K8s sandbox pods
///
/// Controls GPU scheduling behavior when `spec.gpu` is true.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GpuConfig {
    /// Node selector for GPU nodes. Pods will be scheduled on nodes matching these labels.
    #[serde(default)]
    pub node_selector: std::collections::HashMap<String, String>,

    /// Tolerations for GPU nodes. Allows pods to tolerate GPU node taints.
    /// Each entry is a JSON object with key, operator, value, effect fields.
    #[serde(default)]
    pub tolerations: Vec<serde_json::Value>,

    /// GPU resource request quantity (e.g., "1" for nvidia.com/gpu).
    /// Set as both request and limit so the scheduler pre-reserves the GPU.
    #[serde(default = "default_gpu_resource_request")]
    pub resource_request: String,
}

fn default_gpu_resource_request() -> String {
    "1".to_string()
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            node_selector: std::collections::HashMap::new(),
            tolerations: Vec::new(),
            resource_request: default_gpu_resource_request(),
        }
    }
}

/// Sandbox default configuration.
///
/// Controls backend selection (Docker vs Kubernetes), auto-cleanup behavior,
/// resource limits, tool timeouts, and VNC settings applied to all sandboxes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SandboxConfig {
    /// Sandbox backend type (default: docker)
    ///
    /// Selects which SandboxManager implementation to use.
    /// - Docker: uses bollard to talk to Docker daemon
    /// - Kubernetes: uses kube-rs to manage K8s pods
    #[serde(default)]
    pub backend: BackendType,

    /// Default inactivity timeout in minutes (default: 30)
    pub default_inactivity_timeout: u64,

    /// Cleanup dry-run mode (default: false)
    pub cleanup_dry_run: bool,

    /// State monitor interval in seconds (default: 60)
    pub state_monitor_interval: u64,

    /// Deleted sandbox retention period in days (default: 15)
    ///
    /// Soft-deleted sandboxes are retained for this period before permanent deletion.
    /// During this period, deleted sandboxes can be restored via API, CLI, or dashboard.
    pub deleted_sandbox_retention_days: u64,

    /// Default VNC resolution (default: 2560x1440)
    #[serde(default = "default_vnc_resolution")]
    pub default_vnc_resolution: String,

    /// VNC port for sandbox containers (default: 5901)
    #[serde(default = "default_vnc_port")]
    pub vnc_port: u16,

    /// Maximum number of browser tabs per sandbox (default: 20)
    ///
    /// When a sandbox opens more tabs than this limit, the oldest tabs
    /// are automatically closed (FIFO eviction). This prevents resource
    /// exhaustion from unbounded tab growth while allowing VNC users to
    /// see multiple visited pages in the browser tab strip.
    #[serde(default = "default_max_browser_tabs")]
    pub max_browser_tabs: u32,

    /// Tool execution timeout configuration
    #[serde(default)]
    pub tool_timeouts: ToolTimeoutConfig,

    /// Default resource limits for sandbox creation (default: all unlimited)
    ///
    /// These limits are applied when creating sandboxes without explicit resource limits.
    /// They help prevent resource exhaustion in production environments.
    /// Request-level limits take precedence over these defaults.
    #[serde(default)]
    pub default_resource_limits: DefaultResourceLimits,

    /// Kubernetes backend configuration
    ///
    /// Only used when `backend` is set to `Kubernetes`.
    #[serde(default)]
    pub kubernetes: KubernetesConfig,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            backend: BackendType::default(),
            default_inactivity_timeout: 30,
            cleanup_dry_run: false,
            state_monitor_interval: 60,
            deleted_sandbox_retention_days: 15,
            default_vnc_resolution: default_vnc_resolution(),
            vnc_port: default_vnc_port(),
            max_browser_tabs: default_max_browser_tabs(),
            tool_timeouts: ToolTimeoutConfig::default(),
            default_resource_limits: DefaultResourceLimits::default(),
            kubernetes: KubernetesConfig::default(),
        }
    }
}

fn default_vnc_resolution() -> String {
    "2560x1440".to_string()
}

fn default_vnc_port() -> u16 {
    5901
}

fn default_max_browser_tabs() -> u32 {
    20
}

/// SSH gateway configuration.
///
/// Controls the embedded SSH server that allows direct terminal access to sandboxes
/// via standard SSH clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SshConfig {
    /// SSH gateway port (default: 2222)
    pub port: u16,

    /// API base URL for DSB server
    #[serde(default = "default_api_url")]
    pub api_url: String,

    /// API key (optional)
    pub api_key: Option<String>,

    /// Host key file path (optional, generates ephemeral key if not specified)
    pub host_key_path: Option<String>,

    /// How often to check for stale SSH sessions in seconds (default: 30)
    #[serde(default = "default_ssh_cleanup_check_interval")]
    pub cleanup_check_interval: u64,

    /// Seconds of inactivity before SSH session is considered stale (default: 300)
    #[serde(default = "default_ssh_session_timeout")]
    pub session_timeout: u64,

    /// Timeout for graceful SSH session termination in seconds (default: 60)
    #[serde(default = "default_ssh_termination_timeout")]
    pub termination_timeout: u64,

    /// Sandbox backend for exec operations: "docker" or "kubernetes" (default: "docker")
    #[serde(default = "default_ssh_backend")]
    pub backend: String,

    /// Kubernetes namespace for sandbox pods (default: "dsb-sandboxes")
    #[serde(default = "default_ssh_kubernetes_namespace")]
    pub kubernetes_namespace: String,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            port: 2222,
            api_url: default_api_url(),
            api_key: None,
            host_key_path: None,
            cleanup_check_interval: default_ssh_cleanup_check_interval(),
            session_timeout: default_ssh_session_timeout(),
            termination_timeout: default_ssh_termination_timeout(),
            backend: default_ssh_backend(),
            kubernetes_namespace: default_ssh_kubernetes_namespace(),
        }
    }
}

fn default_api_url() -> String {
    "http://localhost:8080".to_string()
}

fn default_ssh_cleanup_check_interval() -> u64 {
    30
}

fn default_ssh_session_timeout() -> u64 {
    300
}

fn default_ssh_termination_timeout() -> u64 {
    60
}

fn default_ssh_backend() -> String {
    "docker".to_string()
}

fn default_ssh_kubernetes_namespace() -> String {
    "dsb-sandboxes".to_string()
}

/// Logging configuration.
///
/// Controls log output format (pretty for development, JSON for production),
/// log level, file rotation, and ANSI color support.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Log level (default: info)
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Log format: "pretty" (development) or "json" (production)
    #[serde(default = "default_log_format")]
    pub format: String,

    /// Log to file (in addition to stdout)
    #[serde(default)]
    pub file: Option<String>,

    /// Rotate log files when they exceed this size (MB)
    #[serde(default = "default_log_max_file_size_mb")]
    pub max_file_size_mb: usize,

    /// Keep this many rotated log files
    #[serde(default = "default_log_max_files")]
    pub max_files: usize,

    /// Enable ANSI colors (only for pretty format)
    #[serde(default = "default_ansi_enabled")]
    pub ansi: bool,

    /// Log filter targets (e.g., "dsb=debug,docker=warn")
    #[serde(default)]
    pub filters: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            ansi: default_ansi_enabled(),
            file: None,
            max_file_size_mb: default_log_max_file_size_mb(),
            max_files: default_log_max_files(),
            filters: None,
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "pretty".to_string()
}

fn default_ansi_enabled() -> bool {
    true
}

fn default_log_max_file_size_mb() -> usize {
    100 // 100 MB
}

fn default_log_max_files() -> usize {
    10 // Keep 10 files
}

/// Static file server configuration.
///
/// Controls file serving for sandbox uploads/downloads, cache headers,
/// directory browsing, and ZIP archive downloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StaticServerConfig {
    /// Base path for static file storage (path for **reading** files via static API)
    ///
    /// # Important: Two Different Paths
    ///
    /// When DSB runs in a Docker container with Docker socket mounted, there are TWO paths:
    ///
    /// 1. **`base_path`** (this field): Path **inside the DSB container** where the static file API reads files
    /// 2. **`host_path`** (see below): Path on the **HOST machine** for creating bind mounts to sandbox containers
    ///
    /// # How It Works
    ///
    /// ```text
    /// ┌─────────────────────────────────────────────────────────────┐
    /// │ HOST MACHINE                                                  │
    /// │                                                              │
    /// │  ~/data/dsb/          ◄── host_path (for bind mounts)        │
    /// │    └── {sandbox_id}/                                         │
    /// │        └── file.txt                                          │
    /// │                                                              │
    /// │  Docker socket: /var/run/docker.sock                         │
    /// └─────────────────────────────────────────────────────────────┘
    ///                          │ mount
    ///                          ▼
    /// ┌─────────────────────────────────────────────────────────────┐
    /// │ DSB CONTAINER                                                 │
    /// │                                                              │
    /// │  /var/lib/dsb/static-files/  ◄── base_path (for reading)     │
    /// │    └── {sandbox_id}/                                         │
    /// │        └── file.txt                                          │
    /// │                                                              │
    /// │  Static File API serves files from base_path                │
    /// │  Manager creates bind mounts from host_path                 │
    /// └─────────────────────────────────────────────────────────────┘
    ///                          │ bind mount
    ///                          ▼
    /// ┌─────────────────────────────────────────────────────────────┐
    /// │ SANDBOX CONTAINER                                             │
    /// │                                                              │
    /// │  /public/file.txt          ◄── mounted from host_path        │
    /// │                                                              │
    /// └─────────────────────────────────────────────────────────────┘
    /// ```
    ///
    /// # Configuration Examples
    ///
    /// **Local Development** (DSB runs directly on host, not in container):
    /// ```bash
    /// DSB_STATIC_SERVER__BASE_PATH=~/data/dsb
    /// # host_path is None (not needed)
    /// ```
    ///
    /// **Docker Compose** (DSB runs in container):
    /// ```yaml
    /// services:
    ///   dsb-server:
    ///     volumes:
    ///       # Mount host directory into container
    ///       - ${STATIC_FILES_HOST_PATH:-~/data/dsb}:/var/lib/dsb/static-files
    ///     environment:
    ///       # Path for reading files inside container
    ///       - DSB_STATIC_SERVER__BASE_PATH=/var/lib/dsb/static-files
    ///       # Path for bind mounts on host
    ///       - DSB_STATIC_SERVER__HOST_PATH=~/data/dsb
    /// ```
    ///
    /// Default: "/var/lib/dsb/static-files"
    #[serde(default = "default_static_files_base_path")]
    pub base_path: String,

    /// Host path for bind mounts to sandbox containers (Docker Compose only)
    ///
    /// # When to Use This
    ///
    /// This field is **only needed when DSB runs in a Docker container** with:
    /// - Docker socket mounted (`/var/run/docker.sock`)
    /// - Host directory mounted into the container
    ///
    /// In this setup:
    /// - `base_path` = path **inside the DSB container** for reading files (e.g., `/var/lib/dsb/static-files`)
    /// - `host_path` = path on the **HOST machine** for bind mounts (e.g., `/tmp/dsb-test-static` or `~/data/dsb`)
    ///
    /// # How It Works
    ///
    /// When creating sandbox containers via the Docker API:
    /// 1. The bind mount source path must be on the **HOST** filesystem
    /// 2. DSB creates directories at `host_path/{sandbox_id}/` on the host
    /// 3. Sandbox containers mount this directory at `/public`
    /// 4. The static file API serves files from `base_path/{sandbox_id}/` inside the DSB container
    ///
    /// # Docker Compose Example
    ///
    /// ```yaml
    /// services:
    ///   dsb-server:
    ///     volumes:
    ///       # Mount host directory into container
    ///       - ${STATIC_FILES_HOST_PATH:-/tmp/dsb-test-static}:/var/lib/dsb/static-files
    ///     environment:
    ///       # Container-internal path for reading files
    ///       - DSB_STATIC_SERVER__BASE_PATH=/var/lib/dsb/static-files
    ///       # Host path for creating bind mounts to sandboxes
    ///       - DSB_STATIC_SERVER__HOST_PATH=/tmp/dsb-test-static
    /// ```
    ///
    /// # Local Development
    ///
    /// When running DSB directly on the host (not in a container):
    /// - Leave this as `None`
    /// - DSB will use `base_path` for both reading files and creating bind mounts
    ///
    /// Default: None
    #[serde(default)]
    pub host_path: Option<String>,

    /// API key for static file access (None = use server.api_key)
    pub api_key: Option<String>,

    /// Maximum file size for upload in MB (default: 100)
    #[serde(default = "default_max_file_size")]
    pub max_file_size_mb: usize,

    /// Maximum file size for sandbox file uploads in MB (default: 10)
    ///
    /// This limit applies to file uploads via the sandbox API endpoints
    /// (e.g., uploading files to a specific sandbox).
    #[serde(default = "default_sandbox_upload_max_file_size")]
    pub sandbox_upload_max_file_size_mb: usize,

    /// Enable directory browsing (default: false for security)
    #[serde(default)]
    pub enable_directory_browsing: bool,

    /// Default cache control header for all files (default: "public, max-age=3600")
    #[serde(default = "default_cache_control")]
    pub cache_control: String,

    /// Per-file-type cache control overrides
    ///
    /// Maps MIME type patterns to cache control directives.
    /// Supports exact MIME types (e.g., "text/html") and wildcards (e.g., "image/*").
    /// Example: {"text/html": "no-cache", "image/*": "public, max-age=86400"}
    #[serde(default)]
    pub cache_control_by_type: std::collections::HashMap<String, String>,

    /// Require authentication for static file access (default: false)
    ///
    /// - false: Allow all requests without API key (development mode)
    /// - true: Require valid API key for all static file requests
    #[serde(default)]
    pub require_auth: bool,

    /// Enable downloading all files as a ZIP archive (default: true)
    ///
    /// - true: Allow users to download all sandbox files as a ZIP archive
    /// - false: Disable the ZIP download feature
    #[serde(default = "default_enable_zip_download")]
    pub enable_zip_download: bool,

    /// Maximum size for ZIP download in MB (default: 500)
    ///
    /// If the total size of files to download exceeds this limit,
    /// the download will be rejected with an error.
    #[serde(default = "default_max_zip_size_mb")]
    pub max_zip_size_mb: usize,

    /// Prefix for ZIP download filename (default: "sandbox-")
    ///
    /// The downloaded ZIP file will be named: `{prefix}{sandbox_id}.zip`
    /// For example, with prefix "agent_session_", the file would be "agent_session_{uuid}.zip"
    #[serde(default = "default_zip_download_file_prefix")]
    pub zip_download_file_prefix: String,
}

impl Default for StaticServerConfig {
    fn default() -> Self {
        Self {
            base_path: default_static_files_base_path(),
            host_path: None,
            api_key: None,
            max_file_size_mb: default_max_file_size(),
            sandbox_upload_max_file_size_mb: default_sandbox_upload_max_file_size(),
            enable_directory_browsing: false,
            cache_control: default_cache_control(),
            cache_control_by_type: std::collections::HashMap::new(),
            require_auth: false,
            enable_zip_download: default_enable_zip_download(),
            max_zip_size_mb: default_max_zip_size_mb(),
            zip_download_file_prefix: default_zip_download_file_prefix(),
        }
    }
}

fn default_static_files_base_path() -> String {
    "/var/lib/dsb/static-files".to_string()
}

fn default_max_file_size() -> usize {
    100
}

fn default_sandbox_upload_max_file_size() -> usize {
    10
}

fn default_cache_control() -> String {
    "public, max-age=3600".to_string()
}

pub(crate) fn default_enable_zip_download() -> bool {
    true
}

pub(crate) fn default_max_zip_size_mb() -> usize {
    500
}

pub(crate) fn default_zip_download_file_prefix() -> String {
    "sandbox-".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();

        // Server defaults
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.api_key, None);
        assert_eq!(config.server.web_terminal_api_key, None);
        assert_eq!(config.server.ssh_gateway_api_key, None);
        assert_eq!(config.server.vnc_api_key, None);
        assert!(!config.server.require_auth);
        assert_eq!(config.server.admin_api_key, None);
        assert_eq!(config.server.session_token_ttl_secs, 300);

        // Database defaults
        assert_eq!(config.database.url, None);
        assert_eq!(config.database.host, "localhost");
        assert_eq!(config.database.port, 5432);
        assert_eq!(config.database.name, "dsb");
        assert_eq!(config.database.user, "postgres");
        assert_eq!(config.database.password, None);

        // Docker defaults
        assert_eq!(config.docker.registry, "docker.io");
        assert_eq!(config.docker.host, None);
        assert_eq!(config.docker.default_image, "docker.io/dsb/sandbox:latest");
        assert_eq!(config.docker.test_image, "python:3.12");

        // Sandbox defaults
        assert_eq!(config.sandbox.default_inactivity_timeout, 30);
        assert!(!config.sandbox.cleanup_dry_run);
        assert_eq!(config.sandbox.state_monitor_interval, 60);
        assert_eq!(config.sandbox.backend, BackendType::Docker);

        // Kubernetes defaults
        assert_eq!(config.sandbox.kubernetes.namespace, "dsb-sandboxes");
        assert!(config.sandbox.kubernetes.image_pull_secrets.is_empty());
        assert!(config.sandbox.kubernetes.tolerations.is_empty());
        assert!(config.sandbox.kubernetes.node_selector.is_empty());
        assert_eq!(config.sandbox.kubernetes.pvc_storage_class, "");
        assert_eq!(config.sandbox.kubernetes.pvc_access_mode, "ReadWriteMany");
        assert_eq!(
            config.sandbox.kubernetes.pvc_name,
            "dsb-static-files-shared"
        );
        assert_eq!(config.sandbox.kubernetes.pod_ready_timeout_secs, 120);
        assert_eq!(
            config.sandbox.kubernetes.resource_defaults.cpu_request,
            "500m"
        );
        assert_eq!(
            config.sandbox.kubernetes.resource_defaults.memory_request,
            "1Gi"
        );
        assert_eq!(
            config.sandbox.kubernetes.resource_defaults.cpu_limit,
            "2000m"
        );
        assert_eq!(
            config.sandbox.kubernetes.resource_defaults.memory_limit,
            "4Gi"
        );

        // SSH defaults
        assert_eq!(config.ssh.port, 2222);
        assert_eq!(config.ssh.api_url, "http://localhost:8080");
        assert_eq!(config.ssh.api_key, None);
        assert_eq!(config.ssh.host_key_path, None);

        // Logging defaults
        assert_eq!(config.logging.level, "info");

        // Static server defaults
        assert_eq!(config.static_server.base_path, "/var/lib/dsb/static-files");
        assert_eq!(config.static_server.api_key, None);
        assert_eq!(config.static_server.max_file_size_mb, 100);
        assert!(!config.static_server.enable_directory_browsing);
    }

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.port, 8080);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.api_key, None);
        assert_eq!(config.session_token_ttl_secs, 300);
        assert_eq!(config.cookie_secret, None);
    }

    #[test]
    fn test_database_config_default() {
        let config = DatabaseConfig::default();
        assert_eq!(config.url, None);
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5432);
        assert_eq!(config.name, "dsb");
        assert_eq!(config.user, "postgres");
        assert_eq!(config.password, None);
        assert_eq!(config.pool_max_size, Some(10));
    }

    #[test]
    fn test_docker_defaults() {
        let config = DockerConfig::default();
        assert_eq!(config.registry, "docker.io");
        assert_eq!(config.host, None);
        assert_eq!(config.default_image, "docker.io/dsb/sandbox:latest");
        assert_eq!(config.test_image, "python:3.12");
    }

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.default_inactivity_timeout, 30);
        assert!(!config.cleanup_dry_run);
        assert_eq!(config.state_monitor_interval, 60);
        assert_eq!(config.deleted_sandbox_retention_days, 15);
    }

    #[test]
    fn test_ssh_config_default() {
        let config = SshConfig::default();
        assert_eq!(config.port, 2222);
        assert_eq!(config.api_url, "http://localhost:8080");
        assert_eq!(config.api_key, None);
        assert_eq!(config.host_key_path, None);
    }

    #[test]
    fn test_logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, "info");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();

        // Test that Config can be serialized to JSON
        let json = serde_json::to_value(&config).unwrap();

        // Verify some key fields exist
        assert_eq!(json["server"]["port"], 8080);
        assert_eq!(json["server"]["host"], "0.0.0.0");
        assert_eq!(json["database"]["host"], "localhost");
        assert_eq!(json["database"]["port"], 5432);
        assert_eq!(json["docker"]["registry"], "docker.io");
        assert_eq!(json["ssh"]["port"], 2222);
        assert_eq!(json["logging"]["level"], "info");
    }

    #[test]
    fn test_config_deserialization() {
        let json = serde_json::json!({
            "server": {
                "port": 9000,
                "host": "127.0.0.1",
                "api_key": "test-key"
            },
            "database": {
                "url": "postgresql://user:pass@localhost:5432/db",
                "host": "dbhost",
                "port": 5433,
                "name": "testdb",
                "user": "testuser",
                "password": "testpass"
            },
            "docker": {
                "registry": "registry.example.com",
                "host": "unix:///var/run/docker.sock",
                "default_image": "custom/image:v1",
                "test_image": "test/image:v1"
            },
            "sandbox": {
                "default_inactivity_timeout": 60,
                "cleanup_dry_run": true,
                "state_monitor_interval": 30,
                "deleted_sandbox_retention_days": 7
            },
            "ssh": {
                "port": 2223,
                "api_url": "http://api.example.com",
                "api_key": "ssh-key",
                "host_key_path": "/path/to/key"
            },
            "logging": {
                "level": "debug"
            },
            "static_server": {
                "base_path": "/custom/static-files",
                "max_file_size_mb": 200,
                "enable_directory_browsing": true
            }
        });

        let config: Config = serde_json::from_value(json).unwrap();

        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.api_key, Some("test-key".to_string()));

        assert_eq!(
            config.database.url,
            Some("postgresql://user:pass@localhost:5432/db".to_string())
        );
        assert_eq!(config.database.host, "dbhost");
        assert_eq!(config.database.port, 5433);
        assert_eq!(config.database.name, "testdb");
        assert_eq!(config.database.user, "testuser");
        assert_eq!(config.database.password, Some("testpass".to_string()));

        assert_eq!(config.docker.registry, "registry.example.com");
        assert_eq!(
            config.docker.host,
            Some("unix:///var/run/docker.sock".to_string())
        );
        assert_eq!(config.docker.default_image, "custom/image:v1");
        assert_eq!(config.docker.test_image, "test/image:v1");

        assert_eq!(config.sandbox.default_inactivity_timeout, 60);
        assert!(config.sandbox.cleanup_dry_run);
        assert_eq!(config.sandbox.state_monitor_interval, 30);
        assert_eq!(config.sandbox.deleted_sandbox_retention_days, 7);

        assert_eq!(config.ssh.port, 2223);
        assert_eq!(config.ssh.api_url, "http://api.example.com");
        assert_eq!(config.ssh.api_key, Some("ssh-key".to_string()));
        assert_eq!(config.ssh.host_key_path, Some("/path/to/key".to_string()));

        assert_eq!(config.logging.level, "debug");

        // Static server config
        assert_eq!(config.static_server.base_path, "/custom/static-files");
        assert_eq!(config.static_server.api_key, None);
        assert_eq!(config.static_server.max_file_size_mb, 200);
        assert!(config.static_server.enable_directory_browsing);
    }

    #[test]
    fn test_config_with_optional_fields() {
        let json = serde_json::json!({
            "server": {
                "port": 8080
                // host, api_key omitted - should use defaults
            },
            "database": {
                "password": "required-password"
                // other fields omitted - should use defaults
            },
            "docker": {
                // all fields omitted - should use defaults
            },
            "ssh": {
                // all fields omitted - should use defaults
            },
            "logging": {
                // all fields omitted - should use defaults
            }
        });

        let config: Config = serde_json::from_value(json).unwrap();

        // Server should have specified port and default host
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.api_key, None);

        // Database should have password and defaults for other fields
        assert_eq!(
            config.database.password,
            Some("required-password".to_string())
        );
        assert_eq!(config.database.host, "localhost");
        assert_eq!(config.database.port, 5432);

        // Docker should have all defaults
        assert_eq!(config.docker.registry, "docker.io");
        assert_eq!(config.docker.host, None);

        // SSH should have all defaults
        assert_eq!(config.ssh.port, 2222);
        assert_eq!(config.ssh.api_url, "http://localhost:8080");
    }

    #[test]
    fn test_docker_registry_defaults() {
        let config = DockerConfig::default();

        assert_eq!(config.registry, "docker.io");
        assert_eq!(config.default_image, "docker.io/dsb/sandbox:latest");
        assert_eq!(config.test_image, "python:3.12");
    }

    #[test]
    fn test_sandbox_timeout_defaults() {
        let config = SandboxConfig::default();

        // 30 minutes default inactivity timeout
        assert_eq!(config.default_inactivity_timeout, 30);

        // Cleanup not in dry-run mode by default
        assert!(!config.cleanup_dry_run);

        // State monitor every 60 seconds
        assert_eq!(config.state_monitor_interval, 60);

        // 15 days retention for deleted sandboxes
        assert_eq!(config.deleted_sandbox_retention_days, 15);
    }

    #[test]
    fn test_ssh_port_default() {
        let config = SshConfig::default();
        assert_eq!(config.port, 2222);
    }

    #[test]
    fn test_log_level_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, "info");
        assert_eq!(config.format, "pretty");
        assert!(config.ansi);
        assert_eq!(config.file, None);
        assert_eq!(config.filters, None);
    }

    #[test]
    fn test_static_server_config_default() {
        let config = StaticServerConfig::default();
        assert_eq!(config.base_path, "/var/lib/dsb/static-files");
        assert_eq!(config.api_key, None);
        assert_eq!(config.max_file_size_mb, 100);
        assert!(!config.enable_directory_browsing);
        assert!(!config.require_auth);
    }

    #[test]
    fn test_database_config_get_url_with_url_set() {
        let config = DatabaseConfig {
            url: Some("postgresql://user:pass@localhost:5432/db".to_string()),
            ..Default::default()
        };

        let url = config.get_url();
        assert_eq!(
            url,
            Some("postgresql://user:pass@localhost:5432/db".to_string())
        );
    }

    #[test]
    fn test_database_config_get_url_from_components() {
        let config = DatabaseConfig {
            url: None,
            host: "dbhost".to_string(),
            port: 5433,
            name: "testdb".to_string(),
            user: "testuser".to_string(),
            password: Some("testpass".to_string()),
            ..Default::default()
        };

        let url = config.get_url();
        assert_eq!(
            url,
            Some("postgresql://testuser:testpass@dbhost:5433/testdb".to_string())
        );
    }

    #[test]
    fn test_database_config_get_url_without_password() {
        let config = DatabaseConfig {
            url: None,
            password: None,
            ..Default::default()
        };

        let url = config.get_url();
        assert_eq!(url, None);
    }

    #[test]
    fn test_database_config_get_url_with_defaults() {
        let config = DatabaseConfig {
            url: None,
            password: Some("password".to_string()),
            ..Default::default()
        };

        let url = config.get_url();
        assert_eq!(
            url,
            Some("postgresql://postgres:password@localhost:5432/dsb".to_string())
        );
    }

    #[test]
    fn test_kubernetes_config_deserialization() {
        let json = serde_json::json!({
            "sandbox": {
                "backend": "kubernetes",
                "kubernetes": {
                    "namespace": "custom-ns",
                    "image_pull_secrets": ["my-registry-secret"],
                    "tolerations": [
                        {"key": "dedicated", "operator": "Equal", "value": "sandbox", "effect": "NoSchedule"}
                    ],
                    "node_selector": {
                        "node-type": "sandbox"
                    },
                    "resource_defaults": {
                        "cpu_request": "250m",
                        "memory_request": "512Mi",
                        "cpu_limit": "1000m",
                        "memory_limit": "2Gi"
                    },
                    "pvc_storage_class": "fast-ssd",
                    "pvc_access_mode": "ReadWriteOnce",
                    "pod_ready_timeout_secs": 300
                }
            }
        });

        let config: Config = serde_json::from_value(json).unwrap();

        assert_eq!(config.sandbox.backend, BackendType::Kubernetes);
        assert_eq!(config.sandbox.kubernetes.namespace, "custom-ns");
        assert_eq!(
            config.sandbox.kubernetes.image_pull_secrets,
            vec!["my-registry-secret"]
        );
        assert_eq!(config.sandbox.kubernetes.tolerations.len(), 1);
        assert_eq!(config.sandbox.kubernetes.tolerations[0]["key"], "dedicated");
        assert_eq!(
            config
                .sandbox
                .kubernetes
                .node_selector
                .get("node-type")
                .unwrap(),
            "sandbox"
        );
        assert_eq!(
            config.sandbox.kubernetes.resource_defaults.cpu_request,
            "250m"
        );
        assert_eq!(
            config.sandbox.kubernetes.resource_defaults.memory_request,
            "512Mi"
        );
        assert_eq!(
            config.sandbox.kubernetes.resource_defaults.cpu_limit,
            "1000m"
        );
        assert_eq!(
            config.sandbox.kubernetes.resource_defaults.memory_limit,
            "2Gi"
        );
        assert_eq!(config.sandbox.kubernetes.pvc_storage_class, "fast-ssd");
        assert_eq!(config.sandbox.kubernetes.pvc_access_mode, "ReadWriteOnce");
        assert_eq!(
            config.sandbox.kubernetes.pvc_name,
            "dsb-static-files-shared"
        );
        assert_eq!(config.sandbox.kubernetes.pod_ready_timeout_secs, 300);
    }

    #[test]
    fn test_backend_type_serialization() {
        assert_eq!(
            serde_json::to_string(&BackendType::Docker).unwrap(),
            "\"docker\""
        );
        assert_eq!(
            serde_json::to_string(&BackendType::Kubernetes).unwrap(),
            "\"kubernetes\""
        );

        let docker: BackendType = serde_json::from_str("\"docker\"").unwrap();
        assert_eq!(docker, BackendType::Docker);

        let k8s: BackendType = serde_json::from_str("\"kubernetes\"").unwrap();
        assert_eq!(k8s, BackendType::Kubernetes);
    }
}
