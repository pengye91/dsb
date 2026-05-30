// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # SSH Gateway Session Manager
//!
//! This module handles HTTP communication with the DSB API for SSH session management.
//!
//! ## Overview
//!
//! The `SessionManager` is responsible for:
//! - Creating SSH session records via the DSB API
//! - Authorizing sandbox access before SSH connection
//! - Sending heartbeats to keep sessions alive
//! - Terminating sessions when connections end
//!
//! ## DSB API Integration
//!
//! This module communicates with the DSB API for:
//!
//! - **Authorization**: Validate sandbox access via `/ssh/authorize`
//! - **Session Lifecycle**: Create, heartbeat, terminate sessions
//! - **API Key Authentication**: All requests include `X-API-Key` header
//!
//! **Important Note**: The SSH gateway connects **directly** to Docker daemon for exec operations,
//! not through the DSB API. The DSB API is used only for authorization and session tracking.
//!
//! ## Example
//!
//! ```rust,no_run
//! use ssh_gateway::session::SessionManager;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create session manager with DSB API URL
//! let manager = SessionManager::new("http://localhost:8080", Some("api-key".to_string()));
//!
//! // Authorize sandbox access
//! let sandbox_id = uuid::Uuid::new_v4();
//! let auth = manager.authorize_sandbox(&sandbox_id).await?;
//! assert!(auth.authorized);
//!
//! // Create SSH session
//! let session = manager.create_session(&sandbox_id, "192.168.1.100").await?;
//! println!("Created session: {}", session.id);
//!
//! // Send heartbeat
//! manager.send_heartbeat(&session.id, 1024, 2048).await?;
//!
//! // Terminate session
//! manager.terminate_session(&session.id, "Connection closed").await?;
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// DSB API session response.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct SshSession {
    /// Session ID (UUID)
    pub id: uuid::Uuid,
    /// Associated sandbox ID
    pub sandbox_id: uuid::Uuid,
    /// Session state (e.g., "active", "terminated")
    pub state: String,
    /// Client IP address
    pub client_ip: String,
    /// Connection timestamp (RFC 3339)
    pub connected_at: String,
    /// Last activity timestamp (RFC 3339)
    pub last_activity_at: String,
    /// Cumulative bytes sent to client
    pub bytes_sent: i64,
    /// Cumulative bytes received from client
    pub bytes_received: i64,
}

/// DSB API authorization context.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthContext {
    /// Whether access is authorized
    pub authorized: bool,
    /// Sandbox information (if authorized)
    #[serde(default)]
    pub sandbox: Option<SandboxInfo>,
    /// Granted permissions
    #[serde(default)]
    #[allow(dead_code)]
    pub permissions: Vec<String>,
}

/// Sandbox information from authorization response.
#[derive(Debug, Clone, Deserialize)]
pub struct SandboxInfo {
    /// Sandbox ID
    #[allow(dead_code)]
    pub id: uuid::Uuid,
    /// Sandbox state
    #[allow(dead_code)]
    pub state: String,
    /// Container or pod ID
    pub container_id: String,
}

/// DSB API error response.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ApiError {
    /// Error message
    pub error: String,
    /// Optional hint for resolving the error
    #[serde(skip_deserializing)]
    pub hint: Option<String>,
}

/// Session manager for DSB API communication.
#[derive(Debug, Clone)]
pub struct SessionManager {
    /// HTTP client for API requests
    client: Client,

    /// DSB API base URL (e.g., "http://localhost:8080")
    api_url: String,

    /// Optional API key for authentication
    api_key: Option<String>,
}

impl SessionManager {
    /// Create a new session manager.
    ///
    /// # Arguments
    ///
    /// * `api_url` - DSB API base URL (e.g., "http://localhost:8080")
    /// * `api_key` - Optional API key for SSH gateway authentication
    ///
    /// # Returns
    ///
    /// A new `SessionManager` instance
    pub fn new(api_url: &str, api_key: Option<String>) -> Self {
        Self {
            client: Client::new(),
            api_url: api_url.to_string(),
            api_key,
        }
    }

    /// Get the API URL.
    #[allow(dead_code)]
    pub fn get_api_url(&self) -> &str {
        &self.api_url
    }

    /// Build a request with optional API key authentication.
    fn build_request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        let mut request = self.client.request(method, url);

        // Add API key if configured
        if let Some(ref key) = self.api_key {
            request = request.header("X-API-Key", key);
        }

        request
    }

    /// Authorize sandbox access via DSB API.
    ///
    /// This validates:
    /// 1. API key (if configured)
    /// 2. Sandbox exists
    /// 3. Sandbox is in "running" state
    ///
    /// # Arguments
    ///
    /// * `sandbox_id` - Sandbox UUID to authorize
    ///
    /// # Returns
    ///
    /// Authorization context with container_id and permissions
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Sandbox not found (404)
    /// - Sandbox not running (403)
    /// - Invalid API key (401)
    /// - API communication failure
    #[instrument(skip(self), fields(sandbox_id = %sandbox_id))]
    pub async fn authorize_sandbox(&self, sandbox_id: &uuid::Uuid) -> Result<AuthContext> {
        let url = format!("{}/ssh/authorize/{}", self.api_url, sandbox_id);
        debug!("Authorizing sandbox access: {}", url);

        let response = self
            .build_request(reqwest::Method::GET, &url)
            .send()
            .await
            .context("Failed to send authorize request")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read response body")?;

        if !status.is_success() {
            anyhow::bail!("Authorization failed ({}): {}", status.as_u16(), body);
        }

        let auth: AuthContext =
            serde_json::from_str(&body).context("Failed to parse authorization response")?;

        debug!("Authorization result: authorized={}", auth.authorized);
        Ok(auth)
    }

    /// Create a new SSH session via DSB API.
    ///
    /// # Arguments
    ///
    /// * `sandbox_id` - Sandbox UUID to connect to
    /// * `client_ip` - Client IP address for logging
    ///
    /// # Returns
    ///
    /// Created SSH session with session ID
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Invalid API key (401)
    /// - Sandbox not found (404)
    /// - API communication failure
    #[instrument(skip(self), fields(sandbox_id = %sandbox_id, client_ip = %client_ip))]
    #[allow(dead_code)]
    pub async fn create_session(
        &self,
        sandbox_id: &uuid::Uuid,
        client_ip: &str,
    ) -> Result<SshSession> {
        let url = format!("{}/ssh-sessions", self.api_url);
        debug!("Creating SSH session: {}", url);

        #[derive(Serialize)]
        struct CreateRequest {
            sandbox_id: uuid::Uuid,
            client_ip: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            ssh_version: Option<String>,
            auth_method: String,
        }

        let request_body = CreateRequest {
            sandbox_id: *sandbox_id,
            client_ip: client_ip.to_string(),
            ssh_version: Some("SSH-2.0-Russh_0.45".to_string()),
            auth_method: "api_key".to_string(),
        };

        let response = self
            .build_request(reqwest::Method::POST, &url)
            .json(&request_body)
            .send()
            .await
            .context("Failed to send create session request")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read response body")?;

        if !status.is_success() {
            anyhow::bail!("Failed to create session ({}): {}", status.as_u16(), body);
        }

        let session: SshSession =
            serde_json::from_str(&body).context("Failed to parse session response")?;

        debug!("Created SSH session: {}", session.id);
        Ok(session)
    }

    /// Send heartbeat to update session activity.
    ///
    /// This should be called periodically (e.g., every 30 seconds) to:
    /// - Keep session alive (prevent idle timeout)
    /// - Update byte transfer statistics
    ///
    /// # Arguments
    ///
    /// * `session_id` - SSH session UUID
    /// * `bytes_sent` - Total bytes sent from server to client
    /// * `bytes_received` - Total bytes received from client
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Session not found (404)
    /// - API communication failure
    #[instrument(skip(self), fields(session_id = %session_id, bytes_sent, bytes_received))]
    #[allow(dead_code)]
    pub async fn send_heartbeat(
        &self,
        session_id: &uuid::Uuid,
        bytes_sent: i64,
        bytes_received: i64,
    ) -> Result<()> {
        let url = format!("{}/ssh-sessions/{}/heartbeat", self.api_url, session_id);
        debug!("Sending heartbeat: {}", url);

        #[derive(Serialize)]
        struct HeartbeatRequest {
            bytes_sent: i64,
            bytes_received: i64,
        }

        let request_body = HeartbeatRequest {
            bytes_sent,
            bytes_received,
        };

        let response = self
            .build_request(reqwest::Method::POST, &url)
            .json(&request_body)
            .send()
            .await
            .context("Failed to send heartbeat request")?;

        let status = response.status();

        if !status.is_success() {
            let body = response
                .text()
                .await
                .context("Failed to read response body")?;
            anyhow::bail!("Failed to send heartbeat ({}): {}", status.as_u16(), body);
        }

        debug!("Heartbeat sent successfully");
        Ok(())
    }

    /// Terminate an SSH session.
    ///
    /// Call this when:
    /// - Client disconnects
    /// - Authentication fails
    /// - Connection error occurs
    ///
    /// # Arguments
    ///
    /// * `session_id` - SSH session UUID
    /// * `reason` - Termination reason for logging
    ///
    /// # Errors
    ///
    /// Returns error if API communication fails, but session will still
    /// be cleaned up by the idle timeout task on the DSB server.
    #[instrument(skip(self), fields(session_id = %session_id, reason = %reason))]
    #[allow(dead_code)]
    pub async fn terminate_session(&self, session_id: &uuid::Uuid, reason: &str) -> Result<()> {
        let url = format!("{}/ssh-sessions/{}/terminate", self.api_url, session_id);
        debug!("Terminating SSH session: {}", url);

        #[derive(Serialize)]
        struct TerminateRequest {
            reason: String,
        }

        let request_body = TerminateRequest {
            reason: reason.to_string(),
        };

        let response = self
            .build_request(reqwest::Method::POST, &url)
            .json(&request_body)
            .send()
            .await
            .context("Failed to send terminate request")?;

        let status = response.status();

        if !status.is_success() {
            let body = response
                .text()
                .await
                .context("Failed to read response body")?;
            anyhow::bail!(
                "Failed to terminate session ({}): {}",
                status.as_u16(),
                body
            );
        }

        debug!("SSH session terminated successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_manager_creation() {
        // Use config system instead of hardcoded URL
        let config = dsb::config::load_for_tests().expect("Failed to load test config");
        let api_url = config.ssh.api_url;

        let manager = SessionManager::new(&api_url, None);
        assert_eq!(manager.api_url, api_url);
        assert!(manager.api_key.is_none());
    }

    #[test]
    fn test_session_manager_with_api_key() {
        // Use config system instead of hardcoded URL
        let config = dsb::config::load_for_tests().expect("Failed to load test config");
        let api_url = config.ssh.api_url;

        let manager = SessionManager::new(&api_url, Some("test-key".to_string()));
        assert_eq!(manager.api_url, api_url);
        assert_eq!(manager.api_key, Some("test-key".to_string()));
    }

    // Note: Full integration tests require a running DSB server
    // These would be in tests/integration_tests.rs
}
