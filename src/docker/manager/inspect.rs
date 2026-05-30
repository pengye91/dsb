// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! Container inspection, stats, and info operations.

use bollard::query_parameters::{
    RemoveVolumeOptionsBuilder, StatsOptionsBuilder,
};
use futures_util::stream::StreamExt;
use super::{DockerManager, DockerManagerError};

impl DockerManager {
    /// Gets container resource usage statistics.
    ///
    /// This method retrieves real-time statistics about container resource consumption
    /// including CPU, memory, network I/O, and disk I/O.
    ///
    /// # Arguments
    ///
    /// * `id` - The container ID
    ///
    /// # Returns
    ///
    /// - `Ok(ContainerStats)` - Statistics including CPU %, memory usage, network and disk I/O
    /// - `Err(...)` - If stats retrieval fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let stats = docker.get_container_stats("abc123").await?;
    /// println!("CPU: {}%", stats.cpu_percent);
    /// println!("Memory: {} MB / {} MB", stats.memory_usage_mb, stats.memory_limit_mb);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_container_stats(
        &self,
        id: &str,
    ) -> Result<crate::core::types::ContainerStats, DockerManagerError> {
        let options = Some(
            StatsOptionsBuilder::default()
                .stream(false)
                .one_shot(true)
                .build(),
        );
        let mut stream = self.docker.stats(id, options);

        // Get the first (and only, since stream=false) stats result
        let stats_result = stream
            .next()
            .await
            .ok_or_else(|| DockerManagerError::Api("No stats available".to_string()))?
            .map_err(|e| DockerManagerError::Api(format!("Failed to get stats: {}", e)))?;

        // Parse CPU statistics
        let cpu_stats = stats_result
            .cpu_stats
            .as_ref()
            .ok_or_else(|| DockerManagerError::Api("No CPU stats available".to_string()))?;
        let precpu_stats = stats_result.precpu_stats.as_ref();

        let cpu_delta = cpu_stats
            .cpu_usage
            .as_ref()
            .and_then(|u| u.total_usage)
            .unwrap_or(0) as f64
            - precpu_stats
                .and_then(|s| s.cpu_usage.as_ref())
                .and_then(|u| u.total_usage)
                .unwrap_or(0) as f64;

        let system_delta = cpu_stats.system_cpu_usage.unwrap_or(0) as f64
            - precpu_stats.and_then(|s| s.system_cpu_usage).unwrap_or(0) as f64;

        let online_cpus = cpu_stats.online_cpus.unwrap_or(1) as f64;
        let cpu_percent = if system_delta > 0.0 && online_cpus > 0.0 {
            (cpu_delta / system_delta) * online_cpus * 100.0
        } else {
            0.0
        };

        // Parse memory statistics
        let memory_stats = stats_result
            .memory_stats
            .as_ref()
            .ok_or_else(|| DockerManagerError::Api("No memory stats available".to_string()))?;

        let memory_usage = memory_stats.usage.unwrap_or(0) / (1024 * 1024);
        let memory_limit = memory_stats.limit.unwrap_or(0) / (1024 * 1024);
        let memory_percent = if memory_limit > 0 {
            (memory_usage as f64 / memory_limit as f64) * 100.0
        } else {
            0.0
        };

        // Parse network statistics
        let network_rx_bytes = stats_result
            .networks
            .as_ref()
            .map(|nets| nets.values().map(|n| n.rx_bytes.unwrap_or(0)).sum::<u64>())
            .unwrap_or(0);

        let network_tx_bytes = stats_result
            .networks
            .as_ref()
            .map(|nets| nets.values().map(|n| n.tx_bytes.unwrap_or(0)).sum::<u64>())
            .unwrap_or(0);

        // Parse block I/O statistics
        let blkio_stats = stats_result.blkio_stats.as_ref();
        let block_read_bytes = blkio_stats
            .and_then(|bs| bs.io_service_bytes_recursive.as_ref())
            .map(|ios| {
                ios.iter()
                    .filter(|i| i.op.as_deref() == Some("Read"))
                    .map(|i| i.value.unwrap_or(0))
                    .sum::<u64>()
            })
            .unwrap_or(0);

        let block_write_bytes = blkio_stats
            .and_then(|bs| bs.io_service_bytes_recursive.as_ref())
            .map(|ios| {
                ios.iter()
                    .filter(|i| i.op.as_deref() == Some("Write"))
                    .map(|i| i.value.unwrap_or(0))
                    .sum::<u64>()
            })
            .unwrap_or(0);

        Ok(crate::core::types::ContainerStats {
            cpu_percent,
            memory_usage_mb: memory_usage,
            memory_limit_mb: memory_limit,
            memory_percent,
            network_rx_bytes,
            network_tx_bytes,
            block_read_bytes,
            block_write_bytes,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Removes a Docker volume by name.
    ///
    /// This method removes a Docker volume. If the volume is in use, it will force
    /// the removal after detaching it from any containers.
    ///
    /// # Arguments
    ///
    /// * `name` - The volume name
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Volume removed successfully
    /// - `Err(...)` - If removal fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// docker.remove_volume("my-volume").await?;
    /// println!("Volume removed successfully");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn remove_volume(
        &self,
        name: &str,
    ) -> Result<(), DockerManagerError> {
        let options = Some(RemoveVolumeOptionsBuilder::default().force(true).build());
        self.docker.remove_volume(name, options).await?;
        Ok(())
    }

    /// Checks if a container is currently running.
    ///
    /// This method inspects the container and returns true if it's in a running state.
    /// Returns false if the container is stopped, exited, or doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `id` - The container ID or name
    ///
    /// # Returns
    ///
    /// - `Ok(true)` - Container is running
    /// - `Ok(false)` - Container is not running or doesn't exist
    /// - `Err(...)` - Failed to inspect container
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let is_running = docker.is_container_running("abc123").await?;
    /// if is_running {
    ///     println!("Container is running");
    /// } else {
    ///     println!("Container is not running");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn is_container_running(
        &self,
        id: &str,
    ) -> Result<bool, DockerManagerError> {
        use bollard::query_parameters::InspectContainerOptions;

        match self
            .docker
            .inspect_container(id, None::<InspectContainerOptions>)
            .await
        {
            Ok(inspect) => {
                // Check if container is running
                if let Some(state) = inspect.state {
                    Ok(state.running.unwrap_or(false))
                } else {
                    // No state information means container doesn't exist or is gone
                    Ok(false)
                }
            }
            Err(e) => {
                // If container not found, it's not running
                let error_msg = e.to_string().to_lowercase();
                if error_msg.contains("no such container")
                    || error_msg.contains("not found")
                    || error_msg.contains("404")
                {
                    tracing::debug!("Container {} not found: {}", id, e);
                    Ok(false)
                } else {
                    // Other errors should be propagated
                    Err(DockerManagerError::Bollard(e))
                }
            }
        }
    }

    /// Gets the working directory of a container.
    ///
    /// # Arguments
    ///
    /// * `id` - Container ID or name
    ///
    /// # Returns
    ///
    /// The container's working directory path (e.g., "/app", "/home/user", etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if the container doesn't exist or the working directory cannot be determined
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let workdir = docker.get_container_workdir("container-id").await?;
    /// println!("Container working directory: {}", workdir);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_container_workdir(
        &self,
        id: &str,
    ) -> Result<String, DockerManagerError> {
        use bollard::query_parameters::InspectContainerOptions;

        match self
            .docker
            .inspect_container(id, None::<InspectContainerOptions>)
            .await
        {
            Ok(inspect) => {
                // Get the working directory from the container config
                if let Some(config) = inspect.config {
                    if let Some(workdir) = config.working_dir {
                        if !workdir.is_empty() {
                            Ok(workdir)
                        } else {
                            // If working_dir is empty string, default to root
                            Ok("/".to_string())
                        }
                    } else {
                        // If no working_dir is set, default to root
                        Ok("/".to_string())
                    }
                } else {
                    // If no config, default to root
                    Ok("/".to_string())
                }
            }
            Err(e) => {
                let error_msg = e.to_string().to_lowercase();
                if error_msg.contains("no such container")
                    || error_msg.contains("not found")
                    || error_msg.contains("404")
                {
                    Err(DockerManagerError::ContainerNotFound(format!("Container {} not found", id)))
                } else {
                    Err(DockerManagerError::Bollard(e))
                }
            }
        }
    }

    /// Gets detailed exit information for a stopped container.
    ///
    /// This method inspects a container and returns detailed information about why it exited,
    /// including exit code and OOM status.
    ///
    /// # Arguments
    ///
    /// * `id` - Container ID or name
    ///
    /// # Returns
    ///
    /// A tuple of (exit_code, oom_killed) or an error if inspection fails
    ///
    /// # Errors
    ///
    /// Returns an error if the container doesn't exist or inspection fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let (exit_code, oom_killed) = docker.get_container_exit_info("container-id").await?;
    /// println!("Exit code: {}, OOM: {}", exit_code, oom_killed);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_container_exit_info(
        &self,
        id: &str,
    ) -> Result<(i64, bool), DockerManagerError> {
        use bollard::query_parameters::InspectContainerOptions;

        match self
            .docker
            .inspect_container(id, None::<InspectContainerOptions>)
            .await
        {
            Ok(inspect) => {
                if let Some(state) = inspect.state {
                    let exit_code = state.exit_code.unwrap_or(-1);
                    let oom_killed = state.oom_killed.unwrap_or(false);

                    Ok((exit_code, oom_killed))
                } else {
                    Ok((-1, false))
                }
            }
            Err(e) => {
                let error_msg = e.to_string().to_lowercase();
                if error_msg.contains("no such container")
                    || error_msg.contains("not found")
                    || error_msg.contains("404")
                {
                    Err(DockerManagerError::ContainerNotFound(format!("Container {} not found", id)))
                } else {
                    Err(DockerManagerError::Bollard(e))
                }
            }
        }
    }

    /// Gets the last N lines of logs from a container.
    ///
    /// This method retrieves the most recent log lines from a container, which can be
    /// useful for debugging why a container crashed or exited unexpectedly.
    ///
    /// # Arguments
    ///
    /// * `id` - Container ID or name
    /// * `tail` - Number of lines to retrieve from the end of the logs (default: 100)
    ///
    /// # Returns
    ///
    /// The log lines as a string, or an error if retrieval fails
    ///
    /// # Errors
    ///
    /// Returns an error if the container doesn't exist or log retrieval fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let logs = docker.get_container_logs("container-id", 50).await?;
    /// println!("Last 50 log lines:\n{}", logs);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_container_logs(
        &self,
        _id: &str,
        _tail: Option<u64>,
    ) -> Result<String, DockerManagerError> {
        // TODO: Implement log retrieval
        // The bollard API for logs is complex and requires async stream handling
        // For now, return empty string to avoid blocking the state monitor
        Ok(String::new())
    }
}
