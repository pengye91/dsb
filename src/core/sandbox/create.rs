use crate::core::features::{
    apply_feature_profile, build_feature_profile, FeatureSelection, ImageFeatureLabel,
};
use crate::core::types::{ApiKeyIdentity, Sandbox, SandboxConfig, SandboxState};
use crate::core::types::ActivityType;
use super::SandboxService;

impl SandboxService {
    /// Creates a new sandbox and starts it immediately.
    ///
    /// This method performs the complete sandbox initialization flow:
    ///
    /// 1. Generates a unique UUID for the sandbox
    /// 2. Creates a sandbox record with `state: Creating`
    /// 3. Stores it in the state store
    /// 4. Creates the Docker container
    /// 5. Starts the container
    /// 6. Updates state to `Running` or `Error` based on results
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the sandbox (image, ports, etc.)
    ///
    /// # Returns
    ///
    /// - `Ok(Sandbox)` - The created sandbox with its assigned UUID and state
    /// - `Err(...)` - If container creation or startup fails
    ///
    /// # Errors
    ///
    /// This method returns an error if:
    /// - Docker daemon is not accessible
    /// - The specified image doesn't exist locally
    /// - Port mappings conflict with existing bindings
    /// - Resource limits are invalid
    /// - Container startup fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::{SandboxService, SandboxConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// let config = SandboxConfig {
    ///     image: "nginx:latest".to_string(),
    ///     name: Some("my-server".to_string()),
    ///     ..Default::default()
    /// };
    ///
    /// let sandbox = service.create_sandbox(config, None).await?;
    /// println!("Created sandbox: {}", sandbox.id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_sandbox(
        &self,
        mut config: SandboxConfig,
        identity: Option<ApiKeyIdentity>,
    ) -> Result<Sandbox, Box<dyn std::error::Error + Send + Sync>> {
        let start = std::time::Instant::now();
        let id = uuid::Uuid::new_v4();

        tracing::info!(
            sandbox_id = %id,
            image = %config.image,
            name = ?config.name,
            pull_policy = ?config.pull_policy,
            "Starting sandbox creation"
        );

        // Apply default resource limits from config (request limits take precedence)
        config.resource_limits = self.merge_resource_limits(config.resource_limits.clone());

        // Set default container name if not provided
        // This ensures containers follow the naming convention that orphan cleanup expects
        if config.name.is_none() {
            config.name = Some(format!("sandbox-{}", id));
            tracing::debug!(
                sandbox_id = %id,
                container_name = %config.name.as_ref().unwrap(),
                "Set default container name for proper orphan cleanup tracking"
            );
        }

        let feature_selection = FeatureSelection {
            enabled: config.features.clone(),
            disabled: Vec::new(),
            enable_all: config.enable_all_features,
        };

        let now = chrono::Utc::now();

        // Create sandbox in Creating state
        let mut sandbox = Sandbox {
            id,
            config: config.clone(),
            state: SandboxState::Creating,
            container_id: None,
            created_at: now,
            updated_at: now,
            error_message: None,
            volume_mounts: config.volumes.clone(),
            activity: crate::core::types::ActivityTracking {
                last_api_activity: now,
                last_container_activity: None,
                activity_count: 0,
            },
            inactivity_timeout_minutes: config.inactivity_timeout_minutes,
            deleted_at: None,
            deleted_by: None,
            api_key_id: identity.as_ref().and_then(|i| i.id),
        };

        // Store in state
        self.state.create_sandbox(sandbox.clone()).await?;

        // Handle pull policy before creating container
        match config.pull_policy {
            crate::core::types::PullPolicy::Always => {
                tracing::debug!("Pull policy: Always - pulling image {}", config.image);
                if let Err(e) = self.backend.pull_image(&config.image).await {
                    tracing::trace!(
                        sandbox_id = %sandbox.id,
                        old_state = ?sandbox.state,
                        new_state = ?SandboxState::Error,
                        reason = "image_pull_failed",
                        "Sandbox state transition"
                    );
                    sandbox.state = SandboxState::Error;
                    sandbox.error_message = Some(format!("Failed to pull image: {}", e));
                    sandbox.updated_at = chrono::Utc::now();
                    self.state.update_sandbox(&sandbox).await?;
                    return Err(e.into());
                }
            }
            crate::core::types::PullPolicy::Missing => {
                match self.backend.image_exists(&config.image).await {
                    Ok(true) => {
                        tracing::debug!("Image {} exists locally, skipping pull", config.image);
                    }
                    Ok(false) => {
                        tracing::debug!("Image {} not found locally, pulling", config.image);
                        if let Err(e) = self.backend.pull_image(&config.image).await {
                            tracing::trace!(
                                sandbox_id = %sandbox.id,
                                old_state = ?sandbox.state,
                                new_state = ?SandboxState::Error,
                                reason = "image_pull_missing",
                                "Sandbox state transition"
                            );
                            sandbox.state = SandboxState::Error;
                            sandbox.error_message = Some(format!("Failed to pull image: {}", e));
                            sandbox.updated_at = chrono::Utc::now();
                            self.state.update_sandbox(&sandbox).await?;
                            return Err(e.into());
                        }
                    }
                    Err(e) => {
                        tracing::trace!(
                            sandbox_id = %sandbox.id,
                            old_state = ?sandbox.state,
                            new_state = ?SandboxState::Error,
                            reason = "image_check_failed",
                            "Sandbox state transition"
                        );
                        sandbox.state = SandboxState::Error;
                        sandbox.error_message =
                            Some(format!("Failed to check image existence: {}", e));
                        sandbox.updated_at = chrono::Utc::now();
                        self.state.update_sandbox(&sandbox).await?;
                        return Err(e.into());
                    }
                }
            }
            crate::core::types::PullPolicy::Never => {
                tracing::debug!("Pull policy: Never - using local image only");
                // Don't pull, let create_container fail if image doesn't exist
            }
        }

        self.apply_image_feature_profile(&id, &mut config, &feature_selection)
            .await;

        // Inject MAX_BROWSER_TABS env var for browser tab eviction
        if config.features.contains(&"browser".to_string()) {
            config.environment.insert(
                "MAX_BROWSER_TABS".to_string(),
                self.max_browser_tabs.to_string(),
            );
        }

        sandbox.config = config.clone();
        self.state.update_sandbox(&sandbox).await?;

        // Create container
        match self.backend.create(Some(&sandbox.id), &config).await {
            Ok(container_id) => {
                sandbox.container_id = Some(container_id.clone());
                sandbox.state = SandboxState::Created;

                // Start container immediately
                match self.backend.start(&container_id).await {
                    Ok(_) => {
                        // Only wait for tool_proxy health if features requiring it are enabled
                        // Features like browser, web, databend require supervisord + tool_proxy
                        // Custom commands (like ["sleep", "3600"]) don't start supervisord
                        // Check if command is supervisord (from features) or truly custom
                        let is_supervisord_command =
                            sandbox.config.command.as_ref().is_some_and(|cmd| {
                                cmd.iter().any(|arg| arg.contains("supervisord"))
                            });
                        let has_custom_command =
                            sandbox.config.command.is_some() && !is_supervisord_command;
                        let needs_tool_proxy = !has_custom_command
                            && (sandbox.config.enable_all_features
                                || sandbox.config.features.iter().any(|f| {
                                    matches!(
                                        f.as_str(),
                                        "browser" | "web" | "databend" | "vnc" | "ssh"
                                    )
                                }));

                        if needs_tool_proxy {
                            // Set state to Starting and return immediately
                            // Health check runs asynchronously in the background
                            sandbox.state = SandboxState::Starting;
                            sandbox.updated_at = chrono::Utc::now();
                            self.state.update_sandbox(&sandbox).await?;

                            // Record creation activity with Starting state
                            self.record_activity(
                                sandbox.id,
                                ActivityType::Create,
                                serde_json::json!({
                                    "image": config.image,
                                    "name": config.name,
                                    "state": "starting"
                                }),
                            )
                            .await;

                            tracing::info!(
                                sandbox_id = %sandbox.id,
                                container_id = ?sandbox.container_id,
                                state = ?sandbox.state,
                                duration_ms = start.elapsed().as_millis(),
                                "Sandbox created, health check running in background"
                            );

                            // Spawn background task for async health check
                            let sandbox_id = sandbox.id;
                            let container_id_clone = container_id.clone();
                            let self_clone = self.clone();
                            let state = self.state.clone();

                            tokio::spawn(async move {
                                match self_clone
                                    .wait_for_tool_health(&container_id_clone, Some(30))
                                    .await
                                {
                                    Ok(_) => {
                                        tracing::debug!(
                                            sandbox_id = %sandbox_id,
                                            "Async health check passed, setting to Running"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            sandbox_id = %sandbox_id,
                                            error = %e,
                                            "Async health check failed, marking sandbox as Running anyway"
                                        );
                                    }
                                }

                                // Transition to Running if still in Starting state
                                if let Some(mut sb) = state.get_sandbox(&sandbox_id).await {
                                    if matches!(sb.state, SandboxState::Starting) {
                                        sb.state = SandboxState::Running;
                                        sb.updated_at = chrono::Utc::now();
                                        let _ = state.update_sandbox(&sb).await;
                                    }
                                }
                            });

                            Ok(sandbox)
                        } else {
                            let reason = if has_custom_command {
                                "custom command specified"
                            } else {
                                "no tool_proxy-dependent features enabled"
                            };
                            tracing::debug!(
                                sandbox_id = %sandbox.id,
                                reason = reason,
                                "Skipping tool_proxy health check"
                            );

                            sandbox.state = SandboxState::Running;
                            sandbox.updated_at = chrono::Utc::now();
                            self.state.update_sandbox(&sandbox).await?;

                            // Record creation activity
                            self.record_activity(
                                sandbox.id,
                                ActivityType::Create,
                                serde_json::json!({
                                    "image": config.image,
                                    "name": config.name,
                                    "state": "running"
                                }),
                            )
                            .await;

                            tracing::info!(
                                sandbox_id = %sandbox.id,
                                container_id = ?sandbox.container_id,
                                state = ?sandbox.state,
                                duration_ms = start.elapsed().as_millis(),
                                "Sandbox created successfully"
                            );

                            Ok(sandbox)
                        }
                    }
                    Err(e) => {
                        sandbox.state = SandboxState::Error;
                        sandbox.error_message = Some(format!("Failed to start: {}", e));
                        sandbox.updated_at = chrono::Utc::now();
                        self.state.update_sandbox(&sandbox).await?;
                        Err(e.into())
                    }
                }
            }
            Err(e) => {
                sandbox.state = SandboxState::Error;
                sandbox.error_message = Some(format!("Failed to create: {}", e));
                sandbox.updated_at = chrono::Utc::now();
                self.state.update_sandbox(&sandbox).await?;
                Err(e.into())
            }
        }
    }

    async fn apply_image_feature_profile(
        &self,
        sandbox_id: &uuid::Uuid,
        config: &mut SandboxConfig,
        feature_selection: &FeatureSelection,
    ) {
        match self.backend.get_image_features(&config.image).await {
            Ok(image_details) => {
                if let Some(labels) = image_details.labels.as_ref() {
                    if let Some(features_json) = labels.get("com.dsb.features") {
                        match serde_json::from_str::<ImageFeatureLabel>(features_json) {
                            Ok(label) => {
                                let profile = build_feature_profile(label, feature_selection);
                                if let Err(error) =
                                    apply_feature_profile(config, &profile, &sandbox_id.to_string())
                                {
                                    tracing::warn!(
                                        sandbox_id = %sandbox_id,
                                        image = %config.image,
                                        error = %error,
                                        "Failed to apply image feature profile"
                                    );
                                } else {
                                    tracing::debug!(
                                        sandbox_id = %sandbox_id,
                                        features = ?config.features,
                                        command = ?config.command,
                                        exposed_ports = ?config.exposed_ports,
                                        "Applied image feature profile"
                                    );
                                }
                            }
                            Err(error) => {
                                tracing::warn!(
                                    sandbox_id = %sandbox_id,
                                    image = %config.image,
                                    error = %error,
                                    "Failed to parse image feature metadata"
                                );
                            }
                        }
                    }
                }

                tracing::debug!("Image features retrieved for {}", config.image);
            }
            Err(error) => {
                tracing::debug!(
                    sandbox_id = %sandbox_id,
                    image = %config.image,
                    error = %error,
                    "Failed to get image features (expected on K8s backend)"
                );
            }
        }
    }

    /// Creates a sandbox with real-time progress streaming.
    ///
    /// This method returns a channel receiver that streams progress events
    /// during sandbox creation, including image pulling, container creation,
    /// and container startup.
    ///
    /// # Arguments
    ///
    /// * `config` - Sandbox configuration
    ///
    /// # Returns
    ///
    /// - `Ok(Receiver)` - Channel receiver that streams progress events
    /// - `Err(...)` - If sandbox creation cannot be started
    ///
    /// # Progress Events
    ///
    /// The receiver will emit events in this order:
    /// 1. `Pulling` - Image pull progress (one or more events)
    /// 2. `Creating` - Container creation started
    /// 3. `Starting` - Container startup started
    /// 4. `Ready` - Sandbox is ready
    /// 5. `Error` - If any step fails (stream ends after this)
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::{SandboxService, SandboxConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// let config = SandboxConfig::default();
    /// let mut receiver = service.create_sandbox_with_progress(config).await?;
    ///
    /// while let Some(event) = receiver.recv().await {
    ///     match event {
    ///         dsb::core::types::SandboxProgressEvent::Pulling { status, .. } => {
    ///             println!("Pulling: {}", status);
    ///         }
    ///         dsb::core::types::SandboxProgressEvent::Ready { sandbox_id, .. } => {
    ///             println!("Ready! ID: {}", sandbox_id);
    ///         }
    ///         _ => {}
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_sandbox_with_progress(
        &self,
        mut config: SandboxConfig,
        identity: Option<ApiKeyIdentity>,
    ) -> Result<
        tokio::sync::mpsc::UnboundedReceiver<crate::core::types::SandboxProgressEvent>,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        let id = uuid::Uuid::new_v4();
        // Use unbounded channel to avoid blocking in async context
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        // Apply default resource limits from config (request limits take precedence)
        config.resource_limits = self.merge_resource_limits(config.resource_limits.clone());

        // Clone what we need for the background task
        let backend = self.backend.clone();
        let state = self.state.clone();
        let config_clone = config.clone();

        // Spawn background task to handle creation
        tokio::spawn(async move {
            let now = chrono::Utc::now();

            // Create initial sandbox record
            let mut sandbox = crate::core::types::Sandbox {
                id,
                config: config_clone.clone(),
                state: crate::core::types::SandboxState::Creating,
                container_id: None,
                created_at: now,
                updated_at: now,
                error_message: None,
                volume_mounts: config_clone.volumes.clone(),
                activity: crate::core::types::ActivityTracking {
                    last_api_activity: now,
                    last_container_activity: None,
                    activity_count: 0,
                },
                inactivity_timeout_minutes: config_clone.inactivity_timeout_minutes,
                deleted_at: None,
                deleted_by: None,
                api_key_id: identity.as_ref().and_then(|i| i.id),
            };

            // Store in state
            if let Err(e) = state.create_sandbox(sandbox.clone()).await {
                let _ = tx.send(crate::core::types::SandboxProgressEvent::Error {
                    message: format!("Failed to create sandbox record: {}", e),
                });
                return;
            }

            // Handle pull policy
            match config_clone.pull_policy {
                crate::core::types::PullPolicy::Always => {
                    let _ = tx.send(crate::core::types::SandboxProgressEvent::Pulling {
                        image: config_clone.image.clone(),
                        status: "Starting pull...".to_string(),
                        current: None,
                        total: None,
                    });

                    let image = config_clone.image.clone();
                    let tx2 = tx.clone();
                    let image_for_closure = image.clone();
                    if let Err(e) = backend
                        .pull_image_with_progress(
                            &image,
                            Box::new(move |status, current, total| {
                                let _ =
                                    tx2.send(crate::core::types::SandboxProgressEvent::Pulling {
                                        image: image_for_closure.clone(),
                                        status,
                                        current,
                                        total,
                                    });
                            }),
                        )
                        .await
                    {
                        sandbox.state = crate::core::types::SandboxState::Error;
                        sandbox.error_message = Some(format!("Failed to pull image: {}", e));
                        sandbox.updated_at = chrono::Utc::now();
                        let _ = state.update_sandbox(&sandbox).await;
                        let _ = tx.send(crate::core::types::SandboxProgressEvent::Error {
                            message: format!("Failed to pull: {}", e),
                        });
                        return;
                    }
                }
                crate::core::types::PullPolicy::Missing => {
                    match backend.image_exists(&config_clone.image).await {
                        Ok(true) => {
                            let _ = tx.send(crate::core::types::SandboxProgressEvent::Pulling {
                                image: config_clone.image.clone(),
                                status: "Image exists locally".to_string(),
                                current: Some(0),
                                total: Some(0),
                            });
                        }
                        Ok(false) => {
                            let _ = tx.send(crate::core::types::SandboxProgressEvent::Pulling {
                                image: config_clone.image.clone(),
                                status: "Starting pull...".to_string(),
                                current: None,
                                total: None,
                            });

                            let image = config_clone.image.clone();
                            let tx2 = tx.clone();
                            let image_for_closure = image.clone();
                            if let Err(e) = backend
                                .pull_image_with_progress(
                                    &image,
                                    Box::new(move |status, current, total| {
                                        let _ = tx2.send(
                                            crate::core::types::SandboxProgressEvent::Pulling {
                                                image: image_for_closure.clone(),
                                                status,
                                                current,
                                                total,
                                            },
                                        );
                                    }),
                                )
                                .await
                            {
                                sandbox.state = crate::core::types::SandboxState::Error;
                                sandbox.error_message =
                                    Some(format!("Failed to pull image: {}", e));
                                sandbox.updated_at = chrono::Utc::now();
                                let _ = state.update_sandbox(&sandbox).await;
                                let _ = tx.send(crate::core::types::SandboxProgressEvent::Error {
                                    message: format!("Failed to pull: {}", e),
                                });
                                return;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(crate::core::types::SandboxProgressEvent::Error {
                                message: format!("Failed to check image: {}", e),
                            });
                            return;
                        }
                    }
                }
                crate::core::types::PullPolicy::Never => {
                    let _ = tx.send(crate::core::types::SandboxProgressEvent::Pulling {
                        image: config_clone.image.clone(),
                        status: "Using local image".to_string(),
                        current: Some(0),
                        total: Some(0),
                    });
                }
            }

            let feature_selection = FeatureSelection {
                enabled: config_clone.features.clone(),
                disabled: Vec::new(),
                enable_all: config_clone.enable_all_features,
            };

            let mut config_with_features = config_clone.clone();
            match backend
                .get_image_features(&config_with_features.image)
                .await
            {
                Ok(image_details) => {
                    if let Some(labels) = image_details.labels.as_ref() {
                        if let Some(features_json) = labels.get("com.dsb.features") {
                            match serde_json::from_str::<ImageFeatureLabel>(features_json) {
                                Ok(label) => {
                                    let profile = build_feature_profile(label, &feature_selection);
                                    if let Err(error) = apply_feature_profile(
                                        &mut config_with_features,
                                        &profile,
                                        &sandbox.id.to_string(),
                                    ) {
                                        tracing::warn!(
                                            sandbox_id = %sandbox.id,
                                            image = %config_with_features.image,
                                            error = %error,
                                            "Failed to apply image feature profile"
                                        );
                                    } else {
                                        tracing::debug!(
                                            sandbox_id = %sandbox.id,
                                            features = ?config_with_features.features,
                                            command = ?config_with_features.command,
                                            exposed_ports = ?config_with_features.exposed_ports,
                                            "Applied image feature profile"
                                        );
                                    }
                                }
                                Err(error) => {
                                    tracing::warn!(
                                        sandbox_id = %sandbox.id,
                                        image = %config_with_features.image,
                                        error = %error,
                                        "Failed to parse image feature metadata"
                                    );
                                }
                            }
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        sandbox_id = %sandbox.id,
                        image = %config_with_features.image,
                        error = %error,
                        "Failed to get image features"
                    );
                }
            }

            sandbox.config = config_with_features.clone();
            let _ = state.update_sandbox(&sandbox).await;

            // Create container
            let _ = tx.send(crate::core::types::SandboxProgressEvent::Creating {
                image: config_with_features.image.clone(),
            });

            let container_id: String = match backend
                .create(Some(&sandbox.id), &config_with_features)
                .await
            {
                Ok(id) => id,
                Err(e) => {
                    sandbox.state = crate::core::types::SandboxState::Error;
                    sandbox.error_message = Some(format!("Failed to create: {}", e));
                    sandbox.updated_at = chrono::Utc::now();
                    let _ = state.update_sandbox(&sandbox).await;
                    let _ = tx.send(crate::core::types::SandboxProgressEvent::Error {
                        message: format!("Failed to create container: {}", e),
                    });
                    return;
                }
            };

            sandbox.container_id = Some(container_id.clone());
            sandbox.state = crate::core::types::SandboxState::Created;
            let _ = state.update_sandbox(&sandbox).await;

            // Start container
            let _ = tx.send(crate::core::types::SandboxProgressEvent::Starting {
                container_id: container_id.clone(),
            });

            match backend.start(&container_id).await {
                Ok(_) => {
                    let is_supervisord_command = sandbox
                        .config
                        .command
                        .as_ref()
                        .is_some_and(|cmd| cmd.iter().any(|arg| arg.contains("supervisord")));
                    let has_custom_command =
                        sandbox.config.command.is_some() && !is_supervisord_command;
                    let needs_tool_proxy = !has_custom_command
                        && (sandbox.config.enable_all_features
                            || sandbox.config.features.iter().any(|f| {
                                matches!(f.as_str(), "browser" | "web" | "databend" | "vnc" | "ssh")
                            }));

                    if needs_tool_proxy {
                        use tokio::time::{interval, Duration};

                        let timeout = Duration::from_secs(30);
                        let poll_interval = Duration::from_millis(100);
                        let start_time = std::time::Instant::now();
                        let mut health_interval = interval(poll_interval);

                        loop {
                            if start_time.elapsed() >= timeout {
                                tracing::warn!(
                                    sandbox_id = %sandbox.id,
                                    container_id = %container_id,
                                    "Tool_proxy health check timed out in streaming create path"
                                );
                                break;
                            }

                            health_interval.tick().await;

                            match backend
                                .exec_http(&container_id, "/health", "GET", None, Some(2))
                                .await
                            {
                                Ok(response) => {
                                    let status = response.get("status").and_then(|v| v.as_str());
                                    let browser_connected =
                                        response.get("browser_connected").and_then(|v| v.as_bool());

                                    if status == Some("healthy") && browser_connected == Some(true)
                                    {
                                        break;
                                    }
                                }
                                Err(error) => {
                                    tracing::debug!(
                                        sandbox_id = %sandbox.id,
                                        container_id = %container_id,
                                        error = %error,
                                        "Streaming create path waiting for tool_proxy health"
                                    );
                                }
                            }
                        }
                    } else {
                        let reason = if has_custom_command {
                            "custom command specified"
                        } else {
                            "no tool_proxy-dependent features enabled"
                        };
                        tracing::debug!(
                            sandbox_id = %sandbox.id,
                            reason = reason,
                            "Skipping tool_proxy health check in streaming create path"
                        );
                    }

                    sandbox.state = crate::core::types::SandboxState::Running;
                    sandbox.updated_at = chrono::Utc::now();
                    let _ = state.update_sandbox(&sandbox).await;

                    let _ = tx.send(crate::core::types::SandboxProgressEvent::Ready {
                        sandbox_id: id,
                        container_id,
                    });
                }
                Err(e) => {
                    sandbox.state = crate::core::types::SandboxState::Error;
                    sandbox.error_message = Some(format!("Failed to start: {}", e));
                    sandbox.updated_at = chrono::Utc::now();
                    let _ = state.update_sandbox(&sandbox).await;
                    let _ = tx.send(crate::core::types::SandboxProgressEvent::Error {
                        message: format!("Failed to start: {}", e),
                    });
                }
            }
        });

        Ok(rx)
    }

}
