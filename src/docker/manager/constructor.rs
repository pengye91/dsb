// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! DockerManager constructors and connection setup.

use std::path::Path;
use std::sync::Arc;
use bollard::Docker;
use bollard::API_DEFAULT_VERSION;
use tracing::{debug, warn};
use crate::config::Config;
use super::{DockerManager, DockerManagerError};

impl DockerManager {
    /// Builds an HTTP client with configured timeouts
    ///
    /// This creates a reqwest::Client with proper timeout settings from the
    /// configuration to prevent indefinite hangs when communicating with
    /// sandbox containers.
    ///
    /// # Critical Timeouts
    ///
    /// - **connect_timeout**: Maximum time to establish TCP connection
    /// - **read_timeout**: Maximum time to read response data (prevents hangs!)
    /// - **pool_idle_timeout**: How long idle connections stay in pool
    fn build_http_client(config: &Config) -> Result<reqwest::Client, DockerManagerError> {
        use std::time::Duration;

        let timeouts = &config.docker.http_client;

        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(timeouts.connect_timeout_secs))
            .read_timeout(Duration::from_secs(timeouts.read_timeout_secs))
            .pool_idle_timeout(Duration::from_secs(timeouts.pool_idle_timeout_secs))
            .build()
            .map_err(|e| DockerManagerError::Http(format!("Failed to create HTTP client: {}", e)))
    }

    /// Creates a new Docker manager connected to the local Docker daemon.
    ///
    /// This method connects using the platform's default mechanism:
    /// - UNIX socket on Linux/Mac
    /// - Named pipe on Windows
    ///
    /// This is a convenience method that uses default configuration.
    /// For custom Docker host configuration, use [`new_with_config`](Self::new_with_config).
    ///
    /// # Returns
    ///
    /// - `Ok(DockerManager)` - Successfully connected to Docker daemon
    /// - `Err(...)` - Failed to connect (daemon not running, permissions issue, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Docker daemon is not running
    /// - User lacks permission to access Docker
    /// - Socket/pipe file doesn't exist
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # fn main() -> Result<(), dsb::docker::DockerManagerError> {
    /// let docker = DockerManager::new()?;
    /// println!("Connected to Docker");
    /// # Ok(())
    /// # }
    /// ```
    pub fn new() -> Result<Self, DockerManagerError> {
        Self::new_with_config(&Config::default())
    }

    /// Creates a new Docker manager with custom configuration.
    ///
    /// This method allows specifying the Docker daemon connection via configuration:
    /// - Uses `config.docker.host` if provided
    /// - Falls back to `DOCKER_HOST` environment variable
    /// - Uses platform-specific defaults if neither is set
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    ///
    /// # Returns
    ///
    /// - `Ok(DockerManager)` - Successfully connected to Docker daemon
    /// - `Err(...)` - Failed to connect (daemon not running, permissions issue, etc.)
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # use dsb::config;
    /// # fn main() -> Result<(), dsb::docker::DockerManagerError> {
    /// let config = config::load()?;
    /// let docker = DockerManager::new_with_config(&config)?;
    /// println!("Connected to Docker");
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_with_config(config: &Config) -> Result<Self, DockerManagerError> {
        // Priority order:
        // 1. config.docker.host (from config file or env var)
        // 2. DOCKER_HOST environment variable
        // 3. Platform-specific defaults

        // When running in a Docker container, prefer the mounted Docker socket
        // This is detected by checking if /var/run/docker.sock exists
        let container_socket = Path::new("/var/run/docker.sock");
        if container_socket.exists() {
            debug!("Detected running in container, using mounted Docker socket at: /var/run/docker.sock");
            let docker =
                Docker::connect_with_unix("/var/run/docker.sock", 120, API_DEFAULT_VERSION)?;
            return Ok(Self {
                docker: Arc::new(docker),
                config: Arc::new(config.clone()),
                http_client: Self::build_http_client(config)?,
                ip_cache: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            });
        }

        let docker_host = config
            .docker
            .host
            .clone()
            .or_else(|| std::env::var("DOCKER_HOST").ok());

        if let Some(host) = docker_host {
            debug!("Connecting to Docker at: {}", host);

            // Parse the host string to determine connection type
            if host.starts_with("unix://") {
                // Unix socket connection
                let socket_path = host.trim_start_matches("unix://");
                let path = Path::new(socket_path);

                if !path.exists() {
                    warn!("Docker socket path does not exist: {}", socket_path);
                    warn!("Falling back to default connection");
                } else {
                    let docker = Docker::connect_with_unix(
                        socket_path,
                        120, // 120 second timeout
                        API_DEFAULT_VERSION,
                    )?;
                    return Ok(Self {
                        docker: Arc::new(docker),
                        config: Arc::new(config.clone()),
                        http_client: Self::build_http_client(config)?,
                        ip_cache: Arc::new(
                            std::sync::RwLock::new(std::collections::HashMap::new()),
                        ),
                    });
                }
            } else if host.starts_with("tcp://") {
                // TCP connection
                let docker = Docker::connect_with_http(
                    host.trim_start_matches("tcp://"),
                    120, // 120 second timeout
                    API_DEFAULT_VERSION,
                )?;
                return Ok(Self {
                    docker: Arc::new(docker),
                    config: Arc::new(config.clone()),
                    http_client: Self::build_http_client(config)?,
                    ip_cache: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
                });
            } else if host.starts_with("http://") || host.starts_with("https://") {
                // HTTP/HTTPS connection
                let docker = Docker::connect_with_http(
                    &host[7..], // Remove protocol prefix
                    120,
                    API_DEFAULT_VERSION,
                )?;
                return Ok(Self {
                    docker: Arc::new(docker),
                    config: Arc::new(config.clone()),
                    http_client: Self::build_http_client(config)?,
                    ip_cache: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
                });
            } else {
                // Assume it's a Unix socket path without protocol
                let path = Path::new(&host);
                if path.exists() {
                    let socket_path = path.to_str().ok_or_else(|| {
                        DockerManagerError::InvalidConfig(
                            "Docker host path contains invalid UTF-8".to_string(),
                        )
                    })?;
                    let docker = Docker::connect_with_unix(
                        socket_path,
                        120,
                        API_DEFAULT_VERSION,
                    )?;
                    return Ok(Self {
                        docker: Arc::new(docker),
                        config: Arc::new(config.clone()),
                        http_client: Self::build_http_client(config)?,
                        ip_cache: Arc::new(
                            std::sync::RwLock::new(std::collections::HashMap::new()),
                        ),
                    });
                }
            }
        }

        // Fall back to platform-specific defaults
        #[cfg(target_os = "macos")]
        {
            // Try Docker Desktop socket for macOS
            // Use config.docker.home_dir if set, otherwise fall back to $HOME env var
            let home = config
                .docker
                .home_dir
                .as_ref()
                .cloned()
                .or_else(|| std::env::var("HOME").ok());
            if let Some(home) = home {
                let docker_socket = Path::new(&home).join(".docker/run/docker.sock");
                if docker_socket.exists() {
                    debug!(
                        "Using Docker Desktop socket at: {}",
                        docker_socket.display()
                    );
                    let docker = Docker::connect_with_unix(
                        docker_socket.to_str().ok_or(
                            DockerManagerError::InvalidConfig(
                                "Docker socket path contains invalid UTF-8 characters".to_string(),
                            )
                        )?,
                        120,
                        API_DEFAULT_VERSION,
                    )?;
                    return Ok(Self {
                        docker: Arc::new(docker),
                        config: Arc::new(config.clone()),
                        http_client: Self::build_http_client(config)?,
                        ip_cache: Arc::new(
                            std::sync::RwLock::new(std::collections::HashMap::new()),
                        ),
                    });
                }
            }
        }

        // Use Docker's default detection mechanism
        debug!("Using Docker default connection mechanism");
        let docker = Arc::new(Docker::connect_with_local_defaults()?);
        Ok(Self {
            docker,
            config: Arc::new(config.clone()),
            http_client: reqwest::Client::new(),
            ip_cache: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        })
    }

    /// Gets a reference to the underlying Bollard Docker client.
    ///
    /// This method provides access to the raw `Arc<Docker>` client for advanced
    /// use cases where you need direct Bollard API access. For most operations,
    /// use the provided `DockerManager` methods instead.
    ///
    /// # Returns
    ///
    /// Clone of the `Arc<Docker>` wrapped client
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # use dsb::config;
    /// # fn main() -> Result<(), dsb::docker::DockerManagerError> {
    /// let config = config::load()?;
    /// let docker_manager = DockerManager::new_with_config(&config)?;
    ///
    /// // Get the underlying client for advanced operations
    /// let docker = docker_manager.docker_client();
    ///
    /// // Use docker directly with Bollard API
    /// # Ok(())
    /// # }
    /// ```
    pub fn docker_client(&self) -> Arc<Docker> {
        self.docker.clone()
    }
}
