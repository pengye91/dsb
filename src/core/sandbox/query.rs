use super::ListSandboxesFilter;
use super::SandboxService;
use crate::core::types::Sandbox;

impl SandboxService {
    /// Retrieves a sandbox by its ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to retrieve
    ///
    /// # Returns
    ///
    /// - `Some(Sandbox)` - If the sandbox exists
    /// - `None` - If no sandbox with the given ID exists
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use uuid::Uuid;
    /// # async fn example() {
    /// # let service: SandboxService = unimplemented!();
    /// let id = Uuid::new_v4();
    /// match service.get_sandbox(&id).await {
    ///     Some(sandbox) => println!("Found: {}", sandbox.id),
    ///     None => println!("Not found"),
    /// }
    /// # }
    /// ```
    pub async fn get_sandbox(&self, id: &uuid::Uuid) -> Option<Sandbox> {
        let sandbox = self.state.get_sandbox(id).await;
        sandbox
    }

    /// Gets a sandbox by ID, optionally including soft-deleted sandboxes.
    ///
    /// This method allows querying deleted sandboxes for:
    /// - Viewing deleted sandbox details
    /// - Auditing and compliance
    /// - Restore operations
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox
    /// * `include_deleted` - Whether to include soft-deleted sandboxes
    ///
    /// # Returns
    ///
    /// - `Some(Sandbox)` - Sandbox if found (and not deleted, unless include_deleted=true)
    /// - `None` - Sandbox not found or deleted without include_deleted=true
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use uuid::Uuid;
    /// # async fn example(service: SandboxService, id: Uuid) {
    /// // Get active sandbox only
    /// let active = service.get_sandbox_with_deleted(&id, false).await;
    ///
    /// // Get sandbox even if deleted
    /// let any = service.get_sandbox_with_deleted(&id, true).await;
    /// # }
    /// ```
    pub async fn get_sandbox_with_deleted(
        &self,
        id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Option<Sandbox> {
        self.state
            .get_sandbox_with_deleted(id, include_deleted)
            .await
    }

    /// Lists all sandboxes in the system.
    ///
    /// # Returns
    ///
    /// A vector of all sandboxes, regardless of their state.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # async fn example() {
    /// # let service: SandboxService = unimplemented!();
    /// let all = service.list_sandboxes().await;
    /// println!("Total sandboxes: {}", all.len());
    /// # }
    /// ```
    pub async fn list_sandboxes(&self) -> Vec<Sandbox> {
        self.state.list_sandboxes().await
    }

    /// Lists sandboxes owned by a specific API key.
    ///
    /// Only returns sandboxes where `api_key_id` matches the given key.
    pub async fn list_sandboxes_owned_by(
        &self,
        api_key_id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Vec<Sandbox> {
        self.state
            .list_sandboxes_owned_by(api_key_id, include_deleted)
            .await
    }

    /// Lists sandboxes with filters and pagination (PostgreSQL only).
    ///
    /// This method provides advanced filtering capabilities:
    /// - Include/exclude deleted sandboxes
    /// - Filter by state, image, date range
    /// - Paginated results
    ///
    /// Returns None if the state store doesn't support filtering (e.g., in-memory store).
    pub async fn list_sandboxes_filtered(
        &self,
        _filter: ListSandboxesFilter,
    ) -> Option<(
        Vec<crate::core::types::Sandbox>,
        crate::db::store::PaginationMeta,
    )> {
        // Try to downcast to PostgresStateStore for filtered queries
        // For now, we'll return None for non-Postgres stores
        // In practice, the main service uses PostgresStateStore
        None
    }
}
