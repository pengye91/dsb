use crate::db::store::helpers::{row_to_sandbox, serialize_sandbox_fields};
use crate::db::store::{
    PaginationMeta, PostgresStateStore, Sandbox, SandboxListFilters, SandboxListResponse,
    StoreError,
};
use deadpool_postgres::Pool;
use tracing::{debug, error, info};

impl PostgresStateStore {
    /// Creates a new PostgreSQL-backed state store.
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool
    ///
    /// # Returns
    ///
    /// * `Ok(Self)` - Store ready to use
    /// * `Err(...)` - Database connection error
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::db::PostgresStateStore;
    /// # use dsb::db::pool::create_pool_from_env;
    /// # use dsb::db::StoreError;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// let pool = create_pool_from_env().await
    ///     .map_err(|e| Box::new(StoreError::from(e)) as Box<dyn std::error::Error + Send + Sync>)?;
    /// let store = PostgresStateStore::new(pool).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(pool: Pool) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Verify connection by pinging the database
        let client = pool
            .get()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        let _rows = client.query_one("SELECT 1", &[]).await?;

        info!("PostgresStateStore created and connected to database");

        Ok(Self { pool })
    }

    /// Creates a new sandbox in the database.
    ///
    /// # Arguments
    ///
    /// * `sandbox` - The sandbox instance to store
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Sandbox stored successfully
    /// * `Err(...)` - Database error
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - Database connection fails
    /// - INSERT operation fails
    /// - Unique constraint violation (duplicate ID)
    pub async fn create_sandbox(
        &self,
        sandbox: Sandbox,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!("About to get pool connection for sandbox {}", sandbox.id);
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        debug!("Got pool connection for sandbox {}", sandbox.id);

        debug!(
            "Creating sandbox {} with image {}",
            sandbox.id, sandbox.config.image
        );

        // Convert u64 to i64 for PostgreSQL
        let timeout = sandbox.config.inactivity_timeout_minutes.map(|v| v as i64);

        // Serialize complex types to JSONB using helper
        let fields = serialize_sandbox_fields(&sandbox)?;
        debug!("Serialized fields for sandbox {}", sandbox.id);
        client.execute(
            r#"
            INSERT INTO sandboxes (
                id, image, name, environment, port_mappings, resource_limits,
                volumes, command, inactivity_timeout_minutes, pull_policy,
                features, enable_all_features, vnc_resolution,
                state, container_id, error_message, volume_mounts,
                last_api_activity, last_container_activity, activity_count,
                created_at, updated_at, deleted_at, deleted_by, api_key_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25)
            "#,
            &[
                &sandbox.id,
                &sandbox.config.image,
                &sandbox.config.name,
                &fields.environment,
                &fields.port_mappings,
                &fields.resource_limits,
                &fields.volumes,
                &fields.command,
                &timeout,
                &sandbox.config.pull_policy.as_str(),
                &fields.features,
                &sandbox.config.enable_all_features,
                &sandbox.config.vnc_resolution,
                &sandbox.state.as_str(),
                &sandbox.container_id,
                &sandbox.error_message,
                &fields.volume_mounts,
                &sandbox.activity.last_api_activity,
                &sandbox.activity.last_container_activity,
                &(sandbox.activity.activity_count as i64),
                &sandbox.created_at,
                &sandbox.updated_at,
                &sandbox.deleted_at,
                &sandbox.deleted_by,
                &sandbox.api_key_id,
            ],
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        info!("Created sandbox {}", sandbox.id);

        Ok(())
    }

    /// Retrieves a sandbox by its ID, optionally including deleted sandboxes.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to retrieve
    /// * `include_deleted` - Whether to include soft-deleted sandboxes
    ///
    /// # Returns
    ///
    /// - `Some(Sandbox)` - If sandbox exists
    /// - `None` - If sandbox doesn't exist
    pub async fn get_sandbox_with_deleted(
        &self,
        id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Option<Sandbox> {
        let client = match self.pool.get().await {
            Ok(c) => c,
            Err(e) => {
                error!(
                    "Failed to get database connection for get_sandbox_with_deleted: {}",
                    e
                );
                return None;
            }
        };

        debug!(
            "Retrieving sandbox {} (include_deleted={})",
            id, include_deleted
        );

        let query = if include_deleted {
            r#"
                SELECT
                    id, image, name, environment, port_mappings, resource_limits,
                    volumes, inactivity_timeout_minutes, pull_policy,
                    features, enable_all_features, vnc_resolution,
                    state, container_id, error_message, volume_mounts,
                    last_api_activity, last_container_activity, activity_count,
                    created_at, updated_at, deleted_at, deleted_by, api_key_id
                FROM sandboxes
                WHERE id = $1
                "#
        } else {
            r#"
                SELECT
                    id, image, name, environment, port_mappings, resource_limits,
                    volumes, inactivity_timeout_minutes, pull_policy,
                    features, enable_all_features, vnc_resolution,
                    state, container_id, error_message, volume_mounts,
                    last_api_activity, last_container_activity, activity_count,
                    created_at, updated_at, deleted_at, deleted_by, api_key_id
                FROM sandboxes
                WHERE id = $1 AND deleted_at IS NULL
                "#
        };

        let row = match client.query_one(query, &[id]).await {
            Ok(r) => r,
            Err(e) => {
                error!(
                    "Failed to query sandbox {} (include_deleted={}): {}",
                    id, include_deleted, e
                );
                return None;
            }
        };

        match row_to_sandbox(row) {
            Ok(sandbox) => {
                debug!("Retrieved sandbox {}", id);
                Some(sandbox)
            }
            Err(e) => {
                error!("Failed to deserialize sandbox {} from row: {}", id, e);
                None
            }
        }
    }

    /// Retrieves a sandbox by ID (excludes deleted by default).
    ///
    /// This is a convenience method that calls get_sandbox_with_deleted with include_deleted=false.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to retrieve
    ///
    /// # Returns
    ///
    /// - `Some(Sandbox)` - If sandbox exists and is not deleted
    /// - `None` - If sandbox doesn't exist or is deleted
    pub async fn get_sandbox_simple(&self, id: &uuid::Uuid) -> Option<Sandbox> {
        self.get_sandbox_with_deleted(id, false).await
    }

    /// Retrieves a sandbox by ID (excludes deleted by default).
    ///
    /// This is the main method that matches the StateStoreTrait.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to retrieve
    ///
    /// # Returns
    ///
    /// - `Some(Sandbox)` - If sandbox exists and is not deleted
    /// - `None` - If sandbox doesn't exist or is deleted
    pub async fn get_sandbox(&self, id: &uuid::Uuid) -> Option<Sandbox> {
        self.get_sandbox_with_deleted(id, false).await
    }

    /// Lists all sandboxes.
    ///
    /// # Returns
    ///
    /// A vector containing all sandboxes. Empty vector if no sandboxes exist.
    pub async fn list_sandboxes(&self) -> Vec<Sandbox> {
        tracing::debug!("PostgresStateStore::list_sandboxes called");

        // Always fetch ALL sandboxes including deleted ones
        // The API handler will filter based on the include_deleted parameter
        let filters = SandboxListFilters {
            include_deleted: true,
            ..Default::default()
        };

        let result = match self.list_sandboxes_filtered(Some(filters)).await {
            Ok(response) => {
                tracing::debug!(
                    "list_sandboxes_filtered returned {} sandboxes",
                    response.data.len()
                );
                response.data
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    operation = "list_sandboxes_filtered",
                    "Database operation failed"
                );
                vec![]
            }
        };
        tracing::debug!(
            "PostgresStateStore::list_sandboxes returning {} sandboxes",
            result.len()
        );
        result
    }

    /// Lists sandboxes with optional filters and pagination.
    ///
    /// # Arguments
    ///
    /// * `filters` - Optional filters to apply (state, image, date ranges, pagination)
    ///
    /// # Returns
    ///
    /// * `Ok(SandboxListResponse)` - Paginated list of sandboxes with metadata
    /// * `Err(...)` - Database error
    pub async fn list_sandboxes_filtered(
        &self,
        filters: Option<SandboxListFilters>,
    ) -> Result<SandboxListResponse, StoreError> {
        let client = self.pool.get().await?;

        let filters = filters.unwrap_or_default();

        // Build WHERE clause dynamically
        let mut where_conditions = Vec::new();
        let mut state_str = None;
        let mut image_pattern = None;
        let mut created_after = None;
        let mut created_before = None;

        // Soft delete filter
        if !filters.include_deleted {
            where_conditions.push("deleted_at IS NULL".to_string());
        }

        // State filter
        if let Some(state) = &filters.state {
            where_conditions.push("state = $2".to_string());
            state_str = Some(state.as_str());
        }

        // Image filter (partial match)
        if let Some(image) = &filters.image {
            if state_str.is_none() {
                where_conditions.push("image LIKE $2".to_string());
                image_pattern = Some(format!("%{}%", image));
            } else {
                where_conditions.push("image LIKE $3".to_string());
                image_pattern = Some(format!("%{}%", image));
            }
        }

        // Created after filter
        if let Some(after) = &filters.created_after {
            let param_num =
                2 + state_str.as_ref().map_or(0, |_| 1) + image_pattern.as_ref().map_or(0, |_| 1);
            where_conditions.push(format!("created_at >= ${}", param_num));
            created_after = Some(after);
        }

        // Created before filter
        if let Some(before) = &filters.created_before {
            let param_num = 2
                + state_str.as_ref().map_or(0, |_| 1)
                + image_pattern.as_ref().map_or(0, |_| 1)
                + created_after.as_ref().map_or(0, |_| 1);
            where_conditions.push(format!("created_at <= ${}", param_num));
            created_before = Some(before);
        }

        let where_clause = if where_conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_conditions.join(" AND "))
        };

        // Get total count
        let count_query = format!("SELECT COUNT(*) FROM sandboxes {}", where_clause);

        let count_row: i64 = if let Some(state) = state_str {
            if let Some(image) = &image_pattern {
                if let Some(after) = &created_after {
                    if let Some(before) = &created_before {
                        client
                            .query_one(&count_query, &[&state, &image, &after, &before])
                            .await?
                            .get(0)
                    } else {
                        // after is Some, before is None
                        client
                            .query_one(&count_query, &[&state, &image, &after])
                            .await?
                            .get(0)
                    }
                } else if let Some(before) = &created_before {
                    client
                        .query_one(&count_query, &[&state, &before])
                        .await?
                        .get(0)
                } else {
                    client.query_one(&count_query, &[&state]).await?.get(0)
                }
            } else if let Some(after) = &created_after {
                client
                    .query_one(&count_query, &[&state, &after])
                    .await?
                    .get(0)
            } else if let Some(before) = &created_before {
                client
                    .query_one(&count_query, &[&state, &before])
                    .await?
                    .get(0)
            } else {
                client.query_one(&count_query, &[&state]).await?.get(0)
            }
        } else if let Some(image) = &image_pattern {
            if let Some(after) = &created_after {
                if let Some(before) = &created_before {
                    client
                        .query_one(&count_query, &[&image, &after, &before])
                        .await?
                        .get(0)
                } else {
                    client
                        .query_one(&count_query, &[&image, &after])
                        .await?
                        .get(0)
                }
            } else if let Some(before) = &created_before {
                client
                    .query_one(&count_query, &[&image, &before])
                    .await?
                    .get(0)
            } else {
                client.query_one(&count_query, &[&image]).await?.get(0)
            }
        } else if let Some(after) = &created_after {
            if let Some(before) = &created_before {
                client
                    .query_one(&count_query, &[&after, &before])
                    .await?
                    .get(0)
            } else {
                client.query_one(&count_query, &[&after]).await?.get(0)
            }
        } else if let Some(before) = &created_before {
            client.query_one(&count_query, &[&before]).await?.get(0)
        } else {
            client.query_one(&count_query, &[]).await?.get(0)
        };

        let total = count_row as usize;

        // Handle pagination
        let page = filters.page.unwrap_or(1).max(1);
        let per_page = filters.per_page.unwrap_or(50).clamp(1, 200);
        let offset = (page - 1) * per_page;
        let total_pages = if total == 0 {
            1
        } else {
            ((total as f64) / (per_page as f64)).ceil() as usize
        };

        // Calculate parameter offset based on WHERE clause parameters
        let where_param_count = state_str.as_ref().map_or(0, |_| 1)
            + image_pattern.as_ref().map_or(0, |_| 1)
            + created_after.as_ref().map_or(0, |_| 1)
            + created_before.as_ref().map_or(0, |_| 1);

        // Parameter offset = number of WHERE clause params + 1 (for first LIMIT parameter)
        let param_offset = where_param_count + 1;

        let select_query = format!(
            r#"
            SELECT
                id, image, name, environment, port_mappings, resource_limits,
                volumes, inactivity_timeout_minutes, pull_policy,
                features, enable_all_features, vnc_resolution,
                state, container_id, error_message, volume_mounts,
                last_api_activity, last_container_activity, activity_count,
                created_at, updated_at, deleted_at, deleted_by, api_key_id
            FROM sandboxes
            {}
            ORDER BY created_at DESC
            LIMIT ${} OFFSET ${}
            "#,
            where_clause,
            param_offset,
            param_offset + 1
        );

        let per_page_val = per_page as i64;
        let offset_val = offset as i64;

        let rows = if let Some(state) = state_str {
            if let Some(image) = &image_pattern {
                if let Some(after) = &created_after {
                    if let Some(before) = &created_before {
                        client
                            .query(
                                &select_query,
                                &[&state, &image, &after, &before, &per_page_val, &offset_val],
                            )
                            .await?
                    } else {
                        client
                            .query(
                                &select_query,
                                &[&state, &image, &after, &per_page_val, &offset_val],
                            )
                            .await?
                    }
                } else if let Some(before) = &created_before {
                    client
                        .query(
                            &select_query,
                            &[&state, &image, &before, &per_page_val, &offset_val],
                        )
                        .await?
                } else {
                    client
                        .query(&select_query, &[&state, &image, &per_page_val, &offset_val])
                        .await?
                }
            } else if let Some(after) = &created_after {
                client
                    .query(&select_query, &[&state, &after, &per_page_val, &offset_val])
                    .await?
            } else if let Some(before) = &created_before {
                client
                    .query(
                        &select_query,
                        &[&state, &before, &per_page_val, &offset_val],
                    )
                    .await?
            } else {
                client
                    .query(&select_query, &[&state, &per_page_val, &offset_val])
                    .await?
            }
        } else if let Some(image) = &image_pattern {
            if let Some(after) = &created_after {
                if let Some(before) = &created_before {
                    client
                        .query(
                            &select_query,
                            &[&image, &after, &before, &per_page_val, &offset_val],
                        )
                        .await?
                } else {
                    client
                        .query(&select_query, &[&image, &after, &per_page_val, &offset_val])
                        .await?
                }
            } else if let Some(before) = &created_before {
                client
                    .query(
                        &select_query,
                        &[&image, &before, &per_page_val, &offset_val],
                    )
                    .await?
            } else {
                client
                    .query(&select_query, &[&image, &per_page_val, &offset_val])
                    .await?
            }
        } else if let Some(after) = &created_after {
            if let Some(before) = &created_before {
                client
                    .query(
                        &select_query,
                        &[&after, &before, &per_page_val, &offset_val],
                    )
                    .await?
            } else {
                client
                    .query(&select_query, &[&after, &per_page_val, &offset_val])
                    .await?
            }
        } else if let Some(before) = &created_before {
            client
                .query(&select_query, &[&before, &per_page_val, &offset_val])
                .await?
        } else {
            client
                .query(&select_query, &[&per_page_val, &offset_val])
                .await?
        };

        let mut sandboxes = Vec::new();
        for row in rows {
            match row_to_sandbox(row) {
                Ok(sandbox) => sandboxes.push(sandbox),
                Err(e) => {
                    error!("Failed to convert row to sandbox: {}", e);
                }
            }
        }

        debug!(
            "Listed {} sandboxes (page {}, total: {})",
            sandboxes.len(),
            page,
            total
        );

        Ok(SandboxListResponse {
            data: sandboxes,
            pagination: PaginationMeta {
                page,
                per_page,
                total,
                total_pages,
                has_next: page < total_pages,
                has_prev: page > 1,
            },
        })
    }

    /// Updates an existing sandbox.
    ///
    /// # Arguments
    ///
    /// * `sandbox` - The sandbox to update (must have valid ID)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Update successful
    /// * `Err(...)` - Update failed
    pub async fn update_sandbox(
        &self,
        sandbox: &Sandbox,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        debug!("Updating sandbox {}", sandbox.id);

        // Convert u64 to i64 for PostgreSQL
        let timeout = sandbox.config.inactivity_timeout_minutes.map(|v| v as i64);

        // Serialize complex types to JSONB using helper
        let fields = serialize_sandbox_fields(sandbox)?;

        let rows_affected = client
            .execute(
                r#"
                UPDATE sandboxes SET
                    image = $2,
                    name = $3,
                    environment = $4,
                    port_mappings = $5,
                    resource_limits = $6,
                    volumes = $7,
                    command = $8,
                    inactivity_timeout_minutes = $9,
                    pull_policy = $10,
                    features = $11,
                    enable_all_features = $12,
                    vnc_resolution = $13,
                    state = $14,
                    container_id = $15,
                    error_message = $16,
                    volume_mounts = $17,
                    last_api_activity = $18,
                    last_container_activity = $19,
                    activity_count = $20,
                    updated_at = $21,
                    deleted_at = $22,
                    deleted_by = $23,
                    api_key_id = $24
                WHERE id = $1
                "#,
                &[
                    &sandbox.id,
                    &sandbox.config.image,
                    &sandbox.config.name,
                    &fields.environment,
                    &fields.port_mappings,
                    &fields.resource_limits,
                    &fields.volumes,
                    &fields.command,
                    &timeout,
                    &sandbox.config.pull_policy.as_str(),
                    &fields.features,
                    &sandbox.config.enable_all_features,
                    &sandbox.config.vnc_resolution,
                    &sandbox.state.as_str(),
                    &sandbox.container_id,
                    &sandbox.error_message,
                    &fields.volume_mounts,
                    &sandbox.activity.last_api_activity,
                    &sandbox.activity.last_container_activity,
                    &(sandbox.activity.activity_count as i64),
                    &sandbox.updated_at,
                    &sandbox.deleted_at,
                    &sandbox.deleted_by,
                    &sandbox.api_key_id,
                ],
            )
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        if rows_affected == 0 {
            return Err(Box::new(StoreError::from(format!(
                "Sandbox {} not found",
                sandbox.id
            ))) as Box<dyn std::error::Error + Send + Sync>);
        }

        info!("Updated sandbox {}", sandbox.id);
        Ok(())
    }

    /// Deletes a sandbox.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to delete
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Delete successful (even if sandbox didn't exist)
    /// * `Err(...)` - Database error during delete
    pub async fn delete_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.pool.get().await.map_err(|e| {
            error!("Failed to get client from pool: {:?}", e);
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        debug!("Deleting sandbox {}", id);

        client
            .execute("DELETE FROM sandboxes WHERE id = $1", &[id])
            .await
            .map_err(|e| {
                error!("Failed to execute delete query: {:?}", e);
                Box::new(e) as Box<dyn std::error::Error + Send + Sync>
            })?;

        info!("Deleted sandbox {}", id);
        Ok(())
    }
}
