use crate::core::store_trait::StateStoreTrait;
use crate::core::types::{ApiKeyIdentity, ApiKeyType};
use crate::core::manager::SandboxManager;
use crate::core::types::ActivityType;
use std::sync::Arc;
use crate::core::activities::ActivityService;
use super::SandboxService;

impl SandboxService {
    /// Creates a new sandbox service instance without activity tracking.
    ///
    /// # Arguments
    ///
    /// * `backend` - Sandbox manager backend for container operations
    /// * `state` - State store for persisting sandbox state (any implementation of StateStoreTrait)
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use dsb::docker::DockerManager;
    /// # use dsb::core::StateStore;
    /// # use std::sync::Arc;
    /// # fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// let docker = DockerManager::new()?;
    /// let state = StateStore::new();
    /// let service = SandboxService::new(Arc::new(docker), Arc::new(state));
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(
        backend: Arc<dyn SandboxManager>,
        state: Arc<dyn StateStoreTrait + Send + Sync>,
    ) -> Self {
        Self {
            backend,
            state,
            activity_service: None,
            default_inactivity_timeout: 30, // Default 30 minutes
            cleanup_dry_run: false,
            state_monitor_interval: 60,         // Default 60 seconds
            deleted_sandbox_retention_days: 15, // Default 15 days
            default_sandbox_image: "dsb/sandbox:latest".to_string(),
            authentication_required: true,
            max_file_size_bytes: 10 * 1024 * 1024, // Default 10MB
            tool_timeouts: Default::default(),
            default_resource_limits: Default::default(),
            max_browser_tabs: 20,
        }
    }

    /// Creates a new sandbox service instance with activity tracking.
    ///
    /// # Arguments
    ///
    /// * `backend` - Sandbox manager backend for container operations
    /// * `state` - State store for persisting sandbox state
    /// * `activity_service` - Activity service for tracking operations
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use dsb::docker::DockerManager;
    /// # use dsb::db::PostgresStateStore;
    /// # use dsb::core::ActivityService;
    /// # use std::sync::Arc;
    /// # use deadpool_postgres::Pool;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let pool: Pool = unimplemented!();
    /// let docker = DockerManager::new()?;
    /// let state = Arc::new(PostgresStateStore::new(pool.clone()).await?);
    /// let activity_service = Arc::new(ActivityService::new(pool));
    /// let service = SandboxService::new_with_activity(Arc::new(docker), state, activity_service);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_with_activity(
        backend: Arc<dyn SandboxManager>,
        state: Arc<dyn StateStoreTrait + Send + Sync>,
        activity_service: Arc<crate::core::activities::ActivityService>,
    ) -> Self {
        Self {
            backend,
            state,
            activity_service: Some(activity_service),
            default_inactivity_timeout: 30, // Default 30 minutes
            cleanup_dry_run: false,
            state_monitor_interval: 60,         // Default 60 seconds
            deleted_sandbox_retention_days: 15, // Default 15 days
            default_sandbox_image: "dsb/sandbox:latest".to_string(),
            authentication_required: true,
            max_file_size_bytes: 10 * 1024 * 1024, // Default 10MB
            tool_timeouts: Default::default(),
            default_resource_limits: Default::default(),
            max_browser_tabs: 20,
        }
    }

    /// Configure cleanup settings from application config.
    ///
    /// # Arguments
    ///
    /// * `default_inactivity_timeout` - Default inactivity timeout in minutes
    /// * `cleanup_dry_run` - If true, cleanup will log but not delete sandboxes
    /// * `state_monitor_interval` - State monitor check interval in seconds
    /// * `deleted_sandbox_retention_days` - Deleted sandbox retention period in days
    pub fn with_cleanup_config(
        mut self,
        default_inactivity_timeout: u64,
        cleanup_dry_run: bool,
        state_monitor_interval: u64,
        deleted_sandbox_retention_days: u64,
    ) -> Self {
        self.default_inactivity_timeout = default_inactivity_timeout;
        self.cleanup_dry_run = cleanup_dry_run;
        self.state_monitor_interval = state_monitor_interval;
        self.deleted_sandbox_retention_days = deleted_sandbox_retention_days;
        self
    }

    /// Configure frontend settings from application config.
    ///
    /// # Arguments
    ///
    /// * `default_sandbox_image` - Default sandbox image for the dashboard
    /// * `authentication_required` - Whether authentication is required (for UI display)
    pub fn with_frontend_config(
        mut self,
        default_sandbox_image: String,
        authentication_required: bool,
    ) -> Self {
        self.default_sandbox_image = default_sandbox_image;
        self.authentication_required = authentication_required;
        self
    }

    /// Configure file upload settings from application config.
    ///
    /// # Arguments
    ///
    /// * `max_file_size_bytes` - Maximum file size for sandbox file uploads/downloads in bytes
    pub fn with_file_upload_config(mut self, max_file_size_bytes: u64) -> Self {
        self.max_file_size_bytes = max_file_size_bytes;
        self
    }

    /// Configure default resource limits from application config.
    ///
    /// These limits are applied when creating sandboxes without explicit resource limits.
    /// Request-level limits take precedence over these defaults.
    ///
    /// # Arguments
    ///
    /// * `default_resource_limits` - Default resource limits configuration
    pub fn with_resource_limits(
        mut self,
        default_resource_limits: crate::config::DefaultResourceLimits,
    ) -> Self {
        self.default_resource_limits = default_resource_limits;
        self
    }

    /// Configure maximum browser tabs per sandbox.
    ///
    /// This limit is passed to sandbox containers as the `MAX_BROWSER_TABS` environment
    /// variable, which the Python browser tools use for FIFO tab eviction.
    pub fn with_max_browser_tabs(mut self, max_browser_tabs: u32) -> Self {
        self.max_browser_tabs = max_browser_tabs;
        self
    }

    /// Merges request resource limits with config defaults.
    ///
    /// Request-level limits take precedence over config defaults.
    /// For each limit field, the request value is used if present, otherwise the default is used.
    pub(super) fn merge_resource_limits(
        &self,
        request_limits: crate::core::types::ResourceLimits,
    ) -> crate::core::types::ResourceLimits {
        use crate::core::types::{ResourceLimits, Ulimit};

        // Convert config ulimits to core ulimits if present
        let merged_ulimits = if request_limits.ulimits.is_some() {
            // Request has ulimits, use them as-is
            request_limits.ulimits.clone()
        } else {
            self.default_resource_limits
                .ulimits
                .as_ref()
                .map(|config_ulimits| {
                    config_ulimits
                        .iter()
                        .map(|u| Ulimit {
                            name: u.name.clone(),
                            soft: u.soft,
                            hard: u.hard,
                        })
                        .collect()
                })
        };

        ResourceLimits {
            // Use request value if present, otherwise use config default
            memory_mb: request_limits
                .memory_mb
                .or(self.default_resource_limits.memory_mb),
            cpu_quota: request_limits
                .cpu_quota
                .or(self.default_resource_limits.cpu_quota),
            cpu_period: request_limits
                .cpu_period
                .or(self.default_resource_limits.cpu_period),
            cpu_shares: request_limits
                .cpu_shares
                .or(self.default_resource_limits.cpu_shares),
            pids_limit: request_limits
                .pids_limit
                .or(self.default_resource_limits.pids_limit),
            ulimits: merged_ulimits,
        }
    }

    /// Checks if the provided API key identity has privileged access.
    ///
    /// Privileged keys (admin/legacy config keys) bypass ownership checks
    /// and have full access to all sandboxes.
    fn is_privileged_key(&self, identity: &ApiKeyIdentity) -> bool {
        matches!(identity.key_type, ApiKeyType::Privileged)
    }

    /// Checks if a sandbox is owned by the given API key identity.
    ///
    /// Returns `Ok(())` if:
    /// - The key is privileged (admin/legacy)
    /// - The sandbox is owned by this database key
    ///
    /// Returns `Err` with a 404 response if the sandbox is not found
    /// or not owned by this key.
    pub async fn check_sandbox_ownership(
        &self,
        identity: &ApiKeyIdentity,
        sandbox_id: &uuid::Uuid,
    ) -> Result<(), crate::core::errors::ApiError> {
        self.check_sandbox_ownership_with_deleted(identity, sandbox_id, false)
            .await
    }

    /// Checks if a sandbox is owned by the given API key identity, optionally
    /// including soft-deleted sandboxes in the authorization lookup.
    pub async fn check_sandbox_ownership_with_deleted(
        &self,
        identity: &ApiKeyIdentity,
        sandbox_id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Result<(), crate::core::errors::ApiError> {
        // Privileged keys bypass ownership checks
        if self.is_privileged_key(identity) {
            return Ok(());
        }

        let Some(api_key_id) = identity.id else {
            // Database key without an ID should not happen, but treat as unauthorized
            return Err(crate::core::errors::ApiError::SandboxNotFound(
                sandbox_id.to_string(),
            ));
        };

        match self
            .state
            .get_sandbox_with_deleted_if_owned_by(sandbox_id, &api_key_id, include_deleted)
            .await
        {
            Some(_) => Ok(()),
            None => {
                if let Some(sandbox) = self
                    .state
                    .get_sandbox_with_deleted(sandbox_id, include_deleted)
                    .await
                {
                    tracing::warn!(
                        sandbox_id = %sandbox_id,
                        api_key_id = %api_key_id,
                        owner_api_key_id = ?sandbox.api_key_id,
                        include_deleted,
                        reason = "ownership_mismatch",
                        "Sandbox ownership check failed"
                    );
                }
                // Return 404 to avoid leaking sandbox existence
                Err(crate::core::errors::ApiError::SandboxNotFound(
                    sandbox_id.to_string(),
                ))
            }
        }
    }

    /// Records an activity event if activity service is available.
    ///
    /// This is a fire-and-forget operation - failures are logged but don't affect
    /// the parent operation.
    pub(super) async fn record_activity(
        &self,
        sandbox_id: uuid::Uuid,
        activity_type: ActivityType,
        details: serde_json::Value,
    ) {
        if let Some(activity_service) = &self.activity_service {
            activity_service
                .record_activity_async(sandbox_id, activity_type, details)
                .await;
        }
    }

    /// Waits for the tool_proxy service to become healthy after container start.
    ///
    /// This method polls the tool_proxy `/health` endpoint to verify that the
    /// service is ready to accept tool execution requests. This is necessary because
    /// tool_proxy.py (running on port 8080) may take several seconds to initialize,
    /// especially for browser tools which need to connect to Chromium via CDP.
    ///
    /// # Arguments
    ///
    /// * `container_id` - The Docker container ID
    /// * `timeout_secs` - Maximum time to wait for health check (default: 30 seconds)
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Tool_proxy is healthy and ready
    /// - `Err(...)` - Timeout or health check failed
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// service.wait_for_tool_health("container-123", Some(30)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub(super) async fn wait_for_tool_health(
        &self,
        container_id: &str,
        timeout_secs: Option<u64>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::time::{interval, Duration};

        let timeout = Duration::from_secs(timeout_secs.unwrap_or(30));
        let poll_interval = Duration::from_millis(100);
        let start_time = std::time::Instant::now();

        tracing::info!(
            container_id = %container_id,
            timeout_secs = timeout_secs.unwrap_or(30),
            "Waiting for tool_proxy to become healthy"
        );

        let mut interval = interval(poll_interval);

        loop {
            // Check timeout
            if start_time.elapsed() >= timeout {
                return Err(format!(
                    "Tool_proxy health check timed out after {} seconds",
                    timeout.as_secs()
                )
                .into());
            }

            // Tick the interval to wait before first check
            interval.tick().await;

            // Try health check
            match self
                .backend
                .exec_http(container_id, "/health", "GET", None, Some(2))
                .await
            {
                Ok(response) => {
                    // Check if response indicates healthy status AND browser is connected
                    let status = response.get("status").and_then(|v| v.as_str());
                    let browser_connected =
                        response.get("browser_connected").and_then(|v| v.as_bool());

                    if status == Some("healthy") && browser_connected == Some(true) {
                        tracing::info!(
                            container_id = %container_id,
                            elapsed_secs = start_time.elapsed().as_secs(),
                            browser_connected = true,
                            "Tool_proxy is healthy with browser connected"
                        );
                        return Ok(());
                    } else if status == Some("healthy") && browser_connected == Some(false) {
                        tracing::debug!(
                            container_id = %container_id,
                            browser_connected = false,
                            "Tool_proxy is healthy but browser not yet connected, waiting..."
                        );
                    } else {
                        tracing::debug!(
                            container_id = %container_id,
                            response = ?response,
                            "Health check returned unexpected status"
                        );
                    }
                }
                Err(e) => {
                    // Health check not ready yet - log and continue polling
                    tracing::debug!(
                        container_id = %container_id,
                        error = %e,
                        "Health check not ready, retrying..."
                    );
                }
            }
        }
    }

    /// Gets the activity service if available.
    ///
    /// Returns None if activity tracking is not enabled (e.g., using in-memory backend).
    pub fn get_activity_service(&self) -> Option<&Arc<ActivityService>> {
        self.activity_service.as_ref()
    }

}
