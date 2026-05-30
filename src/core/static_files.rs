// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Static File Manager Service
//!
//! Manages static file publication and serving for sandboxes.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::{info, warn};

use crate::config::Config;
use crate::core::SandboxService;

/// Type alias for the async future returned by `build_tree_helper`.
type FileTreeFuture<'a> = std::pin::Pin<
    Box<
        dyn std::future::Future<
                Output = Result<Vec<FileNode>, Box<dyn std::error::Error + Send + Sync>>,
            > + Send
            + 'a,
    >,
>;

/// File node for directory tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileNode {
    /// File or directory name
    pub name: String,
    /// Full path relative to the sandbox root
    pub path: String,
    /// Whether this is a directory
    pub is_dir: bool,
    /// File size in bytes (None for directories)
    pub size: Option<u64>,
    /// Child nodes (only for directories)
    pub children: Option<Vec<FileNode>>,
}

/// Static file metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticFileMetadata {
    /// File record ID
    pub id: uuid::Uuid,
    /// Owning sandbox ID
    pub sandbox_id: uuid::Uuid,
    /// Relative path from /public root
    pub file_path: String,
    /// File name
    pub file_name: String,
    /// MIME content type
    pub content_type: String,
    /// File size in bytes
    pub file_size_bytes: i64,
    /// Publication timestamp
    pub published_at: chrono::DateTime<chrono::Utc>,
    /// Last access timestamp
    pub last_accessed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Number of times the file has been accessed
    pub access_count: i64,
}

/// Static file service
///
/// This service manages the lifecycle of static files published by sandboxes.
/// It handles both database metadata and filesystem operations.
///
/// ## Backend modes
///
/// **Docker mode** (`sandbox_service = None`): Reads static files directly from
/// the host filesystem at `base_path/{sandbox_id}`. This relies on Docker bind
/// mounts sharing the directory between the DSB server and sandbox containers.
///
/// **Kubernetes mode** (`sandbox_service = Some(...)`): Proxies all static file
/// operations through `exec` commands into the sandbox pod, since K8s pods do
/// not share a filesystem with the DSB server pod.
#[derive(Clone)]
pub struct StaticFileService {
    config: Arc<Config>,
    sandbox_service: Option<Arc<SandboxService>>,
}

/// Shell-quote a string for safe use in single-quoted shell arguments.
///
/// Replaces each `'` with `'\''` so the string can be safely embedded in a
/// single-quoted shell word. This prevents command injection from file paths.
pub(crate) fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

impl StaticFileService {
    /// Create a new static file service for Docker mode (filesystem-backed).
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            sandbox_service: None,
        }
    }

    /// Create a new static file service with a sandbox backend (K8s mode).
    ///
    /// When a backend is provided, all file operations are proxied through
    /// `exec` commands into the sandbox pod instead of reading from local disk.
    pub fn new_with_backend(config: Arc<Config>, sandbox_service: Arc<SandboxService>) -> Self {
        Self {
            config,
            sandbox_service: Some(sandbox_service),
        }
    }

    /// Get base path for static files
    pub fn base_path(&self) -> &str {
        &self.config.static_server.base_path
    }

    /// Get the configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get sandbox static files directory
    pub fn sandbox_dir(&self, sandbox_id: &uuid::Uuid) -> PathBuf {
        PathBuf::from(self.base_path()).join(sandbox_id.to_string())
    }

    /// Ensure the base directory exists
    pub async fn ensure_base_dir(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let base_path = self.base_path();
        if !Path::new(base_path).exists() {
            fs::create_dir_all(base_path).await?;
            info!("Created static files base directory: {}", base_path);
        }
        Ok(())
    }

    /// Ensure a sandbox directory exists
    pub async fn ensure_sandbox_dir(
        &self,
        sandbox_id: &uuid::Uuid,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        self.ensure_base_dir().await?;
        let sandbox_dir = self.sandbox_dir(sandbox_id);
        if !sandbox_dir.exists() {
            fs::create_dir_all(&sandbox_dir).await?;
            info!("Created sandbox directory: {}", sandbox_dir.display());
        }
        Ok(sandbox_dir)
    }

    // -------------------------------------------------------------------------
    // K8s helpers
    // -------------------------------------------------------------------------

    /// Return a consistent error when a symlink is encountered.
    fn symlink_error() -> Box<dyn std::error::Error + Send + Sync> {
        Box::new(super::ApiError::Validation {
            message: "Symlinks are not allowed".into(),
            field: None,
            code: super::ErrorCode::ValidationError,
        })
    }

    /// Check whether a path inside a sandbox pod is a symlink (K8s mode).
    async fn k8s_check_not_symlink(
        &self,
        sandbox_id: &uuid::Uuid,
        file_path: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let quoted = shell_quote(&format!("/public/{}", file_path));
        let cmd = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("if sudo test -L {}; then echo 'SYMLINK'; fi", quoted),
        ];
        let output = self.k8s_exec(sandbox_id, cmd).await?;
        if output.trim() == "SYMLINK" {
            return Err(Self::symlink_error());
        }
        Ok(())
    }

    /// Execute a command in a sandbox pod via the backend (K8s mode).
    async fn k8s_exec(
        &self,
        sandbox_id: &uuid::Uuid,
        cmd: Vec<String>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let service = self
            .sandbox_service
            .as_ref()
            .ok_or("No sandbox service configured")?;
        let sandbox = service
            .get_sandbox(sandbox_id)
            .await
            .ok_or_else(|| format!("Sandbox {} not found", sandbox_id))?;
        let container_id = sandbox
            .container_id
            .as_ref()
            .ok_or_else(|| format!("Sandbox {} has no container", sandbox_id))?;
        let output = service
            .backend
            .exec(container_id, cmd)
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
        Ok(output)
    }

    // -------------------------------------------------------------------------
    // File operations (dual-mode)
    // -------------------------------------------------------------------------

    /// Get file size from filesystem or sandbox pod.
    pub async fn get_file_size(
        &self,
        sandbox_id: &uuid::Uuid,
        file_path: &str,
    ) -> Result<Option<u64>, Box<dyn std::error::Error + Send + Sync>> {
        if self.sandbox_service.is_some() {
            self.k8s_check_not_symlink(sandbox_id, file_path).await?;
            let quoted = shell_quote(&format!("/public/{}", file_path));
            let cmd = vec![
                "sh".to_string(),
                "-c".to_string(),
                format!("sudo wc -c < {} 2>/dev/null || echo '0'", quoted),
            ];
            let output = self.k8s_exec(sandbox_id, cmd).await?;
            let size = output.trim().parse::<u64>().unwrap_or(0);
            return Ok(if size > 0 { Some(size) } else { None });
        }

        let full_path = self.sandbox_dir(sandbox_id).join(file_path);
        match fs::symlink_metadata(&full_path).await {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(Self::symlink_error());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        match fs::metadata(&full_path).await {
            Ok(metadata) => Ok(Some(metadata.len())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Check if a file exists on filesystem or in sandbox pod.
    pub async fn file_exists(
        &self,
        sandbox_id: &uuid::Uuid,
        file_path: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if self.sandbox_service.is_some() {
            self.k8s_check_not_symlink(sandbox_id, file_path).await?;
            let quoted = shell_quote(&format!("/public/{}", file_path));
            let cmd = vec![
                "sh".to_string(),
                "-c".to_string(),
                format!("sudo test -f {} && echo 'yes' || echo 'no'", quoted),
            ];
            let output = self.k8s_exec(sandbox_id, cmd).await?;
            return Ok(output.trim() == "yes");
        }

        let full_path = self.sandbox_dir(sandbox_id).join(file_path);
        match fs::symlink_metadata(&full_path).await {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(Self::symlink_error());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        }
        Ok(full_path.exists())
    }

    /// Read a file's contents from filesystem or sandbox pod.
    pub async fn read_file(
        &self,
        sandbox_id: &uuid::Uuid,
        file_path: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        if self.sandbox_service.is_some() {
            self.k8s_check_not_symlink(sandbox_id, file_path).await?;
            let quoted = shell_quote(&format!("/public/{}", file_path));
            let cmd = vec![
                "sh".to_string(),
                "-c".to_string(),
                format!("sudo base64 -w0 {}", quoted),
            ];
            let encoded = self.k8s_exec(sandbox_id, cmd).await?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .map_err(|e| format!("Failed to decode base64: {}", e))?;
            return Ok(decoded);
        }

        let full_path = self.sandbox_dir(sandbox_id).join(file_path);
        match fs::symlink_metadata(&full_path).await {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(Self::symlink_error());
                }
            }
            Err(e) => return Err(e.into()),
        }
        let mut file = fs::File::open(&full_path).await?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).await?;
        Ok(contents)
    }

    /// Delete a specific file from filesystem or sandbox pod.
    pub async fn delete_file(
        &self,
        sandbox_id: &uuid::Uuid,
        file_path: &str,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if self.sandbox_service.is_some() {
            self.k8s_check_not_symlink(sandbox_id, file_path).await?;
            let quoted = shell_quote(&format!("/public/{}", file_path));
            let cmd = vec![
                "sh".to_string(),
                "-c".to_string(),
                format!(
                    "sudo test -f {} && sudo rm -f {} && echo 'deleted' || echo 'notfound'",
                    quoted, quoted
                ),
            ];
            let output = self.k8s_exec(sandbox_id, cmd).await?;
            return Ok(output.trim() == "deleted");
        }

        let full_path = self.sandbox_dir(sandbox_id).join(file_path);

        match fs::symlink_metadata(&full_path).await {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(Self::symlink_error());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warn!(
                    "Attempted to delete non-existent file: {}",
                    full_path.display()
                );
                return Ok(false);
            }
            Err(e) => return Err(e.into()),
        }

        if !full_path.exists() {
            warn!(
                "Attempted to delete non-existent file: {}",
                full_path.display()
            );
            return Ok(false);
        }

        fs::remove_file(&full_path).await?;
        info!("Deleted file: {}", full_path.display());
        Ok(true)
    }

    /// Delete all files for a sandbox from filesystem or sandbox pod.
    pub async fn delete_sandbox_files(
        &self,
        sandbox_id: &uuid::Uuid,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        if self.sandbox_service.is_some() {
            let cmd = vec![
                "sh".to_string(),
                "-c".to_string(),
                "count=$(sudo find /public -type f 2>/dev/null | wc -l); sudo find /public -mindepth 1 -delete 2>/dev/null; echo \"$count\"".to_string(),
            ];
            let output = self.k8s_exec(sandbox_id, cmd).await?;
            let count = output.trim().parse::<u64>().unwrap_or(0);
            info!("Deleted {} static files from sandbox {}", count, sandbox_id);
            return Ok(count);
        }

        let sandbox_dir = self.sandbox_dir(sandbox_id);

        if !sandbox_dir.exists() {
            warn!(
                "Attempted to delete non-existent sandbox directory: {}",
                sandbox_dir.display()
            );
            return Ok(0);
        }

        // Count files before deletion
        let mut count = 0;
        let mut entries = fs::read_dir(&sandbox_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.path().is_file() {
                count += 1;
            }
        }

        // Delete entire directory
        fs::remove_dir_all(&sandbox_dir).await?;
        info!(
            "Deleted sandbox directory {} with {} files",
            sandbox_dir.display(),
            count
        );

        Ok(count)
    }

    /// List all files in a sandbox directory (recursively).
    pub async fn list_files(
        &self,
        sandbox_id: &uuid::Uuid,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        if self.sandbox_service.is_some() {
            let cmd = vec![
                "sh".to_string(),
                "-c".to_string(),
                "sudo find /public -type f | sed 's|^/public/||' | sort".to_string(),
            ];
            let output = self.k8s_exec(sandbox_id, cmd).await?;
            let files: Vec<String> = output
                .lines()
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect();
            return Ok(files);
        }

        let sandbox_dir = self.sandbox_dir(sandbox_id);

        if !sandbox_dir.exists() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        let mut dirs_to_visit: Vec<(PathBuf, String)> = vec![(sandbox_dir, String::new())];

        while let Some((dir, relative_path)) = dirs_to_visit.pop() {
            let mut entries = fs::read_dir(&dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_file() {
                    if let Some(file_name) = path.file_name() {
                        if let Some(name) = file_name.to_str() {
                            // Build relative path from sandbox root
                            let file_path = if relative_path.is_empty() {
                                name.to_string()
                            } else {
                                format!("{}/{}", relative_path, name)
                            };
                            files.push(file_path);
                        }
                    }
                } else if path.is_dir() {
                    // Add subdirectory to visit later
                    if let Some(dir_name) = path.file_name() {
                        if let Some(name) = dir_name.to_str() {
                            let new_relative_path = if relative_path.is_empty() {
                                name.to_string()
                            } else {
                                format!("{}/{}", relative_path, name)
                            };
                            dirs_to_visit.push((path, new_relative_path));
                        }
                    }
                }
            }
        }

        files.sort();
        Ok(files)
    }

    /// Get total size of all files in a sandbox directory.
    pub async fn get_total_size(
        &self,
        sandbox_id: &uuid::Uuid,
    ) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        if self.sandbox_service.is_some() {
            let cmd = vec![
                "sh".to_string(),
                "-c".to_string(),
                "sudo find /public -type f -exec stat -c '%s' {} + 2>/dev/null | awk '{s+=$1} END {print s+0}'".to_string(),
            ];
            let output = self.k8s_exec(sandbox_id, cmd).await?;
            let size = output.trim().parse::<u64>().unwrap_or(0);
            return Ok(size);
        }

        let sandbox_dir = self.sandbox_dir(sandbox_id);

        if !sandbox_dir.exists() {
            return Ok(0);
        }

        let mut total_size = 0;
        let mut entries = fs::read_dir(&sandbox_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                if let Ok(metadata) = entry.metadata().await {
                    total_size += metadata.len();
                }
            }
        }

        Ok(total_size)
    }

    /// Build directory tree for a sandbox.
    pub async fn build_directory_tree(
        &self,
        sandbox_id: &uuid::Uuid,
    ) -> Result<Vec<FileNode>, Box<dyn std::error::Error + Send + Sync>> {
        if self.sandbox_service.is_some() {
            let files = self.list_files(sandbox_id).await?;
            if files.is_empty() {
                return Ok(Vec::new());
            }
            return Ok(Self::build_tree_from_paths(files));
        }

        let sandbox_dir = self.sandbox_dir(sandbox_id);

        if !sandbox_dir.exists() {
            return Ok(Vec::new());
        }

        self.build_tree_helper(&sandbox_dir, "").await
    }

    /// Build a directory tree from a flat list of file paths.
    fn build_tree_from_paths(files: Vec<String>) -> Vec<FileNode> {
        #[derive(Default)]
        struct TreeNode {
            is_dir: bool,
            children: BTreeMap<String, TreeNode>,
        }

        let mut root = TreeNode::default();

        for file in files {
            let parts: Vec<&str> = file.split('/').collect();
            let mut current = &mut root;
            for (i, part) in parts.iter().enumerate() {
                let is_last = i == parts.len() - 1;
                current = current.children.entry(part.to_string()).or_default();
                current.is_dir = !is_last;
            }
        }

        fn convert(node: &TreeNode, path: &str) -> Vec<FileNode> {
            let mut result = Vec::new();
            for (name, child) in &node.children {
                let child_path = if path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", path, name)
                };

                if child.is_dir {
                    let children = convert(child, &child_path);
                    result.push(FileNode {
                        name: name.clone(),
                        path: child_path,
                        is_dir: true,
                        size: None,
                        children: if children.is_empty() {
                            None
                        } else {
                            Some(children)
                        },
                    });
                } else {
                    result.push(FileNode {
                        name: name.clone(),
                        path: child_path,
                        is_dir: false,
                        size: None,
                        children: None,
                    });
                }
            }
            // Sort: directories first, then files, both alphabetically
            result.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            });
            result
        }

        convert(&root, "")
    }

    /// Helper function to build tree recursively from filesystem.
    fn build_tree_helper<'a>(
        &'a self,
        dir: &'a Path,
        relative_path: &'a str,
    ) -> FileTreeFuture<'a> {
        Box::pin(async move {
            let mut entries = match fs::read_dir(dir).await {
                Ok(entries) => entries,
                Err(_) => return Ok(Vec::new()),
            };

            let mut nodes = Vec::new();
            let mut dir_entries: Vec<(String, PathBuf, String)> = Vec::new();

            // First pass: collect all entries
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let current_relative_path = if relative_path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", relative_path, name)
                };

                if path.is_dir() {
                    dir_entries.push((name, path, current_relative_path));
                } else if path.is_file() {
                    let size = entry.metadata().await.ok().map(|m| m.len());
                    nodes.push(FileNode {
                        name,
                        path: current_relative_path,
                        is_dir: false,
                        size,
                        children: None,
                    });
                }
            }

            // Second pass: process directories recursively
            for (name, path, current_relative_path) in dir_entries {
                let children = self
                    .build_tree_helper(&path, &current_relative_path)
                    .await?;
                nodes.push(FileNode {
                    name,
                    path: current_relative_path,
                    is_dir: true,
                    size: None,
                    children: if children.is_empty() {
                        None
                    } else {
                        Some(children)
                    },
                });
            }

            // Sort: directories first, then files, both alphabetically
            nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            });

            Ok(nodes)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_config() -> Config {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_str().unwrap().to_string();

        Config {
            static_server: crate::config::StaticServerConfig {
                base_path,
                host_path: None,
                api_key: None,
                max_file_size_mb: 100,
                sandbox_upload_max_file_size_mb: 10,
                enable_directory_browsing: false,
                cache_control: "public, max-age=3600".to_string(),
                cache_control_by_type: HashMap::new(),
                require_auth: false,
                enable_zip_download: true,
                max_zip_size_mb: 500,
                zip_download_file_prefix: "sandbox-".to_string(),
            },
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_static_file_service_base_path() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);

        // Base path should be the temp dir created in create_test_config
        let base_path = service.base_path();
        assert!(!base_path.is_empty());
        assert!(base_path.contains(".tmp"));
    }

    #[tokio::test]
    async fn test_sandbox_dir() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        let dir = service.sandbox_dir(&sandbox_id);
        assert!(dir.to_string_lossy().contains(&sandbox_id.to_string()));
    }

    #[tokio::test]
    async fn test_ensure_sandbox_dir() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        let dir = service.ensure_sandbox_dir(&sandbox_id).await.unwrap();
        assert!(dir.exists());
        assert!(dir.is_dir());
    }

    #[tokio::test]
    async fn test_file_operations() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        // Create sandbox directory
        service.ensure_sandbox_dir(&sandbox_id).await.unwrap();

        // Write a test file
        let test_path = service.sandbox_dir(&sandbox_id).join("test.txt");
        fs::write(&test_path, b"Hello, World!").await.unwrap();

        // Check file exists
        assert!(service.file_exists(&sandbox_id, "test.txt").await.unwrap());

        // Get file size
        let size = service
            .get_file_size(&sandbox_id, "test.txt")
            .await
            .unwrap();
        assert_eq!(size, Some(13));

        // Read file
        let contents = service.read_file(&sandbox_id, "test.txt").await.unwrap();
        assert_eq!(contents, b"Hello, World!");

        // List files
        let files = service.list_files(&sandbox_id).await.unwrap();
        assert_eq!(files, vec!["test.txt"]);

        // Delete file
        let deleted = service.delete_file(&sandbox_id, "test.txt").await.unwrap();
        assert!(deleted);

        // Verify file is gone
        assert!(!service.file_exists(&sandbox_id, "test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_file() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        let deleted = service
            .delete_file(&sandbox_id, "nonexistent.txt")
            .await
            .unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_delete_sandbox_files() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        // Create sandbox directory with files
        service.ensure_sandbox_dir(&sandbox_id).await.unwrap();
        let dir = service.sandbox_dir(&sandbox_id);

        fs::write(dir.join("file1.txt"), b"content1").await.unwrap();
        fs::write(dir.join("file2.txt"), b"content2").await.unwrap();
        fs::write(dir.join("file3.txt"), b"content3").await.unwrap();

        // Delete all files
        let count = service.delete_sandbox_files(&sandbox_id).await.unwrap();
        assert_eq!(count, 3);
        assert!(!dir.exists());
    }

    #[tokio::test]
    async fn test_get_total_size() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        // Create sandbox directory with files
        service.ensure_sandbox_dir(&sandbox_id).await.unwrap();
        let dir = service.sandbox_dir(&sandbox_id);

        fs::write(dir.join("file1.txt"), b"content1").await.unwrap(); // 8 bytes
        fs::write(dir.join("file2.txt"), b"content2").await.unwrap(); // 8 bytes

        let total_size = service.get_total_size(&sandbox_id).await.unwrap();
        assert_eq!(total_size, 16);
    }

    #[tokio::test]
    async fn test_config_getter() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config.clone());

        // Test that config() returns the correct reference
        let retrieved_config = service.config();
        assert_eq!(
            retrieved_config.static_server.base_path,
            config.static_server.base_path
        );
        assert_eq!(retrieved_config.static_server.max_file_size_mb, 100);
        assert_eq!(
            retrieved_config.static_server.cache_control,
            "public, max-age=3600"
        );
    }

    #[tokio::test]
    async fn test_build_directory_tree_empty() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        // Empty directory should return empty tree
        service.ensure_sandbox_dir(&sandbox_id).await.unwrap();
        let tree = service.build_directory_tree(&sandbox_id).await.unwrap();
        assert_eq!(tree.len(), 0);
    }

    #[tokio::test]
    async fn test_build_directory_tree_flat_files() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        // Create sandbox directory with files
        service.ensure_sandbox_dir(&sandbox_id).await.unwrap();
        let dir = service.sandbox_dir(&sandbox_id);

        fs::write(dir.join("a.txt"), b"content").await.unwrap();
        fs::write(dir.join("b.txt"), b"content").await.unwrap();

        let tree = service.build_directory_tree(&sandbox_id).await.unwrap();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].name, "a.txt");
        assert_eq!(tree[1].name, "b.txt");
        assert!(!tree[0].is_dir);
        assert!(!tree[1].is_dir);
    }

    #[tokio::test]
    async fn test_build_directory_tree_with_directories() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        // Create sandbox directory with nested structure
        service.ensure_sandbox_dir(&sandbox_id).await.unwrap();
        let dir = service.sandbox_dir(&sandbox_id);

        // Create files
        fs::write(dir.join("root.txt"), b"root").await.unwrap();

        // Create subdirectory with files
        let subdir = dir.join("subdir");
        fs::create_dir(&subdir).await.unwrap();
        fs::write(subdir.join("nested.txt"), b"nested")
            .await
            .unwrap();

        let tree = service.build_directory_tree(&sandbox_id).await.unwrap();
        assert_eq!(tree.len(), 2);

        // First item should be directory (directories sort first)
        assert!(tree[0].is_dir);
        assert_eq!(tree[0].name, "subdir");
        assert!(tree[0].children.is_some());
        assert_eq!(tree[0].children.as_ref().unwrap().len(), 1);
        assert_eq!(tree[0].children.as_ref().unwrap()[0].name, "nested.txt");

        // Second item should be file
        assert!(!tree[1].is_dir);
        assert_eq!(tree[1].name, "root.txt");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_read_file_symlink_blocked() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        service.ensure_sandbox_dir(&sandbox_id).await.unwrap();
        let sandbox_dir = service.sandbox_dir(&sandbox_id);

        // Create a file outside the sandbox directory
        let outside_file = sandbox_dir.parent().unwrap().join("outside_secret.txt");
        fs::write(&outside_file, b"SECRET_DATA").await.unwrap();

        // Create a symlink inside the sandbox pointing outside
        let symlink_path = sandbox_dir.join("leak.txt");
        std::os::unix::fs::symlink(&outside_file, &symlink_path).unwrap();

        // Attempting to read the symlink should fail
        let result = service.read_file(&sandbox_id, "leak.txt").await;
        assert!(result.is_err(), "Reading a symlink should fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Symlinks are not allowed"),
            "Error should mention symlinks: {}",
            err_msg
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_get_file_size_symlink_blocked() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        service.ensure_sandbox_dir(&sandbox_id).await.unwrap();
        let sandbox_dir = service.sandbox_dir(&sandbox_id);

        let outside_file = sandbox_dir.parent().unwrap().join("outside_secret.txt");
        fs::write(&outside_file, b"SECRET_DATA").await.unwrap();

        let symlink_path = sandbox_dir.join("leak.txt");
        std::os::unix::fs::symlink(&outside_file, &symlink_path).unwrap();

        let result = service.get_file_size(&sandbox_id, "leak.txt").await;
        assert!(result.is_err(), "Getting size of a symlink should fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Symlinks are not allowed"),
            "Error should mention symlinks: {}",
            err_msg
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_file_exists_symlink_blocked() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        service.ensure_sandbox_dir(&sandbox_id).await.unwrap();
        let sandbox_dir = service.sandbox_dir(&sandbox_id);

        let outside_file = sandbox_dir.parent().unwrap().join("outside_secret.txt");
        fs::write(&outside_file, b"SECRET_DATA").await.unwrap();

        let symlink_path = sandbox_dir.join("leak.txt");
        std::os::unix::fs::symlink(&outside_file, &symlink_path).unwrap();

        let result = service.file_exists(&sandbox_id, "leak.txt").await;
        assert!(result.is_err(), "Checking existence of a symlink should fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Symlinks are not allowed"),
            "Error should mention symlinks: {}",
            err_msg
        );
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_delete_file_symlink_blocked() {
        let config = Arc::new(create_test_config());
        let service = StaticFileService::new(config);
        let sandbox_id = uuid::Uuid::new_v4();

        service.ensure_sandbox_dir(&sandbox_id).await.unwrap();
        let sandbox_dir = service.sandbox_dir(&sandbox_id);

        let outside_file = sandbox_dir.parent().unwrap().join("outside_secret.txt");
        fs::write(&outside_file, b"SECRET_DATA").await.unwrap();

        let symlink_path = sandbox_dir.join("leak.txt");
        std::os::unix::fs::symlink(&outside_file, &symlink_path).unwrap();

        let result = service.delete_file(&sandbox_id, "leak.txt").await;
        assert!(result.is_err(), "Deleting a symlink should fail");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Symlinks are not allowed"),
            "Error should mention symlinks: {}",
            err_msg
        );

        // Ensure the target file was NOT deleted
        assert!(outside_file.exists(), "Symlink target should not be deleted");
    }
}
