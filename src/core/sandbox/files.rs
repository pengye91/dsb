use super::SandboxService;
use crate::core::static_files::shell_quote;
use crate::core::types::ActivityType;

impl SandboxService {
    /// Uploads a file to the sandbox's container filesystem.
    ///
    /// This method writes file data directly to the container's filesystem
    /// using Docker exec with base64 encoding for safe data transmission.
    ///
    /// # Arguments
    ///
    /// * `id` - The sandbox UUID
    /// * `dest_path` - Destination path in container (e.g., "/app/data.txt")
    /// * `data` - File contents as bytes
    ///
    /// # Returns
    ///
    /// - `Ok(())` - File uploaded successfully
    /// - `Err(...)` - Upload failed
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Sandbox doesn't exist
    /// - Container is not running
    /// - Destination path is invalid
    /// - Container write operation fails
    pub async fn upload_file(
        &self,
        id: &uuid::Uuid,
        dest_path: &str,
        data: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get sandbox
        let sandbox = self
            .state
            .get_sandbox(id)
            .await
            .ok_or("Sandbox not found")?;

        // Check if running
        if sandbox.state != crate::core::types::SandboxState::Running {
            return Err("Sandbox is not running".into());
        }

        let container_id = sandbox
            .container_id
            .as_ref()
            .ok_or("Container not created")?;

        // Handle relative paths by prepending container's working directory
        let dest_path = if !dest_path.starts_with('/') {
            match self.backend.get_workdir(container_id).await {
                Ok(workdir) => {
                    let workdir = workdir.trim_end_matches('/');
                    let dest_path = dest_path.trim_start_matches('/');
                    format!("{}/{}", workdir, dest_path)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to get container workdir for {}, using '/': {}",
                        container_id,
                        e
                    );
                    format!("/{}", dest_path.trim_start_matches('/'))
                }
            }
        } else {
            dest_path.to_string()
        };

        // Use the full absolute path (without leading /) as the tar entry path.
        // Docker's archive API extracts as root and creates intermediate directories,
        // so we upload to "/" with the full path in the tar entry. This avoids
        // permission issues with mkdir as a non-root user.
        let tar_entry_path = dest_path.trim_start_matches('/');
        if tar_entry_path.is_empty() {
            return Err("Invalid destination path".into());
        }

        // Build a tar archive containing the file at its full path
        let data_len = data.len();
        let tar_data = {
            let mut builder = tar::Builder::new(Vec::new());
            let mut header = tar::Header::new_gnu();
            header.set_path(tar_entry_path)?;
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
            header.set_cksum();
            builder.append(&header, &data[..])?;
            builder.into_inner()?
        };

        // Upload tar archive to "/" — Docker extracts as root, creating all directories
        self.backend
            .upload_archive(container_id, "/", tar_data)
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                Box::new(std::io::Error::other(format!(
                    "Failed to upload file: {}",
                    e
                )))
            })?;

        // Record upload activity
        self.record_activity(
            *id,
            ActivityType::Upload,
            serde_json::json!({
                "path": dest_path,
                "size": data_len
            }),
        )
        .await;

        Ok(())
    }

    /// Build the shell commands used by `download_file` to check, size, and read
    /// a file path inside a sandbox container.
    pub(super) fn build_download_cmds(src_path: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
        let quoted = shell_quote(src_path);
        (
            vec![
                "sh".to_string(),
                "-c".to_string(),
                format!("test -f {} && echo 'exists' || echo 'notfound'", quoted),
            ],
            vec![
                "sh".to_string(),
                "-c".to_string(),
                format!("wc -c < {} 2>/dev/null || echo '0'", quoted),
            ],
            vec![
                "sh".to_string(),
                "-c".to_string(),
                format!("base64 -w0 {}", quoted),
            ],
        )
    }

    /// Downloads a file from the sandbox's container filesystem.
    ///
    /// This method reads file data from the container's filesystem
    /// using Docker exec with base64 encoding for safe data transmission.
    ///
    /// # Arguments
    ///
    /// * `id` - The sandbox UUID
    /// * `src_path` - Source path in container (e.g., "/app/data.txt")
    ///
    /// # Returns
    ///
    /// - `Ok(Vec<u8>)` - File contents as bytes
    /// - `Err(...)` - Download failed
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Sandbox doesn't exist
    /// - Container is not running
    /// - File doesn't exist
    /// - File size exceeds configured limit
    /// - Container read operation fails
    pub async fn download_file(
        &self,
        id: &uuid::Uuid,
        src_path: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        use base64::prelude::*;

        let max_file_size = self.max_file_size_bytes;

        // Get sandbox
        let sandbox = self
            .state
            .get_sandbox(id)
            .await
            .ok_or("Sandbox not found")?;

        // Check if running
        if sandbox.state != crate::core::types::SandboxState::Running {
            return Err("Sandbox is not running".into());
        }

        let container_id = sandbox
            .container_id
            .as_ref()
            .ok_or("Container not created")?;

        // Handle relative paths by prepending container's working directory
        let src_path = if !src_path.starts_with('/') {
            // Relative path - get container's working directory
            match self.backend.get_workdir(container_id).await {
                Ok(workdir) => {
                    // Ensure workdir doesn't end with / and src_path doesn't start with /
                    let workdir = workdir.trim_end_matches('/');
                    let src_path = src_path.trim_start_matches('/');
                    format!("{}/{}", workdir, src_path)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to get container workdir for {}, using '/': {}",
                        container_id,
                        e
                    );
                    // Fallback to root directory
                    format!("/{}", src_path.trim_start_matches('/'))
                }
            }
        } else {
            // Absolute path - use as-is
            src_path.to_string()
        };

        // Build download commands
        let (check_cmd, size_cmd, read_cmd) = Self::build_download_cmds(&src_path);

        // Check file exists
        let check_result = self.backend.exec(container_id, check_cmd).await?;
        if !check_result.contains("exists") {
            return Err("File not found".into());
        }

        // Get file size
        let size_result = self.backend.exec(container_id, size_cmd).await?;
        let file_size: u64 = size_result.trim().parse().unwrap_or(0);

        if file_size > max_file_size {
            return Err(format!("File size {} exceeds limit {}", file_size, max_file_size).into());
        }

        // Read file using base64 for safe transmission
        let encoded = self.backend.exec(container_id, read_cmd).await?;

        // Decode base64
        let decoded = BASE64_STANDARD
            .decode(encoded.trim())
            .map_err(|_| "Failed to decode file content")?;

        // Record download activity
        self.record_activity(
            *id,
            ActivityType::Download,
            serde_json::json!({
                "path": src_path,
                "size": decoded.len()
            }),
        )
        .await;

        Ok(decoded)
    }
}
