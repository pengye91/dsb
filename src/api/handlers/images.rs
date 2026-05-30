// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Docker Image Management API Handlers
//!
//! REST API endpoints for managing Docker images.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;

use crate::api::ApiError;
use crate::core::manager::SandboxManager;

pub use crate::core::types::{ImageDetails, ImageSummary};

/// Request to pull an image
#[derive(Debug, Deserialize, Serialize)]
pub struct PullImageRequest {
    /// Image name (e.g., "nginx")
    pub image: String,
    /// Optional image tag (default: "latest")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

/// Pull progress event for SSE
#[derive(Debug, Serialize)]
pub struct PullProgressEvent {
    /// Status message (e.g., "Pulling fs layer")
    pub status: String,
    /// Layer ID (if applicable)
    pub id: Option<String>,
    /// Human-readable progress string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    /// Detailed progress with current/total bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_detail: Option<ProgressDetail>,
}

/// Progress detail with current and total bytes
#[derive(Debug, Serialize)]
pub struct ProgressDetail {
    /// Bytes downloaded so far
    pub current: u64,
    /// Total bytes to download
    pub total: u64,
}

/// Error response for image operations
#[derive(Debug, Serialize)]
pub struct ImageErrorResponse {
    /// Error message
    pub error: String,
    /// Optional hint for resolving the error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// GET /images - List all local Docker images
///
/// Returns a list of all Docker images currently stored locally.
/// Each image includes basic metadata like ID, tags, size, and creation time.
///
/// # Authentication
///
/// Requires X-API-Key header if configured.
#[tracing::instrument(skip(backend))]
pub async fn list_images(
    State(backend): State<Arc<dyn SandboxManager>>,
) -> Result<Json<Vec<ImageSummary>>, ApiError> {
    let images = backend.list_images().await?;
    Ok(Json(images))
}

/// GET /images/{id} - Inspect image details and detect features
///
/// Returns detailed information about a specific Docker image,
/// including detected DSB features from image labels.
///
/// # Authentication
///
/// Requires X-API-Key header if configured.
#[tracing::instrument(skip(backend), fields(id = %id))]
pub async fn inspect_image(
    State(backend): State<Arc<dyn SandboxManager>>,
    Path(id): Path<String>,
) -> Result<Json<ImageDetails>, ApiError> {
    let details = backend.get_image_features(&id).await?;
    Ok(Json(details))
}

/// POST /images/pull - Pull image from registry
///
/// Initiates pulling a Docker image from a registry.
/// Returns immediately with 202 Accepted, pull happens asynchronously.
/// For pull progress, use the SSE endpoint /images/pull-stream.
///
/// # Authentication
///
/// Requires X-API-Key header if configured.
#[tracing::instrument(skip(backend))]
pub async fn pull_image(
    State(backend): State<Arc<dyn SandboxManager>>,
    Json(req): Json<PullImageRequest>,
) -> Result<StatusCode, ApiError> {
    let full_image = if let Some(tag) = req.tag {
        format!("{}:{}", req.image, tag)
    } else {
        req.image.clone()
    };

    backend.pull_image(&full_image).await?;
    Ok(StatusCode::ACCEPTED)
}

/// POST /images/pull-stream - Pull image with SSE progress streaming
///
/// Initiates pulling a Docker image and streams progress via Server-Sent Events.
///
/// # Authentication
///
/// Requires X-API-Key header if configured.
#[tracing::instrument(skip(backend))]
pub async fn pull_image_stream(
    State(backend): State<Arc<dyn SandboxManager>>,
    Json(req): Json<PullImageRequest>,
) -> Response {
    let full_image = if let Some(tag) = req.tag {
        format!("{}:{}", req.image, tag)
    } else {
        req.image.clone()
    };

    // Use async_stream to create SSE stream from progress callback
    use axum::response::sse::{Event, Sse};

    let stream = async_stream::stream! {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let backend_clone = backend.clone();
        let image_clone = full_image.clone();

        // Spawn pull in background task
        tokio::spawn(async move {
            let tx_clone = tx.clone();
            let result = backend_clone.pull_image_with_progress(&image_clone, Box::new(move |status, current, total| {
                let progress = match (current, total) {
                    (Some(c), Some(t)) => Some(format!("{}%", (c as f64 / t as f64 * 100.0).floor())),
                    _ => None,
                };

                let progress_detail = match (current, total) {
                    (Some(c), Some(t)) => Some(crate::api::handlers::images::ProgressDetail {
                        current: c,
                        total: t,
                    }),
                    _ => None,
                };

                let event = PullProgressEvent {
                    status,
                    id: None,
                    progress,
                    progress_detail,
                };
                let _ = tx_clone.send(event);
            })).await;

            // Send final status
            let final_event = PullProgressEvent {
                status: if result.is_ok() { "Pull complete".to_string() } else { "Pull failed".to_string() },
                id: None,
                progress: None,
                progress_detail: None,
            };
            let _ = tx.send(final_event);
        });

        // Stream all progress events
        while let Some(event) = rx.recv().await {
            match serde_json::to_string(&event) {
                Ok(json) => {
                    yield Ok::<_, Infallible>(Event::default().data(json));
                }
                Err(_) => break,
            }
        }
    };

    Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(5))
                .text("keepalive"),
        )
        .into_response()
}

/// DELETE /images/{id} - Delete local image
///
/// Removes a Docker image from local storage.
/// Returns 204 No Content on success.
///
/// # Authentication
///
/// Requires X-API-Key header if configured.
#[tracing::instrument(skip(backend), fields(id = %id))]
pub async fn delete_image(
    State(backend): State<Arc<dyn SandboxManager>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    backend.delete_image(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_summary_serialization() {
        let summary = ImageSummary {
            id: "sha256:abc123".to_string(),
            repo_tags: vec!["nginx:latest".to_string()],
            size: 132456789,
            created: 1234567890,
            labels: None,
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("nginx:latest"));
        assert!(json.contains("132456789"));
    }

    #[test]
    fn test_pull_image_request_serialization() {
        let req = PullImageRequest {
            image: "nginx".to_string(),
            tag: Some("latest".to_string()),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("nginx"));
        assert!(json.contains("latest"));
    }

    #[test]
    fn test_pull_image_request_without_tag() {
        let req = PullImageRequest {
            image: "nginx".to_string(),
            tag: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("nginx"));
        // tag should not be present if None
        assert!(!json.contains("tag"));
    }

    #[test]
    fn test_image_details_serialization() {
        let details = ImageDetails {
            id: "sha256:abc123".to_string(),
            repo_tags: vec!["nginx:latest".to_string()],
            size: 132456789,
            virtual_size: 132456789,
            created: 1234567890,
            architecture: "amd64".to_string(),
            os: "linux".to_string(),
            labels: None,
            env: None,
            features: vec![],
        };

        let json = serde_json::to_string(&details).unwrap();
        assert!(json.contains("amd64"));
        assert!(json.contains("linux"));
    }

    #[test]
    fn test_pull_progress_event_serialization() {
        let event = PullProgressEvent {
            status: "Pulling fs layer".to_string(),
            id: Some("abc123".to_string()),
            progress: Some("1 MB / 10 MB".to_string()),
            progress_detail: Some(ProgressDetail {
                current: 1000000,
                total: 10000000,
            }),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("Pulling fs layer"));
        assert!(json.contains("1 MB / 10 MB"));
    }

    #[test]
    fn test_error_response_serialization() {
        let error = ImageErrorResponse {
            error: "Image not found".to_string(),
            hint: Some("Check the image name".to_string()),
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("Image not found"));
        assert!(json.contains("Check the image name"));
    }
}
