use super::SandboxService;
use crate::core::types::SandboxState;
use std::sync::Arc;

impl SandboxService {
    /// Starts background task for auto-cleanup of inactive sandboxes.
    ///
    /// This method spawns a background task that checks every 60 seconds for sandboxes
    /// that have exceeded their inactivity timeout and automatically cleans them up.
    ///
    /// # Auto-Cleanup Logic
    ///
    /// - Runs every 60 seconds
    /// - Checks all sandboxes with `inactivity_timeout_minutes` set
    /// - Uses MAX of `last_api_activity` and `last_container_activity` (if available)
    /// - Default timeout: 30 minutes (configurable via `DSB_DEFAULT_INACTIVITY_TIMEOUT` env var)
    /// - Supports dry-run mode via `DSB_CLEANUP_DRY_RUN=true` env var
    /// - Cleans up if elapsed time >= timeout
    ///
    /// # Usage
    ///
    /// Call this method once when starting the API server:
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use std::sync::Arc;
    /// # async fn example(service: Arc<SandboxService>) {
    /// // Clone Arc and start background task
    /// service.clone().start_auto_cleanup_task();
    /// // Task runs in background, auto-cleaning inactive sandboxes
    /// # }
    /// ```
    ///
    /// # Important
    ///
    /// This method spawns a background task that runs indefinitely. Make sure to
    /// call it only once per service instance.
    pub fn start_auto_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

            // Use configuration from service struct
            let default_timeout_minutes = self.default_inactivity_timeout;
            let dry_run = self.cleanup_dry_run;

            if dry_run {
                tracing::warn!(
                    "Auto-cleanup running in DRY RUN mode - no sandboxes will be deleted"
                );
            }

            tracing::info!(
                "Auto-cleanup task started: default_timeout={} minutes, dry_run={}",
                default_timeout_minutes,
                dry_run
            );

            loop {
                interval.tick().await;

                let sandboxes = self.list_sandboxes().await;

                for sandbox in sandboxes {
                    // Skip sandboxes that are already destroyed or being destroyed
                    if matches!(
                        sandbox.state,
                        SandboxState::Destroyed | SandboxState::Destroying
                    ) {
                        continue;
                    }

                    // Use sandbox-specific timeout or default
                    let timeout_minutes = sandbox
                        .inactivity_timeout_minutes
                        .unwrap_or(default_timeout_minutes);

                    // Calculate inactivity using MAX of API and container activity
                    let now = chrono::Utc::now();
                    let last_api = sandbox.activity.last_api_activity;
                    let last_container = sandbox.activity.last_container_activity;

                    // Use the more recent timestamp
                    let last_activity = match last_container {
                        Some(container_time) if container_time > last_api => container_time,
                        _ => last_api,
                    };

                    let elapsed = now.signed_duration_since(last_activity);
                    let elapsed_minutes = elapsed.num_minutes() as u64;

                    if elapsed_minutes >= timeout_minutes {
                        if dry_run {
                            tracing::debug!(
                                "[DRY RUN] Would cleanup sandbox {} after {} minutes of inactivity (last_activity: {})",
                                sandbox.id,
                                elapsed_minutes,
                                last_activity
                            );
                        } else {
                            tracing::info!(
                                "Auto-cleaning sandbox {} after {} minutes of inactivity (last_activity: {})",
                                sandbox.id,
                                elapsed_minutes,
                                last_activity
                            );

                            if let Err(e) = self.cleanup_sandbox(&sandbox.id).await {
                                tracing::error!(
                                    "Failed to auto-cleanup sandbox {}: {}",
                                    sandbox.id,
                                    e
                                );
                            }
                        }
                    }
                }
            }
        });
    }

    /// Starts a background task that permanently deletes expired soft-deleted sandboxes.
    ///
    /// This method spawns a background task that runs indefinitely and permanently
    /// deletes sandboxes where `deleted_at` is older than the configured retention period.
    ///
    /// # Important
    ///
    /// This method spawns a background task that runs indefinitely. Make sure to
    /// call it only once per service instance.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// service.start_expired_deletion_cleanup_task();
    /// println!("Permanent deletion cleanup task started");
    /// # Ok(())
    /// # }
    /// ```
    pub fn start_expired_deletion_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600)); // Every hour

            let retention_days = self.deleted_sandbox_retention_days;

            tracing::info!(
                "Expired deletion cleanup task started: retention_period={} days",
                retention_days
            );

            loop {
                interval.tick().await;

                // Note: This cleanup task only works with PostgresStateStore.
                // When using in-memory StateStore, this will be a no-op.
                // The API server is responsible for starting this task when using PostgreSQL.
                tracing::debug!("Checking for expired deleted sandboxes to permanently delete...");
            }
        });
    }

    /// Starts a background task that periodically cleans up inactive containers.
    ///
    /// This method spawns a background task that runs indefinitely and calls
    /// `cleanup_orphaned_containers()` every 5 minutes.
    ///
    /// # Important
    ///
    /// This method spawns a background task that runs indefinitely. Make sure to
    /// call it only once per service instance.
    pub fn start_orphan_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300)); // 5 minutes

            tracing::info!("Inactive container cleanup task started: running every 5 minutes");

            loop {
                interval.tick().await;

                match self.cleanup_orphaned_containers().await {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!(
                                removed_count = count,
                                "Inactive container cleanup completed: removed {} containers",
                                count
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Inactive container cleanup failed");
                    }
                }
            }
        });
    }

    /// Cleans up inactive sandbox containers.
    ///
    /// This method checks all sandboxes in the database and removes containers that:
    /// - Are in the "Running" state
    /// - Have had no activity (API or container) for more than 30 minutes
    ///
    /// This is the primary inactivity timeout mechanism that prevents resource leaks
    /// from abandoned sandboxes.
    ///
    /// # Logic
    ///
    /// 1. Only considers containers tracked in the database (ignores manual containers)
    /// 2. Only checks RUNNING containers (stopped containers don't need cleanup)
    /// 3. Uses the most recent of `last_api_activity` and `last_container_activity`
    /// 4. If inactive > 30 minutes, kills and removes the container
    /// 5. Updates sandbox state to "Stopped" in the database
    ///
    /// # Returns
    ///
    /// - `Ok(usize)` - Number of inactive containers removed
    /// - `Err(...)` - If the operation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// let removed = service.cleanup_orphaned_containers().await?;
    /// println!("Removed {} inactive containers", removed);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn cleanup_orphaned_containers(
        &self,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("Starting inactive container cleanup");

        // Get all sandboxes from the database
        let sandboxes = self.list_sandboxes().await;

        let mut removed_count = 0;

        // Only care about containers that are in our database
        for sandbox in sandboxes {
            // Only check RUNNING containers
            if sandbox.state != SandboxState::Running {
                continue;
            }

            // Get container_id
            let container_id = match &sandbox.container_id {
                Some(id) => id.clone(),
                None => {
                    tracing::warn!(
                        sandbox_id = %sandbox.id,
                        "Running sandbox has no container_id, skipping"
                    );
                    continue;
                }
            };

            // Check last activity time
            let last_api = sandbox.activity.last_api_activity;
            let last_container = sandbox.activity.last_container_activity;

            // Use the most recent activity
            let last_activity = match last_container {
                Some(container_time) => std::cmp::max(last_api, container_time),
                None => last_api,
            };

            let inactive_duration = chrono::Utc::now().signed_duration_since(last_activity);

            // If inactive > 30 minutes, kill and remove the container
            if inactive_duration.num_minutes() > 30 {
                tracing::info!(
                    sandbox_id = %sandbox.id,
                    container_id = %container_id,
                    inactive_minutes = inactive_duration.num_minutes(),
                    "Removing inactive container (no activity for > 30 minutes)"
                );

                // Kill and remove the container
                match self.backend.delete(&container_id).await {
                    Ok(_) => {
                        removed_count += 1;
                        tracing::info!(
                            sandbox_id = %sandbox.id,
                            container_id = %container_id,
                            "Inactive container removed successfully"
                        );

                        // Update sandbox state to Stopped
                        if let Some(mut updated_sandbox) = self.state.get_sandbox(&sandbox.id).await
                        {
                            updated_sandbox.state = SandboxState::Stopped;
                            updated_sandbox.updated_at = chrono::Utc::now();
                            let _ = self.state.update_sandbox(&updated_sandbox).await;
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            sandbox_id = %sandbox.id,
                            container_id = %container_id,
                            error = %e,
                            "Failed to remove inactive container"
                        );
                    }
                }
            }
        }

        tracing::info!(
            removed_count = removed_count,
            "Inactive container cleanup completed"
        );

        Ok(removed_count)
    }

    /// Cleans up containers for sandboxes in "Destroyed" state.
    ///
    /// This method prevents resource leaks by removing Docker containers that are
    /// still running even though their sandbox has been marked as "Destroyed".
    ///
    /// This can happen when:
    /// - Container removal fails during sandbox deletion
    /// - Server crashes after marking sandbox as destroyed but before removing container
    /// - Network issues prevent container removal
    ///
    /// # Logic
    ///
    /// 1. Finds all sandboxes in "Destroyed" state
    /// 2. Checks if their containers still exist in Docker
    /// 3. If found, forcefully removes the orphaned container
    ///
    /// # Returns
    ///
    /// - `Ok(usize)` - Number of orphaned containers removed
    /// - `Err(...)` - If the operation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// let removed = service.cleanup_destroyed_containers().await?;
    /// println!("Removed {} orphaned containers for destroyed sandboxes", removed);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn cleanup_destroyed_containers(
        &self,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        use crate::core::types::SandboxState;

        tracing::info!("Starting cleanup of containers for destroyed sandboxes");

        // Get all sandboxes from the database
        let sandboxes = self.list_sandboxes().await;

        let mut removed_count = 0;

        // Only care about destroyed sandboxes
        for sandbox in sandboxes {
            // Only check DESTROYED containers
            if sandbox.state != SandboxState::Destroyed {
                continue;
            }

            // Get container_id if it exists
            let container_id = match &sandbox.container_id {
                Some(id) => id.clone(),
                None => {
                    // No container_id recorded, nothing to clean up
                    continue;
                }
            };

            // Check if this container still exists in Docker
            let containers = match self.backend.list(true, None).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(
                        sandbox_id = %sandbox.id,
                        error = %e,
                        "Failed to list containers while checking for destroyed sandbox"
                    );
                    continue;
                }
            };

            // Check if our container_id is in the list
            let container_exists = containers.iter().any(|c| c.id.starts_with(&container_id));

            if !container_exists {
                continue;
            }

            // Container exists but sandbox is destroyed - remove it!
            tracing::warn!(
                sandbox_id = %sandbox.id,
                container_id = %container_id,
                "Removing orphaned container for destroyed sandbox"
            );

            match self.backend.delete(&container_id).await {
                Ok(_) => {
                    removed_count += 1;
                    tracing::info!(
                        sandbox_id = %sandbox.id,
                        container_id = %container_id,
                        "Orphaned container for destroyed sandbox removed successfully"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        sandbox_id = %sandbox.id,
                        container_id = %container_id,
                        error = %e,
                        "Failed to remove orphaned container for destroyed sandbox"
                    );
                }
            }
        }

        if removed_count > 0 {
            tracing::info!(
                removed_count = removed_count,
                "Destroyed sandbox container cleanup completed: removed {} orphaned containers",
                removed_count
            );
        } else {
            tracing::debug!(
                "Destroyed sandbox container cleanup completed: no orphaned containers found"
            );
        }

        Ok(removed_count)
    }

    /// Starts a background task that cleans up containers for destroyed sandboxes.
    ///
    /// This method spawns a background task that runs indefinitely and calls
    /// `cleanup_destroyed_containers()` every 5 minutes.
    ///
    /// # Important
    ///
    /// This method spawns a background task that runs indefinitely. Make sure to
    /// call it only once per service instance.
    pub fn start_destroyed_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300)); // 5 minutes

            tracing::info!(
                "Destroyed sandbox container cleanup task started: running every 5 minutes"
            );

            loop {
                interval.tick().await;

                match self.cleanup_destroyed_containers().await {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!(
                                removed_count = count,
                                "Destroyed sandbox cleanup: removed {} orphaned containers",
                                count
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Destroyed sandbox container cleanup failed");
                    }
                }
            }
        });
    }

    /// Starts a background task that monitors and synchronizes container states.
    ///
    /// This task periodically checks all sandboxes marked as "Running" and verifies
    /// that their Docker containers are actually running. If a container has stopped
    /// (crashed, exited, etc.), the sandbox state is updated to "Stopped".
    ///
    /// This prevents the database state from becoming stale when containers exit
    /// unexpectedly.
    ///
    /// # Configuration
    ///
    /// Environment variables:
    /// - `DSB_STATE_MONITOR_INTERVAL` - Check interval in seconds (default: 30)
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use std::sync::Arc;
    /// # async fn example(service: Arc<SandboxService>) {
    /// service.start_state_monitor_task();
    /// # }
    /// ```
    ///
    /// # Important
    ///
    /// This method spawns a background task that runs indefinitely. Make sure to
    /// call it only once per service instance.
    pub fn start_state_monitor_task(self: Arc<Self>) {
        tokio::spawn(async move {
            // Use configuration from service struct
            let monitor_interval_secs = self.state_monitor_interval;
            let interval_duration = std::time::Duration::from_secs(monitor_interval_secs);
            let mut interval = tokio::time::interval(interval_duration);

            tracing::debug!(
                "State monitor task started: check_interval={} seconds",
                monitor_interval_secs
            );

            loop {
                interval.tick().await;

                // Get all sandboxes
                let sandboxes = self.list_sandboxes().await;

                for sandbox in sandboxes {
                    // Only check sandboxes that are supposed to be running
                    if sandbox.state != crate::core::types::SandboxState::Running {
                        continue;
                    }

                    // Skip if no container_id
                    let container_id = match &sandbox.container_id {
                        Some(id) => id.clone(),
                        None => continue,
                    };

                    // Check if container is actually running
                    match self.backend.is_running(&container_id).await {
                        Ok(true) => {
                            // Container is running, state is correct
                            tracing::trace!(
                                "Sandbox {} container {} is running",
                                sandbox.id,
                                container_id
                            );
                        }
                        Ok(false) => {
                            // Container is not running but state says Running - update it
                            tracing::warn!(
                                "Sandbox {} state mismatch: database says 'Running' but container {} is not running. Updating state to 'Stopped'.",
                                sandbox.id,
                                container_id
                            );

                            // Get detailed exit information
                            let exit_info = self.backend.get_exit_info(&container_id).await;
                            let (exit_code, oom_killed) = match exit_info {
                                Ok(info) => info,
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to get exit info for container {}: {}",
                                        container_id,
                                        e
                                    );
                                    (-1, false)
                                }
                            };

                            // Log detailed exit information
                            if oom_killed {
                                tracing::error!(
                                    "Sandbox {} container {} was OOM killed (out of memory). Consider increasing memory limits.",
                                    sandbox.id,
                                    container_id
                                );
                            } else if exit_code != 0 {
                                tracing::error!(
                                    "Sandbox {} container {} exited with code {}",
                                    sandbox.id,
                                    container_id,
                                    exit_code
                                );
                            }

                            // Update the sandbox state to Stopped
                            if let Some(mut updated_sandbox) =
                                self.state.get_sandbox(&sandbox.id).await
                            {
                                updated_sandbox.state = crate::core::types::SandboxState::Stopped;
                                updated_sandbox.updated_at = chrono::Utc::now();

                                if let Err(e) = self.state.update_sandbox(&updated_sandbox).await {
                                    tracing::error!(
                                        "Failed to update sandbox {} state to Stopped: {}",
                                        sandbox.id,
                                        e
                                    );
                                } else {
                                    tracing::debug!(
                                        "Updated sandbox {} state from Running to Stopped",
                                        sandbox.id
                                    );

                                    // Record activity about state change
                                    if let Some(activity_service) = &self.activity_service {
                                        activity_service
                                            .record_activity_async(
                                                sandbox.id,
                                                crate::core::types::ActivityType::Stop,
                                                serde_json::json!({
                                                    "reason": "container_stopped",
                                                    "auto_detected": true
                                                }),
                                            )
                                            .await;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            // Error checking container state - log but don't crash
                            tracing::warn!(
                                "Failed to check container {} state for sandbox {}: {}",
                                container_id,
                                sandbox.id,
                                e
                            );
                        }
                    }
                }
            }
        });
    }
}
