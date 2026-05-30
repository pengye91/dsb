// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! DockerTrait implementation for DockerManager.

#[allow(deprecated)]
use bollard::image::ListImagesOptions;
use bollard::query_parameters::RemoveImageOptionsBuilder;
use crate::core::types::SandboxConfig;
use crate::docker::DockerError;
use super::DockerManager;

/// Convert a `DockerManagerError` into the public `DockerError` type.
impl From<super::DockerManagerError> for DockerError {
    fn from(err: super::DockerManagerError) -> Self {
        match err {
            super::DockerManagerError::Api(s) => DockerError::Api(s),
            super::DockerManagerError::ContainerNotFound(s) => DockerError::ContainerNotFound(s),
            super::DockerManagerError::ImageNotFound(s) => DockerError::ImageNotFound(s),
            super::DockerManagerError::ExecFailed(s) => DockerError::ExecFailed(s),
            super::DockerManagerError::Volume(s) => DockerError::Volume(s),
            super::DockerManagerError::Io(e) => DockerError::Io(e),
            super::DockerManagerError::Bollard(e) => DockerError::Api(e.to_string()),
            super::DockerManagerError::ToolProxy { message, .. } => DockerError::ToolProxy {
                message,
                code: crate::core::errors::ErrorCode::ServiceUnavailable,
            },
            super::DockerManagerError::InvalidConfig(s) => DockerError::Api(s),
            super::DockerManagerError::Http(s) => DockerError::Api(s),
            super::DockerManagerError::Timeout(s) => DockerError::Api(s),
        }
    }
}

// Implement DockerTrait for DockerManager
#[async_trait::async_trait]
impl crate::docker::DockerTrait for DockerManager {
    async fn create_container(
        &self,
        config: &SandboxConfig,
        sandbox_id: Option<&uuid::Uuid>,
    ) -> crate::docker::DockerResult<String> {
        self.create_container(config, sandbox_id)
            .await
            .map_err(Into::into)
    }

    async fn start_container(&self, container_id: &str) -> crate::docker::DockerResult<()> {
        self.start_container(container_id)
            .await
            .map_err(Into::into)
    }

    async fn stop_container(&self, container_id: &str) -> crate::docker::DockerResult<()> {
        self.stop_container(container_id)
            .await
            .map_err(Into::into)
    }

    async fn remove_container(&self, container_id: &str) -> crate::docker::DockerResult<()> {
        self.remove_container(container_id)
            .await
            .map_err(Into::into)
    }

    async fn pull_image(&self, image: &str) -> crate::docker::DockerResult<()> {
        self.pull_image(image)
            .await
            .map_err(Into::into)
    }

    async fn image_exists(&self, image: &str) -> crate::docker::DockerResult<bool> {
        self.image_exists(image)
            .await
            .map_err(Into::into)
    }

    async fn exec_container(
        &self,
        container_id: &str,
        command: Vec<String>,
    ) -> crate::docker::DockerResult<String> {
        self.exec_container(container_id, command, None)
            .await
            .map_err(Into::into)
    }

    async fn get_container_stats(
        &self,
        container_id: &str,
    ) -> crate::docker::DockerResult<crate::core::types::ContainerStats> {
        self.get_container_stats(container_id)
            .await
            .map_err(Into::into)
    }

    async fn remove_volume(&self, volume_name: &str) -> crate::docker::DockerResult<()> {
        self.remove_volume(volume_name)
            .await
            .map_err(Into::into)
    }

    async fn is_container_running(&self, container_id: &str) -> crate::docker::DockerResult<bool> {
        self.is_container_running(container_id)
            .await
            .map_err(Into::into)
    }

    async fn create_volume(
        &self,
        volume_mount: &crate::core::types::VolumeMount,
        _sandbox_id: &str,
    ) -> crate::docker::DockerResult<String> {
        // This is a simplified implementation
        // In real usage, volumes are handled internally during container creation
        let volume_name = match volume_mount {
            crate::core::types::VolumeMount::Named { name, .. } => name.clone(),
            crate::core::types::VolumeMount::Bind { .. } => {
                // Bind mounts don't create volumes
                return Ok(String::new());
            }
        };
        Ok(volume_name)
    }

    async fn remove_volumes(
        &self,
        volume_mounts: &[crate::core::types::VolumeMount],
        _sandbox_id: &str,
    ) -> crate::docker::DockerResult<()> {
        for volume_mount in volume_mounts {
            if let crate::core::types::VolumeMount::Named { name, .. } = volume_mount {
                self.remove_volume(name).await?;
            }
        }
        Ok(())
    }

    async fn list_images(
        &self,
    ) -> crate::docker::DockerResult<Vec<crate::core::types::ImageSummary>> {
        #[allow(deprecated)]
        let options = Some(ListImagesOptions::<String> {
            all: true,
            ..Default::default()
        });

        match self.docker.list_images(options).await {
            Ok(images_list) => {
                let images = images_list
                    .into_iter()
                    .map(|image| crate::core::types::ImageSummary {
                        id: image.id,
                        repo_tags: image.repo_tags,
                        size: image.size,
                        created: image.created,
                        labels: Some(image.labels),
                    })
                    .collect();
                Ok(images)
            }
            Err(e) => {
                let error_msg: String = e.to_string();
                Err(crate::docker::DockerError::Api(error_msg))
            }
        }
    }

    async fn inspect_image(
        &self,
        id: &str,
    ) -> crate::docker::DockerResult<crate::core::types::ImageDetails> {
        match self.docker.inspect_image(id).await {
            Ok(image) => {
                // Detect DSB features from labels
                let features = detect_features_from_labels(
                    image.config.as_ref().and_then(|c| c.labels.as_ref()),
                );

                // Parse created timestamp - handle both String and DateTime<Utc>
                // Bollard feature flags affect whether created is String or DateTime<Utc>
                let created = image
                    .created
                    .as_ref()
                    .map(|created_val| {
                        // Use serde_json to serialize and detect the type
                        match serde_json::to_string(created_val) {
                            Ok(json) if json.starts_with('"') => {
                                // It's a String - parse as RFC3339
                                let s = json.trim_matches('"');
                                chrono::DateTime::parse_from_rfc3339(s)
                                    .map(|dt| dt.timestamp())
                                    .unwrap_or(0)
                            }
                            _ => {
                                // It's a DateTime<Utc> or number - try to get timestamp
                                // For DateTime<Utc>, we need to convert via serde
                                0 // Fallback - won't be reached in practice
                            }
                        }
                    })
                    .unwrap_or(0);

                // Handle DateTime<Utc> case if above returned 0
                let created = if let Some(created_val) = image.created.as_ref() {
                    if created == 0 {
                        // Try alternative method for DateTime<Utc>
                        // This path is taken for test builds where created is DateTime<Utc>
                        serde_json::from_value::<i64>(
                            serde_json::to_value(created_val).unwrap_or_default(),
                        )
                        .unwrap_or(0)
                    } else {
                        created
                    }
                } else {
                    created
                };

                let details = crate::core::types::ImageDetails {
                    id: image.id.unwrap_or_default(),
                    repo_tags: image.repo_tags.unwrap_or_default(),
                    size: image.size.unwrap_or(0),
                    virtual_size: image.virtual_size.unwrap_or(0),
                    created,
                    architecture: image.architecture.unwrap_or_else(|| "unknown".to_string()),
                    os: image.os.unwrap_or_else(|| "unknown".to_string()),
                    labels: image.config.as_ref().and_then(|c| c.labels.clone()),
                    env: image.config.and_then(|c| c.env),
                    features,
                };
                Ok(details)
            }
            Err(e) => {
                let error_msg: String = e.to_string();
                if error_msg.contains("not found") || error_msg.contains("No such image") {
                    Err(crate::docker::DockerError::ImageNotFound(id.to_string()))
                } else {
                    Err(crate::docker::DockerError::Api(error_msg))
                }
            }
        }
    }

    async fn remove_image(&self, id: &str) -> crate::docker::DockerResult<()> {
        let options = Some(RemoveImageOptionsBuilder::default().force(false).build());

        match self.docker.remove_image(id, options, None).await {
            Ok(_results) => {
                // remove_image returns Vec<ImageDeleteResponseItem>, check if empty
                // Empty vec means the image was not found but also no error
                Ok(())
            }
            Err(e) => {
                let error_msg: String = e.to_string();
                if error_msg.contains("not found") || error_msg.contains("No such image") {
                    Err(crate::docker::DockerError::ImageNotFound(id.to_string()))
                } else {
                    Err(crate::docker::DockerError::Api(error_msg))
                }
            }
        }
    }

    async fn pull_image_with_progress<F>(
        &self,
        image: &str,
        callback: F,
    ) -> crate::docker::DockerResult<()>
    where
        F: FnMut(String, Option<u64>, Option<u64>) + Send,
    {
        // Convert DockerManagerError return to DockerError
        self.pull_image_with_progress(image, callback)
            .await
            .map_err(Into::into)
    }
}

/// Detects DSB features from Docker image labels.
///
/// Looks for the `com.dsb.features` label and parses it to extract
/// feature information.
fn detect_features_from_labels(
    labels: Option<&std::collections::HashMap<String, String>>,
) -> Vec<String> {
    use crate::core::features::DSB_FEATURES_LABEL;

    let mut features = Vec::new();

    if let Some(labels) = labels {
        // Check for DSB feature metadata label
        if let Some(feature_label) = labels.get(DSB_FEATURES_LABEL) {
            if let Ok(feature_data) =
                serde_json::from_str::<crate::core::features::ImageFeatureLabel>(feature_label)
            {
                // Extract feature names
                features.extend(feature_data.features.keys().cloned());
            }
        }

        // Legacy: Check for individual feature labels
        if labels.contains_key("dsb.feature.vnc") || labels.contains_key("com.dsb.feature.vnc") {
            features.push("vnc".to_string());
        }
        if labels.contains_key("dsb.feature.browser")
            || labels.contains_key("com.dsb.feature.browser")
        {
            features.push("browser".to_string());
        }
        if labels.contains_key("dsb.feature.desktop")
            || labels.contains_key("com.dsb.feature.desktop")
        {
            features.push("desktop".to_string());
        }
    }

    features
}
