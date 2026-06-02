// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! Container command execution and HTTP proxy operations.

use super::{DockerManager, DockerManagerError};
use bollard::exec::{CreateExecOptions, StartExecOptions};

impl DockerManager {
    /// Executes a command inside a running container and captures the output.
    ///
    /// This method creates a new exec instance, runs the command, and returns
    /// the combined stdout and stderr output.
    ///
    /// # Arguments
    ///
    /// * `id` - The container ID
    /// * `command` - Command to execute as a list of strings (e.g., `["ls", "-la"]`)
    ///
    /// # Returns
    ///
    /// - `Ok(String)` - Combined stdout and stderr output from the command
    /// - `Err(...)` - If execution fails
    ///
    /// # Execution Details
    ///
    /// - Runs in attached mode (captures output)
    /// - Captures both stdout and stderr
    /// - Waits for command completion before returning
    /// - Uses the container's default shell
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Container doesn't exist
    /// - Container is not running
    /// - Command is invalid or not found
    /// - Container crashes during execution
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let container_id = "abc123";
    ///
    /// // List files in root directory
    /// let output = docker.exec_container(
    ///     container_id,
    ///     vec!["ls".to_string(), "-la".to_string(), "/".to_string()]
    /// ).await?;
    ///
    /// println!("Command output:\n{}", output);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn exec_container(
        &self,
        id: &str,
        command: Vec<String>,
        timeout_secs: Option<u64>,
    ) -> Result<String, DockerManagerError> {
        self.exec_container_with_stdin(id, command, None, timeout_secs)
            .await
    }

    /// Execute a command in a container with optional stdin input.
    pub async fn exec_container_with_stdin(
        &self,
        id: &str,
        command: Vec<String>,
        stdin: Option<String>,
        timeout_secs: Option<u64>,
    ) -> Result<String, DockerManagerError> {
        self.exec_container_with_stdin_result(id, command, stdin, timeout_secs)
            .await
            .map(|result| result.output)
    }

    /// Execute a command in a container with optional stdin input and capture
    /// both the command output and exit status.
    pub async fn exec_container_with_stdin_result(
        &self,
        id: &str,
        command: Vec<String>,
        stdin: Option<String>,
        timeout_secs: Option<u64>,
    ) -> Result<crate::core::manager::ExecCommandResult, DockerManagerError> {
        use bollard::exec::StartExecResults;
        use futures_util::stream::StreamExt;
        use tokio::io::AsyncWriteExt;
        use tokio::time::{timeout as tokio_timeout, Duration};

        // Create exec instance
        let options = CreateExecOptions {
            cmd: Some(command),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            attach_stdin: Some(stdin.is_some()), // Only attach stdin if provided
            ..Default::default()
        };

        let exec = self.docker.create_exec(id, options).await?;
        let exec_id = exec.id.clone();

        // Start the exec instance and get output
        let start_options = Some(StartExecOptions {
            detach: false,
            ..Default::default()
        });

        let result = self.docker.start_exec(&exec_id, start_options).await?;

        // Define the async operation to potentially timeout
        let exec_operation = async {
            match result {
                StartExecResults::Attached {
                    mut output, input, ..
                } => {
                    // Write stdin if provided
                    if let Some(stdin_data) = stdin {
                        let mut writer = input;
                        writer.write_all(stdin_data.as_bytes()).await?;
                        writer.shutdown().await?;
                    }

                    let mut full_output = String::new();
                    while let Some(log_result) = output.next().await {
                        match log_result {
                            Ok(log) => {
                                // LogOutput has variants: StdOut, StdErr, StdIn, Console
                                // All have a `message` field with Bytes
                                use bollard::container::LogOutput;
                                match log {
                                    LogOutput::StdOut { message }
                                    | LogOutput::StdErr { message } => {
                                        full_output.push_str(&String::from_utf8_lossy(&message));
                                    }
                                    LogOutput::Console { message } => {
                                        full_output.push_str(&String::from_utf8_lossy(&message));
                                    }
                                    LogOutput::StdIn { .. } => {}
                                }
                            }
                            Err(e) => return Err(DockerManagerError::Bollard(e)),
                        }
                    }
                    let exec_details = self.docker.inspect_exec(&exec_id).await?;
                    let exit_code = exec_details
                        .exit_code
                        .and_then(|code| i32::try_from(code).ok())
                        .unwrap_or_default();

                    Ok(crate::core::manager::ExecCommandResult {
                        output: full_output,
                        exit_code,
                    })
                }
                StartExecResults::Detached => Ok(crate::core::manager::ExecCommandResult {
                    output: String::from("Command started in detached mode"),
                    exit_code: 0,
                }),
            }
        };

        // Apply timeout if specified
        match timeout_secs {
            Some(timeout_seconds) => {
                let duration = Duration::from_secs(timeout_seconds);
                match tokio_timeout(duration, exec_operation).await {
                    Ok(result) => result,
                    Err(_) => Err(DockerManagerError::Timeout(format!(
                        "Command timed out after {} seconds",
                        timeout_seconds
                    ))),
                }
            }
            None => exec_operation.await,
        }
    }

    /// Execute an HTTP request to the tool_proxy inside a container.
    ///
    /// This method sends an HTTP request to the tool_proxy service running
    /// inside the sandbox container on port 8080. This provides a more
    /// efficient alternative to exec-based tool execution with persistent
    /// browser sessions.
    ///
    /// # Arguments
    ///
    /// * `container_id` - The container ID
    /// * `path` - The API path (e.g., "/web/scrape", "/browser/navigate")
    /// * `method` - HTTP method ("GET" or "POST")
    /// * `body` - Optional JSON body for POST requests
    /// * `timeout_secs` - Optional timeout in seconds
    ///
    /// # Returns
    ///
    /// - `Ok(serde_json::Value)` - JSON response from tool_proxy
    /// - `Err(...)` - If request fails or container is not accessible
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::docker::DockerManager;
    /// # async fn example() -> Result<(), dsb::docker::DockerManagerError> {
    /// # let docker = DockerManager::new()?;
    /// let result = docker.exec_container_http(
    ///     "abc123",
    ///     "/web/scrape",
    ///     "POST",
    ///     Some(serde_json::json!({"url": "https://example.com"})),
    ///     Some(60),
    /// ).await?;
    /// println!("Result: {:?}", result);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn exec_container_http(
        &self,
        container_id: &str,
        path: &str,
        method: &str,
        body: Option<serde_json::Value>,
        timeout_secs: Option<u64>,
    ) -> Result<serde_json::Value, DockerManagerError> {
        use bollard::query_parameters::InspectContainerOptions;
        use tokio::time::{timeout as tokio_timeout, Duration};

        // Check cache first
        let cached_ip = {
            if let Ok(cache) = self.ip_cache.read() {
                cache.get(container_id).cloned()
            } else {
                None
            }
        };

        let container_ip = if let Some(ip) = cached_ip {
            ip
        } else {
            // Get container info to find its IP address
            let inspect = self
                .docker
                .inspect_container(container_id, None::<InspectContainerOptions>)
                .await
                .map_err(|e| {
                    DockerManagerError::Api(format!("Failed to inspect container: {}", e))
                })?;

            // Extract container IP from network settings
            // Prefer the configured network (dsb_dsb-network) over default bridge
            let ip = inspect
                .network_settings
                .as_ref()
                .and_then(|ns| ns.networks.as_ref())
                .and_then(|networks| {
                    // Try to find the configured network first
                    if let Some(ref config_network) = self.config.docker.network {
                        if let Some(net) = networks.get(config_network) {
                            return net.ip_address.as_ref().and_then(|ip| {
                                if ip.is_empty() {
                                    None
                                } else {
                                    Some(ip.to_string())
                                }
                            });
                        }
                    }
                    // Fall back to first available network
                    networks.values().next().and_then(|net| {
                        net.ip_address.as_ref().and_then(|ip| {
                            if ip.is_empty() {
                                None
                            } else {
                                Some(ip.to_string())
                            }
                        })
                    })
                })
                .ok_or_else(|| {
                    DockerManagerError::Api("Container has no IP address".to_string())
                })?;

            // Save to cache
            if let Ok(mut cache) = self.ip_cache.write() {
                cache.insert(container_id.to_string(), ip.clone());
            }

            ip
        };

        // Build URL - use container IP on port 8080
        let url = format!("http://{}:8080{}", container_ip, path);

        // Build request
        let mut request_builder = match method.to_uppercase().as_str() {
            "GET" => self.http_client.get(&url),
            "POST" => self.http_client.post(&url),
            "PUT" => self.http_client.put(&url),
            "DELETE" => self.http_client.delete(&url),
            _ => {
                return Err(DockerManagerError::Api(format!(
                    "Unsupported HTTP method: {}",
                    method
                )))
            }
        };

        tracing::debug!(
            container_id = %container_id,
            url = %url,
            timeout_secs = ?timeout_secs,
            "Sending HTTP request to container"
        );

        // Add body if provided
        if let Some(json_body) = body {
            request_builder = request_builder.json(&json_body);
        }

        // Execute request with optional timeout
        let request_future = request_builder.send();

        tracing::debug!("About to await HTTP request future (tokio_timeout wrapper)");

        let response = match timeout_secs {
            Some(timeout) => {
                let duration = Duration::from_secs(timeout);
                match tokio_timeout(duration, request_future).await {
                    Ok(Ok(resp)) => {
                        tracing::debug!("HTTP request succeeded, got response");
                        resp
                    }
                    Ok(Err(e)) => {
                        tracing::error!("HTTP request failed: {}", e);
                        return Err(DockerManagerError::Http(format!(
                            "HTTP request failed: {}",
                            e
                        )));
                    }
                    Err(_) => {
                        tracing::error!("HTTP request timed out after {} seconds", timeout);
                        return Err(DockerManagerError::Timeout(format!(
                            "HTTP request timed out after {} seconds",
                            timeout
                        )));
                    }
                }
            }
            None => {
                tracing::debug!("No timeout, awaiting request future");
                request_future
                    .await
                    .map_err(|e| DockerManagerError::Http(format!("HTTP request failed: {}", e)))?
            }
        };

        tracing::debug!("About to parse JSON from response");

        // Check response status
        if !response.status().is_success() {
            let status = response.status();

            // Try to parse as JSON - tool_proxy returns RFC 9457 format
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&error_text) {
                // tool_proxy returns {"error_code": "...", "message": "...", "detail": "..."}
                // Extract and use the error_code if present
                if let Some(error_code_str) = error_json.get("error_code").and_then(|v| v.as_str())
                {
                    // Try to parse the error code using unified ErrorCode
                    let _error_code = crate::core::errors::ErrorCode::parse(error_code_str)
                        .unwrap_or(crate::core::errors::ErrorCode::InternalError);

                    let error_msg = error_json
                        .get("detail")
                        .or_else(|| error_json.get("message"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(&error_text);

                    // Return structured error with proper error code preserved
                    return Err(DockerManagerError::ToolProxy {
                        message: error_msg.to_string(),
                        operation: path.to_string(),
                    });
                }
            }

            // Fallback for non-JSON or missing error_code
            return Err(DockerManagerError::Api(format!(
                "HTTP error {}: {}",
                status, error_text
            )));
        }

        // Parse JSON response
        tracing::debug!("Calling response.json().await (uses client read_timeout)");
        let result = response.json::<serde_json::Value>().await.map_err(|e| {
            DockerManagerError::Api(format!("Failed to parse JSON response: {}", e))
        })?;

        tracing::debug!("Successfully parsed JSON response");
        Ok(result)
    }
}
