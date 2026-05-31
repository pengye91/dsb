use crate::core::types::ActivityType;
use super::ExecToolHttpRequest;
use super::SandboxService;

impl SandboxService {
    /// Executes a command inside a running sandbox.
    ///
    /// This method runs a command in the container's default shell and returns
    /// the combined stdout and stderr output.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox
    /// * `command` - The command to execute as a list of strings (e.g., `["ls", "-la"]`)
    ///
    /// # Returns
    ///
    /// - `Ok(String)` - The command output (stdout + stderr combined)
    /// - `Err(...)` - If execution fails
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The sandbox doesn't exist
    /// - The container hasn't been created yet
    /// - The command execution fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::SandboxService;
    /// # use uuid::Uuid;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// # let service: SandboxService = unimplemented!();
    /// let id = Uuid::new_v4();
    /// let output = service.exec_sandbox(&id, vec![
    ///     "cat".to_string(),
    ///     "/etc/os-release".to_string(),
    /// ]).await?;
    ///
    /// println!("Command output:\n{}", output);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn exec_sandbox(
        &self,
        id: &uuid::Uuid,
        command: Vec<String>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.exec_sandbox_with_stdin(id, command, None, None).await
    }

    /// Execute a command in a sandbox with optional stdin input.
    pub async fn exec_sandbox_with_stdin(
        &self,
        id: &uuid::Uuid,
        command: Vec<String>,
        stdin: Option<String>,
        timeout: Option<u64>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.exec_sandbox_result_with_stdin(id, command, stdin, timeout)
            .await
            .map(|result| result.output)
    }

    /// Execute a command in a sandbox with optional stdin input and return the
    /// command output together with its exit code.
    pub async fn exec_sandbox_result_with_stdin(
        &self,
        id: &uuid::Uuid,
        command: Vec<String>,
        stdin: Option<String>,
        timeout: Option<u64>,
    ) -> Result<crate::core::manager::ExecCommandResult, Box<dyn std::error::Error + Send + Sync>>
    {
        let sandbox = self
            .state
            .get_sandbox(id)
            .await
            .ok_or("Sandbox not found")?;

        // Check if sandbox is running
        if sandbox.state != crate::core::types::SandboxState::Running {
            return Err(format!(
                "Sandbox is not running (current state: {})",
                sandbox.state.as_str()
            )
            .into());
        }

        let container_id = sandbox
            .container_id
            .as_ref()
            .ok_or("Container not running or not started")?;

        let result = self
            .backend
            .exec_with_stdin_result(container_id, command.clone(), stdin, timeout)
            .await?;

        // Record exec activity
        self.record_activity(
            *id,
            ActivityType::Exec,
            serde_json::json!({"command": command}),
        )
        .await;

        Ok(result)
    }

    /// Execute a tool via HTTP proxy inside the sandbox container.
    ///
    /// This method provides efficient tool execution via HTTP requests to the
    /// tool_proxy service running inside the sandbox on port 8080.
    ///
    /// # Arguments
    ///
    /// * `id` - The sandbox UUID
    /// * `request` - Tool execution request parameters
    ///
    /// # Returns
    ///
    /// - `Ok(serde_json::Value)` - JSON response from the tool
    /// - `Err(...)` - If the request fails
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::{SandboxService, ExecToolHttpRequest};
    /// # async fn example(service: SandboxService) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    /// let id = uuid::Uuid::new_v4();
    /// let request = ExecToolHttpRequest {
    ///     interpreter: "python".to_string(),
    ///     script_path: "web_tools".to_string(),
    ///     action: "scrape".to_string(),
    ///     args: serde_json::json!({"url": "https://example.com"}),
    ///     timeout: Some(60),
    ///     environment: None,
    /// };
    /// let result = service.exec_tool_http(&id, request).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn exec_tool_http(
        &self,
        id: &uuid::Uuid,
        request: ExecToolHttpRequest,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
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
            .ok_or("Container not running or not started")?;

        // Determine tool timeout based on config and script type
        let tool_timeout = self.get_tool_timeout(&request.script_path, request.timeout);

        // Calculate HTTP timeout with buffer for network overhead
        let http_timeout = tool_timeout + self.tool_timeouts.http_buffer_secs;

        // Build request body for the generic /exec endpoint
        let body = serde_json::json!({
            "interpreter": request.interpreter,
            "script_path": request.script_path,
            "action": request.action,
            "args": request.args,
            "timeout": tool_timeout,
            "environment": request.environment
        });

        // Execute HTTP request to the generic /exec endpoint
        let result = self
            .backend
            .exec_http(
                container_id,
                "/exec",
                "POST",
                Some(body),
                Some(http_timeout),
            )
            .await;

        // Record activity
        self.record_activity(
            *id,
            ActivityType::Exec,
            serde_json::json!({
                "script": request.script_path,
                "action": request.action,
                "method": "http",
            }),
        )
        .await;

        result.map_err(|e| e.into())
    }

    /// Get the tool timeout based on script path and requested timeout.
    ///
    /// If a timeout is requested, it is capped by `max_allowed_secs`.
    /// If not specified, the default timeout is inferred from the script path.
    ///
    /// # Arguments
    ///
    /// * `script_path` - Path to the script being executed
    /// * `requested` - Optional user-requested timeout in seconds
    ///
    /// # Returns
    ///
    /// The timeout to use in seconds
    fn get_tool_timeout(&self, script_path: &str, requested: Option<u64>) -> u64 {
        match requested {
            Some(t) => t.min(self.tool_timeouts.max_allowed_secs),
            None => self.infer_default_timeout(script_path),
        }
    }

    /// Infer the default timeout based on the script path.
    ///
    /// This maps script paths to their appropriate default timeouts
    /// based on the tool type.
    ///
    /// # Arguments
    ///
    /// * `script_path` - Path to the script being executed
    ///
    /// # Returns
    ///
    /// The default timeout in seconds for the tool type
    fn infer_default_timeout(&self, script_path: &str) -> u64 {
        // Browser tools need longer timeouts for page loads
        if script_path.contains("browser") {
            self.tool_timeouts.browser_tools_secs
        // Web scraping tools (non-browser)
        } else if script_path.contains("web") {
            self.tool_timeouts.web_tools_secs
        // Databend database tools
        } else if script_path.contains("databend") {
            self.tool_timeouts.databend_tools_secs
        // Default for all other tools
        } else {
            self.tool_timeouts.default_secs
        }
    }

}
