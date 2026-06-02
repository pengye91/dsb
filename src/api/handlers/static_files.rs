// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Static file API handlers
//!
//! This module implements HTTP handlers for serving and managing static files.

use axum::{
    extract::{Path as AxumPath, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::io::Write;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::core::static_files::{FileNode, StaticFileService};
use crate::utils::mime::detect_mime_type;

/// Error response for static file operations
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error message
    pub error: String,
    /// Optional hint for resolving the error
    pub hint: Option<String>,
}

/// Response for file listing
#[derive(Debug, Serialize)]
pub struct ListFilesResponse {
    /// Sandbox ID
    pub sandbox_id: uuid::Uuid,
    /// List of files with metadata
    pub files: Vec<FileInfo>,
    /// Total number of files
    pub total_count: usize,
    /// Total size of all files in bytes
    pub total_size_bytes: u64,
}

/// File information
#[derive(Debug, Serialize)]
pub struct FileInfo {
    /// File name
    pub file_name: String,
    /// File path
    pub file_path: String,
    /// File size in bytes
    pub file_size_bytes: u64,
    /// MIME content type
    pub content_type: String,
}

/// Response for file operations
#[derive(Debug, Serialize)]
pub struct FileOperationResponse {
    /// Status message
    pub message: String,
    /// Sandbox ID
    pub sandbox_id: uuid::Uuid,
    /// File path
    pub file_path: String,
}

/// Serve static file with authentication
///
/// # Arguments
///
/// * `sandbox_id` - Sandbox UUID
/// * `file_path` - Path to the file (can include subdirectories)
/// * `service` - Static file service
/// * `headers` - HTTP headers (for API key authentication)
///
/// # Returns
///
/// File contents with proper Content-Type header, or error response
#[tracing::instrument(skip(services))]
pub async fn serve_static_file(
    AxumPath((sandbox_id, file_path)): AxumPath<(uuid::Uuid, String)>,
    State(services): State<(Arc<StaticFileService>, Arc<crate::core::SandboxService>)>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let service = &services.0;

    // Sanitize file path (prevent directory traversal)
    let file_path = file_path.trim_start_matches('/');
    if file_path.contains("..") {
        warn!(
            "Path traversal attempt detected for sandbox {}: {}",
            sandbox_id, file_path
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid file path".to_string(),
                hint: Some("Path cannot contain '..'".to_string()),
            }),
        ));
    }

    debug!(
        "Serving static file: sandbox={}, path={}",
        sandbox_id, file_path
    );

    // Check if file exists
    let exists = match service.file_exists(&sandbox_id, file_path).await {
        Ok(exists) => exists,
        Err(e) => {
            error!("Failed to check file existence: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to access file".to_string(),
                    hint: None,
                }),
            ));
        }
    };

    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "File not found".to_string(),
                hint: Some("Check the sandbox_id and file_path".to_string()),
            }),
        ));
    }

    // Read file from disk
    let content = match service.read_file(&sandbox_id, file_path).await {
        Ok(content) => content,
        Err(e) => {
            error!("Failed to read file: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to read file".to_string(),
                    hint: None,
                }),
            ));
        }
    };

    // Detect MIME type
    let content_type = detect_mime_type(file_path);

    // Determine cache control based on file type
    let cache_control = get_cache_control_for_file(service.config(), content_type);

    info!(
        "Served static file: sandbox={}, path={}, size={}, type={}, cache={}",
        sandbox_id,
        file_path,
        content.len(),
        content_type,
        cache_control
    );

    // Build response with proper Content-Type
    let mut response = content.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        content_type
            .parse()
            .unwrap_or_else(|_| "application/octet-stream".parse().unwrap()),
    );

    // Add configurable cache headers
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        cache_control
            .parse()
            .unwrap_or_else(|_| "public, max-age=3600".parse().unwrap()),
    );

    Ok(response)
}

/// List all published files for a sandbox
///
/// # Arguments
///
/// * `sandbox_id` - Sandbox UUID
/// * `service` - Static file service
/// * `headers` - HTTP headers (for API key authentication)
///
/// # Returns
///
/// List of files with metadata, or error response
#[tracing::instrument(skip(services))]
pub async fn list_static_files(
    AxumPath(sandbox_id): AxumPath<uuid::Uuid>,
    State(services): State<(Arc<StaticFileService>, Arc<crate::core::SandboxService>)>,
) -> Result<Json<ListFilesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let service = &services.0;

    debug!("Listing static files for sandbox {}", sandbox_id);

    // List files
    let file_names = match service.list_files(&sandbox_id).await {
        Ok(files) => files,
        Err(e) => {
            let err_msg = e.to_string();
            error!("Failed to list files: {}", err_msg);
            // Return 409 Conflict if the sandbox container is not running,
            // which is a client-error condition rather than a server fault.
            let status = if err_msg.contains("not running") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            return Err((
                status,
                Json(ErrorResponse {
                    error: "Failed to list files".to_string(),
                    hint: None,
                }),
            ));
        }
    };

    // Get file metadata
    let mut files = Vec::new();
    let mut total_size = 0;

    for file_name in &file_names {
        let file_path = file_name;
        match service.get_file_size(&sandbox_id, file_path).await {
            Ok(Some(size)) => {
                total_size += size;
                files.push(FileInfo {
                    file_name: file_name.clone(),
                    file_path: file_path.to_string(),
                    file_size_bytes: size,
                    content_type: detect_mime_type(file_path).to_string(),
                });
            }
            Ok(None) => {
                warn!("File disappeared during listing: {}", file_name);
            }
            Err(e) => {
                warn!("Failed to get file size for {}: {}", file_name, e);
            }
        }
    }

    let total_count = files.len();

    info!(
        "Listed {} static files for sandbox {} (total size: {} bytes)",
        total_count, sandbox_id, total_size
    );

    Ok(Json(ListFilesResponse {
        sandbox_id,
        files,
        total_count,
        total_size_bytes: total_size,
    }))
}

/// Delete a specific file
///
/// # Arguments
///
/// * `sandbox_id` - Sandbox UUID
/// * `file_path` - Path to the file
/// * `service` - Static file service
/// * `headers` - HTTP headers (for API key authentication)
///
/// # Returns
///
/// Success message, or error response
#[tracing::instrument(skip(services))]
pub async fn delete_static_file(
    AxumPath((sandbox_id, file_path)): AxumPath<(uuid::Uuid, String)>,
    State(services): State<(Arc<StaticFileService>, Arc<crate::core::SandboxService>)>,
) -> Result<Json<FileOperationResponse>, (StatusCode, Json<ErrorResponse>)> {
    let service = &services.0;
    let sandbox_service = &services.1;

    // Sanitize file path
    let file_path = file_path.trim_start_matches('/');
    if file_path.contains("..") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid file path".to_string(),
                hint: Some("Path cannot contain '..'".to_string()),
            }),
        ));
    }

    info!(
        "Deleting static file: sandbox={}, path={}",
        sandbox_id, file_path
    );

    let deleted = match service.delete_file(&sandbox_id, file_path).await {
        Ok(deleted) => deleted,
        Err(e) => {
            error!("Failed to delete file: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to delete file".to_string(),
                    hint: None,
                }),
            ));
        }
    };

    if !deleted {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "File not found".to_string(),
                hint: None,
            }),
        ));
    }

    // Record API activity for the write operation
    let _ = sandbox_service.record_api_activity(&sandbox_id).await;

    info!(
        "Deleted static file: {} for sandbox {}",
        file_path, sandbox_id
    );

    Ok(Json(FileOperationResponse {
        message: "File deleted successfully".to_string(),
        sandbox_id,
        file_path: file_path.to_string(),
    }))
}

/// Delete all files for a sandbox
///
/// # Arguments
///
/// * `sandbox_id` - Sandbox UUID
/// * `service` - Static file service
/// * `headers` - HTTP headers (for API key authentication)
///
/// # Returns
///
/// Success message with count of deleted files, or error response
#[tracing::instrument(skip(services))]
pub async fn delete_sandbox_static_files(
    AxumPath(sandbox_id): AxumPath<uuid::Uuid>,
    State(services): State<(Arc<StaticFileService>, Arc<crate::core::SandboxService>)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let service = &services.0;
    let sandbox_service = &services.1;

    info!("Deleting all static files for sandbox {}", sandbox_id);

    let count = match service.delete_sandbox_files(&sandbox_id).await {
        Ok(count) => count,
        Err(e) => {
            error!("Failed to delete sandbox files: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to delete files".to_string(),
                    hint: None,
                }),
            ));
        }
    };

    // Record API activity for the write operation
    let _ = sandbox_service.record_api_activity(&sandbox_id).await;

    info!("Deleted {} static files for sandbox {}", count, sandbox_id);

    Ok(Json(serde_json::json!({
        "message": "Files deleted successfully",
        "sandbox_id": sandbox_id,
        "deleted_count": count,
    })))
}

/// List sandbox files as directory tree
///
/// # Arguments
///
/// * `sandbox_id` - Sandbox UUID
/// * `services` - Service tuple containing StaticFileService and SandboxService
/// * `headers` - HTTP headers (for API key authentication)
///
/// # Returns
///
/// Directory tree structure with files and folders, or error response
#[tracing::instrument(skip(services))]
pub async fn list_sandbox_directory_tree(
    AxumPath(sandbox_id): AxumPath<uuid::Uuid>,
    State(services): State<(Arc<StaticFileService>, Arc<crate::core::SandboxService>)>,
) -> Result<Json<DirectoryTreeResponse>, (StatusCode, Json<ErrorResponse>)> {
    let service = &services.0;

    debug!("Listing directory tree for sandbox {}", sandbox_id);

    // Build directory tree
    let tree = match service.build_directory_tree(&sandbox_id).await {
        Ok(tree) => tree,
        Err(e) => {
            let err_msg = e.to_string();
            error!("Failed to build directory tree: {}", err_msg);
            // Return 409 Conflict if the sandbox container is not running,
            // which is a client-error condition rather than a server fault.
            let status = if err_msg.contains("not running") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            return Err((
                status,
                Json(ErrorResponse {
                    error: "Failed to list directory".to_string(),
                    hint: None,
                }),
            ));
        }
    };

    info!(
        "Listed directory tree for sandbox {} with {} root items",
        sandbox_id,
        tree.len()
    );

    Ok(Json(DirectoryTreeResponse { sandbox_id, tree }))
}

/// Response for directory tree listing
#[derive(Debug, Serialize)]
pub struct DirectoryTreeResponse {
    /// Sandbox ID
    pub sandbox_id: uuid::Uuid,
    /// Directory tree as list of file nodes
    pub tree: Vec<FileNode>,
}

/// Download all files for a sandbox as a ZIP archive
///
/// # Arguments
///
/// * `sandbox_id` - Sandbox UUID
/// * `services` - Service tuple containing StaticFileService and SandboxService
/// * `headers` - HTTP headers (for API key authentication)
///
/// # Returns
///
/// ZIP file containing all files from the sandbox, or error response
#[tracing::instrument(skip(services))]
pub async fn download_sandbox_files_as_zip(
    AxumPath(sandbox_id): AxumPath<uuid::Uuid>,
    State(services): State<(Arc<StaticFileService>, Arc<crate::core::SandboxService>)>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let service = &services.0;
    let config = service.config();

    // Check if zip download is enabled
    if !config.static_server.enable_zip_download {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "ZIP download is disabled".to_string(),
                hint: Some("Enable zip_download in configuration to use this feature".to_string()),
            }),
        ));
    }

    info!(
        "Downloading sandbox files as ZIP for sandbox {}",
        sandbox_id
    );

    // Check if sandbox directory exists
    let sandbox_dir = service.sandbox_dir(&sandbox_id);
    if !sandbox_dir.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Sandbox directory not found".to_string(),
                hint: Some("No files have been published for this sandbox".to_string()),
            }),
        ));
    }

    let max_size_bytes = config.static_server.max_zip_size_mb * 1024 * 1024;

    // Create ZIP archive in a blocking task since zip crate is synchronous
    let sandbox_dir = sandbox_dir.clone();
    let zip_result = tokio::task::spawn_blocking(move || {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut file_count = 0usize;
        add_dir_to_zip(&sandbox_dir, "", &mut writer, &options, &mut file_count)?;

        let cursor = writer.finish()?;
        let buffer = cursor.into_inner();

        Ok::<_, Box<dyn std::error::Error + Send + Sync>>((buffer, file_count))
    })
    .await
    .map_err(|e| {
        error!("ZIP creation task panicked: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to create zip archive".to_string(),
                hint: None,
            }),
        )
    })?
    .map_err(|e| {
        error!("Failed to create zip archive: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to create zip archive".to_string(),
                hint: None,
            }),
        )
    })?;

    let (buffer, file_count) = zip_result;

    // Check if zip size exceeds limit
    if buffer.len() > max_size_bytes {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ErrorResponse {
                error: "ZIP archive too large".to_string(),
                hint: Some(format!(
                    "Total size {} bytes exceeds maximum allowed size of {} MB. \
                    Configure static_server.max_zip_size_mb to increase the limit.",
                    buffer.len(),
                    config.static_server.max_zip_size_mb
                )),
            }),
        ));
    }

    info!(
        "Created ZIP archive for sandbox {} with {} files (size: {} bytes)",
        sandbox_id,
        file_count,
        buffer.len()
    );

    // Build response with proper headers
    let prefix = &config.static_server.zip_download_file_prefix;
    let filename = format!("{}{}.zip", prefix, sandbox_id);
    let mut response = buffer.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, "application/zip".parse().unwrap());
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", filename)
            .parse()
            .unwrap(),
    );

    Ok(response)
}

/// Helper function to recursively add directory contents to zip archive
fn add_dir_to_zip(
    dir_path: &std::path::Path,
    base_path: &str,
    writer: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>,
    options: &zip::write::FileOptions<()>,
    file_count: &mut usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for entry in std::fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Build relative path for the zip entry
        let relative_path = if base_path.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", base_path, name)
        };

        if path.is_file() {
            // Read file content
            let content = std::fs::read(&path)?;

            // Write file to zip
            writer.start_file(relative_path, *options)?;
            writer.write_all(&content)?;
            *file_count += 1;
        } else if path.is_dir() {
            // Recursively add subdirectory
            add_dir_to_zip(&path, &relative_path, writer, options, file_count)?;
        }
    }

    Ok(())
}

/// Helper: Check API key
///
/// Validates the X-API-Key header against the configured API key.
/// Determines cache control for a file based on MIME type
///
/// # Arguments
///
/// * `config` - DSB configuration
/// * `content_type` - MIME type of the file (e.g., "text/html", "image/png")
///
/// # Returns
///
/// The appropriate cache-control header value for this file type
///
/// # Cache Control Priority
///
/// 1. Exact MIME type match (e.g., "text/html" → "no-cache")
/// 2. Wildcard pattern match (e.g., "image/*" → "public, max-age=86400")
/// 3. Default cache control (e.g., "public, max-age=3600")
///
/// # Examples
///
/// ```rust,ignore
/// let cache = get_cache_control_for_file(config, "text/html");
/// // Returns "no-cache" if configured: {"text/html": "no-cache"}
///
/// let cache = get_cache_control_for_file(config, "image/png");
/// // Returns "public, max-age=86400" if configured: {"image/*": "public, max-age=86400"}
///
/// let cache = get_cache_control_for_file(config, "application/pdf");
/// // Returns default "public, max-age=3600" if no pattern matches
/// ```
fn get_cache_control_for_file(config: &Config, content_type: &str) -> String {
    // Try exact MIME type match first
    if let Some(cache_control) = config.static_server.cache_control_by_type.get(content_type) {
        debug!(
            "Using exact MIME match cache control for {}: {}",
            content_type, cache_control
        );
        return cache_control.clone();
    }

    // Try wildcard pattern match (e.g., "image/*" matches "image/png")
    // Extract the type part (before the slash)
    if let Some(type_part) = content_type.split('/').next() {
        let wildcard_pattern = format!("{}/", type_part);
        for (mime_pattern, cache_control) in &config.static_server.cache_control_by_type {
            if mime_pattern.starts_with(&wildcard_pattern) && mime_pattern.ends_with("/*") {
                debug!(
                    "Using wildcard match cache control for {}: {} (pattern: {})",
                    content_type, cache_control, mime_pattern
                );
                return cache_control.clone();
            }
        }
    }

    // Fall back to default cache control
    debug!(
        "Using default cache control for {}: {}",
        content_type, config.static_server.cache_control
    );
    config.static_server.cache_control.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_response_serialization() {
        let error = ErrorResponse {
            error: "Test error".to_string(),
            hint: Some("Test hint".to_string()),
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("Test error"));
        assert!(json.contains("Test hint"));
    }

    #[test]
    fn test_file_info_serialization() {
        let info = FileInfo {
            file_name: "test.html".to_string(),
            file_path: "/test.html".to_string(),
            file_size_bytes: 1024,
            content_type: "text/html".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test.html"));
        assert!(json.contains("text/html"));
    }

    #[test]
    fn test_file_operation_response_serialization() {
        let response = FileOperationResponse {
            message: "Success".to_string(),
            sandbox_id: uuid::Uuid::new_v4(),
            file_path: "/test.txt".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Success"));
        assert!(json.contains("/test.txt"));
    }

    #[test]
    fn test_get_cache_control_default() {
        let config = Config::default();
        let cache = get_cache_control_for_file(&config, "application/pdf");

        // Should return default cache control
        assert_eq!(cache, "public, max-age=3600");
    }

    #[test]
    fn test_get_cache_control_exact_mime_match() {
        use std::collections::HashMap;

        let mut config = Config::default();
        let mut cache_by_type = HashMap::new();
        cache_by_type.insert("text/html".to_string(), "no-cache".to_string());
        cache_by_type.insert("application/json".to_string(), "no-cache".to_string());
        config.static_server.cache_control_by_type = cache_by_type;

        // Test exact match for HTML
        let cache = get_cache_control_for_file(&config, "text/html");
        assert_eq!(cache, "no-cache");

        // Test exact match for JSON
        let cache = get_cache_control_for_file(&config, "application/json");
        assert_eq!(cache, "no-cache");

        // Test non-matching type falls back to default
        let cache = get_cache_control_for_file(&config, "application/pdf");
        assert_eq!(cache, "public, max-age=3600");
    }

    #[test]
    fn test_get_cache_control_wildcard_match() {
        use std::collections::HashMap;

        let mut config = Config::default();
        let mut cache_by_type = HashMap::new();
        cache_by_type.insert("image/*".to_string(), "public, max-age=86400".to_string());
        cache_by_type.insert("font/*".to_string(), "public, max-age=86400".to_string());
        cache_by_type.insert(
            "application/javascript".to_string(),
            "public, max-age=1800, must-revalidate".to_string(),
        );
        config.static_server.cache_control_by_type = cache_by_type;

        // Test wildcard match for images
        let cache = get_cache_control_for_file(&config, "image/png");
        assert_eq!(cache, "public, max-age=86400");

        let cache = get_cache_control_for_file(&config, "image/jpeg");
        assert_eq!(cache, "public, max-age=86400");

        let cache = get_cache_control_for_file(&config, "image/svg+xml");
        assert_eq!(cache, "public, max-age=86400");

        // Test wildcard match for fonts
        let cache = get_cache_control_for_file(&config, "font/woff2");
        assert_eq!(cache, "public, max-age=86400");

        // Test exact match takes priority over wildcard
        let cache = get_cache_control_for_file(&config, "application/javascript");
        assert_eq!(cache, "public, max-age=1800, must-revalidate");

        // Test no match falls back to default
        let cache = get_cache_control_for_file(&config, "application/pdf");
        assert_eq!(cache, "public, max-age=3600");
    }

    #[test]
    fn test_get_cache_control_exact_priority_over_wildcard() {
        use std::collections::HashMap;

        let mut config = Config::default();
        let mut cache_by_type = HashMap::new();
        // Add both wildcard and exact match
        cache_by_type.insert("image/*".to_string(), "public, max-age=86400".to_string());
        cache_by_type.insert("image/png".to_string(), "no-cache".to_string());
        config.static_server.cache_control_by_type = cache_by_type;

        // Exact match should take priority
        let cache = get_cache_control_for_file(&config, "image/png");
        assert_eq!(cache, "no-cache");

        // Other images still use wildcard
        let cache = get_cache_control_for_file(&config, "image/jpeg");
        assert_eq!(cache, "public, max-age=86400");
    }

    #[test]
    fn test_get_cache_control_custom_default() {
        let mut config = Config::default();
        config.static_server.cache_control = "public, max-age=7200".to_string();

        let cache = get_cache_control_for_file(&config, "application/pdf");
        assert_eq!(cache, "public, max-age=7200");
    }

    #[test]
    fn test_get_cache_control_complex_configuration() {
        use std::collections::HashMap;

        let mut config = Config::default();
        config.static_server.cache_control = "public, max-age=3600".to_string();

        let mut cache_by_type = HashMap::new();
        cache_by_type.insert("text/html".to_string(), "no-cache".to_string());
        cache_by_type.insert(
            "text/css".to_string(),
            "public, max-age=1800, must-revalidate".to_string(),
        );
        cache_by_type.insert(
            "application/javascript".to_string(),
            "public, max-age=1800, must-revalidate".to_string(),
        );
        cache_by_type.insert("image/*".to_string(), "public, max-age=86400".to_string());
        cache_by_type.insert("font/*".to_string(), "public, max-age=86400".to_string());
        cache_by_type.insert("application/json".to_string(), "no-cache".to_string());
        config.static_server.cache_control_by_type = cache_by_type;

        // HTML: no cache
        assert_eq!(get_cache_control_for_file(&config, "text/html"), "no-cache");

        // CSS: moderate cache with revalidation
        assert_eq!(
            get_cache_control_for_file(&config, "text/css"),
            "public, max-age=1800, must-revalidate"
        );

        // JavaScript: moderate cache with revalidation
        assert_eq!(
            get_cache_control_for_file(&config, "application/javascript"),
            "public, max-age=1800, must-revalidate"
        );

        // Images: long cache
        assert_eq!(
            get_cache_control_for_file(&config, "image/png"),
            "public, max-age=86400"
        );
        assert_eq!(
            get_cache_control_for_file(&config, "image/jpeg"),
            "public, max-age=86400"
        );

        // Fonts: long cache
        assert_eq!(
            get_cache_control_for_file(&config, "font/woff2"),
            "public, max-age=86400"
        );

        // JSON: no cache
        assert_eq!(
            get_cache_control_for_file(&config, "application/json"),
            "no-cache"
        );

        // PDF: default cache (no match)
        assert_eq!(
            get_cache_control_for_file(&config, "application/pdf"),
            "public, max-age=3600"
        );
    }
}
