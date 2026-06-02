use super::SandboxService;

impl SandboxService {
    /// Records API activity for a sandbox (updates last_api_activity timestamp).
    ///
    /// This method should be called whenever an API operation is performed on a sandbox
    /// (e.g., get, exec, stop, stats). It updates the activity tracking information
    /// used for auto-cleanup decisions.
    ///
    /// # Arguments
    ///
    /// * `id` - The sandbox UUID
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # async fn example(service: SandboxService) -> Result<(), Box<dyn std::error::Error>> {
    /// # use uuid::Uuid;
    /// # let id = Uuid::new_v4();
    /// service.record_api_activity(&id).await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn record_api_activity(&self, id: &uuid::Uuid) {
        if let Some(mut sandbox) = self.state.get_sandbox(id).await {
            sandbox.activity.last_api_activity = chrono::Utc::now();
            sandbox.activity.activity_count += 1;
            sandbox.updated_at = chrono::Utc::now();
            let _ = self.state.update_sandbox(&sandbox).await;
        }
    }

    /// Updates container activity timestamp for a sandbox.
    ///
    /// This method should be called when the container shows actual resource usage
    /// (CPU, memory, etc.), indicating active work being performed.
    ///
    /// # Arguments
    ///
    /// * `id` - The sandbox UUID
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # async fn example(service: SandboxService) -> Result<(), Box<dyn std::error::Error>> {
    /// # use uuid::Uuid;
    /// # let id = Uuid::new_v4();
    /// service.update_container_activity(&id).await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update_container_activity(&self, id: &uuid::Uuid) {
        if let Some(mut sandbox) = self.state.get_sandbox(id).await {
            sandbox.activity.last_container_activity = Some(chrono::Utc::now());
            sandbox.updated_at = chrono::Utc::now();
            let _ = self.state.update_sandbox(&sandbox).await;
        }
    }

    /// Gets sandbox resource usage statistics.
    ///
    /// This method retrieves real-time statistics about container resource consumption
    /// including CPU, memory, network I/O, and disk I/O. It also updates the container
    /// activity timestamp.
    ///
    /// # Arguments
    ///
    /// * `id` - The sandbox UUID
    ///
    /// # Returns
    ///
    /// - `Ok(ContainerStats)` - Resource usage statistics
    /// - `Err(...)` - If sandbox not found, container not running, or stats retrieval fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # async fn example(service: SandboxService) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # use uuid::Uuid;
    /// # let id = Uuid::new_v4();
    /// let stats = service.get_sandbox_stats(&id).await?;
    /// println!("CPU: {}%", stats.cpu_percent);
    /// println!("Memory: {}%", stats.memory_percent);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_sandbox_stats(
        &self,
        id: &uuid::Uuid,
    ) -> Result<crate::core::types::ContainerStats, Box<dyn std::error::Error + Send + Sync>> {
        let sandbox = self
            .state
            .get_sandbox(id)
            .await
            .ok_or("Sandbox not found")?;

        let container_id = sandbox
            .container_id
            .as_ref()
            .ok_or("Container not created")?;

        // Get stats from backend
        let stats = self.backend.stats(container_id).await?;

        Ok(stats)
    }

    /// Creates a streaming channel for sandbox resource statistics.
    ///
    /// This method sets up a background task that continuously polls container stats
    /// every 1 second and sends them through a channel. This is useful for real-time
    /// monitoring via Server-Sent Events (SSE).
    ///
    /// # Arguments
    ///
    /// * `id` - The sandbox UUID
    ///
    /// # Returns
    ///
    /// - `Ok(Receiver)` - Channel receiver that streams ContainerStats
    /// - `Err(...)` - If sandbox not found or not running
    ///
    /// # Channel Details
    ///
    /// - Channel buffer size: 100 messages
    /// - Polling interval: 1 second
    /// - Channel closes when: container is gone, client disconnects, or error occurs
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # async fn example(service: SandboxService) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # use uuid::Uuid;
    /// # let id = Uuid::new_v4();
    /// let mut receiver = service.stream_sandbox_stats(&id).await?;
    ///
    /// // Receive stats in a loop
    /// while let Some(stats) = receiver.recv().await {
    ///     println!("CPU: {}% Memory: {}%", stats.cpu_percent, stats.memory_percent);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn stream_sandbox_stats(
        &self,
        id: &uuid::Uuid,
    ) -> Result<
        tokio::sync::mpsc::Receiver<crate::core::types::ContainerStats>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        // Verify sandbox exists and is running
        let sandbox = self
            .state
            .get_sandbox(id)
            .await
            .ok_or("Sandbox not found")?;

        if sandbox.state != crate::core::types::SandboxState::Running {
            return Err("Sandbox is not running".into());
        }

        let container_id = sandbox
            .container_id
            .as_ref()
            .ok_or("Container not created")?
            .clone();

        // Create channel for streaming
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let backend = self.backend.clone();

        // Spawn background task for continuous stats collection
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));

            loop {
                interval.tick().await;

                match backend.stats(&container_id).await {
                    Ok(stats) => {
                        // Send stats to channel
                        // If send fails, client disconnected, so break
                        if tx.send(stats).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        // Container might be gone or error occurred
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }
}
