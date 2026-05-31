// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Docker Compose lifecycle management for DSB stack
//!
//! This module provides the DSBStack struct which manages the startup and shutdown
//! of the DSB services including dsb-server (in docker) and dsb-mcp-server (local process).

use std::process::Stdio;
use std::time::Duration;

use anyhow::Context;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Manages the DSB stack lifecycle - dsb-server (docker) and dsb-mcp-server (local)
pub struct DSBStack {
    /// The MCP server child process
    mcp_server_process: Option<Child>,
}

impl DSBStack {
    /// Starts the DSB stack:
    /// 1. Starts docker-compose up -d dsb-server (and dependencies)
    /// 2. Polls http://localhost:8080/health until healthy
    /// 3. Starts dsb-mcp-server as local process on port 3223
    /// 4. Polls http://localhost:3223/mcp until reachable
    pub async fn start() -> anyhow::Result<Self> {
        info!("Starting DSB stack...");

        let mut stack = DSBStack {
            mcp_server_process: None,
        };

        // Step 1: Start docker-compose services
        stack.start_docker_compose().await?;

        // Step 2: Wait for dsb-server to be healthy
        stack.wait_for_dsb_server_healthy().await?;

        // Step 3: Start dsb-mcp-server as local process
        stack.start_mcp_server().await?;

        // Step 4: Wait for MCP server to be reachable
        stack.wait_for_mcp_server().await?;

        info!("DSB stack started successfully");
        Ok(stack)
    }

    /// Execute docker compose up -d from the project root
    async fn start_docker_compose(&mut self) -> anyhow::Result<()> {
        info!("Starting docker-compose services...");

        // Get the project root (parent of dsb-agent-tester)
        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Could not find project root"))?;

        let docker_compose_dir = project_root.join("docker");

        let output = Command::new("docker")
            .args([
                "compose",
                "-f",
                "docker-compose.yml",
                "up",
                "-d",
                "dsb-server",
            ])
            .current_dir(&docker_compose_dir)
            .output()
            .await
            .context("Failed to execute docker compose up")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("docker compose up failed: {}", stderr);
            anyhow::bail!("docker compose up failed with status: {}", output.status);
        }

        info!("docker-compose services started");
        Ok(())
    }

    /// Wait for dsb-server health check to pass
    async fn wait_for_dsb_server_healthy(&self) -> anyhow::Result<()> {
        info!("Waiting for dsb-server to be healthy at http://localhost:8080/health...");

        let client = reqwest::Client::new();
        let max_attempts = 60;
        let retry_interval = Duration::from_secs(2);

        for attempt in 1..=max_attempts {
            match client
                .get("http://localhost:8080/health")
                .timeout(Duration::from_secs(5))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    info!("dsb-server is healthy");
                    return Ok(());
                }
                Ok(resp) => {
                    info!(
                        "dsb-server returned non-success status: {} (attempt {}/{})",
                        resp.status(),
                        attempt,
                        max_attempts
                    );
                }
                Err(e) => {
                    debug!(
                        "dsb-server not ready yet: {} (attempt {}/{})",
                        e, attempt, max_attempts
                    );
                }
            }
            sleep(retry_interval).await;
        }

        anyhow::bail!(
            "dsb-server failed to become healthy after {} attempts",
            max_attempts
        )
    }

    /// Start dsb-mcp-server as a local process
    async fn start_mcp_server(&mut self) -> anyhow::Result<()> {
        info!("Starting dsb-mcp-server on port 3223...");

        // Get the project root
        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Could not find project root"))?;

        // Check if the binary exists (debug or release)
        let debug_binary = project_root.join("target/debug/dsb-mcp-server");
        let release_binary = project_root.join("target/release/dsb-mcp-server");

        let binary_path = if debug_binary.exists() {
            debug_binary
        } else if release_binary.exists() {
            release_binary
        } else {
            // Try to build it
            info!("MCP server binary not found, building...");
            Command::new("cargo")
                .args(["build", "--bin", "dsb-mcp-server"])
                .current_dir(project_root)
                .output()
                .await
                .context("Failed to build dsb-mcp-server")?;

            if debug_binary.exists() {
                debug_binary
            } else {
                anyhow::bail!("dsb-mcp-server binary not found after build")
            }
        };

        info!("Starting MCP server from: {:?}", binary_path);

        let mut child = Command::new(&binary_path)
            .args(["--port", "3223", "--dsb-api-url", "http://localhost:8080"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn dsb-mcp-server process")?;

        // Log stdout in background
        if let Some(stdout) = child.stdout.take() {
            let mut reader = BufReader::new(stdout).lines();
            tokio::spawn(async move {
                while let Ok(Some(line)) = reader.next_line().await {
                    info!("[dsb-mcp-server] {}", line);
                }
            });
        }

        // Log stderr in background
        if let Some(stderr) = child.stderr.take() {
            let mut reader = BufReader::new(stderr).lines();
            tokio::spawn(async move {
                while let Ok(Some(line)) = reader.next_line().await {
                    warn!("[dsb-mcp-server] {}", line);
                }
            });
        }

        // Give the process a moment to start
        sleep(Duration::from_secs(1)).await;

        // Check if process is still running
        if let Ok(exit_status) = child.try_wait() {
            if exit_status.is_some() {
                anyhow::bail!("dsb-mcp-server exited unexpectedly during startup");
            }
        }

        self.mcp_server_process = Some(child);
        info!("dsb-mcp-server process started");
        Ok(())
    }

    /// Wait for MCP server to be reachable at http://localhost:3223/mcp
    async fn wait_for_mcp_server(&self) -> anyhow::Result<()> {
        info!("Waiting for dsb-mcp-server to be reachable at http://localhost:3223/mcp...");

        let client = reqwest::Client::new();
        let max_attempts = 30;
        let retry_interval = Duration::from_secs(2);

        for attempt in 1..=max_attempts {
            match client
                .get("http://localhost:3223/mcp")
                .timeout(Duration::from_secs(5))
                .send()
                .await
            {
                Ok(resp) => {
                    info!(
                        "dsb-mcp-server is reachable (status: {}) (attempt {}/{})",
                        resp.status(),
                        attempt,
                        max_attempts
                    );
                    return Ok(());
                }
                Err(e) => {
                    debug!(
                        "dsb-mcp-server not ready yet: {} (attempt {}/{})",
                        e, attempt, max_attempts
                    );
                }
            }
            sleep(retry_interval).await;
        }

        anyhow::bail!(
            "dsb-mcp-server failed to become reachable after {} attempts",
            max_attempts
        )
    }

    /// Stops the DSB stack:
    /// 1. Stops dsb-mcp-server process
    /// 2. Executes docker compose down dsb-server
    /// 3. Verifies containers stopped
    pub async fn stop(mut self) -> anyhow::Result<()> {
        info!("Stopping DSB stack...");

        // Step 1: Stop MCP server process
        // Use std::mem::take to move the child out without consuming self
        if let Some(mut child) = self.mcp_server_process.take() {
            info!("Stopping dsb-mcp-server process...");
            if let Err(e) = child.kill().await {
                warn!("Failed to kill dsb-mcp-server: {}", e);
            }
            // Wait for process to fully terminate
            let _ = child.wait().await;
        }

        // Step 2: Stop docker-compose services
        self.stop_docker_compose().await?;

        info!("DSB stack stopped successfully");
        Ok(())
    }

    /// Execute docker compose down dsb-server
    async fn stop_docker_compose(&self) -> anyhow::Result<()> {
        info!("Stopping docker-compose services...");

        let project_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Could not find project root"))?;

        let docker_compose_dir = project_root.join("docker");

        let output = Command::new("docker")
            .args(["compose", "-f", "docker-compose.yml", "down", "dsb-server"])
            .current_dir(&docker_compose_dir)
            .output()
            .await
            .context("Failed to execute docker compose down")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("docker compose down reported non-success: {}", stderr);
        }

        // Step 3: Verify containers stopped
        self.verify_containers_stopped().await?;

        info!("docker-compose services stopped");
        Ok(())
    }

    /// Verify that dsb-server container has stopped
    async fn verify_containers_stopped(&self) -> anyhow::Result<()> {
        info!("Verifying containers stopped...");

        let output = Command::new("docker")
            .args([
                "ps",
                "--filter",
                "name=dsb-server",
                "--format",
                "{{.Names}}",
            ])
            .output()
            .await
            .context("Failed to execute docker ps")?;

        let container_output = String::from_utf8_lossy(&output.stdout);
        let running_containers: Vec<&str> =
            container_output.lines().filter(|s| !s.is_empty()).collect();

        if !running_containers.is_empty() {
            warn!("Found still-running containers: {:?}", running_containers);
            // Don't fail - this is just verification
        } else {
            info!("All dsb-server containers stopped");
        }

        Ok(())
    }
}

impl Drop for DSBStack {
    fn drop(&mut self) {
        // If stop() wasn't called explicitly, try to clean up
        if self.mcp_server_process.is_some() {
            warn!("DSBStack dropped without calling stop() - resource cleanup may be incomplete");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dsb_stack_lifecycle() -> anyhow::Result<()> {
        // This test requires docker to be available
        // In CI, this might be skipped
        info!("Starting DSB stack lifecycle test...");

        let stack = DSBStack::start().await?;

        // Verify services are running
        let client = reqwest::Client::new();
        let health_resp = client
            .get("http://localhost:8080/health")
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        assert!(
            health_resp.status().is_success(),
            "dsb-server health check failed"
        );

        // Stop the stack
        stack.stop().await?;

        info!("DSB stack lifecycle test passed");
        Ok(())
    }
}
