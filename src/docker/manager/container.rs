// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! Container lifecycle operations.

use std::collections::HashMap;
use bollard::models::{ContainerCreateBody, ContainerSummary, HostConfig, PortBinding};
use bollard::query_parameters::{
    CreateContainerOptions, ListContainersOptionsBuilder,
    RemoveContainerOptionsBuilder, StartContainerOptions, StopContainerOptionsBuilder,
};
use crate::core::types::{PortProtocol, SandboxConfig};
use super::{DockerManager, DockerManagerError};

impl DockerManager {
    /// Creates a new Docker container without starting it.
    ///
    /// This is the "fast path" container creation that assumes the image already
    /// exists locally. For image pulling, see [`pull_image`](Self::pull_image).
    ///
    /// # Arguments
    ///
    /// * `config` - Sandbox configuration including image, ports, and resource limits
    ///
    /// # Returns
    ///
    /// - `Ok(String)` - The ID of the created container
    /// - `Err(...)` - If container creation fails
    ///
    /// # Container Configuration
    ///
    /// The created container will have:
    /// - **Image**: From `config.image`
    /// - **Name**: From `config.name` (optional)
    /// - **Ports**: Mapped according to `config.port_mappings`
    /// - **Environment**: All variables from `config.environment`
    /// - **Resources**: Limits from `config.resource_limits`
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Image doesn't exist locally
    /// - Port mappings conflict with existing bindings
    /// - Container name already exists
    /// - Resource limits are invalid
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # use dsb::core::{SandboxConfig, PortMapping, PortProtocol};
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let config = SandboxConfig {
    ///     image: "nginx:latest".to_string(),
    ///     name: Some("my-container".to_string()),
    ///     port_mappings: vec![
    ///         PortMapping {
    ///             host_port: 8080,
    ///             container_port: 80,
    ///             protocol: PortProtocol::Tcp,
    ///         }
    ///     ],
    ///     ..Default::default()
    /// };
    ///
    /// let container_id = docker.create_container(&config, None).await?;
    /// println!("Created: {}", container_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_container(
        &self,
        config: &SandboxConfig,
        sandbox_id: Option<&uuid::Uuid>,
    ) -> Result<String, DockerManagerError> {
        let start = std::time::Instant::now();

        tracing::info!(
            sandbox_id = ?sandbox_id,
            image = %config.image,
            name = ?config.name,
            "Creating container"
        );

        // Build port bindings
        let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
        for mapping in &config.port_mappings {
            let protocol_str = match mapping.protocol {
                PortProtocol::Tcp => "tcp",
                PortProtocol::Udp => "udp",
            };
            let key = format!("{}/{}", mapping.container_port, protocol_str);
            port_bindings.insert(
                key,
                Some(vec![PortBinding {
                    host_ip: Some(String::from("0.0.0.0")),
                    host_port: Some(mapping.host_port.to_string()),
                }]),
            );
        }

        // Build exposed ports for Docker internal networking
        let mut exposed_ports_map: HashMap<String, HashMap<(), ()>> = HashMap::new();
        for port in &config.exposed_ports {
            exposed_ports_map.insert(format!("{}/tcp", port), HashMap::new());
            exposed_ports_map.insert(format!("{}/udp", port), HashMap::new());
        }

        // Build volume bindings from VolumeMount enum
        let mut binds: Vec<String> = config
            .volumes
            .iter()
            .map(|volume| match volume {
                crate::core::types::VolumeMount::Bind {
                    host_path,
                    container_path,
                    read_only,
                } => {
                    format!(
                        "{}:{}{}",
                        host_path,
                        container_path,
                        if *read_only { ":ro" } else { "" }
                    )
                }
                crate::core::types::VolumeMount::Named {
                    name,
                    container_path,
                    read_only,
                } => {
                    format!(
                        "{}:{}{}",
                        name,
                        container_path,
                        if *read_only { ":ro" } else { "" }
                    )
                }
            })
            .collect();

        // Add static file server mount (always enabled)
        if let Some(sid) = sandbox_id {
            // For bind mounts to sandbox containers, we need the HOST path
            // Use host_path if configured, otherwise fall back to base_path
            // NOTE: We expand tilde (~) to home directory for proper bind mount paths
            let home_dir = self.config.docker.home_dir.as_deref();
            let path_to_expand = self
                .config
                .static_server
                .host_path
                .as_deref()
                .unwrap_or(&self.config.static_server.base_path);
            let base_path = super::expand_tilde(path_to_expand, home_dir);

            let static_files_dir = format!("{}/{}", base_path, sid);

            // Create directory on host (DSB runs as root, so this works)
            if let Err(e) = std::fs::create_dir_all(&static_files_dir) {
                tracing::warn!(
                    "Failed to create static files directory {}: {}",
                    static_files_dir,
                    e
                );
            } else {
                // Make directory writable by the dsb user (UID 1000) inside sandbox containers.
                // We do two things:
                // 1. chown to UID 1000 (dsb user) so ownership matches the container user
                // 2. chmod 0777 as fallback so any UID can write
                // Both are needed because SELinux/AppArmor may override one or the other.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;

                    // chmod 0777 FIRST as defense-in-depth (this should always work for root)
                    if let Err(e) = std::fs::set_permissions(
                        &static_files_dir,
                        std::fs::Permissions::from_mode(0o777),
                    ) {
                        tracing::error!(
                            "Failed to set permissions 0777 on static files directory {}: {}",
                            static_files_dir,
                            e
                        );
                    }

                    // chown to dsb user (UID 1000, GID 1000)
                    // This may fail if DSB server is not running as root
                    let path_cstr = std::ffi::CString::new(static_files_dir.as_str())
                        .map_err(|_| {
                            DockerManagerError::Api(
                                "Static files directory path contains null byte".to_string(),
                            )
                        })?;
                    // SAFETY: We call libc::chown with a valid null-terminated C string path.
                    // The path was just created by Docker and is guaranteed to exist.
                    // The uid/gid (1000) are hardcoded to match the dsb user.
                    let ret = unsafe { libc::chown(path_cstr.as_ptr(), 1000, 1000) };
                    if ret != 0 {
                        let errno_val =
                            std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
                        tracing::warn!(
                            "Failed to chown static files directory {} to 1000:1000: errno {}",
                            static_files_dir,
                            errno_val
                        );
                        // chmod 0777 was applied above as fallback - verify it worked
                        if let Ok(metadata) = std::fs::metadata(&static_files_dir) {
                            let perms = metadata.permissions().mode() & 0o777;
                            if perms != 0o777 {
                                tracing::error!(
                                    "Static files directory {} has permissions {:o}, dsb user (UID 1000) cannot write! Fix by running DSB as root or ensuring host directory allows world-write.",
                                    static_files_dir,
                                    perms
                                );
                            } else {
                                tracing::info!(
                                    "Static files directory {} is world-writable ({:o}), dsb user can write despite failed chown",
                                    static_files_dir,
                                    perms
                                );
                            }
                        }
                    } else {
                        tracing::info!(
                            "Chowned static files directory {} to 1000:1000 for sandbox {}",
                            static_files_dir,
                            sid
                        );
                    }
                }

                tracing::info!(
                    "Created static files directory {} for sandbox {}",
                    static_files_dir,
                    sid
                );

                // Add bind mount for /public
                // Format: "host_path:container_path:options"
                // The :z flag tells Docker to relabel the directory for SELinux compatibility.
                // On non-SELinux systems this is a no-op.
                binds.push(format!("{}:/public:rw,z", static_files_dir));
                tracing::info!(
                    "Static file server enabled for sandbox {} at /public (host: {})",
                    sid,
                    static_files_dir
                );
            }
        } else {
            tracing::warn!("No sandbox_id provided for static files directory");
        }

        // Build ulimits
        let ulimits: Vec<bollard::models::ResourcesUlimits> = config
            .resource_limits
            .ulimits
            .as_ref()
            .map(|ulimits_vec| {
                ulimits_vec
                    .iter()
                    .map(|u| bollard::models::ResourcesUlimits {
                        name: Some(u.name.clone()),
                        soft: Some(u.soft),
                        hard: Some(u.hard),
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Build host config with all resource limits and volumes
        let host_config = HostConfig {
            port_bindings: Some(port_bindings),
            memory: config
                .resource_limits
                .memory_mb
                .map(|m| (m * 1024 * 1024) as i64),
            cpu_quota: config.resource_limits.cpu_quota,
            cpu_period: config.resource_limits.cpu_period,
            cpu_shares: config.resource_limits.cpu_shares.map(|s| s as i64),
            pids_limit: config.resource_limits.pids_limit,
            binds: if binds.is_empty() { None } else { Some(binds) },
            ulimits: if ulimits.is_empty() {
                None
            } else {
                Some(ulimits)
            },
            ..Default::default()
        };

        // Build environment variables
        // Start with proxy env vars from server config (deployment .env)
        let mut env_map: std::collections::HashMap<String, String> =
            self.config.docker.proxy_env.clone();
        // Merge request-provided env vars, overriding proxy defaults if there are conflicts
        for (k, v) in &config.environment {
            env_map.insert(k.clone(), v.clone());
        }
        let env: Vec<String> = env_map
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // Check if features are enabled (VNC, browser, etc.)
        // When features are enabled, preserve the image's default CMD/entrypoint to run supervisord
        let has_features = config.enable_all_features || !config.features.is_empty();

        // Prepare command - default to tail -f /dev/null if not specified
        // However, if features are enabled, use None to preserve image's default CMD (supervisord)
        let cmd = if let Some(command) = &config.command {
            tracing::debug!("Using specified command: {:?}", command);
            Some(command.to_vec())
        } else if has_features {
            tracing::debug!("Features enabled, preserving image default CMD (supervisord)");
            None // Use image's default CMD
        } else {
            tracing::debug!("No command specified, using default: tail -f /dev/null");
            // Default command to keep container alive
            Some(vec![
                "tail".to_string(),
                "-f".to_string(),
                "/dev/null".to_string(),
            ])
        };

        // Container config
        let container_config = ContainerCreateBody {
            image: Some(config.image.clone()),
            env: if env.is_empty() { None } else { Some(env) },
            cmd,
            // Only clear entrypoint if no features (preserves supervisord entrypoint when features enabled)
            entrypoint: if has_features { None } else { Some(vec![]) },
            host_config: Some(host_config),
            exposed_ports: if exposed_ports_map.is_empty() {
                None
            } else {
                Some(exposed_ports_map)
            },
            ..Default::default()
        };

        // Create container
        let options = Some(CreateContainerOptions {
            name: config.name.clone(),
            platform: String::new(),
        });

        let result = self
            .docker
            .create_container(options, container_config)
            .await?;

        let container_id = result.id;

        // Connect to DSB network if configured (for container-to-container communication)
        if let Some(ref network) = self.config.docker.network {
            tracing::debug!(
                "Connecting container {} to network: {}",
                container_id,
                network
            );
            match self
                .docker
                .connect_network(
                    network,
                    bollard::models::NetworkConnectRequest {
                        container: Some(container_id.clone()),
                        endpoint_config: Some(bollard::models::EndpointSettings {
                            aliases: Some(vec![config
                                .name
                                .clone()
                                .unwrap_or_else(|| "sandbox".to_string())]),
                            ..Default::default()
                        }),
                    },
                )
                .await
            {
                Ok(_) => {
                    tracing::info!(
                        "Container {} connected to network: {}",
                        container_id,
                        network
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to connect container {} to network {}: {}",
                        container_id,
                        network,
                        e
                    );
                    // Don't fail the whole operation if network connection fails
                    // The container can still be used without being on the custom network
                }
            }
        }

        tracing::info!(
            sandbox_id = ?sandbox_id,
            container_id = %container_id,
            image = %config.image,
            duration_ms = start.elapsed().as_millis(),
            "Container created successfully"
        );

        Ok(container_id)
    }

    /// Starts a created container.
    ///
    /// # Arguments
    ///
    /// * `id` - The container ID to start
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Container started successfully
    /// - `Err(...)` - If start fails (container doesn't exist, already running, etc.)
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let container_id = "abc123";
    /// docker.start_container(container_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn start_container(
        &self,
        id: &str,
    ) -> Result<(), DockerManagerError> {
        tracing::info!(container_id = %id, "Starting container");

        self.docker
            .start_container(id, None::<StartContainerOptions>)
            .await?;

        tracing::info!(
            container_id = %id,
            "Container started successfully"
        );

        Ok(())
    }

    /// Stops a running container gracefully.
    ///
    /// This sends a SIGTERM signal to the container, allowing it to gracefully
    /// shut down. If the container doesn't stop within 10 seconds, Docker will
    /// forcibly kill it.
    ///
    /// # Arguments
    ///
    /// * `id` - The container ID to stop
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Container stopped successfully (or already stopped)
    /// - `Err(...)` - If stop operation fails
    ///
    /// # Timeout
    ///
    /// Uses a 10-second timeout before force killing the container.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let container_id = "abc123";
    /// docker.stop_container(container_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn stop_container(
        &self,
        id: &str,
    ) -> Result<(), DockerManagerError> {
        tracing::info!(container_id = %id, "Stopping container");

        let options = StopContainerOptionsBuilder::default().t(10).build();
        self.docker.stop_container(id, Some(options)).await?;

        tracing::info!(
            container_id = %id,
            "Container stopped successfully"
        );

        Ok(())
    }

    /// Removes a container permanently, optionally removing associated volumes.
    ///
    /// This operation cannot be undone. The container and its data will be deleted.
    ///
    /// # Arguments
    ///
    /// * `id` - The container ID to remove
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Container removed successfully
    /// - `Err(...)` - If removal fails
    ///
    /// # Behavior
    ///
    /// - **Force**: Always uses force removal to stop the container if running
    /// - **Volumes**: Always removes associated volumes
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let container_id = "abc123";
    /// docker.remove_container(container_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn remove_container(
        &self,
        id: &str,
    ) -> Result<(), DockerManagerError> {
        tracing::info!(container_id = %id, "Removing container");

        // Retry logic with exponential backoff
        let max_retries = 3;
        let mut last_error: Option<DockerManagerError> = None;

        for attempt in 1..=max_retries {
            let options = RemoveContainerOptionsBuilder::default()
                .force(true)
                .v(true)
                .build();

            match self.docker.remove_container(id, Some(options)).await {
                Ok(_) => {
                    // Verify container is actually removed
                    match self.verify_container_removed(id).await {
                        Ok(removed) => {
                            if removed {
                                tracing::info!(
                                    container_id = %id,
                                    attempt = attempt,
                                    "Container removed successfully"
                                );
                                // Evict from IP cache
                                if let Ok(mut cache) = self.ip_cache.write() {
                                    cache.remove(id);
                                }
                                return Ok(());
                            } else {
                                let msg = format!(
                                    "Container removal reported success but container still exists (attempt {})",
                                    attempt
                                );
                                tracing::warn!(container_id = %id, attempt = attempt, "{}", &msg);
                                last_error = Some(DockerManagerError::Api(msg));
                            }
                        }
                        Err(e) => {
                            // Verification error - container is likely gone
                            tracing::debug!(
                                container_id = %id,
                                error = %e,
                                "Container removal verification failed (likely already removed)"
                            );
                            // Evict from IP cache
                            if let Ok(mut cache) = self.ip_cache.write() {
                                cache.remove(id);
                            }
                            return Ok(());
                        }
                    }
                }
                Err(e) => {
                    // Check if container is already gone (404 or "No such container")
                    let error_msg = e.to_string().to_lowercase();
                    if error_msg.contains("no such container")
                        || error_msg.contains("not found")
                        || error_msg.contains("404")
                    {
                        tracing::info!(
                            container_id = %id,
                            "Container not found (already removed): {}",
                            e
                        );
                        // Evict from IP cache
                        if let Ok(mut cache) = self.ip_cache.write() {
                            cache.remove(id);
                        }
                        return Ok(());
                    }

                    let msg = format!("Attempt {} failed: {}", attempt, e);
                    last_error = Some(DockerManagerError::Bollard(e));

                    if attempt < max_retries {
                        let delay = std::time::Duration::from_millis(500 * attempt as u64);
                        tracing::warn!(
                            container_id = %id,
                            attempt = attempt,
                            max_retries = max_retries,
                            error = %msg,
                            "Container removal failed, retrying in {:?}",
                            delay
                        );
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        let error_msg = format!("Failed to remove container after {} attempts", max_retries);
        tracing::error!(container_id = %id, "{}", &error_msg);
        Err(last_error.unwrap_or_else(|| DockerManagerError::Api(error_msg)))
    }

    /// Verifies that a container has been removed.
    ///
    /// # Arguments
    ///
    /// * `id` - Container ID to verify
    ///
    /// # Returns
    ///
    /// - `Ok(true)` - Container has been removed
    /// - `Ok(false)` - Container still exists
    /// - `Err(...)` - Verification check failed
    async fn verify_container_removed(
        &self,
        id: &str,
    ) -> Result<bool, DockerManagerError> {
        use bollard::query_parameters::InspectContainerOptions;

        match self
            .docker
            .inspect_container(id, None::<InspectContainerOptions>)
            .await
        {
            Ok(_) => Ok(false), // Container still exists
            Err(e) => {
                // Check if error is "not found"
                let error_str = e.to_string().to_lowercase();
                if error_str.contains("not found") || error_str.contains("no such container") {
                    Ok(true) // Container successfully removed
                } else {
                    Err(DockerManagerError::Bollard(e))
                }
            }
        }
    }

    /// Lists all containers matching the given filters.
    ///
    /// This method returns a list of containers including stopped ones.
    ///
    /// # Arguments
    ///
    /// * `all` - If true, returns all containers including stopped ones
    /// * `filters` - Optional filters to apply (e.g., by image, status, label)
    ///
    /// # Returns
    ///
    /// - `Ok(Vec<ContainerSummary>)` - List of containers
    /// - `Err(...)` - If listing fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # use std::collections::HashMap;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// // List all containers including stopped ones
    /// let containers = docker.list_containers(true, None).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_containers(
        &self,
        all: bool,
        filters: Option<HashMap<String, Vec<String>>>,
    ) -> Result<Vec<ContainerSummary>, DockerManagerError> {
        let mut builder = ListContainersOptionsBuilder::default();
        builder = builder.all(all);

        if let Some(f) = filters {
            builder = builder.filters(&f);
        }

        let options = builder.build();

        let containers = self.docker.list_containers(Some(options)).await?;
        Ok(containers)
    }
}
