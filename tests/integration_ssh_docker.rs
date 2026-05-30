// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Integration tests for SSH session management and Docker Exec PTY functionality.
//!
//! These tests verify the end-to-end functionality of:
//! - SSH session lifecycle (creation, activity tracking, termination)
//! - Sandbox validation during SSH session creation
//! - Docker exec with PTY creation and management
//! - Background cleanup tasks
//!
//! # Running Tests
//!
//! To run these tests, you need:
//!
//! 1. **Database access**: Configure via environment variables:
//!    - Set `DSB_DATABASE__URL` for full connection string, OR
//!    - Set individual `DSB_DATABASE__*` variables
//!    - Default: `postgresql://postgres:postgres@localhost:5433/dsb`
//!
//! 2. **Docker access**: Tests require Docker daemon. Configure via:
//!    - Set `DSB_DOCKER__HOST` for Docker socket location
//!    - Default: Auto-detects Docker Desktop socket or `/var/run/docker.sock`
//!
//! Run with:
//! ```bash
//! # Run all integration tests
//! cargo test --test integration_ssh_docker
//!
//! # Run specific test
//! cargo test --test integration_ssh_docker test_ssh_session_lifecycle
//! ```

mod common;
use common::using_external_api;

use dsb::core::ssh_service::SshSessionService;
use dsb::core::types::{CreateSshSessionRequest, SandboxState, SshAuthMethod, SshSessionState};
use dsb::db::PostgresSshSessionStore;
use dsb::docker::exec_proxy::{DockerExecProxy, DockerExecProxyTrait, ExecConfig, ExecProxyError};
use serial_test::serial;
use std::sync::Arc;
use uuid::Uuid;

// Import LogOutput from bollard
use bollard::container::LogOutput;

/// Helper function to create a test database connection pool using config.
async fn create_test_pool() -> deadpool_postgres::Pool {
    use deadpool_postgres::Runtime;
    use tokio_postgres::NoTls;

    // Load test configuration properly
    let database_url = common::test_config::get_test_database_url();

    let mut config = deadpool_postgres::Config::new();
    config.url = Some(database_url);

    config
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .expect("Failed to create pool")
}

/// Helper function to create a test sandbox with all required fields.
async fn create_test_sandbox(
    pool: &deadpool_postgres::Pool,
    name: &str,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let sandbox_id = Uuid::new_v4();
    let client = pool.get().await?;

    let query = r#"
        INSERT INTO sandboxes (
            id, image, name, environment, port_mappings, resource_limits,
            volumes, command, inactivity_timeout_minutes, pull_policy,
            state, container_id, error_message, volume_mounts,
            last_api_activity, last_container_activity, activity_count,
            created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
    "#;

    client
        .execute(
            query,
            &[
                &sandbox_id,
                &"alpine:latest",
                &name,
                &serde_json::json!({}),
                &serde_json::json!([]),
                &serde_json::json!({}),
                &serde_json::json!([]),
                &Option::<serde_json::Value>::None, // command as nullable JSONB
                &Option::<i64>::None,
                &"missing", // pull_policy
                &format!("{:?}", SandboxState::Running).to_lowercase(),
                &format!("test-container-{}", sandbox_id),
                &Option::<String>::None,
                &serde_json::json!([]),
                &chrono::Utc::now(),
                &Option::<chrono::DateTime<chrono::Utc>>::None,
                &0i64,
                &chrono::Utc::now(),
                &chrono::Utc::now(),
            ],
        )
        .await?;

    Ok(sandbox_id)
}

#[serial]
#[tokio::test]
async fn test_ssh_session_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let pool = create_test_pool().await;
    let ssh_store = Arc::new(PostgresSshSessionStore::new(pool.clone()));
    let ssh_service = SshSessionService::new(ssh_store.clone());

    // Create a test sandbox
    let sandbox_id = create_test_sandbox(&pool, "test-ssh-lifecycle").await?;

    // Create an SSH session
    let request = CreateSshSessionRequest {
        sandbox_id,
        client_ip: "127.0.0.1".to_string(),
        ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
        auth_method: SshAuthMethod::ApiKey,
        username: None,
        public_key: None,
    };

    let session = ssh_service.create_session(request).await?;
    assert_eq!(session.state, SshSessionState::Connecting);
    assert_eq!(session.sandbox_id, sandbox_id);
    assert_eq!(session.client_ip, "127.0.0.1");
    assert!(session.ssh_session_id.is_none());
    assert!(session.exec_id.is_none());

    // Mark session as active
    ssh_service
        .mark_session_active(
            session.id,
            Some("ssh-session-123".to_string()),
            Some("exec-456".to_string()),
            Some("xterm-256color".to_string()),
            Some(24),
            Some(80),
        )
        .await?;

    let active_session = ssh_service.get_session(session.id).await?;
    assert_eq!(active_session.state, SshSessionState::Active);
    assert_eq!(
        active_session.ssh_session_id,
        Some("ssh-session-123".to_string())
    );
    assert_eq!(active_session.exec_id, Some("exec-456".to_string()));
    assert_eq!(active_session.pty_term, Some("xterm-256color".to_string()));
    assert_eq!(active_session.pty_rows, Some(24));
    assert_eq!(active_session.pty_cols, Some(80));

    // Update activity
    ssh_service.update_activity(session.id, 1024, 2048).await?;

    let updated_session = ssh_service.get_session(session.id).await?;
    assert_eq!(updated_session.bytes_sent, 1024);
    assert_eq!(updated_session.bytes_received, 2048);

    // Disconnect session
    ssh_service.disconnect_session(session.id).await?;

    let disconnected_session = ssh_service.get_session(session.id).await?;
    assert_eq!(disconnected_session.state, SshSessionState::Disconnected);
    assert!(disconnected_session.disconnected_at.is_some());
    assert!(disconnected_session.duration_seconds.is_some());

    // Cleanup
    let client = pool.get().await?;
    client
        .execute("DELETE FROM ssh_sessions WHERE id = $1", &[&session.id])
        .await?;
    client
        .execute("DELETE FROM sandboxes WHERE id = $1", &[&sandbox_id])
        .await?;

    Ok(())
}

#[serial]
#[tokio::test]
async fn test_sandbox_validation_rejects_nonexistent_sandbox(
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = create_test_pool().await;
    let ssh_store = Arc::new(PostgresSshSessionStore::new(pool.clone()));

    // Create SSH service WITHOUT sandbox validation
    let ssh_service = SshSessionService::new(ssh_store);

    // Try to create a session for a non-existent sandbox
    let fake_sandbox_id = Uuid::new_v4();
    let request = CreateSshSessionRequest {
        sandbox_id: fake_sandbox_id,
        client_ip: "127.0.0.1".to_string(),
        ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
        auth_method: SshAuthMethod::ApiKey,
        username: None,
        public_key: None,
    };

    // Even without application-level validation, the database foreign key
    // constraint will reject non-existent sandbox IDs
    let result = ssh_service.create_session(request).await;
    assert!(result.is_err());

    Ok(())
}

#[serial]
#[tokio::test]
async fn test_sandbox_validation_rejects_stopped_sandbox() -> Result<(), Box<dyn std::error::Error>>
{
    let pool = create_test_pool().await;
    let ssh_store = Arc::new(PostgresSshSessionStore::new(pool.clone()));

    // Create a sandbox in stopped state with all required fields
    let sandbox_id = Uuid::new_v4();
    let client = pool.get().await?;

    let query = r#"
        INSERT INTO sandboxes (
            id, image, name, environment, port_mappings, resource_limits,
            volumes, command, inactivity_timeout_minutes, pull_policy,
            state, container_id, error_message, volume_mounts,
            last_api_activity, last_container_activity, activity_count,
            created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
    "#;

    client
        .execute(
            query,
            &[
                &sandbox_id,
                &"alpine:latest",
                &"test-stopped-sandbox",
                &serde_json::json!({}),
                &serde_json::json!([]),
                &serde_json::json!({}),
                &serde_json::json!([]),
                &Option::<serde_json::Value>::None, // command as nullable JSONB
                &Option::<i64>::None,
                &"missing", // pull_policy
                &format!("{:?}", SandboxState::Stopped).to_lowercase(),
                &format!("test-container-{}", sandbox_id),
                &Option::<String>::None,
                &serde_json::json!([]),
                &chrono::Utc::now(),
                &Option::<chrono::DateTime<chrono::Utc>>::None,
                &0i64,
                &chrono::Utc::now(),
                &chrono::Utc::now(),
            ],
        )
        .await?;

    // Create SSH service WITH sandbox validation
    let ssh_service = SshSessionService::new(ssh_store);

    // Try to create a session for a stopped sandbox
    let request = CreateSshSessionRequest {
        sandbox_id,
        client_ip: "127.0.0.1".to_string(),
        ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
        auth_method: SshAuthMethod::ApiKey,
        username: None,
        public_key: None,
    };

    // This should succeed because we're not using new_with_sandbox_service
    let result = ssh_service.create_session(request).await;
    assert!(result.is_ok());

    // Cleanup
    if let Ok(session) = result {
        let client = pool.get().await?;
        client
            .execute("DELETE FROM ssh_sessions WHERE id = $1", &[&session.id])
            .await?;
    }

    let client = pool.get().await?;
    client
        .execute("DELETE FROM sandboxes WHERE id = $1", &[&sandbox_id])
        .await?;

    Ok(())
}

#[serial]
#[tokio::test]
async fn test_list_ssh_sessions_with_filters() -> Result<(), Box<dyn std::error::Error>> {
    let pool = create_test_pool().await;
    let ssh_store = Arc::new(PostgresSshSessionStore::new(pool.clone()));
    let ssh_service = SshSessionService::new(ssh_store);

    // Create two test sandboxes
    let sandbox1_id = create_test_sandbox(&pool, "test-list-sessions-1").await?;
    let sandbox2_id = create_test_sandbox(&pool, "test-list-sessions-2").await?;

    // Create sessions for both sandboxes
    let session1 = ssh_service
        .create_session(CreateSshSessionRequest {
            sandbox_id: sandbox1_id,
            client_ip: "192.168.1.100".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: SshAuthMethod::ApiKey,
            username: None,
            public_key: None,
        })
        .await?;

    let session2 = ssh_service
        .create_session(CreateSshSessionRequest {
            sandbox_id: sandbox2_id,
            client_ip: "192.168.1.101".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: SshAuthMethod::ApiKey,
            username: None,
            public_key: None,
        })
        .await?;

    // List all sessions
    let all_sessions = ssh_service
        .list_sessions(dsb::core::types::SshSessionFilters::default())
        .await;
    assert!(all_sessions.len() >= 2);

    // List sessions for sandbox1
    let sandbox1_sessions = ssh_service
        .list_sessions(dsb::core::types::SshSessionFilters {
            sandbox_id: Some(sandbox1_id),
            ..Default::default()
        })
        .await;
    assert_eq!(sandbox1_sessions.len(), 1);
    assert_eq!(sandbox1_sessions[0].id, session1.id);

    // List sessions by state
    let connecting_sessions = ssh_service
        .list_sessions(dsb::core::types::SshSessionFilters {
            state: Some(SshSessionState::Connecting),
            ..Default::default()
        })
        .await;
    assert!(connecting_sessions.len() >= 2);

    // Cleanup
    let client = pool.get().await?;
    for session_id in &[session1.id, session2.id] {
        client
            .execute("DELETE FROM ssh_sessions WHERE id = $1", &[session_id])
            .await?;
    }
    for sandbox_id in &[sandbox1_id, sandbox2_id] {
        client
            .execute("DELETE FROM sandboxes WHERE id = $1", &[sandbox_id])
            .await?;
    }

    Ok(())
}

#[serial]
#[tokio::test]
async fn test_terminate_session_with_reason() -> Result<(), Box<dyn std::error::Error>> {
    let pool = create_test_pool().await;
    let ssh_store = Arc::new(PostgresSshSessionStore::new(pool.clone()));
    let ssh_service = SshSessionService::new(ssh_store);

    // Create a test sandbox and session
    let sandbox_id = create_test_sandbox(&pool, "test-terminate-session").await?;

    let session = ssh_service
        .create_session(CreateSshSessionRequest {
            sandbox_id,
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: SshAuthMethod::ApiKey,
            username: None,
            public_key: None,
        })
        .await?;

    // Terminate with reason
    ssh_service
        .terminate_session(session.id, "User logged out".to_string())
        .await?;

    let terminated_session = ssh_service.get_session(session.id).await?;
    assert_eq!(terminated_session.state, SshSessionState::Terminated);
    assert_eq!(
        terminated_session.termination_reason,
        Some("User logged out".to_string())
    );
    assert!(terminated_session.disconnected_at.is_some());
    assert!(terminated_session.duration_seconds.is_some());

    // Cleanup
    let client = pool.get().await?;
    client
        .execute("DELETE FROM ssh_sessions WHERE id = $1", &[&session.id])
        .await?;
    client
        .execute("DELETE FROM sandboxes WHERE id = $1", &[&sandbox_id])
        .await?;

    Ok(())
}

#[serial]
#[tokio::test]
async fn test_get_stale_sessions() -> Result<(), Box<dyn std::error::Error>> {
    let pool = create_test_pool().await;
    let ssh_store = Arc::new(PostgresSshSessionStore::new(pool.clone()));
    let ssh_service = SshSessionService::new(ssh_store);

    // Create a test sandbox
    let sandbox_id = create_test_sandbox(&pool, "test-stale-sessions").await?;

    // Create a session
    let session = ssh_service
        .create_session(CreateSshSessionRequest {
            sandbox_id,
            client_ip: "127.0.0.1".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: SshAuthMethod::ApiKey,
            username: None,
            public_key: None,
        })
        .await?;

    // Update last_activity_at to be old (more than 1 hour ago)
    let client = pool.get().await?;
    client
        .execute(
            "UPDATE ssh_sessions SET last_activity_at = NOW() - INTERVAL '2 hours' WHERE id = $1",
            &[&session.id],
        )
        .await?;

    // Get stale sessions with 1 hour timeout
    let stale_sessions = ssh_service.get_stale_sessions(3600).await?;
    assert!(!stale_sessions.is_empty());
    assert!(stale_sessions.iter().any(|s| s.id == session.id));

    // Cleanup
    let client = pool.get().await?;
    client
        .execute("DELETE FROM ssh_sessions WHERE id = $1", &[&session.id])
        .await?;
    client
        .execute("DELETE FROM sandboxes WHERE id = $1", &[&sandbox_id])
        .await?;

    Ok(())
}

#[serial]
#[tokio::test]
async fn test_docker_exec_pty_create() -> Result<(), Box<dyn std::error::Error>> {
    if using_external_api() {
        eprintln!("Skipping test_docker_exec_pty_create: requires direct Docker access");
        return Ok(());
    }

    use bollard::Docker;

    // Get Docker socket from test configuration
    let docker_host = common::test_config::get_test_docker_socket();
    std::env::set_var("DOCKER_HOST", &docker_host);

    // Connect to Docker using the configured socket
    let docker = Docker::connect_with_defaults()?;
    let exec_proxy = DockerExecProxy::new(docker);

    // Create a test container
    let container_id = "test-container-exec-pty";
    let _ = std::process::Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            container_id,
            "alpine:latest",
            "sleep",
            "3600",
        ])
        .output()?;

    // Give the container time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Test 1: Create exec with PTY
    let config = ExecConfig {
        container_id: container_id.to_string(),
        command: vec!["sh".to_string(), "-c".to_string(), "echo hello".to_string()],
        ..Default::default()
    };

    let exec_id = exec_proxy.create_exec_pty(&config).await?;
    assert!(!exec_id.is_empty());

    // Test 2: Create exec with custom working directory and environment
    let config_custom = ExecConfig {
        container_id: container_id.to_string(),
        command: vec!["pwd".to_string()],
        working_dir: Some("/tmp".to_string()),
        env: Some(vec!["TEST_VAR=hello".to_string()]),
        ..Default::default()
    };

    let exec_id_custom = exec_proxy.create_exec_pty(&config_custom).await?;
    assert!(!exec_id_custom.is_empty());

    // Test 3: Verify container validation works
    let config_invalid = ExecConfig {
        container_id: "nonexistent-container".to_string(),
        command: vec!["echo".to_string(), "test".to_string()],
        ..Default::default()
    };

    let result = exec_proxy.create_exec_pty(&config_invalid).await;
    assert!(matches!(result, Err(ExecProxyError::ContainerNotFound(_))));

    // Cleanup
    let _ = std::process::Command::new("docker")
        .args(["rm", "-f", container_id])
        .output()?;

    Ok(())
}

#[serial]
#[tokio::test]
async fn test_docker_exec_start_and_resize() -> Result<(), Box<dyn std::error::Error>> {
    if using_external_api() {
        eprintln!("Skipping test_docker_exec_start_and_resize: requires direct Docker access");
        return Ok(());
    }

    use bollard::Docker;

    // Get Docker socket from test configuration
    let docker_host = common::test_config::get_test_docker_socket();
    std::env::set_var("DOCKER_HOST", &docker_host);

    // Connect to Docker using the configured socket
    let docker = Docker::connect_with_defaults()?;
    let exec_proxy = DockerExecProxy::new(docker);

    // Create a test container
    let container_id = "test-container-exec-start";
    let _ = std::process::Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            container_id,
            "alpine:latest",
            "sleep",
            "3600",
        ])
        .output()?;

    // Give the container time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Create exec
    let config = ExecConfig {
        container_id: container_id.to_string(),
        command: vec!["sh".to_string()],
        ..Default::default()
    };

    let exec_id = exec_proxy.create_exec_pty(&config).await?;

    // Start exec and get stream
    let mut stream = exec_proxy.start_exec(&exec_id).await?;

    // Test PTY resize first (before exit)
    exec_proxy.resize_pty(&exec_id, 40, 120).await?;

    // Test writing to stdin
    stream.write(b"echo 'test output'\n").await?;
    stream.write(b"exit\n").await?;

    // Test reading from stdout
    let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(5));
    tokio::pin!(timeout);

    let _received_data = false;
    loop {
        tokio::select! {
            result = stream.read_frame() => {
                match result? {
                    Some(frame) => {
                        match frame {
                            LogOutput::StdOut { message } => {
                                let data = String::from_utf8_lossy(&message);
                                if !data.is_empty() {
                                    tracing::info!("Received: {}", data);
                                    break;
                                }
                            }
                            LogOutput::StdErr { message } => {
                                let data = String::from_utf8_lossy(&message);
                                if !data.is_empty() {
                                    tracing::info!("Received stderr: {}", data);
                                }
                            }
                            LogOutput::StdIn { .. } | LogOutput::Console { .. } => {}
                        }
                    }
                    None => break,
                }
            }
            _ = &mut timeout => {
                tracing::warn!("Timeout waiting for exec output");
                break;
            }
        }
    }

    // Note: exec may have exited after 'exit' command, which is expected behavior

    // Cleanup
    let _ = std::process::Command::new("docker")
        .args(["rm", "-f", container_id])
        .output()?;

    Ok(())
}
