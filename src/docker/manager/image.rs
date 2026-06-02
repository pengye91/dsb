// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! Docker image management operations.

use super::{DockerManager, DockerManagerError};
use bollard::query_parameters::CreateImageOptionsBuilder;
use futures_util::stream::StreamExt;

impl DockerManager {
    /// Pulls a Docker image from a registry.
    ///
    /// This method downloads an image from a Docker registry (e.g., Docker Hub)
    /// to the local system. For large images, this can take significant time.
    ///
    /// # Arguments
    ///
    /// * `image` - Image reference to pull (e.g., "nginx:latest", "myregistry/myimage:v1.0")
    ///
    /// # Returns
    ///
    /// - `Ok(())` - Image pulled successfully
    /// - `Err(...)` - If pull fails
    ///
    /// # Performance Note
    ///
    /// This is a blocking operation that downloads the entire image.
    /// For faster container creation, pre-pull images before calling [`create_container`](Self::create_container).
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// docker.pull_image("nginx:latest").await?;
    /// println!("Image pulled successfully");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn pull_image(&self, image: &str) -> Result<(), DockerManagerError> {
        let start = std::time::Instant::now();

        tracing::info!(image = %image, "Pulling image");

        let options = Some(
            CreateImageOptionsBuilder::default()
                .from_image(image)
                .build(),
        );

        let mut stream = self.docker.create_image(options, None, None);
        let mut last_progress_time = std::time::Instant::now();

        while let Some(result) = stream.next().await {
            match result {
                Ok(progress) => {
                    // Log progress every 5 seconds to avoid log explosion
                    if last_progress_time.elapsed() >= std::time::Duration::from_secs(5) {
                        if let Some(status) = progress.status {
                            tracing::debug!("Pull progress: {}", status);
                        }
                        last_progress_time = std::time::Instant::now();
                    }

                    // Check for errors in the progress stream
                    if let Some(error) = progress.error {
                        return Err(DockerManagerError::ImageNotFound(format!(
                            "Failed to pull image: {}",
                            error
                        )));
                    }
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        image = %image,
                        "Failed to pull image"
                    );
                    return Err(DockerManagerError::Bollard(e));
                }
            }
        }

        tracing::info!(
            image = %image,
            duration_ms = start.elapsed().as_millis(),
            "Image pulled successfully"
        );
        Ok(())
    }

    /// Pulls a Docker image with progress callbacks for real-time updates.
    ///
    /// This method downloads an image and calls the provided callback function
    /// with progress updates during the pull operation.
    ///
    /// # Arguments
    ///
    /// * `image` - Image reference to pull
    /// * `callback` - Function called with (status, current_bytes, total_bytes)
    ///
    /// # Callback Signature
    ///
    /// The callback receives:
    /// - `status`: Text description (e.g., "Pulling fs layer")
    /// - `current`: Current bytes downloaded (None if not available)
    /// - `total`: Total bytes to download (None if not available)
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// docker.pull_image_with_progress("nginx:latest", |status, current, total| {
    ///     if let (Some(c), Some(t)) = (current, total) {
    ///         println!("{}: {}/{} bytes", status, c, t);
    ///     } else {
    ///         println!("{}...", status);
    ///     }
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn pull_image_with_progress<F>(
        &self,
        image: &str,
        mut callback: F,
    ) -> Result<(), DockerManagerError>
    where
        F: FnMut(String, Option<u64>, Option<u64>),
    {
        tracing::debug!("Pulling image: {}", image);

        let options = Some(
            CreateImageOptionsBuilder::default()
                .from_image(image)
                .build(),
        );

        let mut stream = self.docker.create_image(options, None, None);

        while let Some(result) = stream.next().await {
            match result {
                Ok(progress) => {
                    // Extract progress information
                    let status = progress.status.unwrap_or_else(|| "Pulling...".to_string());
                    let current = progress
                        .progress_detail
                        .as_ref()
                        .and_then(|d| d.current)
                        .map(|v| v as u64);
                    let total = progress
                        .progress_detail
                        .as_ref()
                        .and_then(|d| d.total)
                        .map(|v| v as u64);

                    // Call the callback with progress
                    callback(status, current, total);

                    // Check for errors in the progress stream
                    if let Some(error) = progress.error {
                        return Err(DockerManagerError::ImageNotFound(format!(
                            "Failed to pull image: {}",
                            error
                        )));
                    }
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        image = %image,
                        "Failed to pull image"
                    );
                    return Err(DockerManagerError::Bollard(e));
                }
            }
        }

        tracing::debug!("Successfully pulled image: {}", image);
        Ok(())
    }

    /// Checks if a Docker image exists locally.
    ///
    /// # Arguments
    ///
    /// * `image` - Image reference to check (e.g., "nginx:latest")
    ///
    /// # Returns
    ///
    /// - `Ok(true)` - Image exists locally
    /// - `Ok(false)` - Image doesn't exist locally
    /// - `Err(...)` - Failed to check (Docker daemon issue)
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let exists = docker.image_exists("nginx:latest").await?;
    /// if exists {
    ///     println!("Image exists locally");
    /// } else {
    ///     println!("Need to pull image");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn image_exists(&self, image: &str) -> Result<bool, DockerManagerError> {
        match self.docker.inspect_image(image).await {
            Ok(_) => {
                tracing::debug!("Image {} exists locally", image);
                Ok(true)
            }
            Err(e) => {
                // Check if it's a "not found" error
                let error_msg = e.to_string().to_lowercase();
                if error_msg.contains("no such image") || error_msg.contains("not found") {
                    tracing::debug!("Image {} does not exist locally", image);
                    Ok(false)
                } else {
                    // Some other error (Docker daemon issue, etc.)
                    tracing::error!(
                        error = %e,
                        image = %image,
                        "Failed to inspect image"
                    );
                    Err(DockerManagerError::Bollard(e))
                }
            }
        }
    }
}
