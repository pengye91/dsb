use crate::db::store::helpers::row_to_sandbox;
use crate::db::store::{PostgresStateStore, Sandbox, StoreError};
use tracing::{debug, error, info};

impl PostgresStateStore {
    /// Lists all non-deleted sandboxes owned by a specific API key.
    pub async fn fetch_sandboxes_owned_by(
        &self,
        api_key_id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Vec<Sandbox> {
        let client = match self.pool.get().await {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to get database connection: {}", e);
                return vec![];
            }
        };

        let deleted_filter = if include_deleted {
            "" // No filter - include all sandboxes
        } else {
            "AND deleted_at IS NULL"
        };

        let query = format!(
            r#"
                SELECT
                    id, image, name, environment, port_mappings, resource_limits,
                    volumes, command, inactivity_timeout_minutes, pull_policy,
                    features, enable_all_features, vnc_resolution,
                    state, container_id, error_message, volume_mounts,
                    last_api_activity, last_container_activity, activity_count,
                    created_at, updated_at, deleted_at, deleted_by, api_key_id
                FROM sandboxes
                WHERE api_key_id = $1 {}
                ORDER BY created_at DESC
                "#,
            deleted_filter
        );

        let rows = match client.query(&query, &[api_key_id]).await {
            Ok(rows) => rows,
            Err(e) => {
                error!("Failed to list sandboxes for API key {}: {}", api_key_id, e);
                return vec![];
            }
        };

        rows.into_iter()
            .filter_map(|row| match row_to_sandbox(row) {
                Ok(sandbox) => Some(sandbox),
                Err(e) => {
                    error!("Failed to convert owned sandbox row: {}", e);
                    None
                }
            })
            .collect()
    }

    /// Retrieves a non-deleted sandbox only if owned by the given API key.
    pub async fn fetch_sandbox_if_owned_by(
        &self,
        id: &uuid::Uuid,
        api_key_id: &uuid::Uuid,
    ) -> Option<Sandbox> {
        let client = match self.pool.get().await {
            Ok(c) => c,
            Err(e) => {
                error!(
                    "Failed to get database connection for fetch_sandbox_if_owned_by: {}",
                    e
                );
                return None;
            }
        };
        let row = match client
            .query_opt(
                r#"
                SELECT
                    id, image, name, environment, port_mappings, resource_limits,
                    volumes, command, inactivity_timeout_minutes, pull_policy,
                    features, enable_all_features, vnc_resolution,
                    state, container_id, error_message, volume_mounts,
                    last_api_activity, last_container_activity, activity_count,
                    created_at, updated_at, deleted_at, deleted_by, api_key_id
                FROM sandboxes
                WHERE id = $1 AND api_key_id = $2 AND deleted_at IS NULL
                "#,
                &[id, api_key_id],
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                error!(
                    "Failed to query sandbox {} owned by {}: {}",
                    id, api_key_id, e
                );
                return None;
            }
        };

        let row = row?;
        match row_to_sandbox(row) {
            Ok(s) => Some(s),
            Err(e) => {
                error!("Failed to deserialize sandbox {} from row: {}", id, e);
                None
            }
        }
    }

    /// Retrieves a sandbox only if owned by the given API key, optionally
    /// including soft-deleted sandboxes.
    pub async fn fetch_sandbox_with_deleted_if_owned_by(
        &self,
        id: &uuid::Uuid,
        api_key_id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Option<Sandbox> {
        let client = match self.pool.get().await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to get database connection for fetch_sandbox_with_deleted_if_owned_by: {}", e);
                return None;
            }
        };
        let query = if include_deleted {
            r#"
            SELECT
                id, image, name, environment, port_mappings, resource_limits,
                volumes, command, inactivity_timeout_minutes, pull_policy,
                features, enable_all_features, vnc_resolution,
                state, container_id, error_message, volume_mounts,
                last_api_activity, last_container_activity, activity_count,
                created_at, updated_at, deleted_at, deleted_by, api_key_id
            FROM sandboxes
            WHERE id = $1 AND api_key_id = $2
            "#
        } else {
            r#"
            SELECT
                id, image, name, environment, port_mappings, resource_limits,
                volumes, command, inactivity_timeout_minutes, pull_policy,
                features, enable_all_features, vnc_resolution,
                state, container_id, error_message, volume_mounts,
                last_api_activity, last_container_activity, activity_count,
                created_at, updated_at, deleted_at, deleted_by, api_key_id
            FROM sandboxes
            WHERE id = $1 AND api_key_id = $2 AND deleted_at IS NULL
            "#
        };

        let row = match client.query_opt(query, &[id, api_key_id]).await {
            Ok(r) => r,
            Err(e) => {
                error!(
                    "Failed to query sandbox {} owned by {} (include_deleted={}): {}",
                    id, api_key_id, include_deleted, e
                );
                return None;
            }
        };
        let row = row?;
        match row_to_sandbox(row) {
            Ok(s) => Some(s),
            Err(e) => {
                error!("Failed to deserialize sandbox {} from row: {}", id, e);
                None
            }
        }
    }

    /// Soft deletes a sandbox by marking it as deleted.
    ///
    /// This method sets deleted_at and deleted_by, and updates state to 'destroying'.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to soft delete
    /// * `deleted_by` - Optional identifier of who/what deleted the sandbox
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Soft delete successful
    /// * `Err(StoreError::NotFound)` - Sandbox not found
    /// * `Err(...)` - Database error
    pub async fn soft_delete_sandbox(
        &self,
        id: &uuid::Uuid,
        deleted_by: Option<String>,
    ) -> Result<(), StoreError> {
        let client = self.pool.get().await.map_err(|e| {
            error!("Failed to get client from pool: {:?}", e);
            StoreError::Message(e.to_string())
        })?;

        debug!(
            "Soft deleting sandbox {} (deleted_by: {:?})",
            id, deleted_by
        );

        let rows_affected = client
            .execute(
                r#"
                UPDATE sandboxes SET
                    deleted_at = NOW(),
                    deleted_by = $2,
                    state = 'destroying',
                    updated_at = NOW()
                WHERE id = $1 AND deleted_at IS NULL
                "#,
                &[id, &deleted_by],
            )
            .await
            .map_err(|e| {
                error!("Failed to execute soft delete query: {:?}", e);
                StoreError::Postgres(e)
            })?;

        if rows_affected == 0 {
            return Err(StoreError::NotFound(*id));
        }

        info!("Soft deleted sandbox {}", id);
        Ok(())
    }

    /// Restores a soft-deleted sandbox.
    ///
    /// This method clears deleted_at and deleted_by fields, effectively restoring the sandbox.
    /// The state is set to 'stopped' to allow the sandbox to be started again.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to restore
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Restore successful
    /// * `Err(StoreError::NotFound)` - Sandbox not found or not deleted
    /// * `Err(...)` - Database error
    pub async fn restore_sandbox(&self, id: &uuid::Uuid) -> Result<(), StoreError> {
        let client = self.pool.get().await.map_err(|e| {
            error!("Failed to get client from pool: {:?}", e);
            StoreError::Message(e.to_string())
        })?;

        debug!("Restoring sandbox {}", id);

        let rows_affected = client
            .execute(
                r#"
                UPDATE sandboxes SET
                    deleted_at = NULL,
                    deleted_by = NULL,
                    state = 'stopped',
                    updated_at = NOW()
                WHERE id = $1 AND deleted_at IS NOT NULL
                "#,
                &[id],
            )
            .await
            .map_err(|e| {
                error!("Failed to execute restore query: {:?}", e);
                StoreError::Postgres(e)
            })?;

        if rows_affected == 0 {
            return Err(StoreError::NotFound(*id));
        }

        info!("Restored sandbox {}", id);
        Ok(())
    }

    /// Permanently deletes a sandbox from the database.
    ///
    /// This method performs a hard delete, removing the sandbox record entirely.
    /// Use this for cleanup after the retention period has expired.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to permanently delete
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Permanent delete successful (even if sandbox didn't exist)
    /// * `Err(...)` - Database error during delete
    pub async fn permanently_delete_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.pool.get().await.map_err(|e| {
            error!("Failed to get client from pool: {:?}", e);
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        debug!("Permanently deleting sandbox {}", id);

        client
            .execute("DELETE FROM sandboxes WHERE id = $1", &[id])
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        info!("Permanently deleted sandbox {}", id);
        Ok(())
    }

    /// Permanently deletes all sandboxes past their retention period.
    ///
    /// This method permanently deletes all soft-deleted sandboxes where
    /// deleted_at is older than the specified retention period in days.
    ///
    /// # Arguments
    ///
    /// * `retention_days` - Number of days to retain deleted sandboxes
    ///
    /// # Returns
    ///
    /// * `Ok(count)` - Number of sandboxes permanently deleted
    /// * `Err(...)` - Database error
    pub async fn cleanup_expired_sandboxes(
        &self,
        retention_days: i64,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.pool.get().await.map_err(|e| {
            error!("Failed to get client from pool: {:?}", e);
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        debug!(
            "Cleaning up sandboxes deleted more than {} days ago",
            retention_days
        );

        let rows_affected = client
            .execute(
                r#"
                DELETE FROM sandboxes
                WHERE deleted_at IS NOT NULL
                  AND deleted_at < NOW() - (MAKE_INTERVAL(secs => $1))
                "#,
                &[&((retention_days * 86400) as f64)], // Convert days to seconds as f64 for MAKE_INTERVAL
            )
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        info!("Permanently deleted {} expired sandboxes", rows_affected);
        Ok(rows_affected as usize)
    }

    /// Recovers sandboxes stuck in "Creating" state.
    ///
    /// When the server restarts, sandboxes that were in "Creating" state
    /// may be left stuck. This method recovers them by:
    /// 1. Finding sandboxes in "Creating" state for more than the timeout period
    /// 2. Updating their state to "Failed" if they don't have a container_id
    /// 3. Updating their state to "Running" if they have a container_id (container exists)
    ///
    /// # Arguments
    ///
    /// * `timeout_secs` - Timeout in seconds for considering a sandbox "stuck" (default: 300 = 5 minutes)
    ///
    /// # Returns
    ///
    /// * `Ok((recovered_count, failed_count))` - Number of sandboxes recovered to Running, and failed
    /// * `Err(...)` - Database error
    pub async fn recover_stuck_sandboxes(
        &self,
        timeout_secs: i64,
    ) -> Result<(usize, usize), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.pool.get().await.map_err(|e| {
            error!("Failed to get client from pool: {:?}", e);
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        debug!(
            "Recovering sandboxes stuck in Creating state for more than {} seconds",
            timeout_secs
        );

        // Find sandboxes in Creating state that are older than timeout
        let rows = client
            .query(
                r#"
                SELECT id, container_id, state, created_at
                FROM sandboxes
                WHERE state = 'Creating'
                  AND created_at < NOW() - (MAKE_INTERVAL(secs => $1))
                ORDER BY created_at ASC
                "#,
                &[&(timeout_secs as f64)],
            )
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let mut recovered_count = 0;
        let mut failed_count = 0;

        for row in rows {
            let id: uuid::Uuid = row.get("id");
            let container_id: Option<String> = row.get("container_id");

            if container_id.is_some() {
                // Sandbox has a container_id - assume it's running
                client
                    .execute(
                        "UPDATE sandboxes SET state = 'Running', updated_at = NOW() WHERE id = $1",
                        &[&id],
                    )
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

                info!("Recovered stuck sandbox {} (has container) -> Running", id);
                recovered_count += 1;
            } else {
                // Sandbox has no container_id - mark as failed
                let error_msg = "Sandbox creation timed out during server restart";
                client
                    .execute(
                        "UPDATE sandboxes SET state = 'Failed', error_message = $1, updated_at = NOW() WHERE id = $2",
                        &[&error_msg, &id],
                    )
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

                info!("Failed stuck sandbox {} (no container) -> Failed", id);
                failed_count += 1;
            }
        }

        info!(
            "Recovered {} stuck sandboxes ({} recovered to Running, {} marked as Failed)",
            recovered_count + failed_count,
            recovered_count,
            failed_count
        );
        Ok((recovered_count, failed_count))
    }
}
