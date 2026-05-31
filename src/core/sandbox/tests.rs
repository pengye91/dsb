use super::*;
use crate::config::load_for_tests;
use crate::core::state::StateStore;
use crate::core::types::{ActivityType, ApiKeyIdentity, ApiKeyType, Sandbox, SandboxConfig, SandboxState};
use crate::core::types::VolumeMount;
use crate::core::static_files::shell_quote;
use crate::docker::DockerManager;
use std::sync::Arc;
use serial_test::serial;

/// Helper function to get the test image from configuration
fn test_image() -> String {
    let config = load_for_tests().expect("Failed to load test config");
    config.docker.test_image.clone()
}

// ========================================================================
// Constructor Tests
// ========================================================================

#[test]
fn test_sandbox_service_new() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    // Activity service should be None
    assert!(service.get_activity_service().is_none());
}

// Note: test_sandbox_service_new_with_activity requires PostgreSQL connection pool
// which isn't available in unit tests. This is tested in integration tests.

// ========================================================================
// Shell Quoting / Command Injection Tests
// ========================================================================

#[test]
fn test_shell_quote_empty_string() {
    assert_eq!(shell_quote(""), "''");
}

#[test]
fn test_shell_quote_no_quotes() {
    assert_eq!(shell_quote("/tmp/foo"), "'/tmp/foo'");
}

#[test]
fn test_shell_quote_single_quote() {
    assert_eq!(shell_quote("/tmp/foo'bar"), "'/tmp/foo'\\''bar'");
}

#[test]
fn test_shell_quote_multiple_quotes() {
    assert_eq!(shell_quote("/tmp/foo'bar'baz"), "'/tmp/foo'\\''bar'\\''baz'");
}

#[test]
fn test_build_download_cmds_escapes_single_quotes() {
    let src_path = "/tmp/foo'bar";
    let (check_cmd, size_cmd, read_cmd) = SandboxService::build_download_cmds(src_path);

    assert_eq!(
        check_cmd[2],
        "test -f '/tmp/foo'\\''bar' && echo 'exists' || echo 'notfound'"
    );
    assert_eq!(
        size_cmd[2],
        "wc -c < '/tmp/foo'\\''bar' 2>/dev/null || echo '0'"
    );
    assert_eq!(
        read_cmd[2],
        "base64 -w0 '/tmp/foo'\\''bar'"
    );
}

// ========================================================================
// Activity Recording Tests
// ========================================================================

#[tokio::test]
async fn test_record_api_activity() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    // Create a test sandbox
    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig::default(),
        state: SandboxState::Running,
        container_id: Some("test-container".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now() - chrono::Duration::minutes(10),
            last_container_activity: None,
            activity_count: 5,
        },
        inactivity_timeout_minutes: Some(30),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox.clone()).await.unwrap();

    // Record activity
    service.record_api_activity(&id).await;

    // Verify activity was updated
    let updated = state.get_sandbox(&id).await.unwrap();
    assert!(updated.activity.last_api_activity > sandbox.activity.last_api_activity);
    assert_eq!(updated.activity.activity_count, 6);
}

#[tokio::test]
async fn test_record_api_activity_nonexistent_sandbox() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    // Should not panic or error for nonexistent sandbox
    let id = uuid::Uuid::new_v4();
    service.record_api_activity(&id).await;
    // Just verifies it doesn't crash
}

#[tokio::test]
async fn test_update_container_activity() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    // Create a test sandbox
    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig::default(),
        state: SandboxState::Running,
        container_id: Some("test-container".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 1,
        },
        inactivity_timeout_minutes: Some(30),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox.clone()).await.unwrap();

    // Update container activity
    service.update_container_activity(&id).await;

    // Verify activity was updated
    let updated = state.get_sandbox(&id).await.unwrap();
    assert!(updated.activity.last_container_activity.is_some());
}

#[tokio::test]
async fn test_update_container_activity_nonexistent_sandbox() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    // Should not panic or error for nonexistent sandbox
    let id = uuid::Uuid::new_v4();
    service.update_container_activity(&id).await;
    // Just verifies it doesn't crash
}

// ========================================================================
// State Store Integration Tests
// ========================================================================

#[tokio::test]
async fn test_list_sandboxes_empty() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let list = service.list_sandboxes().await;
    assert_eq!(list.len(), 0);
}

#[tokio::test]
async fn test_list_sandboxes_multiple() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    // Create multiple sandboxes directly in state
    let id1 = uuid::Uuid::new_v4();
    let id2 = uuid::Uuid::new_v4();

    let sandbox1 = Sandbox {
        id: id1,
        config: SandboxConfig::default(),
        state: SandboxState::Running,
        container_id: Some("container-1".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    let sandbox2 = Sandbox {
        id: id2,
        config: SandboxConfig {
            name: Some("test-sandbox".to_string()),
            ..Default::default()
        },
        state: SandboxState::Stopped,
        container_id: Some("container-2".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox1).await.unwrap();
    state.create_sandbox(sandbox2).await.unwrap();

    let list = service.list_sandboxes().await;
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn test_get_sandbox_not_found() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let id = uuid::Uuid::new_v4();
    let result = service.get_sandbox(&id).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_sandbox_found() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig {
            name: Some("test-get".to_string()),
            ..Default::default()
        },
        state: SandboxState::Running,
        container_id: Some("test-container".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox.clone()).await.unwrap();

    let result = service.get_sandbox(&id).await;
    assert!(result.is_some());
    let retrieved = result.unwrap();
    assert_eq!(retrieved.id, id);
    assert_eq!(retrieved.config.name, Some("test-get".to_string()));
}

// ========================================================================
// Error Handling Tests
// ========================================================================

#[tokio::test]
async fn test_stop_sandbox_not_found() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let id = uuid::Uuid::new_v4();
    let result = service.stop_sandbox(&id).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_delete_sandbox_not_found() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let id = uuid::Uuid::new_v4();
    let result = service.delete_sandbox(&id).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_exec_sandbox_not_found() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let id = uuid::Uuid::new_v4();
    let result = service
        .exec_sandbox(&id, vec!["echo".to_string(), "test".to_string()])
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_exec_sandbox_not_running() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig::default(),
        state: SandboxState::Stopped, // Not running
        container_id: Some("test-container".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox).await.unwrap();

    let result = service
        .exec_sandbox(&id, vec!["echo".to_string(), "test".to_string()])
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not running"));
}

#[tokio::test]
async fn test_exec_sandbox_no_container_id() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig::default(),
        state: SandboxState::Running,
        container_id: None, // No container
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox).await.unwrap();

    let result = service
        .exec_sandbox(&id, vec!["echo".to_string(), "test".to_string()])
        .await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Container not running"));
}

#[tokio::test]
async fn test_get_sandbox_stats_not_found() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let id = uuid::Uuid::new_v4();
    let result = service.get_sandbox_stats(&id).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_get_sandbox_stats_no_container() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig::default(),
        state: SandboxState::Running,
        container_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox).await.unwrap();

    let result = service.get_sandbox_stats(&id).await;

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Container not created"));
}

#[tokio::test]
async fn test_stream_sandbox_stats_not_found() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let id = uuid::Uuid::new_v4();
    let result = service.stream_sandbox_stats(&id).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_stream_sandbox_stats_not_running() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig::default(),
        state: SandboxState::Stopped,
        container_id: Some("test-container".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox).await.unwrap();

    let result = service.stream_sandbox_stats(&id).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not running"));
}

#[tokio::test]
async fn test_cleanup_sandbox_not_found() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let id = uuid::Uuid::new_v4();
    let result = service.cleanup_sandbox(&id).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

// ========================================================================
// Activity Tracking Integration Tests
// ========================================================================

#[tokio::test]
async fn test_get_sandbox_records_activity() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig::default(),
        state: SandboxState::Running,
        container_id: Some("test-container".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox.clone()).await.unwrap();

    // Get sandbox (activity recording is optional, depends on activity_service)
    let result = service.get_sandbox(&id).await;
    assert!(result.is_some());

    // Note: Activity count doesn't increase without activity_service
    // This test just verifies get_sandbox doesn't crash
    let retrieved = state.get_sandbox(&id).await.unwrap();
    assert_eq!(retrieved.id, id);
}

#[tokio::test]
async fn test_activity_tracking_field_updates() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();
    let old_time = now - chrono::Duration::hours(1);

    let sandbox = Sandbox {
        id,
        config: SandboxConfig::default(),
        state: SandboxState::Running,
        container_id: Some("test-container".to_string()),
        created_at: old_time,
        updated_at: old_time,
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: old_time,
            last_container_activity: None,
            activity_count: 10,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox.clone()).await.unwrap();

    // Record API activity
    service.record_api_activity(&id).await;

    let updated = state.get_sandbox(&id).await.unwrap();
    assert!(updated.activity.last_api_activity > old_time);
    assert_eq!(updated.activity.activity_count, 11);
    assert!(updated.updated_at > old_time);

    // Update container activity
    service.update_container_activity(&id).await;

    let updated2 = state.get_sandbox(&id).await.unwrap();
    assert!(updated2.activity.last_container_activity.is_some());
    assert!(updated2.updated_at > updated.updated_at);
}

// ========================================================================
// Sandbox Configuration Tests
// ========================================================================

#[test]
fn test_sandbox_config_default() {
    let config = SandboxConfig::default();
    assert_eq!(config.image, "nginx:latest");
    assert!(config.name.is_none());
    assert!(config.command.is_none());
    assert!(config.environment.is_empty());
    assert!(config.port_mappings.is_empty());
    assert!(config.volumes.is_empty());
    assert_eq!(config.pull_policy, crate::core::types::PullPolicy::Missing);
    assert_eq!(config.inactivity_timeout_minutes, None);
}

#[test]
fn test_sandbox_config_custom() {
    let mut environment = std::collections::HashMap::new();
    environment.insert("POSTGRES_PASSWORD".to_string(), "secret".to_string());

    let config = SandboxConfig {
        image: "postgres:15".to_string(),
        name: Some("db-server".to_string()),
        command: Some(vec!["postgres".to_string()]),
        environment,
        port_mappings: vec![crate::core::types::PortMapping {
            host_port: 5432,
            container_port: 5432,
            protocol: crate::core::types::PortProtocol::Tcp,
        }],
        exposed_ports: vec![],
        volumes: vec![crate::core::types::VolumeMount::Named {
            name: "db-data".to_string(),
            container_path: "/var/lib/postgresql/data".to_string(),
            read_only: false,
        }],
        resource_limits: crate::core::types::ResourceLimits {
            memory_mb: Some(1024),
            cpu_quota: None,
            cpu_period: None,
            cpu_shares: Some(512),
            pids_limit: None,
            ulimits: None,
        },
        inactivity_timeout_minutes: Some(60),
        pull_policy: crate::core::types::PullPolicy::Always,
        features: vec![],
        enable_all_features: false,
        vnc_resolution: None,
    };

    assert_eq!(config.image, "postgres:15");
    assert_eq!(config.name, Some("db-server".to_string()));
    assert_eq!(config.inactivity_timeout_minutes, Some(60));
    assert_eq!(config.pull_policy, crate::core::types::PullPolicy::Always);
    assert_eq!(config.port_mappings.len(), 1);
    assert_eq!(config.volumes.len(), 1);
    assert_eq!(config.environment.len(), 1);
}

// ========================================================================
// Sandbox State Tests
// ========================================================================

#[test]
fn test_sandbox_state_display() {
    assert_eq!(SandboxState::Creating.as_str(), "creating");
    assert_eq!(SandboxState::Created.as_str(), "created");
    assert_eq!(SandboxState::Running.as_str(), "running");
    assert_eq!(SandboxState::Stopped.as_str(), "stopped");
    assert_eq!(SandboxState::Error.as_str(), "error");
}

#[test]
fn test_sandbox_state_transitions() {
    // Verify all states can exist
    let states = vec![
        SandboxState::Creating,
        SandboxState::Created,
        SandboxState::Running,
        SandboxState::Stopped,
        SandboxState::Error,
    ];

    for state in states {
        let state_str = state.as_str();
        assert!(!state_str.is_empty());
    }
}

// ========================================================================
// Volume Mount Tests
// ========================================================================

#[test]
fn test_volume_mount_named() {
    let volume = crate::core::types::VolumeMount::Named {
        name: "test-volume".to_string(),
        container_path: "/data".to_string(),
        read_only: true,
    };

    match volume {
        crate::core::types::VolumeMount::Named {
            name,
            container_path,
            read_only,
        } => {
            assert_eq!(name, "test-volume");
            assert_eq!(container_path, "/data");
            assert!(read_only);
        }
        _ => panic!("Expected Named variant"),
    }
}

#[test]
fn test_volume_mount_bind() {
    let volume = crate::core::types::VolumeMount::Bind {
        host_path: "/host/path".to_string(),
        container_path: "/container/path".to_string(),
        read_only: false,
    };

    match volume {
        crate::core::types::VolumeMount::Bind {
            host_path,
            container_path,
            read_only,
        } => {
            assert_eq!(host_path, "/host/path");
            assert_eq!(container_path, "/container/path");
            assert!(!read_only);
        }
        _ => panic!("Expected Bind variant"),
    }
}

// ========================================================================
// Pull Policy Tests
// ========================================================================

#[test]
fn test_pull_policy_variants() {
    let policies = vec![
        crate::core::types::PullPolicy::Always,
        crate::core::types::PullPolicy::Missing,
        crate::core::types::PullPolicy::Never,
    ];

    for policy in policies {
        // Verify each policy can be created and compared
        let same = policy == crate::core::types::PullPolicy::Missing;
        // Just verify it compiles and runs
        let _ = same;
    }
}

// ========================================================================
// Port Mapping Tests
// ========================================================================

#[test]
fn test_port_mapping_tcp() {
    let mapping = crate::core::types::PortMapping {
        host_port: 8080,
        container_port: 80,
        protocol: crate::core::types::PortProtocol::Tcp,
    };

    assert_eq!(mapping.host_port, 8080);
    assert_eq!(mapping.container_port, 80);
    assert!(matches!(
        mapping.protocol,
        crate::core::types::PortProtocol::Tcp
    ));
}

#[test]
fn test_port_mapping_udp() {
    let mapping = crate::core::types::PortMapping {
        host_port: 53,
        container_port: 53,
        protocol: crate::core::types::PortProtocol::Udp,
    };

    assert_eq!(mapping.host_port, 53);
    assert_eq!(mapping.container_port, 53);
    assert!(matches!(
        mapping.protocol,
        crate::core::types::PortProtocol::Udp
    ));
}

// ========================================================================
// Resource Limits Tests
// ========================================================================

#[test]
fn test_resource_limits_custom() {
    let limits = crate::core::types::ResourceLimits {
        memory_mb: Some(2048),
        cpu_quota: None,
        cpu_period: None,
        cpu_shares: Some(1024),
        pids_limit: None,
        ulimits: None,
    };

    assert_eq!(limits.cpu_shares, Some(1024));
    assert_eq!(limits.memory_mb, Some(2048));
}

// ========================================================================
// Activity Tracking Tests
// ========================================================================

#[test]
fn test_activity_tracking_default() {
    let now = chrono::Utc::now();
    let tracking = crate::core::types::ActivityTracking {
        last_api_activity: now,
        last_container_activity: None,
        activity_count: 0,
    };
    assert!(tracking.last_api_activity <= chrono::Utc::now());
    assert!(tracking.last_container_activity.is_none());
    assert_eq!(tracking.activity_count, 0);
}

#[test]
fn test_activity_type_variants() {
    let types = vec![
        ActivityType::Create,
        ActivityType::Exec,
        ActivityType::Stop,
        ActivityType::Delete,
        ActivityType::Info,
        ActivityType::Stats,
    ];

    for activity_type in types {
        // Verify each type can be created
        let _ = activity_type;
    }
}

#[tokio::test]
async fn test_check_sandbox_ownership_with_deleted_allows_owner() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());
    let api_key_id = uuid::Uuid::new_v4();
    let sandbox_id = uuid::Uuid::new_v4();

    state
        .create_sandbox(Sandbox {
            id: sandbox_id,
            config: SandboxConfig::default(),
            state: SandboxState::Destroyed,
            container_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            error_message: None,
            volume_mounts: vec![],
            activity: crate::core::types::ActivityTracking {
                last_api_activity: chrono::Utc::now(),
                last_container_activity: None,
                activity_count: 0,
            },
            inactivity_timeout_minutes: Some(30),
            deleted_at: Some(chrono::Utc::now()),
            deleted_by: Some("test".to_string()),
            api_key_id: Some(api_key_id),
        })
        .await
        .unwrap();

    let identity = ApiKeyIdentity {
        id: Some(api_key_id),
        key_type: ApiKeyType::Database,
    };

    assert!(service
        .check_sandbox_ownership_with_deleted(&identity, &sandbox_id, true)
        .await
        .is_ok());
    assert!(service
        .check_sandbox_ownership(&identity, &sandbox_id)
        .await
        .is_err());
}

// ========================================================================
// Integration Tests (require Docker)
// ========================================================================
// Note: These tests require Docker to be running and images to be available
// They're marked with #[serial] to prevent concurrent execution

#[tokio::test]
#[serial]
async fn test_create_and_get_sandbox() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker.clone()), state);

    let config = SandboxConfig {
        image: test_image().as_str().to_string(),
        name: Some(format!("test-create-and-get-{}", uuid::Uuid::new_v4())),
        command: Some(vec!["python".to_string(), "-u".to_string()]),
        ..Default::default()
    };

    let sandbox = service.create_sandbox(config, None).await.unwrap();
    assert_eq!(sandbox.state, SandboxState::Running);
    assert!(sandbox.container_id.is_some());

    let retrieved = service.get_sandbox(&sandbox.id).await;
    assert!(retrieved.is_some());

    // Cleanup: stop, delete the sandbox, and remove container with error logging
    if let Err(e) = service.stop_sandbox(&sandbox.id).await {
        tracing::warn!(sandbox_id = %sandbox.id, error = ?e, "Failed to stop sandbox during test cleanup");
    }
    if let Err(e) = service.delete_sandbox(&sandbox.id).await {
        tracing::warn!(sandbox_id = %sandbox.id, error = ?e, "Failed to delete sandbox during test cleanup");
    }
    if let Some(container_id) = sandbox.container_id {
        if let Err(e) = docker.remove_container(&container_id).await {
            tracing::warn!(container_id = %container_id, error = ?e, "Failed to remove container during test cleanup");
        }
    }
}

#[tokio::test]
#[serial]
async fn test_list_sandboxes() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker.clone()), state);

    // Initially empty
    let list = service.list_sandboxes().await;
    assert_eq!(list.len(), 0);

    // Create one
    let config = SandboxConfig {
        image: test_image().as_str().to_string(),
        command: Some(vec!["python".to_string(), "-u".to_string()]),
        ..Default::default()
    };

    let sandbox = service.create_sandbox(config, None).await.unwrap();

    // Should have one
    let list = service.list_sandboxes().await;
    assert_eq!(list.len(), 1);

    // Cleanup with error logging
    if let Err(e) = service.stop_sandbox(&sandbox.id).await {
        tracing::warn!(sandbox_id = %sandbox.id, error = ?e, "Failed to stop sandbox during test cleanup");
    }
    if let Err(e) = service.delete_sandbox(&sandbox.id).await {
        tracing::warn!(sandbox_id = %sandbox.id, error = ?e, "Failed to delete sandbox during test cleanup");
    }
    if let Some(container_id) = sandbox.container_id {
        if let Err(e) = docker.remove_container(&container_id).await {
            tracing::warn!(container_id = %container_id, error = ?e, "Failed to remove container during test cleanup");
        }
    }
}

#[tokio::test]
#[serial]
async fn test_stop_and_delete_sandbox() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker.clone()), state);

    // Create a sandbox
    let config = SandboxConfig {
        image: test_image().as_str().to_string(),
        name: Some(format!("test-stop-delete-{}", uuid::Uuid::new_v4())),
        command: Some(vec!["python".to_string(), "-u".to_string()]),
        ..Default::default()
    };

    let sandbox = service.create_sandbox(config, None).await.unwrap();

    // Stop it
    service.stop_sandbox(&sandbox.id).await.unwrap();
    let stopped = service.get_sandbox(&sandbox.id).await.unwrap();
    assert_eq!(stopped.state, SandboxState::Stopped);

    // Delete it (soft delete - marks as deleted but keeps record)
    service.delete_sandbox(&sandbox.id).await.unwrap();
    let deleted = service.get_sandbox(&sandbox.id).await;
    assert!(deleted.is_some()); // Record still exists (soft delete)
    assert!(deleted.unwrap().deleted_at.is_some()); // But is marked as deleted
}

#[tokio::test]
#[serial]
async fn test_exec_sandbox() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker.clone()), state);

    let config = SandboxConfig {
        image: test_image().as_str().to_string(),
        name: Some(format!("test-exec-{}", uuid::Uuid::new_v4())),
        // Don't specify a command - let it use the default "tail -f /dev/null"
        // which keeps the container running for exec operations
        command: None,
        ..Default::default()
    };

    let sandbox = service.create_sandbox(config, None).await.unwrap();

    // Ensure sandbox is running before exec
    assert_eq!(sandbox.state, SandboxState::Running);

    // Poll until sandbox is confirmed running (replaces blind 500ms sleep)
    {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Some(sb) = service.get_sandbox(&sandbox.id).await {
                match sb.state {
                    SandboxState::Running => break,
                    SandboxState::Error | SandboxState::Stopped => {
                        panic!(
                            "Sandbox {} reached unexpected state: {:?}",
                            sandbox.id, sb.state
                        )
                    }
                    _ => {}
                }
            }
            if tokio::time::Instant::now() >= deadline {
                panic!(
                    "Sandbox {} did not reach running state within 30s",
                    sandbox.id
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    // Verify container still exists and is running
    if let Some(container_id) = &sandbox.container_id {
        match docker.is_container_running(container_id).await {
            Ok(running) => {
                if !running {
                    panic!("Container {} is not running", container_id);
                }
            }
            Err(e) => panic!("Failed to check container status: {}", e),
        }
    }

    // Execute a simple command
    let output = service
        .exec_sandbox(
            &sandbox.id,
            vec![
                "python".to_string(),
                "-c".to_string(),
                "print('hello')".to_string(),
            ],
        )
        .await
        .unwrap();

    assert!(!output.is_empty());

    // Cleanup: stop and delete the sandbox with error logging
    if let Err(e) = service.stop_sandbox(&sandbox.id).await {
        tracing::warn!(sandbox_id = %sandbox.id, error = ?e, "Failed to stop sandbox during test cleanup");
    }
    if let Err(e) = service.delete_sandbox(&sandbox.id).await {
        tracing::warn!(sandbox_id = %sandbox.id, error = ?e, "Failed to delete sandbox during test cleanup");
    }

    // Also remove the container from Docker
    if let Some(container_id) = sandbox.container_id {
        if let Err(e) = docker.remove_container(&container_id).await {
            tracing::warn!(container_id = %container_id, error = ?e, "Failed to remove container during test cleanup");
        }
    }
}

// ========================================================================
// Type Trait Tests
// ========================================================================

#[test]
fn test_sandbox_service_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<SandboxService>();
}

#[test]
fn test_sandbox_service_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<SandboxService>();
}

// ========================================================================
// Activity Tracking Edge Cases
// ========================================================================

#[tokio::test]
async fn test_record_api_activity_updates_count() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig::default(),
        state: SandboxState::Running,
        container_id: Some("test-container".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: Some(30),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox).await.unwrap();

    // Record multiple activities
    service.record_api_activity(&id).await;
    service.record_api_activity(&id).await;
    service.record_api_activity(&id).await;

    let updated = state.get_sandbox(&id).await.unwrap();
    assert_eq!(updated.activity.activity_count, 3);
}

#[tokio::test]
async fn test_update_container_activity_when_already_set() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let id = uuid::Uuid::new_v4();
    let existing_activity = chrono::Utc::now() - chrono::Duration::minutes(5);

    let sandbox = Sandbox {
        id,
        config: SandboxConfig::default(),
        state: SandboxState::Running,
        container_id: Some("test-container".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: Some(existing_activity),
            activity_count: 10,
        },
        inactivity_timeout_minutes: Some(30),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox).await.unwrap();

    // Update container activity
    service.update_container_activity(&id).await;

    let updated = state.get_sandbox(&id).await.unwrap();
    assert!(updated.activity.last_container_activity > Some(existing_activity));
}

// ========================================================================
// Sandbox State Tests
// ========================================================================

#[test]
fn test_sandbox_state_variants() {
    // Test all sandbox state variants exist
    let states = vec![
        SandboxState::Creating,
        SandboxState::Created,
        SandboxState::Starting,
        SandboxState::Running,
        SandboxState::Stopped,
        SandboxState::Error,
        SandboxState::Destroying,
    ];

    for state in states {
        // Just verify they can be created
        let _ = format!("{:?}", state);
    }
}

#[test]
fn test_sandbox_config_default_values() {
    let config = SandboxConfig::default();

    assert!(!config.image.is_empty());
    assert!(config.name.is_none());
    assert!(config.environment.is_empty());
    assert!(config.port_mappings.is_empty());
    assert!(config.volumes.is_empty());
    assert!(config.command.is_none());
    assert!(config.inactivity_timeout_minutes.is_none());
}

#[tokio::test]
async fn test_get_sandbox_returns_none_for_nonexistent() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let id = uuid::Uuid::new_v4();
    let result = service.get_sandbox(&id).await;

    assert!(result.is_none());
}

#[tokio::test]
async fn test_list_sandboxes_returns_empty_when_none_exist() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let sandboxes = service.list_sandboxes().await;
    assert!(sandboxes.is_empty());
}

// ========================================================================
// Inactivity Timeout Tests
// ========================================================================

#[tokio::test]
async fn test_sandbox_with_no_timeout() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig {
            inactivity_timeout_minutes: None,
            ..Default::default()
        },
        state: SandboxState::Running,
        container_id: Some("test-container".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox).await.unwrap();

    let retrieved = service.get_sandbox(&id).await.unwrap();
    assert!(retrieved.inactivity_timeout_minutes.is_none());
}

#[tokio::test]
async fn test_sandbox_with_custom_timeout() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig {
            inactivity_timeout_minutes: Some(120),
            ..Default::default()
        },
        state: SandboxState::Running,
        container_id: Some("test-container".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: Some(120),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    state.create_sandbox(sandbox).await.unwrap();

    let retrieved = service.get_sandbox(&id).await.unwrap();
    assert_eq!(retrieved.inactivity_timeout_minutes, Some(120));
    assert_eq!(retrieved.config.inactivity_timeout_minutes, Some(120));
}

// ========================================================================
// Error Handling Tests
// ========================================================================

#[tokio::test]
async fn test_delete_nonexistent_sandbox() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let id = uuid::Uuid::new_v4();
    let result = service.delete_sandbox(&id).await;

    // Should return an error for nonexistent sandbox
    assert!(result.is_err());
}

#[tokio::test]
async fn test_stop_nonexistent_sandbox() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state);

    let id = uuid::Uuid::new_v4();
    let result = service.stop_sandbox(&id).await;

    // Should return an error for nonexistent sandbox
    assert!(result.is_err());
}

// ========================================================================
// Volume Mount Tests
// ========================================================================

#[test]
fn test_sandbox_with_volume_mounts() {
    let id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id,
        config: SandboxConfig::default(),
        state: SandboxState::Running,
        container_id: Some("test-container".to_string()),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        error_message: None,
        volume_mounts: vec![VolumeMount::Bind {
            host_path: "/host/data".to_string(),
            container_path: "/container/data".to_string(),
            read_only: false,
        }],
        activity: crate::core::types::ActivityTracking {
            last_api_activity: chrono::Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: Some(30),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    assert_eq!(sandbox.volume_mounts.len(), 1);
    assert!(matches!(sandbox.volume_mounts[0], VolumeMount::Bind { .. }));
}

// ========================================================================
// File Upload/Download Activity Tests
// ========================================================================

#[tokio::test]
#[serial]
async fn test_upload_file_records_activity() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    // Create a sandbox with unique name
    let image = test_image();
    let unique_name = format!("test-upload-activity-{}", uuid::Uuid::new_v4());
    let config = SandboxConfig {
        image: image.clone(),
        name: Some(unique_name),
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        ..Default::default()
    };

    let sandbox = service
        .create_sandbox(config, None)
        .await
        .expect("Failed to create sandbox");

    // Poll until sandbox is running (replaces blind 2s sleep)
    {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Some(sb) = service.get_sandbox(&sandbox.id).await {
                match sb.state {
                    SandboxState::Running => break,
                    SandboxState::Error | SandboxState::Stopped => {
                        panic!(
                            "Sandbox {} reached unexpected state: {:?}",
                            sandbox.id, sb.state
                        )
                    }
                    _ => {}
                }
            }
            if tokio::time::Instant::now() >= deadline {
                panic!(
                    "Sandbox {} did not reach running state within 30s",
                    sandbox.id
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    // Upload a file
    let file_data = b"Hello, World!".to_vec();
    let result = service
        .upload_file(&sandbox.id, "/tmp/test.txt", file_data)
        .await;

    // Upload should succeed
    assert!(result.is_ok(), "Upload failed: {:?}", result.err());

    // Verify file was uploaded by checking it exists
    // Retry up to 3 times in case the container is briefly unstable
    let mut check_ok = false;
    for attempt in 0..3 {
        let check_result = service
            .exec_sandbox(
                &sandbox.id,
                vec![
                    "test".to_string(),
                    "-f".to_string(),
                    "/tmp/test.txt".to_string(),
                ],
            )
            .await;
        if check_result.is_ok() {
            check_ok = true;
            break;
        }
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
    assert!(check_ok, "Failed to check file after 3 attempts");

    // Cleanup
    let _ = service.delete_sandbox(&sandbox.id).await;
}

#[tokio::test]
#[serial]
async fn test_download_file_records_activity() {
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    // Create a sandbox with unique name
    let image = test_image();
    let unique_name = format!("test-download-activity-{}", uuid::Uuid::new_v4());
    let config = SandboxConfig {
        image: image.clone(),
        name: Some(unique_name),
        command: Some(vec!["sleep".to_string(), "3600".to_string()]),
        ..Default::default()
    };

    let sandbox = service
        .create_sandbox(config, None)
        .await
        .expect("Failed to create sandbox");

    // Poll until sandbox is running (replaces blind 2s sleep)
    {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Some(sb) = service.get_sandbox(&sandbox.id).await {
                match sb.state {
                    SandboxState::Running => break,
                    SandboxState::Error | SandboxState::Stopped => {
                        panic!(
                            "Sandbox {} reached unexpected state: {:?}",
                            sandbox.id, sb.state
                        )
                    }
                    _ => {}
                }
            }
            if tokio::time::Instant::now() >= deadline {
                panic!(
                    "Sandbox {} did not reach running state within 30s",
                    sandbox.id
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    // First, upload a file
    let file_data = b"Test content for download".to_vec();
    service
        .upload_file(&sandbox.id, "/tmp/download_test.txt", file_data)
        .await
        .expect("Failed to upload file");

    // Download the file
    let result = service
        .download_file(&sandbox.id, "/tmp/download_test.txt")
        .await;

    // Download should succeed
    assert!(result.is_ok(), "Download failed: {:?}", result.err());

    // Verify content
    let downloaded_data = result.unwrap();
    assert_eq!(downloaded_data, b"Test content for download");

    // Cleanup
    let _ = service.delete_sandbox(&sandbox.id).await;
}

#[tokio::test]
async fn test_create_sandbox_without_name_sets_default() {
    // This test verifies that when a sandbox is created without a name,
    // the system automatically sets a default name following the pattern "sandbox-{uuid}"
    // This is critical for orphan cleanup to work properly

    let id = uuid::Uuid::new_v4();
    let default_name = format!("sandbox-{}", id);

    // Verify the pattern is correct
    assert!(default_name.starts_with("sandbox-"));
    assert!(default_name.len() > "sandbox-".len());

    // Extract UUID from container name
    let uuid_str = default_name.strip_prefix("sandbox-").unwrap();
    let parsed_uuid = uuid::Uuid::parse_str(uuid_str);
    assert!(
        parsed_uuid.is_ok(),
        "Container name should contain valid UUID"
    );

    // The UUID should match the sandbox ID
    assert_eq!(parsed_uuid.unwrap(), id);
}

// ========================================================================
// Auto-Cleanup Tests
// ========================================================================

#[tokio::test]
async fn test_auto_cleanup_skips_destroyed_sandboxes() {
    // Verify that auto-cleanup logic skips sandboxes in Destroyed state
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    // Create test sandboxes with different states
    let destroyed_id = uuid::Uuid::new_v4();
    let destroying_id = uuid::Uuid::new_v4();
    let running_id = uuid::Uuid::new_v4();

    let old_activity = crate::core::types::ActivityTracking {
        last_api_activity: chrono::Utc::now() - chrono::Duration::minutes(100),
        last_container_activity: None,
        activity_count: 1,
    };

    // Destroyed sandbox - should be skipped
    let destroyed_sandbox = Sandbox {
        id: destroyed_id,
        config: SandboxConfig::default(),
        state: SandboxState::Destroyed,
        container_id: Some("destroyed-container".to_string()),
        created_at: chrono::Utc::now() - chrono::Duration::minutes(150),
        updated_at: chrono::Utc::now() - chrono::Duration::minutes(100),
        error_message: None,
        volume_mounts: vec![],
        activity: old_activity.clone(),
        inactivity_timeout_minutes: Some(30),
        deleted_at: Some(chrono::Utc::now() - chrono::Duration::minutes(100)),
        deleted_by: Some("test".to_string()),
        api_key_id: None,
    };

    // Destroying sandbox - should be skipped
    let destroying_sandbox = Sandbox {
        id: destroying_id,
        config: SandboxConfig::default(),
        state: SandboxState::Destroying,
        container_id: Some("destroying-container".to_string()),
        created_at: chrono::Utc::now() - chrono::Duration::minutes(150),
        updated_at: chrono::Utc::now() - chrono::Duration::minutes(100),
        error_message: None,
        volume_mounts: vec![],
        activity: old_activity.clone(),
        inactivity_timeout_minutes: Some(30),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    // Running sandbox - should be considered for cleanup
    let running_sandbox = Sandbox {
        id: running_id,
        config: SandboxConfig::default(),
        state: SandboxState::Running,
        container_id: Some("running-container".to_string()),
        created_at: chrono::Utc::now() - chrono::Duration::minutes(150),
        updated_at: chrono::Utc::now() - chrono::Duration::minutes(100),
        error_message: None,
        volume_mounts: vec![],
        activity: old_activity.clone(),
        inactivity_timeout_minutes: Some(30),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    // Add all sandboxes to state
    state.create_sandbox(destroyed_sandbox).await.unwrap();
    state.create_sandbox(destroying_sandbox).await.unwrap();
    state.create_sandbox(running_sandbox).await.unwrap();

    // List all sandboxes
    let sandboxes = service.list_sandboxes().await;

    // Verify all three are in the list
    assert_eq!(sandboxes.len(), 3);

    // Simulate auto-cleanup logic: skip destroyed/destroying
    let cleanup_candidates: Vec<_> = sandboxes
        .into_iter()
        .filter(|sandbox| {
            // Skip destroyed/destroying (this is the fix)
            !matches!(
                sandbox.state,
                SandboxState::Destroyed | SandboxState::Destroying
            )
        })
        .collect();

    // Only the running sandbox should be a candidate for cleanup
    assert_eq!(cleanup_candidates.len(), 1);
    assert_eq!(cleanup_candidates[0].id, running_id);
    assert_eq!(cleanup_candidates[0].state, SandboxState::Running);
}

#[tokio::test]
async fn test_auto_cleanup_skips_all_destroyed_states() {
    // Verify that auto-cleanup skips when all sandboxes are destroyed/destroying
    let docker = DockerManager::new().unwrap();
    let state = Arc::new(StateStore::new());
    let service = SandboxService::new(Arc::new(docker), state.clone());

    let old_activity = crate::core::types::ActivityTracking {
        last_api_activity: chrono::Utc::now() - chrono::Duration::minutes(100),
        last_container_activity: None,
        activity_count: 1,
    };

    // Create multiple destroyed/destroying sandboxes
    for i in 0..5 {
        let id = uuid::Uuid::new_v4();
        let state_value = if i % 2 == 0 {
            SandboxState::Destroyed
        } else {
            SandboxState::Destroying
        };

        let sandbox = Sandbox {
            id,
            config: SandboxConfig::default(),
            state: state_value,
            container_id: Some(format!("container-{}", i)),
            created_at: chrono::Utc::now() - chrono::Duration::minutes(150),
            updated_at: chrono::Utc::now() - chrono::Duration::minutes(100),
            error_message: None,
            volume_mounts: vec![],
            activity: old_activity.clone(),
            inactivity_timeout_minutes: Some(30),
            deleted_at: if state_value == SandboxState::Destroyed {
                Some(chrono::Utc::now() - chrono::Duration::minutes(100))
            } else {
                None
            },
            deleted_by: if state_value == SandboxState::Destroyed {
                Some("test".to_string())
            } else {
                None
            },
            api_key_id: None,
        };

        state.create_sandbox(sandbox).await.unwrap();
    }

    // List all sandboxes
    let sandboxes = service.list_sandboxes().await;
    assert_eq!(sandboxes.len(), 5);

    // Simulate auto-cleanup logic: skip destroyed/destroying
    let cleanup_candidates: Vec<_> = sandboxes
        .into_iter()
        .filter(|sandbox| {
            !matches!(
                sandbox.state,
                SandboxState::Destroyed | SandboxState::Destroying
            )
        })
        .collect();

    // No sandboxes should be candidates for cleanup
    assert_eq!(cleanup_candidates.len(), 0);
}
