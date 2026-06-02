use super::SandboxService;
use crate::core::manager::ManagerError;
use crate::core::types::{ActivityType, SandboxState};

impl SandboxService {
    /// Stops a running sandbox.
    ///
    /// This stops the Docker container but keeps the sandbox record.
    /// The container still exists and can be restarted (though restart
    /// functionality is not currently exposed).
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to stop
    ///
    /// # Returns
    ///
    /// - `Ok(())` - If the sandbox was stopped successfully
    /// - `Err(...)` - If the sandbox doesn't exist or stop fails
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The sandbox doesn't exist
    /// - The container has already been removed
    /// - Docker daemon is inaccessible
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use uuid::Uuid;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// let id = Uuid::new_v4();
    /// service.stop_sandbox(&id).await?;
    /// println!("Sandbox stopped");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn stop_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut sandbox = self
            .state
            .get_sandbox(id)
            .await
            .ok_or("Sandbox not found")?;

        if let Some(container_id) = &sandbox.container_id {
            match self.backend.stop(container_id).await {
                Ok(()) => {}
                // Container already removed externally (e.g. pruned, crashed, or removed
                // by a concurrent test cleanup). Treat this as "already stopped" so the
                // state store can still be updated cleanly.
                Err(ManagerError::Api(ref msg)) if msg.contains("404") => {
                    tracing::warn!(
                        sandbox_id = %id,
                        container_id = %container_id,
                        "Container not found during stop (already removed) – marking sandbox as stopped"
                    );
                }
                Err(e) => return Err(Box::new(e)),
            }
        }

        sandbox.state = SandboxState::Stopped;
        sandbox.updated_at = chrono::Utc::now();
        self.state.update_sandbox(&sandbox).await?;

        // Record stop activity
        self.record_activity(*id, ActivityType::Stop, serde_json::json!({}))
            .await;

        Ok(())
    }

    /// Starts a stopped sandbox.
    ///
    /// This method starts a Docker container that was previously stopped.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to start
    ///
    /// # Returns
    ///
    /// - `Ok(())` - If the sandbox was started successfully
    /// - `Err(...)` - If the sandbox doesn't exist or container start fails
    ///
    /// # Errors
    ///
    /// - Returns error if sandbox doesn't exist in the state store
    /// - Returns error if container is not assigned or start fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use uuid::Uuid;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// let id = Uuid::new_v4();
    /// service.start_sandbox(&id).await?;
    /// println!("Sandbox started");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn start_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut sandbox = self
            .state
            .get_sandbox(id)
            .await
            .ok_or("Sandbox not found")?;

        if let Some(container_id) = &sandbox.container_id {
            self.backend.start(container_id).await?;
        } else {
            return Err("Sandbox has no container assigned".into());
        }

        sandbox.state = SandboxState::Running;
        sandbox.updated_at = chrono::Utc::now();
        self.state.update_sandbox(&sandbox).await?;

        // Record start activity
        self.record_activity(*id, ActivityType::Start, serde_json::json!({}))
            .await;

        Ok(())
    }

    /// Deletes a sandbox (soft delete).
    ///
    /// This method:
    /// 1. Removes the Docker container (if it exists) - NOW STRICT, will fail if container removal fails
    /// 2. Marks the sandbox record as deleted in the state store (keeps record for auditing)
    ///
    /// The sandbox record is preserved in the database with:
    /// - `deleted_at` set to current timestamp
    /// - `deleted_by` set to "system"
    /// - `state` set to Destroying
    /// - `container_id` cleared
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to delete
    ///
    /// # Returns
    ///
    /// - `Ok(())` - If the sandbox was deleted successfully
    /// - `Err(...)` - If the sandbox doesn't exist or container removal fails
    ///
    /// # Errors
    ///
    /// - Returns error if sandbox doesn't exist in the state store
    /// - Returns error if container removal fails (NEW: strict mode)
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use uuid::Uuid;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// let id = Uuid::new_v4();
    /// service.delete_sandbox(&id).await?;
    /// println!("Sandbox deleted");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get sandbox including deleted ones, in case we're retrying a delete
        let mut sandbox = self
            .state
            .get_sandbox_with_deleted(id, true)
            .await
            .ok_or("Sandbox not found")?;

        // Remove container - fail if this fails to ensure atomic cleanup
        if let Some(container_id) = &sandbox.container_id {
            self.backend.delete(container_id).await?;
        }

        // Record delete activity before soft deleting
        self.record_activity(*id, ActivityType::Delete, serde_json::json!({}))
            .await;

        // Soft delete: mark sandbox as deleted instead of removing from state
        sandbox.deleted_at = Some(chrono::Utc::now());
        sandbox.deleted_by = Some("system".to_string());
        sandbox.state = crate::core::types::SandboxState::Destroyed;
        sandbox.updated_at = chrono::Utc::now();
        sandbox.container_id = None; // Container has been removed

        self.state.update_sandbox(&sandbox).await?;

        // Mark all activities for this sandbox as deleted
        if let Some(activity_service) = &self.activity_service {
            let _ = activity_service.mark_sandbox_activities_deleted(id).await;
        }

        Ok(())
    }
}
