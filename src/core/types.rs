// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Core Type Definitions
//!
//! This module defines all the core data types used throughout the DSB (Distributed Sandboxes) system.
//!
//! ## Overview
//!
//! The types are organized into several categories:
//!
//! - **Sandbox State Management**: [`SandboxState`] enum representing the lifecycle of a sandbox
//! - **Configuration Types**: [`SandboxConfig`], [`PortMapping`], [`ResourceLimits`]
//! - **Data Transfer Objects**: [`CreateSandboxRequest`], [`SandboxResponse`]
//! - **Core Domain Objects**: [`Sandbox`] representing a complete sandbox instance
//!
//! ## Example: Creating a Sandbox Configuration
//!
//! ```rust
//! use dsb::core::types::{SandboxConfig, PortMapping, PortProtocol, ResourceLimits, PullPolicy};
//! use std::collections::HashMap;
//!
//! let config = SandboxConfig {
//!     image: "nginx:latest".to_string(),
//!     name: Some("my-web-server".to_string()),
//!     environment: {
//!         let mut env = HashMap::new();
//!         env.insert("ENV".to_string(), "production".to_string());
//!         env
//!     },
//!     port_mappings: vec![
//!         PortMapping {
//!             host_port: 8080,
//!             container_port: 80,
//!             protocol: PortProtocol::Tcp,
//!         }
//!     ],
//!     exposed_ports: vec![],
//!     resource_limits: ResourceLimits {
//!         memory_mb: Some(512),
//!         cpu_quota: Some(50000),
//!         cpu_period: Some(100000),
//!         cpu_shares: None,
//!         pids_limit: Some(100),
//!         ulimits: None,
//!     },
//!     volumes: vec![],
//!     command: None,
//!     inactivity_timeout_minutes: None,
//!     pull_policy: PullPolicy::Missing,
//!     features: vec![],
//!     enable_all_features: false,
//!     vnc_resolution: None,
//! };
//! ```
//!
//! ## Example: Serializing to JSON
//!
//! ```rust
//! use dsb::core::types::{SandboxState, Sandbox};
//! use serde_json;
//! use uuid::Uuid;
//! use chrono::Utc;
//!
//! let state = SandboxState::Running;
//! let json = serde_json::to_string(&state).unwrap();
//! assert_eq!(json, "\"running\"");
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::core::errors::{ApiError, ErrorCode};

/// Image summary for list view
#[derive(Debug, Serialize, Deserialize)]
pub struct ImageSummary {
    /// Image ID (digest)
    pub id: String,
    /// Repository tags (e.g., ["nginx:latest"])
    pub repo_tags: Vec<String>,
    /// Image size in bytes
    pub size: i64,
    /// Creation timestamp (Unix epoch seconds)
    pub created: i64,
    /// Image labels (key-value pairs)
    pub labels: Option<HashMap<String, String>>,
}

/// Detailed image information with detected features
#[derive(Debug, Serialize, Deserialize)]
pub struct ImageDetails {
    /// Image ID (digest)
    pub id: String,
    /// Repository tags
    pub repo_tags: Vec<String>,
    /// Image size in bytes
    pub size: i64,
    /// Virtual size including parent layers
    pub virtual_size: i64,
    /// Creation timestamp (Unix epoch seconds)
    pub created: i64,
    /// CPU architecture (e.g., "amd64")
    pub architecture: String,
    /// Operating system (e.g., "linux")
    pub os: String,
    /// Image labels (key-value pairs)
    pub labels: Option<HashMap<String, String>>,
    /// Environment variables defined in the image
    pub env: Option<Vec<String>>,
    /// Detected DSB features from image labels
    pub features: Vec<String>,
}

/// Identity information extracted from a validated API key.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiKeyIdentity {
    /// The API key ID (UUID from database).
    /// `None` for admin/legacy config keys.
    pub id: Option<uuid::Uuid>,
    /// The type of API key used for authentication.
    pub key_type: ApiKeyType,
}

/// Type of API key used for authentication.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ApiKeyType {
    /// Admin or legacy config key — full access to all sandboxes.
    Privileged,
    /// Database-backed API key — scoped to owned sandboxes only.
    Database,
}

/// Represents the current state of a sandbox in its lifecycle.
///
/// # Lifecycle Flow
///
/// The sandbox follows this state transition flow:
///
/// ```text
/// Creating → Created → Starting → Running → Stopped
///                                    ↓
///                                  Error
/// ```
///
/// # States
///
/// - **Creating** - The sandbox is being provisioned, container is being created
/// - **Created** - The container has been created but not yet started
/// - **Starting** - The container is starting up (transitional state)
/// - **Running** - The sandbox is active and ready to accept commands
/// - **Stopped** - The sandbox has been stopped but not destroyed
/// - **Error** - An error occurred during sandbox operations (check `error_message`)
/// - **Destroying** - The sandbox is being destroyed (transitional state)
/// - **Destroyed** - The sandbox has been destroyed/deleted (final state)
///
/// # Example
///
/// ```rust
/// use dsb::core::SandboxState;
///
/// let state = SandboxState::Creating;
/// assert_eq!(state as i32, 0); // States are ordered
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxState {
    /// Sandbox is being created
    Creating,
    /// Sandbox has been created but not started
    Created,
    /// Sandbox is starting (transitional)
    Starting,
    /// Sandbox is running and ready
    Running,
    /// Sandbox has been stopped
    Stopped,
    /// Sandbox encountered an error
    Error,
    /// Sandbox is being destroyed (transitional)
    Destroying,
    /// Sandbox has been destroyed (final state for deleted sandboxes)
    Destroyed,
}

impl SandboxState {
    /// Returns the string representation of the state for database storage.
    ///
    /// This provides a stable string representation that doesn't depend on
    /// the Debug format, making it safe for database serialization.
    ///
    /// # Example
    ///
    /// ```rust
    /// use dsb::core::types::SandboxState;
    ///
    /// let state = SandboxState::Running;
    /// assert_eq!(state.as_str(), "running");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Creating => "creating",
            Self::Created => "created",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Error => "error",
            Self::Destroying => "destroying",
            Self::Destroyed => "destroyed",
        }
    }
}

impl std::str::FromStr for SandboxState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "creating" => Ok(Self::Creating),
            "created" => Ok(Self::Created),
            "starting" => Ok(Self::Starting),
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "error" => Ok(Self::Error),
            "destroying" => Ok(Self::Destroying),
            "destroyed" => Ok(Self::Destroyed),
            _ => Err(format!("Invalid sandbox state: {}", s)),
        }
    }
}

/// Image pull policy controlling when to pull Docker images.
///
/// Similar to Kubernetes' image pull policy, this enum defines the
/// strategy for pulling images before creating containers.
///
/// # Policy Behavior
///
/// - **Always** - Always pull the image before creating the container
/// - **Missing** (default) - Only pull if the image doesn't exist locally
/// - **Never** - Never pull, fail if image doesn't exist locally
///
/// # Example
///
/// ```rust
/// use dsb::core::types::PullPolicy;
///
/// let policy = PullPolicy::Missing; // Default: pull only if needed
/// assert!(matches!(policy, PullPolicy::Missing));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PullPolicy {
    /// Always pull the image before creating the container
    ///
    /// Ensures you always have the latest version of the image.
    /// Best for development environments or when using :latest tags.
    Always,

    /// Only pull if the image doesn't exist locally (default)
    ///
    /// Checks if the image exists locally first. If it does, uses the
    /// local version. If not, pulls it from the registry.
    /// Best for production and CI/CD to balance speed and freshness.
    #[default]
    Missing,

    /// Never pull the image
    ///
    /// Fails if the image doesn't exist locally. Useful for:
    /// - Air-gapped environments
    /// - CI/CD with pre-pulled images
    /// - When you want explicit control over image versions
    Never,
}

impl PullPolicy {
    /// Returns the string representation of the pull policy for database storage.
    ///
    /// This provides a stable string representation that doesn't depend on
    /// the Debug format, making it safe for database serialization.
    ///
    /// # Example
    ///
    /// ```rust
    /// use dsb::core::types::PullPolicy;
    ///
    /// let policy = PullPolicy::Always;
    /// assert_eq!(policy.as_str(), "always");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Missing => "missing",
            Self::Never => "never",
        }
    }
}

/// Complete configuration for creating a sandbox.
///
/// This struct defines all parameters needed to create and configure a Docker container
/// as a sandbox. It includes image specification, environment variables, port mappings,
/// resource limits, volume mounts, and auto-cleanup settings.
///
/// # Fields
///
/// - `image` - Docker image to use (e.g., "nginx:latest")
/// - `name` - Optional name for the sandbox (used for container naming)
/// - `environment` - Environment variables to set in the container
/// - `port_mappings` - Port mappings from host to container
/// - `resource_limits` - Resource constraints (memory, CPU, PIDs)
/// - `volumes` - Volume mounts (bind mounts and named volumes)
/// - `inactivity_timeout_minutes` - Auto-cleanup timeout (None = disabled)
///
/// # Example
///
/// ```rust
/// use dsb::core::types::{SandboxConfig, VolumeMount};
///
/// let config = SandboxConfig {
///     image: "redis:alpine".to_string(),
///     name: Some("my-cache".to_string()),
///     volumes: vec![
///         VolumeMount::Bind {
///             host_path: "/tmp/data".to_string(),
///             container_path: "/data".to_string(),
///             read_only: false,
///         }
///     ],
///     inactivity_timeout_minutes: Some(30), // Auto-cleanup after 30 min of inactivity
///     ..Default::default()
/// };
///
/// assert_eq!(config.image, "redis:alpine");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Docker image to use for the sandbox
    ///
    /// Should be a valid Docker image reference that exists locally or in a registry.
    /// Examples: "nginx:latest", "docker.io/myorg/myimage:v1.0"
    pub image: String,

    /// Optional name for the sandbox
    ///
    /// If provided, this name will be used for the Docker container.
    /// Names must be unique. If not provided, Docker will generate a name.
    pub name: Option<String>,

    /// Environment variables to set in the container
    ///
    /// Key-value pairs that will be set as environment variables in the container.
    /// For example, {"RUST_LOG": "debug", "PORT": "8080"}
    pub environment: HashMap<String, String>,

    /// Port mappings from host to container
    ///
    /// Each mapping specifies how to forward ports from the host to the container.
    /// For example, mapping host port 8080 to container port 80.
    pub port_mappings: Vec<PortMapping>,

    /// Ports to expose internally (Docker network only)
    ///
    /// These ports are exposed in the Docker ExposedPorts configuration but NOT
    /// published to the host. This allows multiple sandboxes with the same features
    /// to run without port conflicts. Feature-detected ports go here.
    #[serde(default)]
    pub exposed_ports: Vec<u16>,

    /// Resource limits to apply to the container
    ///
    /// Controls the maximum resources the container can use.
    /// See [`ResourceLimits`] for details.
    pub resource_limits: ResourceLimits,

    /// Volume mounts to attach to the container
    ///
    /// Supports both bind mounts (host paths) and named volumes (Docker-managed).
    /// See [`VolumeMount`] for details.
    pub volumes: Vec<VolumeMount>,

    /// Command to run in the container (optional)
    ///
    /// If specified, this command will be executed when the container starts.
    /// If not specified, defaults to `["tail", "-f", "/dev/null"]` to keep
    /// the container running for interactive operations.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use dsb::core::types::SandboxConfig;
    /// # let config = SandboxConfig {
    /// # command: Some(vec!["sleep".to_string(), "infinity".to_string()]),
    /// # ..Default::default()
    /// # };
    /// ```
    ///
    /// Common choices:
    /// - `["sleep", "infinity"]` - Sleep forever (simple)
    /// - `["tail", "-f", "/dev/null"]` - Tail null device forever (keeps container alive)
    /// - `["python", "-m", "http.server", "8080"]` - Run Python HTTP server
    /// - `["/bin/bash"]` - Interactive bash shell (requires -i flag for exec)
    pub command: Option<Vec<String>>,

    /// Auto-cleanup timeout in minutes (optional)
    ///
    /// If set, the sandbox will be automatically cleaned up after this many minutes
    /// of inactivity (no API calls). Set to None to disable auto-cleanup.
    pub inactivity_timeout_minutes: Option<u64>,

    /// Image pull policy controlling when to pull Docker images
    ///
    /// Defaults to `PullPolicy::Missing`, which only pulls if the image
    /// doesn't exist locally. See [`PullPolicy`] for details.
    ///
    /// # Example
    ///
    /// ```rust
    /// use dsb::core::types::{SandboxConfig, PullPolicy};
    ///
    /// let config = SandboxConfig {
    ///     pull_policy: PullPolicy::Always,
    ///     ..Default::default()
    /// };
    /// ```
    #[serde(default)]
    pub pull_policy: PullPolicy,

    /// Feature profiles to enable from image metadata
    ///
    /// When the image has feature metadata labels, specific features can be
    /// enabled by name (e.g., ["vnc", "browser"]). Features will automatically
    /// configure ports, volumes, commands, and environment variables.
    #[serde(default)]
    pub features: Vec<String>,

    /// Enable all available features from image metadata
    ///
    /// When set to true, enables all features marked with `enabled_by_default: true`
    /// in the image metadata. Individual features can still be disabled using
    /// the `features` field as a blacklist (currently not supported, reserved
    /// for future use).
    #[serde(default)]
    pub enable_all_features: bool,

    /// VNC resolution (optional)
    ///
    /// If specified, sets the VNC display resolution. Format: "WIDTHxHEIGHT"
    /// Examples: "1280x720", "1920x1080", "2560x1440", "3840x2160"
    /// Defaults to the value configured in the application settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vnc_resolution: Option<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            image: "nginx:latest".to_string(),
            name: None,
            environment: HashMap::new(),
            port_mappings: Vec::new(),
            exposed_ports: Vec::new(),
            resource_limits: ResourceLimits::default(),
            volumes: Vec::new(),
            command: None, // Will default to tail -f /dev/null in container creation
            inactivity_timeout_minutes: None,
            pull_policy: PullPolicy::default(), // Missing
            features: Vec::new(),
            enable_all_features: false,
            vnc_resolution: None,
        }
    }
}

/// Port mapping configuration for connecting host ports to container ports.
///
/// # Example
///
/// ```rust
/// use dsb::core::types::{PortMapping, PortProtocol};
///
/// let mapping = PortMapping {
///     host_port: 8080,
///     container_port: 80,
///     protocol: PortProtocol::Tcp,
/// };
/// ```
///
/// This would forward TCP traffic from port 8080 on the host to port 80 in the container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    /// Port number on the host machine
    ///
    /// The port that will be exposed on the host. Traffic to this port
    /// will be forwarded to the container_port.
    pub host_port: u16,

    /// Port number inside the container
    ///
    /// The port that the container application is listening on.
    pub container_port: u16,

    /// Protocol to use for the port mapping
    pub protocol: PortProtocol,
}

/// Protocol type for port mappings.
///
/// # Example
///
/// ```rust
/// use dsb::core::types::PortProtocol;
///
/// let tcp = PortProtocol::Tcp;
/// let udp = PortProtocol::Udp;
///
/// // Serializes to lowercase JSON
/// assert_eq!(serde_json::to_string(&tcp).unwrap(), "\"tcp\"");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PortProtocol {
    /// Transmission Control Protocol - reliable, connection-oriented
    Tcp,
    /// User Datagram Protocol - unreliable, connectionless
    Udp,
}

/// Volume mount configuration for connecting host paths or named volumes to containers.
///
/// DSB supports both bind mounts (host path → container path) and named volumes
/// (Docker-managed volumes with lifecycle tied to containers).
///
/// # Examples
///
/// ## Bind Mount
///
/// ```rust
/// use dsb::core::types::VolumeMount;
///
/// let mount = VolumeMount::Bind {
///     host_path: "/tmp/data".to_string(),
///     container_path: "/data".to_string(),
///     read_only: false,
/// };
/// ```
///
/// This maps the host directory `/tmp/data` to `/data` in the container with read-write access.
///
/// ## Named Volume
///
/// ```rust
/// use dsb::core::types::VolumeMount;
///
/// let mount = VolumeMount::Named {
///     name: "my-volume".to_string(),
///     container_path: "/data".to_string(),
///     read_only: true,
/// };
/// ```
///
/// This mounts the Docker-managed volume `my-volume` to `/data` as read-only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum VolumeMount {
    /// Bind mount: map a host directory path into the container
    Bind {
        /// Path on the host filesystem
        host_path: String,

        /// Path inside the container
        container_path: String,

        /// Whether the mount is read-only (true) or read-write (false)
        read_only: bool,
    },

    /// Named volume: Docker-managed volume with lifecycle tied to container
    Named {
        /// Name of the Docker volume
        name: String,

        /// Path inside the container
        container_path: String,

        /// Whether the mount is read-only (true) or read-write (false)
        read_only: bool,
    },
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
/// ```rust
/// use dsb::core::types::Ulimit;
///
/// let ulimit = Ulimit {
///     name: "nofile".to_string(),
///     soft: 65536,  // Soft limit (can be increased by process)
///     hard: 65536,  // Hard limit (cannot be exceeded)
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ulimit {
    /// Name of the ulimit (e.g., "nofile", "nproc", "memlock")
    pub name: String,

    /// Soft limit (can be increased by the process up to the hard limit)
    pub soft: i64,

    /// Hard limit (absolute maximum that cannot be exceeded)
    pub hard: i64,
}

/// Resource limits for controlling container resource usage.
///
/// These limits help prevent containers from consuming excessive resources
/// and affecting other processes on the host system.
///
/// # CPU Limits
///
/// CPU limits can be specified in two ways:
///
/// 1. **CPU Quota/Period** - Absolute CPU time limits:
///    - `cpu_period` - The timeframe in microseconds (default: 100000 = 100ms)
///    - `cpu_quota` - Maximum CPU time in microseconds per period
///
///    For example, to limit to 50% of a single CPU core:
///    - `cpu_period`: 100000 (100ms)
///    - `cpu_quota`: 50000 (50ms per 100ms period = 50%)
///
/// 2. **CPU Shares** - Relative CPU weight (default: 1024):
///    - Higher values get more CPU time relative to other containers
///    - Does not guarantee specific CPU amounts, only relative priority
///    - Example: 2048 gets twice as much CPU as 1024 when under contention
///
/// # Example
///
/// ```rust
/// use dsb::core::types::{ResourceLimits, Ulimit};
///
/// let limits = ResourceLimits {
///     memory_mb: Some(512),        // 512 MB RAM
///     cpu_quota: Some(50000),      // 50% CPU (absolute)
///     cpu_period: Some(100000),    // 100ms period
///     cpu_shares: Some(2048),      // 2x CPU weight (relative)
///     pids_limit: Some(100),       // Max 100 processes
///     ulimits: Some(vec![
///         Ulimit {
///             name: "nofile".to_string(),
///             soft: 65536,
///             hard: 65536,
///         }
///     ]),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    /// Memory limit in megabytes
    ///
    /// If set, the container cannot use more than this amount of RAM.
    /// Example: Some(512) for 512 MB, None for unlimited.
    pub memory_mb: Option<u64>,

    /// CPU quota in microseconds per period
    ///
    /// Works together with cpu_period to limit CPU usage.
    /// See struct-level documentation for calculation examples.
    pub cpu_quota: Option<i64>,

    /// CPU period in microseconds
    ///
    /// The timeframe for CPU quota enforcement. Default is typically 100000 (100ms).
    pub cpu_period: Option<i64>,

    /// CPU shares (relative weight, default 1024)
    ///
    /// Relative CPU weight when containers compete for CPU time.
    /// Higher values get more CPU. Default is 1024.
    /// Example: Some(2048) gets 2x CPU compared to default.
    pub cpu_shares: Option<u64>,

    /// Maximum number of processes (PIDs) in the container
    ///
    /// Prevents fork bombs and limits process creation.
    /// Example: Some(100) for max 100 processes, None for unlimited.
    pub pids_limit: Option<i64>,

    /// Per-process resource limits (ulimits)
    ///
    /// Controls resource limits for individual processes.
    /// See [`Ulimit`] for details.
    #[serde(default)]
    pub ulimits: Option<Vec<Ulimit>>,
}

/// Container resource usage statistics.
///
/// Real-time metrics about container resource consumption.
/// Collected from Docker's container stats API.
///
/// # Fields
///
/// - **CPU** - Percentage of CPU usage relative to total host CPU capacity
/// - **Memory** - Current usage, limit, and percentage
/// - **Network** - Bytes transmitted and received
/// - **Disk I/O** - Bytes read and written from block devices
///
/// # Example
///
/// ```rust
/// use dsb::core::types::ContainerStats;
/// use chrono::Utc;
///
/// let stats = ContainerStats {
///     cpu_percent: 45.2,
///     memory_usage_mb: 256,
///     memory_limit_mb: 512,
///     memory_percent: 50.0,
///     network_rx_bytes: 1024000,
///     network_tx_bytes: 512000,
///     block_read_bytes: 2048000,
///     block_write_bytes: 1024000,
///     timestamp: Utc::now(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerStats {
    /// CPU usage as a percentage of total host CPU capacity
    ///
    /// Example: 50.0 means using 50% of one CPU core,
    /// or 25% of a 4-core system.
    pub cpu_percent: f64,

    /// Current memory usage in megabytes
    pub memory_usage_mb: u64,

    /// Memory limit in megabytes
    pub memory_limit_mb: u64,

    /// Memory usage as a percentage of the limit
    pub memory_percent: f64,

    /// Network bytes received (total)
    pub network_rx_bytes: u64,

    /// Network bytes transmitted (total)
    pub network_tx_bytes: u64,

    /// Block I/O bytes read (total)
    pub block_read_bytes: u64,

    /// Block I/O bytes written (total)
    pub block_write_bytes: u64,

    /// Timestamp when these stats were collected
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Backend-agnostic summary of a sandbox container/pod.
///
/// This struct provides a unified representation of sandbox information that
/// works across different backends (Docker, Kubernetes, etc.). It replaces
/// backend-specific types like `bollard::models::ContainerSummary` in the
/// `SandboxManager` trait, enabling true backend independence.
///
/// # Fields
///
/// - `id` - The container or pod identifier
/// - `name` - Human-readable name (if set)
/// - `image` - Image reference used to create the container/pod
/// - `state` - Current state (e.g., "running", "stopped", "exited")
/// - `status` - Additional human-readable status info
/// - `created` - Unix timestamp of creation time
/// - `ports` - Port mappings as (host_port, container_port) pairs
/// - `labels` - Key/value metadata labels
/// - `node_name` - Kubernetes node name (K8s backend only)
/// - `pod_ip` - Kubernetes pod IP address (K8s backend only)
///
/// # Example
///
/// ```rust
/// use dsb::core::types::SandboxInfo;
/// use std::collections::HashMap;
///
/// let info = SandboxInfo {
///     id: "abc123".to_string(),
///     name: Some("my-sandbox".to_string()),
///     image: Some("nginx:latest".to_string()),
///     state: Some("running".to_string()),
///     status: Some("Up 5 minutes".to_string()),
///     created: Some(1700000000),
///     ports: vec![(8080, 80)],
///     labels: HashMap::new(),
///     node_name: None,
///     pod_ip: None,
/// };
///
/// assert_eq!(info.id, "abc123");
/// assert_eq!(info.state.as_deref(), Some("running"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxInfo {
    /// Container or pod identifier
    pub id: String,

    /// Human-readable name (optional, e.g., "my-sandbox")
    pub name: Option<String>,

    /// Image reference used to create the container/pod
    pub image: Option<String>,

    /// Current state (e.g., "running", "stopped", "exited")
    pub state: Option<String>,

    /// Additional human-readable status info (e.g., "Up 5 minutes", "Exit 0")
    pub status: Option<String>,

    /// Unix timestamp of creation time
    pub created: Option<i64>,

    /// Port mappings as (host_port, container_port) pairs
    #[serde(default)]
    pub ports: Vec<(u16, u16)>,

    /// Key/value metadata labels
    #[serde(default)]
    pub labels: HashMap<String, String>,

    /// Kubernetes node name where the pod is scheduled (K8s backend only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,

    /// Kubernetes pod IP address (K8s backend only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod_ip: Option<String>,
}

/// Real-time progress events for sandbox operations.
///
/// These events are streamed via SSE to provide real-time feedback
/// during long-running operations like image pulls and container creation.
///
/// # Example
///
/// ```rust
/// use dsb::core::types::SandboxProgressEvent;
///
/// let event = SandboxProgressEvent::Pulling {
///     image: "nginx:latest".to_string(),
///     status: "Pulling fs layer".to_string(),
///     current: Some(1024000),
///     total: Some(5000000),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SandboxProgressEvent {
    /// Image is being pulled from registry
    #[serde(rename = "pulling")]
    Pulling {
        /// Image reference being pulled
        image: String,
        /// Status message (e.g., "Pulling fs layer", "Extracting")
        status: String,
        /// Current bytes downloaded (if available)
        #[serde(skip_serializing_if = "Option::is_none")]
        current: Option<u64>,
        /// Total bytes to download (if available)
        #[serde(skip_serializing_if = "Option::is_none")]
        total: Option<u64>,
    },

    /// Container is being created
    #[serde(rename = "creating")]
    Creating {
        /// Image used for container
        image: String,
    },

    /// Container is being started
    #[serde(rename = "starting")]
    Starting {
        /// Container ID
        container_id: String,
    },

    /// Sandbox is ready and running
    #[serde(rename = "ready")]
    Ready {
        /// Sandbox UUID
        sandbox_id: uuid::Uuid,
        /// Container ID
        container_id: String,
    },

    /// Operation failed
    #[serde(rename = "error")]
    Error {
        /// Error message
        message: String,
    },
}

/// Activity tracking for automatic cleanup.
///
/// Tracks sandbox activity to determine when it's safe to auto-cleanup
/// after a period of inactivity.
///
/// # Activity Types
///
/// - **API Activity** - API calls like exec, info, stats, etc.
/// - **Container Activity** - Actual container resource usage (optional)
///
/// # Example
///
/// ```rust
/// use dsb::core::types::ActivityTracking;
/// use chrono::Utc;
///
/// let activity = ActivityTracking {
///     last_api_activity: Utc::now(),
///     last_container_activity: Some(Utc::now()),
///     activity_count: 42,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityTracking {
    /// Timestamp of the last API activity
    ///
    /// Updated on every API call to the sandbox (exec, info, stats, etc.)
    pub last_api_activity: chrono::DateTime<chrono::Utc>,

    /// Timestamp of the last detected container activity
    ///
    /// Updated when the container shows resource usage (CPU, memory, etc.)
    /// May be None if container has been idle since creation.
    pub last_container_activity: Option<chrono::DateTime<chrono::Utc>>,

    /// Total number of API activities recorded
    ///
    /// Incremented on each API call. Useful for analytics.
    pub activity_count: u64,
}

/// Complete representation of a sandbox instance.
///
/// A Sandbox represents a running or stopped container with all its metadata,
/// configuration, state information, volume mounts, and activity tracking.
/// This is the primary domain object used throughout the DSB system.
///
/// # Lifecycle
///
/// 1. Sandbox is created with `state: Creating`
/// 2. Container is created -> `state: Created`
/// 3. Container starts -> `state: Running`
/// 4. Container stops -> `state: Stopped`
/// 5. Container is soft-deleted -> `deleted_at` is set, record preserved
///
/// # Soft Delete
///
/// When a sandbox is deleted, instead of removing it from the database,
/// it's marked as deleted with:
/// - `deleted_at`: Timestamp of deletion
/// - `deleted_by`: User/system that performed the deletion
///
/// This preserves complete history while maintaining query performance.
///
/// # Example
///
/// ```rust
/// use dsb::core::types::{Sandbox, SandboxConfig, SandboxState, ActivityTracking};
/// use uuid::Uuid;
/// use chrono::Utc;
///
/// let config = SandboxConfig::default();
/// let now = Utc::now();
///
/// let sandbox = Sandbox {
///     id: Uuid::new_v4(),
///     config: config.clone(),
///     state: SandboxState::Running,
///     container_id: Some("abc123".to_string()),
///     created_at: now,
///     updated_at: now,
///     error_message: None,
///     volume_mounts: vec![],
///     activity: ActivityTracking {
///         last_api_activity: now,
///         last_container_activity: None,
///         activity_count: 0,
///     },
///     inactivity_timeout_minutes: None,
///     deleted_at: None,
///     deleted_by: None,
///     api_key_id: None,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sandbox {
    /// Unique identifier for this sandbox (UUID v4)
    pub id: uuid::Uuid,

    /// Configuration used to create this sandbox
    pub config: SandboxConfig,

    /// Current state of the sandbox
    pub state: SandboxState,

    /// Docker container ID if created, None if not yet created
    pub container_id: Option<String>,

    /// Timestamp when the sandbox was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Timestamp of the last state update
    pub updated_at: chrono::DateTime<chrono::Utc>,

    /// Error message if state is Error, None otherwise
    pub error_message: Option<String>,

    /// Volume mounts attached to this sandbox
    ///
    /// Tracked for proper cleanup when sandbox is destroyed.
    pub volume_mounts: Vec<VolumeMount>,

    /// Activity tracking for auto-cleanup
    ///
    /// Tracks API and container activity to determine inactivity.
    pub activity: ActivityTracking,

    /// Auto-cleanup timeout in minutes (None = disabled)
    ///
    /// If set, sandbox will be auto-cleaned after this many minutes of inactivity.
    pub inactivity_timeout_minutes: Option<u64>,

    /// Timestamp when the sandbox was soft-deleted (None if not deleted)
    ///
    /// When set, the sandbox has been deleted but preserved for history.
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,

    /// User/system that performed the deletion (None if not deleted)
    ///
    /// Tracks who or what deleted the sandbox for audit purposes.
    pub deleted_by: Option<String>,

    /// API key that owns this sandbox (None for admin/legacy-created sandboxes)
    ///
    /// Used for multi-tenancy isolation. When set, only the owning API key
    /// (or admin/legacy keys) can access this sandbox.
    pub api_key_id: Option<uuid::Uuid>,
}

/// Request payload for creating a new sandbox.
///
/// This is the expected JSON structure for POST requests to `/sandboxes`.
/// All fields except `image` are optional to allow flexible creation with defaults.
///
/// # Example JSON Payload
///
/// ```json
/// {
///   "image": "nginx:latest",
///   "name": "my-web-server",
///   "environment": {
///     "ENV": "production"
///   },
///   "port_mappings": [
///     {
///       "host_port": 8080,
///       "container_port": 80,
///       "protocol": "tcp"
///     }
///   ],
///   "resource_limits": {
///     "memory_mb": 512,
///     "cpu_shares": 2048
///   },
///   "volumes": [
///     {
///       "type": "bind",
///       "host_path": "/tmp/data",
///       "container_path": "/data",
///       "read_only": false
///     }
///   ],
///   "inactivity_timeout_minutes": 30
/// }
/// ```
///
/// # Example Usage
///
/// ```rust
/// use dsb::core::types::CreateSandboxRequest;
/// use serde_json;
///
/// let json = r#"{
///   "image": "redis:alpine",
///   "name": "cache",
///   "volumes": [
///     {
///       "type": "named",
///       "name": "cache-data",
///       "container_path": "/data",
///       "read_only": false
///     }
///   ]
/// }"#;
///
/// let req: CreateSandboxRequest = serde_json::from_str(json).unwrap();
/// assert_eq!(req.image, "redis:alpine");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateSandboxRequest {
    /// Docker image to use (required)
    pub image: String,

    /// Optional sandbox name
    pub name: Option<String>,

    /// Optional environment variables
    pub environment: Option<HashMap<String, String>>,

    /// Optional port mappings
    pub port_mappings: Option<Vec<PortMapping>>,

    /// Optional resource limits
    pub resource_limits: Option<ResourceLimits>,

    /// Optional volume mounts
    pub volumes: Option<Vec<VolumeMount>>,

    /// Optional command to run in the container
    ///
    /// If specified, this command will be executed when the container starts.
    /// If not specified, defaults to `["tail", "-f", "/dev/null"]` to keep
    /// the container running for interactive operations.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "command": ["sleep", "infinity"]
    /// }
    /// ```
    pub command: Option<Vec<String>>,

    /// Optional auto-cleanup timeout in minutes
    pub inactivity_timeout_minutes: Option<u64>,

    /// Optional pull policy (defaults to PullPolicy::Missing)
    ///
    /// If not provided, uses the default behavior of pulling only when
    /// the image doesn't exist locally.
    #[serde(default)]
    pub pull_policy: PullPolicy,

    /// Feature profiles to enable from image metadata (e.g., ["vnc", "browser"])
    ///
    /// When the image has feature metadata labels, specific features can be
    /// enabled by name. Features will automatically configure ports, volumes,
    /// commands, and environment variables.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "features": ["vnc", "browser"]
    /// }
    /// ```
    #[serde(default)]
    pub features: Vec<String>,

    /// Enable all available features from image metadata
    ///
    /// When set to true, enables all features marked with `enabled_by_default: true`
    /// in the image metadata.
    ///
    /// # Example
    ///
    /// ```json
    /// {
    ///   "enable_all_features": true
    /// }
    /// ```
    #[serde(default)]
    pub enable_all_features: bool,

    /// VNC resolution (optional)
    ///
    /// If specified, sets the VNC display resolution. Format: "WIDTHxHEIGHT"
    /// Examples: "1280x720", "1920x1080", "2560x1440", "3840x2160"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vnc_resolution: Option<String>,
}

impl CreateSandboxRequest {
    /// Validates the request, returning an error if invalid.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The image field is empty or contains only whitespace
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.image.trim().is_empty() {
            return Err(ApiError::Validation {
                message: "image cannot be empty".to_string(),
                field: Some("image".to_string()),
                code: ErrorCode::ValidationError,
            });
        }
        Ok(())
    }
}

/// Kubernetes-specific sandbox information.
///
/// This struct contains Kubernetes-specific details about a sandbox,
/// including node assignment, pod IP, and service name. It is only
/// populated when the backend is Kubernetes.
///
/// # Example
///
/// ```rust
/// use dsb::core::types::KubernetesInfo;
///
/// let info = KubernetesInfo {
///     node_name: Some("node-1".to_string()),
///     pod_ip: Some("10.0.0.1".to_string()),
///     service_name: Some("dsb-svc-abc123".to_string()),
///     message: None,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KubernetesInfo {
    /// Name of the Kubernetes node where the pod is scheduled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,

    /// IP address of the Kubernetes pod
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pod_ip: Option<String>,

    /// Name of the Kubernetes Service exposing the sandbox
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,

    /// Human-readable message about the current status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Response payload for sandbox information.
///
/// This is the JSON structure returned by GET requests to `/sandboxes/{id}`.
/// It contains a subset of [`Sandbox`] fields appropriate for API responses.
///
/// # Example Response
///
/// ```json
/// {
///   "id": "550e8400-e29b-41d4-a716-446655440000",
///   "state": "running",
///   "config": {
///     "image": "nginx:latest"
///   },
///   "container_id": "abc123def456",
///   "created_at": "2025-12-28T00:00:00Z",
///   "updated_at": "2025-12-28T00:00:00Z",
///   "deleted_at": null,
///   "deleted_by": null,
///   "kubernetes": {
///     "node_name": "node-1",
///     "pod_ip": "10.0.0.1",
///     "service_name": "dsb-svc-abc123"
///   }
/// }
/// ```
///
/// # Example Conversion
///
/// ```rust
/// use dsb::core::types::{Sandbox, SandboxConfig, SandboxState, SandboxResponse, ActivityTracking, KubernetesInfo};
/// use uuid::Uuid;
/// use chrono::Utc;
///
/// let now = Utc::now();
/// let sandbox = Sandbox {
///     id: Uuid::new_v4(),
///     config: SandboxConfig::default(),
///     state: SandboxState::Running,
///     container_id: Some("abc123".to_string()),
///     created_at: now,
///     updated_at: now,
///     error_message: None,
///     volume_mounts: vec![],
///     activity: ActivityTracking {
///         last_api_activity: now,
///         last_container_activity: None,
///         activity_count: 0,
///     },
///     inactivity_timeout_minutes: None,
///     deleted_at: None,
///     deleted_by: None,
///     api_key_id: None,
/// };
///
/// let response: SandboxResponse = sandbox.clone().into();
/// assert_eq!(response.id, sandbox.id);
/// assert_eq!(response.state, sandbox.state);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResponse {
    /// Unique identifier for the sandbox
    pub id: uuid::Uuid,

    /// Current state of the sandbox
    pub state: SandboxState,

    /// Sandbox configuration
    pub config: SandboxConfig,

    /// Docker container ID if available
    pub container_id: Option<String>,

    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Last update timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,

    /// Soft delete timestamp (None if not deleted)
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,

    /// User/system that performed the deletion (None if not deleted)
    pub deleted_by: Option<String>,

    /// API key that owns this sandbox (None for admin/legacy-created sandboxes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_id: Option<uuid::Uuid>,

    /// Kubernetes-specific sandbox information (only populated when backend is kubernetes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kubernetes: Option<KubernetesInfo>,
}

impl From<Sandbox> for SandboxResponse {
    fn from(sandbox: Sandbox) -> Self {
        Self {
            id: sandbox.id,
            state: sandbox.state,
            config: sandbox.config,
            container_id: sandbox.container_id,
            created_at: sandbox.created_at,
            updated_at: sandbox.updated_at,
            deleted_at: sandbox.deleted_at,
            deleted_by: sandbox.deleted_by,
            api_key_id: sandbox.api_key_id,
            kubernetes: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_state_serialization() {
        let state = SandboxState::Running;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"running\"");

        let deserialized: SandboxState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, SandboxState::Running);
    }

    #[test]
    fn test_sandbox_config_default() {
        let config = SandboxConfig::default();
        assert_eq!(config.image, "nginx:latest");
        assert!(config.name.is_none());
        assert!(config.environment.is_empty());
        assert!(config.port_mappings.is_empty());
        assert!(config.volumes.is_empty());
        assert!(config.inactivity_timeout_minutes.is_none());
    }

    #[test]
    fn test_port_mapping_serialization() {
        let mapping = PortMapping {
            host_port: 8080,
            container_port: 80,
            protocol: PortProtocol::Tcp,
        };

        let json = serde_json::to_string(&mapping).unwrap();
        assert!(json.contains("8080"));
        assert!(json.contains("80"));
        assert!(json.contains("tcp"));
    }

    #[test]
    fn test_sandbox_to_response() {
        use chrono::Utc;

        let sandbox = Sandbox {
            id: uuid::Uuid::new_v4(),
            config: SandboxConfig::default(),
            state: SandboxState::Running,
            container_id: Some("test-container".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            error_message: None,
            volume_mounts: vec![],
            activity: ActivityTracking {
                last_api_activity: Utc::now(),
                last_container_activity: None,
                activity_count: 0,
            },
            inactivity_timeout_minutes: None,
            deleted_at: None,
            deleted_by: None,
            api_key_id: None,
        };

        let response: SandboxResponse = sandbox.clone().into();
        assert_eq!(response.id, sandbox.id);
        assert_eq!(response.state, sandbox.state);
        assert_eq!(response.container_id, sandbox.container_id);
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert!(limits.memory_mb.is_none());
        assert!(limits.cpu_quota.is_none());
        assert!(limits.cpu_period.is_none());
        assert!(limits.cpu_shares.is_none());
        assert!(limits.pids_limit.is_none());
        assert!(limits.ulimits.is_none() || limits.ulimits.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_resource_limits_with_cpu_shares() {
        let limits = ResourceLimits {
            cpu_shares: Some(2048),
            ..Default::default()
        };

        assert_eq!(limits.cpu_shares, Some(2048));
        let json = serde_json::to_string(&limits).unwrap();
        assert!(json.contains("cpu_shares"));
        assert!(json.contains("2048"));
    }

    #[test]
    fn test_volume_mount_bind_serialization() {
        let mount = VolumeMount::Bind {
            host_path: "/tmp/data".to_string(),
            container_path: "/data".to_string(),
            read_only: false,
        };

        let json = serde_json::to_string(&mount).unwrap();
        assert!(json.contains("\"type\":\"bind\""));
        assert!(json.contains("/tmp/data"));
        assert!(json.contains("/data"));
    }

    #[test]
    fn test_volume_mount_named_serialization() {
        let mount = VolumeMount::Named {
            name: "my-volume".to_string(),
            container_path: "/data".to_string(),
            read_only: true,
        };

        let json = serde_json::to_string(&mount).unwrap();
        assert!(json.contains("\"type\":\"named\""));
        assert!(json.contains("my-volume"));
        assert!(json.contains("true")); // read_only
    }

    #[test]
    fn test_volume_mount_deserialization() {
        let json = r#"{
            "type": "bind",
            "host_path": "/host/path",
            "container_path": "/container/path",
            "read_only": false
        }"#;

        let mount: VolumeMount = serde_json::from_str(json).unwrap();
        match mount {
            VolumeMount::Bind {
                host_path,
                container_path,
                read_only,
            } => {
                assert_eq!(host_path, "/host/path");
                assert_eq!(container_path, "/container/path");
                assert!(!read_only);
            }
            _ => panic!("Expected Bind variant"),
        }
    }

    #[test]
    fn test_ulimit_serialization() {
        let ulimit = Ulimit {
            name: "nofile".to_string(),
            soft: 65536,
            hard: 65536,
        };

        let json = serde_json::to_string(&ulimit).unwrap();
        assert!(json.contains("nofile"));
        assert!(json.contains("65536"));

        let deserialized: Ulimit = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "nofile");
        assert_eq!(deserialized.soft, 65536);
        assert_eq!(deserialized.hard, 65536);
    }

    #[test]
    fn test_container_stats_serialization() {
        let stats = ContainerStats {
            cpu_percent: 45.5,
            memory_usage_mb: 256,
            memory_limit_mb: 512,
            memory_percent: 50.0,
            network_rx_bytes: 1024000,
            network_tx_bytes: 512000,
            block_read_bytes: 2048000,
            block_write_bytes: 1024000,
            timestamp: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("cpu_percent"));
        assert!(json.contains("45.5"));

        let deserialized: ContainerStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.cpu_percent, 45.5);
        assert_eq!(deserialized.memory_usage_mb, 256);
    }

    #[test]
    fn test_activity_tracking_serialization() {
        let activity = ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: Some(chrono::Utc::now()),
            activity_count: 42,
        };

        let json = serde_json::to_string(&activity).unwrap();
        assert!(json.contains("activity_count"));
        assert!(json.contains("42"));

        let deserialized: ActivityTracking = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.activity_count, 42);
        assert!(deserialized.last_container_activity.is_some());
    }

    #[test]
    fn test_create_sandbox_request_deserialization() {
        let json = r#"{
            "image": "nginx:latest",
            "name": "test",
            "environment": {"KEY": "value"}
        }"#;

        let req: CreateSandboxRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.image, "nginx:latest");
        assert_eq!(req.name, Some("test".to_string()));
        assert!(req.environment.is_some());
    }

    #[test]
    fn test_create_sandbox_request_with_volumes() {
        let json = r#"{
            "image": "nginx:latest",
            "volumes": [
                {
                    "type": "bind",
                    "host_path": "/tmp/data",
                    "container_path": "/data",
                    "read_only": false
                }
            ],
            "inactivity_timeout_minutes": 30
        }"#;

        let req: CreateSandboxRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.image, "nginx:latest");
        assert!(req.volumes.is_some());
        assert_eq!(req.volumes.as_ref().unwrap().len(), 1);
        assert_eq!(req.inactivity_timeout_minutes, Some(30));
    }

    #[test]
    fn test_sandbox_config_with_all_fields() {
        let config = SandboxConfig {
            image: "redis:alpine".to_string(),
            name: Some("cache".to_string()),
            environment: {
                let mut map = HashMap::new();
                map.insert("KEY".to_string(), "value".to_string());
                map
            },
            port_mappings: vec![],
            exposed_ports: vec![],
            resource_limits: ResourceLimits {
                memory_mb: Some(512),
                cpu_shares: Some(2048),
                ..Default::default()
            },
            volumes: vec![VolumeMount::Named {
                name: "data".to_string(),
                container_path: "/data".to_string(),
                read_only: false,
            }],
            command: Some(vec![
                "redis-server".to_string(),
                "--save".to_string(),
                "".to_string(),
            ]),
            inactivity_timeout_minutes: Some(60),
            pull_policy: PullPolicy::Always,
            features: vec![],
            enable_all_features: false,
            vnc_resolution: None,
        };

        assert_eq!(config.image, "redis:alpine");
        assert_eq!(config.volumes.len(), 1);
        assert_eq!(config.inactivity_timeout_minutes, Some(60));
        assert_eq!(config.pull_policy, PullPolicy::Always);
    }

    #[test]
    fn test_pull_policy_default() {
        let policy = PullPolicy::default();
        assert_eq!(policy, PullPolicy::Missing);
    }

    #[test]
    fn test_pull_policy_serialization() {
        let policy = PullPolicy::Always;
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(json, "\"always\"");

        let deserialized: PullPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, PullPolicy::Always);
    }

    #[test]
    fn test_pull_policy_missing_serialization() {
        let policy = PullPolicy::Missing;
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(json, "\"missing\"");

        let deserialized: PullPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, PullPolicy::Missing);
    }

    #[test]
    fn test_pull_policy_never_serialization() {
        let policy = PullPolicy::Never;
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(json, "\"never\"");

        let deserialized: PullPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, PullPolicy::Never);
    }

    #[test]
    fn test_sandbox_config_with_pull_policy() {
        let config = SandboxConfig {
            pull_policy: PullPolicy::Never,
            ..Default::default()
        };
        assert_eq!(config.pull_policy, PullPolicy::Never);
    }

    #[test]
    fn test_sandbox_config_default_pull_policy() {
        let config = SandboxConfig::default();
        assert_eq!(config.pull_policy, PullPolicy::Missing);
    }

    #[test]
    fn test_create_sandbox_request_with_pull_policy() {
        let json = r#"{
            "image": "nginx:latest",
            "pull_policy": "always"
        }"#;

        let req: CreateSandboxRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.image, "nginx:latest");
        assert_eq!(req.pull_policy, PullPolicy::Always);
    }

    #[test]
    fn test_create_sandbox_request_default_pull_policy() {
        let json = r#"{
            "image": "nginx:latest"
        }"#;

        let req: CreateSandboxRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.image, "nginx:latest");
        assert_eq!(req.pull_policy, PullPolicy::Missing); // Default
    }

    #[test]
    fn test_progress_event_serialization() {
        let event = SandboxProgressEvent::Pulling {
            image: "nginx:latest".to_string(),
            status: "Downloading".to_string(),
            current: Some(1024),
            total: Some(4096),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"pulling\""));
        assert!(json.contains("1024"));
        assert!(json.contains("Downloading"));
    }

    #[test]
    fn test_progress_event_deserialization() {
        let json = r#"{
            "type": "pulling",
            "image": "nginx:latest",
            "status": "Downloading",
            "current": 1024,
            "total": 4096
        }"#;

        let event: SandboxProgressEvent = serde_json::from_str(json).unwrap();
        match event {
            SandboxProgressEvent::Pulling {
                image,
                status,
                current,
                total,
            } => {
                assert_eq!(image, "nginx:latest");
                assert_eq!(status, "Downloading");
                assert_eq!(current, Some(1024));
                assert_eq!(total, Some(4096));
            }
            _ => panic!("Expected Pulling event"),
        }
    }

    #[test]
    fn test_sandbox_config_with_command() {
        let config = SandboxConfig {
            image: "nginx:latest".to_string(),
            command: Some(vec![
                "nginx".to_string(),
                "-g".to_string(),
                "daemon off;".to_string(),
            ]),
            ..Default::default()
        };
        assert_eq!(config.image, "nginx:latest");
        assert_eq!(
            config.command,
            Some(vec![
                "nginx".to_string(),
                "-g".to_string(),
                "daemon off;".to_string()
            ])
        );
    }

    #[test]
    fn test_sandbox_config_without_command() {
        let config = SandboxConfig {
            image: "alpine:latest".to_string(),
            ..Default::default()
        };
        assert_eq!(config.command, None);
    }

    #[test]
    fn test_create_sandbox_request_with_command() {
        let json = r#"{
            "image": "nginx:latest",
            "command": ["nginx", "-g", "daemon off;"]
        }"#;

        let req: CreateSandboxRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.image, "nginx:latest");
        assert_eq!(
            req.command,
            Some(vec![
                "nginx".to_string(),
                "-g".to_string(),
                "daemon off;".to_string()
            ])
        );
    }

    #[test]
    fn test_create_sandbox_request_without_command() {
        let json = r#"{
            "image": "python:3.12"
        }"#;

        let req: CreateSandboxRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.image, "python:3.12");
        assert_eq!(req.command, None);
    }

    #[test]
    fn test_sandbox_config_command_serialization() {
        let config = SandboxConfig {
            image: "redis:alpine".to_string(),
            command: Some(vec!["redis-server".to_string()]),
            ..Default::default()
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"command\":[\"redis-server\"]"));
    }

    #[test]
    fn test_sandbox_config_default_includes_command() {
        let config = SandboxConfig::default();
        assert_eq!(config.command, None);
    }

    #[test]
    fn test_activity_type_create_serialization() {
        let activity_type = ActivityType::Create;
        let json = serde_json::to_string(&activity_type).unwrap();
        assert_eq!(json, "\"create\"");
    }

    #[test]
    fn test_activity_type_all_variants() {
        let types = vec![
            ActivityType::Create,
            ActivityType::Delete,
            ActivityType::Exec,
            ActivityType::Stats,
            ActivityType::Stop,
            ActivityType::Cleanup,
            ActivityType::Info,
            ActivityType::ContainerActivity,
        ];

        for activity_type in types {
            let json = serde_json::to_string(&activity_type).unwrap();
            let deserialized: ActivityType = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, activity_type);
        }
    }

    #[test]
    fn test_sandbox_activity_creation() {
        use chrono::Utc;

        let activity = SandboxActivity {
            id: uuid::Uuid::new_v4(),
            sandbox_id: uuid::Uuid::new_v4(),
            activity_type: ActivityType::Create,
            timestamp: Utc::now(),
            details: serde_json::json!({"test": "data"}),
            sandbox_is_deleted: false,
        };

        assert_eq!(activity.activity_type, ActivityType::Create);
        assert!(!activity.sandbox_is_deleted);
    }

    #[test]
    fn test_activity_response_from_sandbox_activity() {
        use chrono::Utc;

        let sandbox_activity = SandboxActivity {
            id: uuid::Uuid::new_v4(),
            sandbox_id: uuid::Uuid::new_v4(),
            activity_type: ActivityType::Exec,
            timestamp: Utc::now(),
            details: serde_json::json!({"command": "ls"}),
            sandbox_is_deleted: false,
        };

        let activity_id = sandbox_activity.id;
        let response: ActivityResponse = sandbox_activity.into();
        assert_eq!(response.activity_type, ActivityType::Exec);
        assert_eq!(response.id, activity_id);
    }

    #[test]
    fn test_ssh_session_state_serialization() {
        let state = SshSessionState::Active;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"active\"");

        let deserialized: SshSessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, SshSessionState::Active);
    }

    #[test]
    fn test_ssh_session_all_states() {
        let states = vec![
            SshSessionState::Connecting,
            SshSessionState::Active,
            SshSessionState::Disconnected,
            SshSessionState::Terminated,
        ];

        for state in states {
            let json = serde_json::to_string(&state).unwrap();
            let deserialized: SshSessionState = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, state);
        }
    }

    #[test]
    fn test_ssh_auth_method_serialization() {
        let methods = vec![SshAuthMethod::ApiKey, SshAuthMethod::Certificate];

        for method in methods {
            let json = serde_json::to_string(&method).unwrap();
            let deserialized: SshAuthMethod = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, method);
        }
    }

    #[test]
    fn test_ssh_session_filters_default() {
        let filters = SshSessionFilters::default();
        assert!(filters.sandbox_id.is_none());
        assert!(filters.state.is_none());
        assert!(filters.limit.is_none());
        assert!(filters.offset.is_none());
    }

    #[test]
    fn test_create_ssh_session_request() {
        let request = CreateSshSessionRequest {
            sandbox_id: uuid::Uuid::new_v4(),
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH".to_string()),
            auth_method: SshAuthMethod::ApiKey,
            username: None,
            public_key: None,
        };

        assert_eq!(request.client_ip, "127.0.0.1");
        assert!(request.ssh_version.is_some());
    }

    #[test]
    fn test_ssh_session_response_fields() {
        use chrono::Utc;

        let session = SshSession {
            id: uuid::Uuid::new_v4(),
            sandbox_id: uuid::Uuid::new_v4(),
            client_ip: "192.168.1.1".to_string(),
            ssh_version: None,
            auth_method: SshAuthMethod::ApiKey,
            ssh_session_id: None,
            exec_id: None,
            pty_term: Some("xterm-256color".to_string()),
            pty_rows: Some(24),
            pty_cols: Some(80),
            state: SshSessionState::Active,
            connected_at: Utc::now(),
            disconnected_at: None,
            last_activity_at: Utc::now(),
            bytes_sent: 1024,
            bytes_received: 2048,
            duration_seconds: Some(60),
            termination_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let response: SshSessionResponse = session.into();
        assert_eq!(response.state, SshSessionState::Active);
        assert_eq!(response.bytes_sent, 1024);
        assert_eq!(response.bytes_received, 2048);
    }
}

/// Type of sandbox activity being recorded.
///
/// Each variant represents a different type of operation that can be performed on a sandbox.
/// This enables detailed tracking and auditing of all sandbox interactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityType {
    /// Sandbox was created
    Create,
    /// Sandbox was deleted
    Delete,
    /// Sandbox was restored from deleted state
    Restore,
    /// Command was executed in sandbox
    Exec,
    /// Container stats were retrieved
    Stats,
    /// Sandbox was stopped
    Stop,
    /// Sandbox was started
    Start,
    /// Sandbox was cleaned up (forced cleanup)
    Cleanup,
    /// API call to get sandbox info
    Info,
    /// Container activity detected (via stats showing resource usage)
    ContainerActivity,
    /// File was uploaded to sandbox
    Upload,
    /// File was downloaded from sandbox
    Download,
}

/// Individual activity record for a sandbox.
///
/// This struct represents a single activity event in the activity log.
/// Activities are recorded for audit purposes, troubleshooting, and
/// determining sandbox inactivity for auto-cleanup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxActivity {
    /// Unique activity identifier
    pub id: uuid::Uuid,

    /// Sandbox this activity belongs to
    pub sandbox_id: uuid::Uuid,

    /// Type of activity
    pub activity_type: ActivityType,

    /// When the activity occurred
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Additional activity-specific details (JSONB for flexibility)
    pub details: serde_json::Value,

    /// Whether the sandbox has been deleted (preserves activity history)
    pub sandbox_is_deleted: bool,
}

/// Response type for activity queries.
///
/// This is a simplified version of `SandboxActivity` used in API responses
/// to avoid exposing internal implementation details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityResponse {
    /// Activity record ID
    pub id: uuid::Uuid,
    /// Associated sandbox ID
    pub sandbox_id: uuid::Uuid,
    /// Type of activity (create, exec, delete, etc.)
    pub activity_type: ActivityType,
    /// When the activity occurred
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Additional activity details as JSON
    pub details: serde_json::Value,
}

impl From<SandboxActivity> for ActivityResponse {
    fn from(activity: SandboxActivity) -> Self {
        Self {
            id: activity.id,
            sandbox_id: activity.sandbox_id,
            activity_type: activity.activity_type,
            timestamp: activity.timestamp,
            details: activity.details,
        }
    }
}

/// Represents the current state of an SSH session.
///
/// # Lifecycle Flow
///
/// SSH sessions follow this state transition flow:
///
/// ```text
/// Connecting → Active → Disconnected → Terminated
///                ↓
///              Error
/// ```
///
/// # States
///
/// - **Connecting** - Initial SSH handshake and authentication in progress
/// - **Active** - SSH session established, PTY allocated, data flowing
/// - **Disconnected** - Client disconnected cleanly
/// - **Terminated** - Server terminated the session
/// - **Error** - Abnormal termination due to error
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SshSessionState {
    /// SSH session is being established
    Connecting,
    /// SSH session is active and data is flowing
    Active,
    /// Client disconnected cleanly
    Disconnected,
    /// Server terminated the session
    Terminated,
    /// Session terminated due to error
    Error,
}

impl SshSessionState {
    /// Returns the string representation of the state for database storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Active => "active",
            Self::Disconnected => "disconnected",
            Self::Terminated => "terminated",
            Self::Error => "error",
        }
    }

    /// Returns true if the session is currently active.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Returns true if the session is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Disconnected | Self::Terminated | Self::Error)
    }
}

/// Authentication method used for SSH session.
///
/// Currently only API key authentication is supported, but the enum
/// allows for future expansion to certificate-based authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshAuthMethod {
    /// Authentication via DSB API key (X-API-Key header)
    ApiKey,
    /// Authentication via SSH certificate (future enhancement)
    Certificate,
}

impl SshAuthMethod {
    /// Returns the string representation for database storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::Certificate => "certificate",
        }
    }
}

/// SSH session record representing an active or terminated SSH connection to a sandbox.
///
/// This struct tracks the complete lifecycle of an SSH session from connection
/// to termination, including authentication details, PTY information, and statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshSession {
    /// Unique session identifier
    pub id: uuid::Uuid,

    /// Sandbox this session connects to
    pub sandbox_id: uuid::Uuid,

    /// Client IP address
    pub client_ip: String,

    /// Client SSH version string (optional)
    pub ssh_version: Option<String>,

    /// Authentication method used
    pub auth_method: SshAuthMethod,

    /// SSH protocol session ID (from russh)
    pub ssh_session_id: Option<String>,

    /// Docker exec instance ID
    pub exec_id: Option<String>,

    /// PTY terminal type (e.g., "xterm-256color")
    pub pty_term: Option<String>,

    /// PTY rows
    pub pty_rows: Option<i32>,

    /// PTY columns
    pub pty_cols: Option<i32>,

    /// Session state
    pub state: SshSessionState,

    /// When the session was established
    pub connected_at: chrono::DateTime<chrono::Utc>,

    /// When the session was disconnected (if applicable)
    pub disconnected_at: Option<chrono::DateTime<chrono::Utc>>,

    /// Last activity timestamp (for idle timeout detection)
    pub last_activity_at: chrono::DateTime<chrono::Utc>,

    /// Bytes sent from server to client
    pub bytes_sent: i64,

    /// Bytes received from client
    pub bytes_received: i64,

    /// Session duration in seconds (calculated on disconnect)
    pub duration_seconds: Option<i32>,

    /// Reason for termination (if applicable)
    pub termination_reason: Option<String>,

    /// When the record was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// When the record was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Response type for SSH session queries.
///
/// Simplified version of `SshSession` for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshSessionResponse {
    /// Session ID
    pub id: uuid::Uuid,
    /// Associated sandbox ID
    pub sandbox_id: uuid::Uuid,
    /// Current session state
    pub state: SshSessionState,
    /// Client IP address
    pub client_ip: String,
    /// Terminal type (e.g., "xterm-256color")
    pub pty_term: Option<String>,
    /// Terminal rows
    pub pty_rows: Option<i32>,
    /// Terminal columns
    pub pty_cols: Option<i32>,
    /// Connection timestamp
    pub connected_at: chrono::DateTime<chrono::Utc>,
    /// Disconnection timestamp (if disconnected)
    pub disconnected_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Last activity timestamp
    pub last_activity_at: chrono::DateTime<chrono::Utc>,
    /// Cumulative bytes sent to client
    pub bytes_sent: i64,
    /// Cumulative bytes received from client
    pub bytes_received: i64,
    /// Session duration in seconds
    pub duration_seconds: Option<i32>,
    /// Reason for termination (if applicable)
    pub termination_reason: Option<String>,
}

impl From<SshSession> for SshSessionResponse {
    fn from(session: SshSession) -> Self {
        Self {
            id: session.id,
            sandbox_id: session.sandbox_id,
            state: session.state,
            client_ip: session.client_ip,
            pty_term: session.pty_term,
            pty_rows: session.pty_rows,
            pty_cols: session.pty_cols,
            connected_at: session.connected_at,
            disconnected_at: session.disconnected_at,
            last_activity_at: session.last_activity_at,
            bytes_sent: session.bytes_sent,
            bytes_received: session.bytes_received,
            duration_seconds: session.duration_seconds,
            termination_reason: session.termination_reason,
        }
    }
}

/// Request to create an SSH session.
///
/// This request is made by the SSH gateway when establishing a new SSH connection.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSshSessionRequest {
    /// Sandbox to connect to
    pub sandbox_id: uuid::Uuid,

    /// Client IP address
    pub client_ip: String,

    /// SSH protocol version (optional)
    pub ssh_version: Option<String>,

    /// Authentication method used
    pub auth_method: SshAuthMethod,

    /// SSH username (optional, for certificate-based auth)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    /// SSH public key (optional, for certificate-based auth)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
}

impl CreateSshSessionRequest {
    /// Validate the SSH session request.
    ///
    /// Checks for empty username and invalid public key format.
    pub fn validate(&self) -> Result<(), ApiError> {
        // Validate username if present
        if let Some(username) = &self.username {
            if username.trim().is_empty() {
                return Err(ApiError::Validation {
                    message: "SSH username cannot be empty".to_string(),
                    field: Some("username".to_string()),
                    code: ErrorCode::ValidationError,
                });
            }
        }

        // Validate public key format if present
        if let Some(public_key) = &self.public_key {
            let trimmed = public_key.trim();
            if trimmed.is_empty() {
                return Err(ApiError::Validation {
                    message: "SSH public key cannot be empty".to_string(),
                    field: Some("public_key".to_string()),
                    code: ErrorCode::ValidationError,
                });
            }

            // Basic SSH public key format validation
            // Valid SSH public keys should start with one of these prefixes
            let valid_prefixes = [
                "ssh-rsa",
                "ssh-ed25519",
                "ecdsa-sha2-nistp256",
                "ecdsa-sha2-nistp384",
                "ecdsa-sha2-nistp521",
                "ssh-dss",
            ];

            let has_valid_prefix = valid_prefixes
                .iter()
                .any(|prefix| trimmed.starts_with(prefix));

            if !has_valid_prefix {
                return Err(ApiError::Validation {
                    message: "Invalid SSH public key format. Must be a valid SSH public key (e.g., ssh-rsa, ssh-ed25519, ecdsa-sha2-nistp256)".to_string(),
                    field: Some("public_key".to_string()),
                    code: ErrorCode::ValidationError,
                });
            }
        }

        Ok(())
    }
}

/// Filters for listing SSH sessions.
#[derive(Debug, Clone, Default)]
pub struct SshSessionFilters {
    /// Filter by sandbox ID
    pub sandbox_id: Option<uuid::Uuid>,

    /// Filter by session state
    pub state: Option<SshSessionState>,

    /// Maximum number of results
    pub limit: Option<usize>,

    /// Offset for pagination
    pub offset: Option<usize>,
}
