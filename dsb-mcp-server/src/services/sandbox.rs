// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Sandbox Service for session-based sandbox management.
//!
//! This service provides MCP tools for managing DSB sandboxes through session IDs
//! rather than direct sandbox IDs. It handles sandbox lifecycle, command execution,
//! file operations, and static file serving.
//!
//! The service mirrors the Python implementation at `tools/dsb_tools/sandbox.py`,
//! providing session-based sandbox caching and automatic sandbox creation.

use crate::dsb_client::DSBClient;
use crate::session::SessionManager;
use crate::settings::Settings;
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext, wrapper::Parameters},
    model::{
        CallToolRequestParam, CallToolResult, ErrorCode, ErrorData, Implementation,
        InitializeRequestParam, InitializeResult, ListToolsResult, PaginatedRequestParam,
        ServerCapabilities, ServerInfo,
    },
    schemars,
    service::RequestContext,
    tool, tool_router, RoleServer, ServerHandler,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ========== Service Definition ==========

/// Sandbox service with session-based tool routing.
///
/// Manages sandboxes via session IDs, providing automatic sandbox creation,
/// reuse, and cleanup. All tools accept `session_id` as their first parameter
/// and resolve to the underlying sandbox ID through the `SessionManager`.
#[derive(Debug, Clone)]
pub struct SandboxService {
    dsb_client: Arc<DSBClient>,
    session_manager: Arc<SessionManager>,
    settings: Arc<Settings>,
    tool_router: ToolRouter<SandboxService>,
}

impl SandboxService {
    /// Create a new sandbox service.
    pub fn new(
        dsb_client: Arc<DSBClient>,
        session_manager: Arc<SessionManager>,
        settings: Arc<Settings>,
    ) -> Self {
        Self {
            dsb_client,
            session_manager,
            settings,
            tool_router: Self::tool_router(),
        }
    }

    /// Resolve a session to an existing sandbox ID, returning an error if none exists.
    fn resolve_session(&self, session_id: &str) -> Result<Uuid, ErrorData> {
        self.session_manager.get(session_id).ok_or_else(|| {
            ErrorData::invalid_params(
                "session_id",
                Some(serde_json::json!(format!(
                    "No sandbox found for session '{}'. Create a sandbox first using create_sandbox.",
                    session_id
                ))),
            )
        })
    }

    /// Resolve a session to a sandbox ID, creating one if none exists.
    async fn resolve_or_create_session(&self, session_id: &str) -> Result<Uuid, ErrorData> {
        self.session_manager
            .resolve_or_create(session_id, &self.dsb_client, &self.settings)
            .await
            .map_err(|e| {
                ErrorData::internal_error(
                    "resolve_or_create",
                    Some(serde_json::json!(e.to_string())),
                )
            })
    }
}

// ========== Tool Argument Schemas ==========

/// Arguments for creating a sandbox bound to a session.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateSandboxArgs {
    /// Session ID to bind this sandbox. All operations with this session_id will use this sandbox automatically.
    pub session_id: String,
    /// Optional name for the sandbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Docker image to use (e.g., "docker.io/dsb/sandbox:dev"). If not provided, uses default from settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Environment variables as key-value pairs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<HashMap<String, String>>,
    /// Volume mounts as key-value pairs (host_path -> container_path).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volumes: Option<HashMap<String, String>>,
    /// Optional command to run instead of default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// Resource limits (e.g., {"memory_mb": 512, "cpu_quota": 100000}).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_limits: Option<crate::dsb_client::ResourceLimits>,
}

/// Arguments for destroying a sandbox.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DestroySandboxArgs {
    /// Session ID of the sandbox to destroy.
    pub session_id: String,
}

/// Arguments for getting sandbox details.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetSandboxArgs {
    /// Session ID of the sandbox to get.
    pub session_id: String,
}

/// Arguments for executing a command in a sandbox.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExecuteCommandArgs {
    /// Session ID of the sandbox to execute the command in.
    pub session_id: String,
    /// Command to execute (will be run via sh -c).
    pub command: String,
    /// Working directory for the command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Environment variables for the command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<HashMap<String, String>>,
    /// Timeout in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// Arguments for uploading a file to a sandbox.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileUploadArgs {
    /// Session ID of the sandbox to upload to.
    pub session_id: String,
    /// Path where the file should be created.
    pub file_path: String,
    /// Content to write to the file.
    pub content: String,
}

/// Arguments for downloading a file from a sandbox.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileDownloadArgs {
    /// Session ID of the sandbox to download from.
    pub session_id: String,
    /// Path of the file to download.
    pub file_path: String,
}

/// Arguments for getting a static file URL.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetStaticFileUrlArgs {
    /// Session ID of the sandbox.
    pub session_id: String,
    /// Path to the file relative to the sandbox's static directory (e.g., "index.html").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

/// Arguments for listing static files.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListStaticFilesArgs {
    /// Session ID of the sandbox.
    pub session_id: String,
}

// ========== Shell command builders ==========

/// Shell-quote a string for safe use in single-quoted shell arguments.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Build a shell command vector for `sh -c`.
///
/// Quotes `working_dir` to prevent command injection via directory paths.
/// Environment values are single-quote escaped.
fn build_shell_command(
    command: &str,
    working_dir: Option<&str>,
    environment: Option<&HashMap<String, String>>,
) -> Vec<String> {
    let mut shell_cmd = String::new();

    if let Some(env) = environment {
        for (key, value) in env {
            // Escape single quotes in values
            let escaped = value.replace('\'', "'\\''");
            shell_cmd.push_str(&format!("export {}='{}'; ", key, escaped));
        }
    }

    if let Some(dir) = working_dir {
        shell_cmd.push_str(&format!("cd {} && ", shell_quote(dir)));
    }

    shell_cmd.push_str(command);

    vec!["sh".to_string(), "-c".to_string(), shell_cmd]
}

// ========== Tool Router ==========

#[tool_router]
impl SandboxService {
    #[tool(
        description = "Create a new sandbox bound to a session ID. If a sandbox already exists for this session and is running, returns the existing one. All subsequent operations with this session_id will automatically use this sandbox."
    )]
    async fn create_sandbox(
        &self,
        Parameters(CreateSandboxArgs {
            session_id,
            name,
            image,
            environment,
            volumes,
            command,
            resource_limits,
        }): Parameters<CreateSandboxArgs>,
    ) -> Result<String, ErrorData> {
        info!(session_id = %session_id, "Creating sandbox for session");

        // Check if a sandbox already exists for this session_id
        if let Some(cached_id) = self.session_manager.get(&session_id) {
            debug!(
                session_id = %session_id,
                sandbox_id = %cached_id,
                "Found cached sandbox, checking status"
            );

            // Verify the sandbox is still running
            match self.dsb_client.get_sandbox(cached_id).await {
                Ok(sandbox) if sandbox.state == "running" => {
                    info!(
                        session_id = %session_id,
                        sandbox_id = %cached_id,
                        "Reusing existing running sandbox"
                    );
                    let result = serde_json::json!({
                        "id": sandbox.id.to_string(),
                        "name": sandbox.config.name,
                        "image": sandbox.config.image,
                        "state": sandbox.state,
                        "created_at": sandbox.created_at,
                        "session_id": session_id,
                        "message": "Sandbox already exists and is running",
                        "reused": true,
                    });
                    return serde_json::to_string_pretty(&result).map_err(|e| {
                        ErrorData::internal_error(
                            "create_sandbox",
                            Some(serde_json::json!(format!(
                                "JSON serialization failed: {}",
                                e
                            ))),
                        )
                    });
                }
                Ok(sandbox) => {
                    debug!(
                        session_id = %session_id,
                        sandbox_id = %cached_id,
                        state = %sandbox.state,
                        "Existing sandbox is not running, creating new one"
                    );
                }
                Err(e) => {
                    warn!(
                        session_id = %session_id,
                        sandbox_id = %cached_id,
                        error = %e,
                        "Failed to query existing sandbox, creating new one"
                    );
                }
            }
        }

        // Determine image
        let final_image = image.unwrap_or_else(|| self.settings.sandbox.default_image.clone());

        // Build the environment, starting with defaults from settings
        // and merging any user-provided overrides.
        let mut final_env = self.settings.get_sandbox_env();
        if let Some(user_env) = environment {
            final_env.extend(user_env);
        }

        // Build volume mounts if provided
        let volume_mounts = volumes.map(|vols| {
            vols.into_iter()
                .map(
                    |(host_path, container_path)| crate::dsb_client::VolumeMount {
                        r#type: "bind".to_string(),
                        host_path,
                        container_path,
                        read_only: false,
                    },
                )
                .collect::<Vec<_>>()
        });

        let config = crate::dsb_client::CreateSandboxConfig {
            image: final_image,
            name,
            environment: Some(final_env),
            port_mappings: None,
            resource_limits,
            volumes: volume_mounts,
            command,
            inactivity_timeout_minutes: None,
            pull_policy: None,
        };

        // Create new sandbox
        let sandbox = self
            .dsb_client
            .create_sandbox_full(config)
            .await
            .map_err(|e| {
                ErrorData::internal_error("create_sandbox", Some(serde_json::json!(e.to_string())))
            })?;

        info!(
            session_id = %session_id,
            sandbox_id = %sandbox.id,
            state = %sandbox.state,
            "Created new sandbox"
        );

        // Cache the session-to-sandbox mapping
        self.session_manager.set(session_id.clone(), sandbox.id);

        let result = serde_json::json!({
            "id": sandbox.id.to_string(),
            "name": sandbox.config.name,
            "image": sandbox.config.image,
            "state": sandbox.state,
            "created_at": sandbox.created_at,
            "session_id": session_id,
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "create_sandbox",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Destroy a sandbox associated with a session ID. Deletes the sandbox and removes it from the session cache."
    )]
    async fn destroy_sandbox(
        &self,
        Parameters(DestroySandboxArgs { session_id }): Parameters<DestroySandboxArgs>,
    ) -> Result<String, ErrorData> {
        let sandbox_id = self.resolve_session(&session_id)?;

        info!(
            session_id = %session_id,
            sandbox_id = %sandbox_id,
            "Destroying sandbox"
        );

        self.dsb_client
            .delete_sandbox(sandbox_id)
            .await
            .map_err(|e| {
                ErrorData::internal_error("destroy_sandbox", Some(serde_json::json!(e.to_string())))
            })?;

        // Remove from cache
        self.session_manager.remove(&session_id);

        let result = serde_json::json!({
            "success": true,
            "message": format!("Destroyed sandbox for session {}", session_id),
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "destroy_sandbox",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Get details of a sandbox associated with a session ID. Returns sandbox ID, name, image, state, and timestamps."
    )]
    async fn get_sandbox(
        &self,
        Parameters(GetSandboxArgs { session_id }): Parameters<GetSandboxArgs>,
    ) -> Result<String, ErrorData> {
        let sandbox_id = self.resolve_session(&session_id)?;

        let sandbox = self.dsb_client.get_sandbox(sandbox_id).await.map_err(|e| {
            ErrorData::internal_error("get_sandbox", Some(serde_json::json!(e.to_string())))
        })?;

        let result = serde_json::json!({
            "id": sandbox.id.to_string(),
            "session_id": session_id,
            "name": sandbox.config.name,
            "image": sandbox.config.image,
            "state": sandbox.state,
            "created_at": sandbox.created_at,
            "updated_at": sandbox.updated_at,
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "get_sandbox",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Execute a command in a sandbox. If no sandbox exists for the session, one is created automatically. Returns stdout, stderr, exitCode, errorType, and timedOut."
    )]
    async fn execute_command(
        &self,
        Parameters(ExecuteCommandArgs {
            session_id,
            command,
            working_dir,
            environment,
            timeout: _timeout,
        }): Parameters<ExecuteCommandArgs>,
    ) -> Result<String, ErrorData> {
        info!(
            session_id = %session_id,
            command = %&command[..command.len().min(100)],
            "Executing command"
        );

        // Resolve or create sandbox for this session
        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let cmd = build_shell_command(&command, working_dir.as_deref(), environment.as_ref());

        let result = self
            .dsb_client
            .exec_command(sandbox_id, cmd)
            .await
            .map_err(|e| {
                ErrorData::internal_error("execute_command", Some(serde_json::json!(e.to_string())))
            })?;

        // Format result matching Python MCP spec pattern
        let exit_code = result.exit_code;
        let error_type = if exit_code == 0 {
            serde_json::Value::Null
        } else {
            serde_json::Value::String("CommandExecutionError".to_string())
        };
        let result_data = serde_json::json!({
            "stdout": result.output,
            "stderr": "",
            "exitCode": exit_code,
            "errorType": error_type,
            "timedOut": false,
        });

        serde_json::to_string_pretty(&result_data).map_err(|e| {
            ErrorData::internal_error(
                "execute_command",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Upload file content to a sandbox. If no sandbox exists for the session, one is created automatically. The content is written to the specified path in the sandbox."
    )]
    async fn file_upload(
        &self,
        Parameters(FileUploadArgs {
            session_id,
            file_path,
            content,
        }): Parameters<FileUploadArgs>,
    ) -> Result<String, ErrorData> {
        info!(
            session_id = %session_id,
            file_path = %file_path,
            "Uploading file"
        );

        // Resolve or create sandbox
        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        // Upload via DSB client
        self.dsb_client
            .upload_file(sandbox_id, &file_path, content)
            .await
            .map_err(|e| {
                ErrorData::internal_error("file_upload", Some(serde_json::json!(e.to_string())))
            })?;

        let result = serde_json::json!({
            "success": true,
            "file_path": file_path,
            "message": format!("File uploaded successfully at {}", file_path),
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "file_upload",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Download file content from a sandbox. Returns the file content and metadata."
    )]
    async fn file_download(
        &self,
        Parameters(FileDownloadArgs {
            session_id,
            file_path,
        }): Parameters<FileDownloadArgs>,
    ) -> Result<String, ErrorData> {
        info!(
            session_id = %session_id,
            file_path = %file_path,
            "Downloading file"
        );

        // Resolve or create sandbox
        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        // Download via DSB client
        let content = self
            .dsb_client
            .download_file(sandbox_id, &file_path)
            .await
            .map_err(|e| {
                ErrorData::internal_error("file_download", Some(serde_json::json!(e.to_string())))
            })?;

        let result = serde_json::json!({
            "success": true,
            "file_path": file_path,
            "content": content,
            "encoding": self.settings.sandbox.file_encoding,
            "message": format!("File downloaded successfully from {}", file_path),
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "file_download",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "Get the URL to access a static file served by the sandbox's built-in static file server. Useful for sharing files, viewing HTML pages, or downloading assets."
    )]
    async fn get_static_file_url(
        &self,
        Parameters(GetStaticFileUrlArgs {
            session_id,
            file_path,
        }): Parameters<GetStaticFileUrlArgs>,
    ) -> Result<String, ErrorData> {
        // Resolve or create sandbox
        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        // Apply default from settings if not specified
        let file_path =
            file_path.unwrap_or_else(|| self.settings.sandbox.default_static_file.clone());

        let api_url = &self.settings.dsb.api_url;
        let static_endpoint = &self.settings.sandbox.static_file_endpoint;
        let files_endpoint = &self.settings.sandbox.static_files_endpoint;

        let static_url = format!("{}{}{}/{}", api_url, static_endpoint, sandbox_id, file_path);
        let files_list_url = format!("{}{}{}", api_url, files_endpoint, sandbox_id);

        let result = serde_json::json!({
            "success": true,
            "session_id": session_id,
            "sandbox_id": sandbox_id.to_string(),
            "file_path": file_path,
            "static_url": static_url,
            "files_list_url": files_list_url,
            "message": format!("Static file URL generated for {}", file_path),
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "get_static_file_url",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }

    #[tool(
        description = "List all available static files in the sandbox's static directory. Returns file list with URLs and metadata."
    )]
    async fn list_static_files(
        &self,
        Parameters(ListStaticFilesArgs { session_id }): Parameters<ListStaticFilesArgs>,
    ) -> Result<String, ErrorData> {
        // Resolve or create sandbox
        let sandbox_id = self.resolve_or_create_session(&session_id).await?;

        let api_url = &self.settings.dsb.api_url;
        let files_endpoint = &self.settings.sandbox.static_files_endpoint;
        let files_list_url = format!("{}{}{}", api_url, files_endpoint, sandbox_id);

        // Make HTTP request to list files
        let files = self
            .dsb_client
            .list_static_files(sandbox_id)
            .await
            .map_err(|e| {
                ErrorData::internal_error(
                    "list_static_files",
                    Some(serde_json::json!(e.to_string())),
                )
            })?;

        let result = serde_json::json!({
            "success": true,
            "session_id": session_id,
            "sandbox_id": sandbox_id.to_string(),
            "files_list_url": files_list_url,
            "files": files,
            "total": files.len(),
            "message": format!("Found {} static file(s) in sandbox", files.len()),
        });

        serde_json::to_string_pretty(&result).map_err(|e| {
            ErrorData::internal_error(
                "list_static_files",
                Some(serde_json::json!(format!(
                    "JSON serialization failed: {}",
                    e
                ))),
            )
        })
    }
}

// ========== ServerHandler Implementation ==========

impl ServerHandler for SandboxService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::V_2025_06_18,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "dsb-sandbox-service".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "DSB Sandbox Service - Manage sandboxes via session IDs for code execution, file operations, and static file serving.".to_string(),
            ),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        Ok(self.get_info())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = self.tool_router.list_all();
        tracing::info!("list_tools: returning {} sandbox tools", tools.len());
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: Default::default(),
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let tool_name = &request.name;
        let tool_route = self.tool_router.map.get(tool_name).ok_or_else(|| {
            ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("Tool not found: {}", tool_name),
                None,
            )
        })?;

        let tool_ctx = ToolCallContext::new(self, request.clone(), ctx);
        (tool_route.call)(tool_ctx).await
    }
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_service() -> SandboxService {
        let settings = Settings::load_for_tests().unwrap();
        let dsb_client = Arc::new(DSBClient::new(settings.clone()).unwrap());
        let session_manager = Arc::new(SessionManager::new());
        let settings = Arc::new(settings);
        SandboxService::new(dsb_client, session_manager, settings)
    }

    #[test]
    fn test_create_sandbox_args_deserialization() {
        let json = r#"{
            "session_id": "test-session-1",
            "name": "my-sandbox",
            "image": "python:3.12"
        }"#;
        let args: CreateSandboxArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
        assert_eq!(args.name, Some("my-sandbox".to_string()));
        assert_eq!(args.image, Some("python:3.12".to_string()));
        assert!(args.volumes.is_none());
        assert!(args.command.is_none());
        assert!(args.resource_limits.is_none());
    }

    #[test]
    fn test_create_sandbox_args_minimal() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: CreateSandboxArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
        assert!(args.name.is_none());
        assert!(args.image.is_none());
    }

    #[test]
    fn test_create_sandbox_args_with_volumes() {
        let json = r#"{
            "session_id": "test-session-1",
            "volumes": {"/host/path": "/container/path"}
        }"#;
        let args: CreateSandboxArgs = serde_json::from_str(json).unwrap();
        assert_eq!(
            args.volumes.unwrap().get("/host/path").unwrap(),
            "/container/path"
        );
    }

    #[test]
    fn test_execute_command_args_deserialization() {
        let json = r#"{
            "session_id": "test-session-1",
            "command": "echo hello",
            "working_dir": "/workspace",
            "environment": {"FOO": "bar"},
            "timeout": 30
        }"#;
        let args: ExecuteCommandArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
        assert_eq!(args.command, "echo hello");
        assert_eq!(args.working_dir, Some("/workspace".to_string()));
        assert_eq!(args.environment.unwrap().get("FOO").unwrap(), "bar");
        assert_eq!(args.timeout, Some(30));
    }

    #[test]
    fn test_execute_command_args_minimal() {
        let json = r#"{
            "session_id": "test-session-1",
            "command": "ls"
        }"#;
        let args: ExecuteCommandArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.command, "ls");
        assert!(args.working_dir.is_none());
    }

    #[test]
    fn test_file_upload_args_deserialization() {
        let json = r#"{
            "session_id": "test-session-1",
            "file_path": "/app/config.json",
            "content": "{\"key\": \"value\"}"
        }"#;
        let args: FileUploadArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.file_path, "/app/config.json");
        assert_eq!(args.content, "{\"key\": \"value\"}");
    }

    #[test]
    fn test_file_download_args_deserialization() {
        let json = r#"{
            "session_id": "test-session-1",
            "file_path": "/app/config.json"
        }"#;
        let args: FileDownloadArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.file_path, "/app/config.json");
    }

    #[test]
    fn test_get_static_file_url_args_deserialization() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: GetStaticFileUrlArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
        assert!(args.file_path.is_none());
    }

    #[test]
    fn test_get_static_file_url_args_with_path() {
        let json = r#"{
            "session_id": "test-session-1",
            "file_path": "index.html"
        }"#;
        let args: GetStaticFileUrlArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.file_path, Some("index.html".to_string()));
    }

    #[test]
    fn test_list_static_files_args_deserialization() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: ListStaticFilesArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
    }

    #[test]
    fn test_destroy_sandbox_args_deserialization() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: DestroySandboxArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
    }

    #[test]
    fn test_get_sandbox_args_deserialization() {
        let json = r#"{"session_id": "test-session-1"}"#;
        let args: GetSandboxArgs = serde_json::from_str(json).unwrap();
        assert_eq!(args.session_id, "test-session-1");
    }

    #[test]
    fn test_resource_limits_deserialization() {
        let json = r#"{
            "memory_mb": 512,
            "cpu_quota": 100000
        }"#;
        let limits: crate::dsb_client::ResourceLimits = serde_json::from_str(json).unwrap();
        assert_eq!(limits.memory_mb, Some(512));
        assert_eq!(limits.cpu_quota, Some(100000));
        assert!(limits.cpu_period.is_none());
    }

    #[tokio::test]
    async fn test_all_8_tools_registered() {
        let service = create_test_service();
        let tools = service.tool_router.list_all();
        assert_eq!(tools.len(), 8, "Should have exactly 8 tools registered");

        let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();

        assert!(tool_names.contains(&"create_sandbox".to_string()));
        assert!(tool_names.contains(&"destroy_sandbox".to_string()));
        assert!(tool_names.contains(&"get_sandbox".to_string()));
        assert!(tool_names.contains(&"execute_command".to_string()));
        assert!(tool_names.contains(&"file_upload".to_string()));
        assert!(tool_names.contains(&"file_download".to_string()));
        assert!(tool_names.contains(&"get_static_file_url".to_string()));
        assert!(tool_names.contains(&"list_static_files".to_string()));
    }

    #[tokio::test]
    async fn test_tool_router_not_empty() {
        let service = create_test_service();
        let tools = service.tool_router.list_all();
        assert!(
            !tools.is_empty(),
            "Tool router should have tools registered"
        );
    }

    #[test]
    fn test_resolve_session_missing_returns_error() {
        let service = create_test_service();
        let result = service.resolve_session("nonexistent-session");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_session_found() {
        let service = create_test_service();
        let sandbox_id = Uuid::new_v4();
        service
            .session_manager
            .set("test-session".to_string(), sandbox_id);

        let result = service.resolve_session("test-session");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), sandbox_id);
    }

    #[tokio::test]
    async fn test_handler_get_static_file_url_direct() {
        let service = create_test_service();

        // Set up a cached session
        let sandbox_id = Uuid::new_v4();
        service
            .session_manager
            .set("test-session".to_string(), sandbox_id);

        let args = Parameters(GetStaticFileUrlArgs {
            session_id: "test-session".to_string(),
            file_path: Some("index.html".to_string()),
        });

        let result = service.get_static_file_url(args).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("index.html"));
        assert!(output.contains(&sandbox_id.to_string()));
    }

    #[tokio::test]
    async fn test_handler_get_sandbox_no_session() {
        let service = create_test_service();

        let args = Parameters(GetSandboxArgs {
            session_id: "nonexistent".to_string(),
        });

        let result = service.get_sandbox(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handler_destroy_sandbox_no_session() {
        let service = create_test_service();

        let args = Parameters(DestroySandboxArgs {
            session_id: "nonexistent".to_string(),
        });

        let result = service.destroy_sandbox(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_server_handler_info() {
        let service = create_test_service();
        let info = service.get_info();
        assert_eq!(info.server_info.name, "dsb-sandbox-service");
    }

    // === Shell command injection tests ===

    #[test]
    fn test_shell_quote_basic() {
        assert_eq!(shell_quote("/workspace"), "'/workspace'");
    }

    #[test]
    fn test_shell_quote_with_single_quote() {
        assert_eq!(shell_quote("/path/with'quote"), "'/path/with'\\''quote'");
    }

    #[test]
    fn test_build_shell_command_quotes_working_dir() {
        let env: HashMap<String, String> = HashMap::new();
        let cmd = build_shell_command("echo hello", Some("; rm -rf /"), Some(&env));
        assert_eq!(cmd[0], "sh");
        assert_eq!(cmd[1], "-c");
        // working_dir must be single-quoted so metacharacters are literal
        assert_eq!(cmd[2], "cd '; rm -rf /' && echo hello");
    }

    #[test]
    fn test_build_shell_command_command_with_metacharacters() {
        let cmd = build_shell_command("echo hello; ls", None, None);
        assert_eq!(cmd[0], "sh");
        assert_eq!(cmd[1], "-c");
        // command should be passed through as-is (intentional shell code)
        assert_eq!(cmd[2], "echo hello; ls");
    }

    #[test]
    fn test_build_shell_command_escapes_environment_values() {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar'baz".to_string());
        let cmd = build_shell_command("echo $FOO", None, Some(&env));
        assert_eq!(cmd[2], "export FOO='bar'\\''baz'; echo $FOO");
    }

    #[test]
    fn test_build_shell_command_working_dir_with_single_quote() {
        let cmd = build_shell_command("echo hello", Some("/path/with'quote"), None);
        assert_eq!(cmd[2], "cd '/path/with'\\''quote' && echo hello");
    }
}
