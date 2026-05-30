// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! DSB API client
//!
//! HTTP client for communicating with the DSB server.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error};
use uuid::Uuid;

use crate::settings::Settings;
use schemars::JsonSchema;

/// Configuration for creating a sandbox with full options.
///
/// Used with [`DSBClient::create_sandbox_full`] to specify all
/// available sandbox creation parameters.
#[derive(Debug, Clone, Default)]
pub struct CreateSandboxConfig {
    /// Docker image to use (e.g., 'python:3.12').
    pub image: String,
    /// Optional name for the sandbox.
    pub name: Option<String>,
    /// Environment variables as key-value pairs.
    pub environment: Option<HashMap<String, String>>,
    /// Port mappings from host to container.
    pub port_mappings: Option<Vec<PortMapping>>,
    /// Resource limits for the container.
    pub resource_limits: Option<ResourceLimits>,
    /// Volume mounts.
    pub volumes: Option<Vec<VolumeMount>>,
    /// Command to run in the container.
    pub command: Option<Vec<String>>,
    /// Auto-delete sandbox after inactivity timeout (minutes).
    pub inactivity_timeout_minutes: Option<u64>,
    /// Docker image pull policy (e.g., 'Always', 'IfNotPresent').
    pub pull_policy: Option<String>,
}

/// Paginated API response wrapper used by the DSB server for list endpoints
#[derive(Debug, Deserialize)]
struct PaginatedResponse<T> {
    data: Vec<T>,
}

/// DSB API client
#[derive(Clone, Debug)]
pub struct DSBClient {
    settings: Settings,
    client: Client,
}

impl DSBClient {
    /// Create a new DSB client
    pub fn new(settings: Settings) -> Result<Self, String> {
        let mut builder =
            Client::builder().timeout(std::time::Duration::from_secs(settings.dsb.timeout_secs));

        if let Some(api_key) = &settings.dsb.api_key {
            let mut headers = reqwest::header::HeaderMap::new();
            if let Ok(mut val) = reqwest::header::HeaderValue::from_str(api_key) {
                val.set_sensitive(true);
                headers.insert("x-api-key", val);
            }
            builder = builder.default_headers(headers);
        }

        let client = builder
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self { settings, client })
    }

    /// Get the DSB API URL
    pub fn api_url(&self) -> &str {
        &self.settings.dsb.api_url
    }

    /// Get the configured SearXNG API URL.
    pub fn searxng_api_url(&self) -> &str {
        &self.settings.web.searxng_url
    }

    /// Get the configured request timeout in seconds.
    pub fn timeout_secs(&self) -> u64 {
        self.settings.dsb.timeout_secs
    }

    /// Create a sandbox
    pub async fn create_sandbox(
        &self,
        image: String,
        name: Option<String>,
    ) -> anyhow::Result<Sandbox> {
        let url = format!("{}/sandboxes", self.settings.dsb.api_url);
        let mut body = serde_json::json!({ "image": image });
        if let Some(name) = name {
            body["name"] = serde_json::json!(name);
        }

        debug!("Creating sandbox with request: {}", body);

        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("Failed to create sandbox: {}", error_text);
            return Err(anyhow::anyhow!("Failed to create sandbox: {}", error_text));
        }

        let sandbox: Sandbox = response.json().await?;
        Ok(sandbox)
    }

    /// Create a sandbox with full configuration
    pub async fn create_sandbox_full(
        &self,
        config: CreateSandboxConfig,
    ) -> anyhow::Result<Sandbox> {
        let url = format!("{}/sandboxes", self.settings.dsb.api_url);
        let mut body = serde_json::json!({ "image": config.image });

        if let Some(name) = config.name {
            body["name"] = serde_json::json!(name);
        }
        if let Some(env) = config.environment {
            body["environment"] = serde_json::json!(env);
        }
        if let Some(ports) = config.port_mappings {
            body["port_mappings"] = serde_json::json!(ports);
        }
        if let Some(limits) = config.resource_limits {
            body["resource_limits"] = serde_json::json!(limits);
        }
        if let Some(vols) = config.volumes {
            body["volumes"] = serde_json::json!(vols);
        }
        if let Some(cmd) = config.command {
            body["command"] = serde_json::json!(cmd);
        }
        if let Some(timeout) = config.inactivity_timeout_minutes {
            body["inactivity_timeout_minutes"] = serde_json::json!(timeout);
        }
        if let Some(policy) = config.pull_policy {
            body["pull_policy"] = serde_json::json!(policy);
        }

        debug!("Creating sandbox with full config, request: {}", body);

        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            error!("Failed to create sandbox: {}", error_text);
            return Err(anyhow::anyhow!("Failed to create sandbox: {}", error_text));
        }

        let sandbox: Sandbox = response.json().await?;
        Ok(sandbox)
    }

    /// List sandboxes
    pub async fn list_sandboxes(&self) -> anyhow::Result<Vec<Sandbox>> {
        let url = format!("{}/sandboxes", self.settings.dsb.api_url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Failed to list sandboxes: {}", error_text));
        }

        // API returns paginated response: {"data": [...], "pagination": {...}}
        let paginated: PaginatedResponse<Sandbox> = response.json().await?;
        Ok(paginated.data)
    }

    /// Delete a sandbox
    pub async fn delete_sandbox(&self, sandbox_id: Uuid) -> anyhow::Result<()> {
        let url = format!("{}/sandboxes/{}", self.settings.dsb.api_url, sandbox_id);

        let response = self.client.delete(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Failed to delete sandbox: {}", error_text));
        }

        Ok(())
    }

    /// Get server health info
    pub async fn get_health(&self) -> anyhow::Result<HealthInfo> {
        let url = format!("{}/health", self.settings.dsb.api_url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Failed to get health info: {}", error_text));
        }

        let health: HealthInfo = response.json().await?;
        Ok(health)
    }

    /// Get a specific sandbox by ID
    pub async fn get_sandbox(&self, sandbox_id: Uuid) -> anyhow::Result<Sandbox> {
        let url = format!("{}/sandboxes/{}", self.settings.dsb.api_url, sandbox_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Failed to get sandbox: {}", error_text));
        }

        let sandbox: Sandbox = response.json().await?;
        Ok(sandbox)
    }

    /// Execute a command in a sandbox
    pub async fn exec_command(
        &self,
        sandbox_id: Uuid,
        command: Vec<String>,
    ) -> anyhow::Result<ExecResult> {
        self.exec_command_with_stdin(sandbox_id, command, None)
            .await
    }

    /// Execute a command in a sandbox with optional stdin.
    pub async fn exec_command_with_stdin(
        &self,
        sandbox_id: Uuid,
        command: Vec<String>,
        stdin: Option<String>,
    ) -> anyhow::Result<ExecResult> {
        let url = format!(
            "{}/sandboxes/{}/exec",
            self.settings.dsb.api_url, sandbox_id
        );

        let body = ExecCommandRequest { command, stdin };

        debug!(
            "Executing command in sandbox {}: {:?}",
            sandbox_id, body.command
        );

        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Failed to execute command: {}", error_text));
        }

        let result: ExecResult = response.json().await?;
        Ok(result)
    }

    /// Execute a structured tool action in a sandbox through the DSB HTTP tool endpoint.
    pub async fn execute_tool(
        &self,
        sandbox_id: Uuid,
        interpreter: &str,
        script_path: &str,
        action: &str,
        args: Option<serde_json::Value>,
        timeout: Option<u64>,
    ) -> anyhow::Result<serde_json::Value> {
        let url = format!(
            "{}/sandboxes/{}/tools",
            self.settings.dsb.api_url, sandbox_id
        );
        let body = ToolExecutionRequest {
            interpreter: interpreter.to_string(),
            script_path: script_path.to_string(),
            action: action.to_string(),
            args,
            timeout,
        };

        debug!(
            "Executing tool action in sandbox {}: {} {}",
            sandbox_id, script_path, action
        );

        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Failed to execute tool: {}", error_text));
        }

        let result: serde_json::Value = response.json().await?;
        Ok(result)
    }

    /// Upload a file to a sandbox via the HTTP multipart endpoint.
    ///
    /// Uses `POST /sandboxes/{id}/upload` with `multipart/form-data`.
    /// This avoids the kube-exec URI length limits that plague the base64
    /// exec fallback for files larger than a few kilobytes.
    pub async fn upload_file(
        &self,
        sandbox_id: Uuid,
        path: &str,
        content: String,
    ) -> anyhow::Result<()> {
        let url = format!(
            "{}/sandboxes/{}/upload",
            self.settings.dsb.api_url, sandbox_id
        );

        let form = reqwest::multipart::Form::new()
            .text("path", path.to_string())
            .part(
                "file",
                reqwest::multipart::Part::bytes(content.into_bytes()).file_name("upload.bin"),
            );

        debug!("Uploading file to sandbox {}: {}", sandbox_id, path);

        let response = self.client.post(&url).multipart(form).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Failed to upload file: {}", error_text));
        }

        Ok(())
    }

    /// Download a file from a sandbox.
    ///
    /// Uses the DSB server's `/sandboxes/{id}/download?path=...` endpoint
    /// to retrieve file contents.
    pub async fn download_file(&self, sandbox_id: Uuid, path: &str) -> anyhow::Result<String> {
        let url = format!(
            "{}/sandboxes/{}/download?path={}",
            self.settings.dsb.api_url,
            sandbox_id,
            urlencoding::encode(path)
        );

        debug!("Downloading file from sandbox {}: {}", sandbox_id, path);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Failed to download file: {}", error_text));
        }

        let content = response.text().await?;
        Ok(content)
    }

    /// List static files for a sandbox.
    ///
    /// Queries the DSB server's static files endpoint to retrieve a list
    /// of files in the sandbox's `/public` directory.
    pub async fn list_static_files(
        &self,
        sandbox_id: Uuid,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let url = format!("{}/static/files/{}", self.settings.dsb.api_url, sandbox_id);

        debug!("Listing static files for sandbox {}", sandbox_id);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!(
                "Failed to list static files: {}",
                error_text
            ));
        }

        // Parse the response - handle both array and object formats
        let body: serde_json::Value = response.json().await?;
        let files = match body {
            serde_json::Value::Array(arr) => arr,
            serde_json::Value::Object(ref map) => map
                .get("files")
                .or_else(|| map.get("data"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
            _ => vec![],
        };

        Ok(files)
    }
}

#[derive(Debug, Serialize)]
struct ToolExecutionRequest {
    interpreter: String,
    script_path: String,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

/// Sandbox representation from the DSB API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sandbox {
    /// Sandbox ID (UUID)
    pub id: Uuid,
    /// Current state (e.g., "running", "stopped")
    pub state: String,
    /// Sandbox configuration
    pub config: SandboxConfig,
    /// Container or pod ID
    #[serde(default, rename = "container_id", alias = "containerId")]
    pub container_id: Option<String>,
    /// Creation timestamp (RFC 3339)
    #[serde(rename = "created_at", alias = "createdAt")]
    pub created_at: String,
    /// Last update timestamp (RFC 3339)
    #[serde(rename = "updated_at", alias = "updatedAt")]
    pub updated_at: String,
}

/// Sandbox configuration from the DSB API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Docker image name
    pub image: String,
    /// Optional sandbox name
    pub name: Option<String>,
    /// Environment variables as JSON object
    #[serde(default)]
    pub environment: serde_json::Value,
}

/// Result of executing a command in a sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    /// Combined stdout and stderr output
    pub output: String,
    /// Process exit code
    #[serde(default, rename = "exit_code", alias = "exitCode")]
    pub exit_code: i32,
}

/// Request body for executing a command in a sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecCommandRequest {
    /// Command and arguments to execute
    command: Vec<String>,
    /// Optional stdin input
    #[serde(skip_serializing_if = "Option::is_none")]
    stdin: Option<String>,
}

/// Health check response from the DSB API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthInfo {
    /// Service status (e.g., "ok")
    pub status: String,
}

/// Port mapping for exposing container ports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct PortMapping {
    /// Port on the host machine
    #[serde(rename = "host_port", alias = "hostPort")]
    pub host_port: u16,
    /// Port inside the container
    #[serde(rename = "container_port", alias = "containerPort")]
    pub container_port: u16,
    /// Protocol (tcp or udp)
    pub protocol: String,
}

/// Resource limits for a container.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct ResourceLimits {
    /// Memory limit in megabytes
    #[serde(rename = "memory_mb", alias = "memoryMb")]
    pub memory_mb: Option<u64>,
    /// CPU quota (microseconds of CPU time per period)
    #[serde(rename = "cpu_quota", alias = "cpuQuota")]
    pub cpu_quota: Option<u64>,
    /// CPU period in microseconds
    #[serde(rename = "cpu_period", alias = "cpuPeriod")]
    pub cpu_period: Option<u64>,
    /// CPU shares (relative weight)
    #[serde(rename = "cpu_shares", alias = "cpuShares")]
    pub cpu_shares: Option<u64>,
    /// Maximum number of processes
    #[serde(rename = "pids_limit", alias = "pidsLimit")]
    pub pids_limit: Option<u64>,
}

/// Volume mount configuration for a container.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct VolumeMount {
    /// Mount type (bind, volume, tmpfs)
    pub r#type: String,
    /// Source path on the host
    #[serde(rename = "host_path", alias = "hostPath")]
    pub host_path: String,
    /// Destination path in the container
    #[serde(rename = "container_path", alias = "containerPath")]
    pub container_path: String,
    /// Whether the mount is read-only
    #[serde(rename = "read_only", alias = "readOnly")]
    pub read_only: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_sandbox_serialization() {
        let sandbox = Sandbox {
            id: Uuid::new_v4(),
            state: "running".to_string(),
            config: SandboxConfig {
                image: "python:3.12".to_string(),
                name: Some("test".to_string()),
                environment: serde_json::json!({"DEBUG": "true"}),
            },
            container_id: Some("container-123".to_string()),
            created_at: "2025-01-06T10:00:00Z".to_string(),
            updated_at: "2025-01-06T10:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&sandbox).unwrap();
        assert!(json.contains("\"id\""));
        assert!(json.contains("\"state\""));
        assert!(json.contains("running"));
    }

    #[test]
    fn test_sandbox_deserialization() {
        let json = r#"{
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "state": "running",
            "config": {
                "image": "python:3.12",
                "name": "test",
                "environment": {}
            },
            "containerId": "container-123",
            "createdAt": "2025-01-06T10:00:00Z",
            "updatedAt": "2025-01-06T10:00:00Z"
        }"#;

        let sandbox: Sandbox = serde_json::from_str(json).unwrap();
        assert_eq!(sandbox.state, "running");
        assert_eq!(sandbox.config.image, "python:3.12");
        assert_eq!(sandbox.config.name, Some("test".to_string()));
    }

    #[test]
    fn test_exec_result_serialization() {
        let result = ExecResult {
            output: "hello world".to_string(),
            exit_code: 0,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"output\""));
        assert!(json.contains("\"exit_code\""));
        assert!(json.contains("hello world"));
    }

    #[test]
    fn test_exec_result_deserialization() {
        let json = r#"{
            "output": "test output",
            "exit_code": 0
        }"#;

        let result: ExecResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.output, "test output");
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_exec_result_deserialization_legacy_camel_case() {
        let json = r#"{
            "output": "test output",
            "exitCode": 7
        }"#;

        let result: ExecResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.output, "test output");
        assert_eq!(result.exit_code, 7);
    }

    #[test]
    fn test_exec_command_request_serialization_with_stdin() {
        let request = ExecCommandRequest {
            command: vec!["cat".to_string()],
            stdin: Some("hello".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"command\""));
        assert!(json.contains("\"stdin\":\"hello\""));
    }

    #[test]
    fn test_tool_execution_request_serialization() {
        let request = ToolExecutionRequest {
            interpreter: "python".to_string(),
            script_path: "/opt/tools/web_tools.py".to_string(),
            action: "web_scrape".to_string(),
            args: Some(serde_json::json!({"url": "https://example.com"})),
            timeout: Some(60),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"interpreter\":\"python\""));
        assert!(json.contains("\"script_path\":\"/opt/tools/web_tools.py\""));
        assert!(json.contains("\"action\":\"web_scrape\""));
        assert!(json.contains("\"timeout\":60"));
    }

    #[test]
    fn test_sandbox_config_default_environment() {
        let config = SandboxConfig {
            image: "python:3.12".to_string(),
            name: None,
            environment: serde_json::json!({}),
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: SandboxConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.image, "python:3.12");
    }

    #[test]
    fn test_client_creation() {
        let settings = Settings::default();
        let result = DSBClient::new(settings);
        assert!(result.is_ok(), "DSBClient::new should succeed with default settings");
        let client = result.unwrap();
        assert_eq!(client.settings.dsb.api_url, "http://localhost:8080");
        assert_eq!(
            client.settings.web.searxng_url,
            "http://localhost:8888/search"
        );
    }
}
