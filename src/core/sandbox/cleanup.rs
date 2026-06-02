use super::SandboxService;
use crate::core::types::{ActivityType, SandboxState};

impl SandboxService {
    /// Force cleanup all sandbox resources (container + volumes + state).
    ///
    /// This method immediately removes all resources associated with a sandbox:
    /// 1. Removes the Docker container (force stops if running)
    /// 2. Removes all named volumes (bind mounts are host-managed)
    /// 3. Removes the sandbox state record
    ///
    /// # Arguments
    ///
    /// * `id` - The sandbox UUID
    ///
    /// # Returns
    ///
    /// - `Ok(())` - All resources cleaned up successfully
    /// - `Err(...)` - If sandbox not found or cleanup fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # async fn example(service: SandboxService) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # use uuid::Uuid;
    /// # let id = Uuid::new_v4();
    /// service.cleanup_sandbox(&id).await?;
    /// println!("Sandbox cleaned up");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn cleanup_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get sandbox including deleted ones, since we need to clean up containers
        // even for sandboxes that were already soft-deleted
        let sandbox = self
            .state
            .get_sandbox_with_deleted(id, true)
            .await
            .ok_or("Sandbox not found")?;

        // Remove container (force stop if running)
        if let Some(container_id) = &sandbox.container_id {
            self.backend.delete(container_id).await.map_err(|e| {
                tracing::error!(
                    sandbox_id = %id,
                    container_id = %container_id,
                    error = %e,
                    "Failed to remove container during cleanup - sandbox record will NOT be deleted"
                );
                e
            })?;
        }

        // Remove named volumes (bind mounts are host-managed, so we skip them)
        for volume in &sandbox.volume_mounts {
            if let crate::core::types::VolumeMount::Named { name, .. } = volume {
                if let Err(e) = self.backend.remove_volume(name).await {
                    tracing::warn!(
                        sandbox_id = %id,
                        volume_name = %name,
                        error = %e,
                        "Failed to remove volume during cleanup (non-fatal)"
                    );
                }
            }
        }

        // Soft delete: mark sandbox as deleted instead of removing from state
        // This allows viewing deleted sandboxes in the dashboard
        if let Some(mut sandbox) = self.state.get_sandbox_with_deleted(id, true).await {
            sandbox.deleted_at = Some(chrono::Utc::now());
            sandbox.deleted_by = Some("auto-cleanup".to_string());
            sandbox.state = crate::core::types::SandboxState::Destroyed;
            sandbox.updated_at = chrono::Utc::now();
            sandbox.container_id = None; // Container has been removed

            self.state.update_sandbox(&sandbox).await?;
        }

        Ok(())
    }

    /// Restores a soft-deleted sandbox by recreating its container.
    ///
    /// This method restores a previously deleted sandbox within the retention period.
    /// It recreates the Docker container using the stored configuration and clears
    /// the deleted_at/deleted_by fields.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to restore
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Sandbox restored successfully
    /// - `Err(...)` - If restoration fails
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The sandbox doesn't exist
    /// - The sandbox is not deleted (can't restore active sandboxes)
    /// - Container creation fails
    /// - Image pull fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use uuid::Uuid;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// let id = Uuid::new_v4();
    /// service.restore_sandbox(&id).await?;
    /// println!("Sandbox restored");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn restore_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get sandbox including deleted ones
        let mut sandbox = self
            .state
            .get_sandbox_with_deleted(id, true)
            .await
            .ok_or("Sandbox not found")?;

        // Verify sandbox is deleted
        if sandbox.deleted_at.is_none() {
            return Err("Sandbox is not deleted and cannot be restored".into());
        }

        tracing::info!("Restoring sandbox {}", id);

        // Create new container using the stored configuration
        let container_id = self
            .create_container_for_sandbox(&sandbox)
            .await
            .map_err(|e| {
                tracing::error!(
                    sandbox_id = %id,
                    error = %e,
                    "Failed to create container during restore"
                );
                e
            })?;

        // Update sandbox state
        sandbox.container_id = Some(container_id.clone());
        sandbox.deleted_at = None;
        sandbox.deleted_by = None;
        sandbox.state = crate::core::types::SandboxState::Created;
        sandbox.updated_at = chrono::Utc::now();

        // Update in state store
        self.state.update_sandbox(&sandbox).await?;

        // Start the container
        match self.backend.start(&container_id).await {
            Ok(_) => {
                // Update state to running
                sandbox.state = crate::core::types::SandboxState::Running;
                sandbox.updated_at = chrono::Utc::now();
                // Update last_api_activity to prevent immediate auto-cleanup after restore
                sandbox.activity.last_api_activity = chrono::Utc::now();
                sandbox.activity.activity_count += 1;
                self.state.update_sandbox(&sandbox).await?;

                // Record restore activity
                self.record_activity(*id, ActivityType::Restore, serde_json::json!({}))
                    .await;

                tracing::info!("Successfully restored sandbox {}", id);
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    sandbox_id = %id,
                    container_id = %container_id,
                    error = %e,
                    "Failed to start container during restore"
                );
                // Rollback: mark as deleted again if start fails
                let mut sb = sandbox.clone();
                sb.container_id = None;
                sb.deleted_at = Some(chrono::Utc::now());
                sb.deleted_by = Some("system".to_string());
                sb.state = crate::core::types::SandboxState::Error;
                let _ = self.state.update_sandbox(&sb).await;
                Err(e.into())
            }
        }
    }

    /// Helper method to create a container for a sandbox (used by both create and restore)
    pub(super) async fn create_container_for_sandbox(
        &self,
        sandbox: &crate::core::types::Sandbox,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Pull image if needed
        match sandbox.config.pull_policy {
            crate::core::types::PullPolicy::Always => {
                self.backend.pull_image(&sandbox.config.image).await?;
            }
            crate::core::types::PullPolicy::Missing => {
                // Try to pull, but continue if it fails (image might already exist)
                let _ = self.backend.pull_image(&sandbox.config.image).await;
            }
            crate::core::types::PullPolicy::Never => {
                // Don't pull
            }
        }

        // Create container
        let container_id = self
            .backend
            .create(Some(&sandbox.id), &sandbox.config)
            .await?;

        Ok(container_id)
    }

    /// Shuts down a sandbox by removing its container but keeping the database record.
    ///
    /// This method is used during graceful server shutdown to:
    /// 1. Remove the Docker container (force stop if running)
    /// 2. Update the sandbox state to Stopped in the database
    /// 3. Keep the database record for auditing purposes
    ///
    /// This is different from `cleanup_sandbox` which deletes the database record,
    /// and different from `stop_sandbox` which only stops but doesn't remove the container.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to shut down
    ///
    /// # Returns
    ///
    /// - `Ok(())` - If the sandbox was shut down successfully
    /// - `Err(...)` - If the sandbox doesn't exist or container removal fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use uuid::Uuid;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// let id = Uuid::new_v4();
    /// service.shutdown_cleanup_sandbox(&id).await?;
    /// println!("Sandbox container removed, database record preserved");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn shutdown_cleanup_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get sandbox (we need to update it after container removal)
        let mut sandbox = self
            .state
            .get_sandbox(id)
            .await
            .ok_or("Sandbox not found")?;

        // Remove container (force stop if running)
        if let Some(container_id) = &sandbox.container_id {
            self.backend.delete(container_id).await?;
        }

        // Update sandbox state to Stopped (keep database record for audit)
        sandbox.state = SandboxState::Stopped;
        sandbox.updated_at = chrono::Utc::now();
        self.state.update_sandbox(&sandbox).await?;

        // Record shutdown activity
        self.record_activity(*id, ActivityType::Stop, serde_json::json!({}))
            .await;

        Ok(())
    }
}
