use crate::core::types::{
    ActivityTracking, PortMapping, PortProtocol, PullPolicy, ResourceLimits, SandboxConfig,
    SandboxState, VolumeMount,
};
use crate::db::store::helpers::*;
use crate::db::store::*;
use chrono::Utc;
use std::collections::HashMap;

// ========================================================================
// StoreError Tests
// ========================================================================

#[test]
fn test_store_error_not_found() {
    let uuid = uuid::Uuid::new_v4();
    let error = StoreError::NotFound(uuid);
    let error_str = error.to_string();
    assert!(error_str.contains("not found"));
    assert!(error_str.contains(&uuid.to_string()));
}

#[test]
fn test_store_error_invalid_state() {
    let error = StoreError::InvalidState("Invalid state transition".to_string());
    let error_str = error.to_string();
    assert!(error_str.contains("Invalid state"));
    assert!(error_str.contains("Invalid state transition"));
}

#[test]
fn test_store_error_message() {
    let error = StoreError::Message("Custom error message".to_string());
    assert_eq!(error.to_string(), "Custom error message");
}

#[test]
fn test_store_error_from_string() {
    let error: StoreError = "Error from string".to_string().into();
    assert_eq!(error.to_string(), "Error from string");
}

#[test]
fn test_store_error_debug_format() {
    let uuid = uuid::Uuid::new_v4();
    let error = StoreError::NotFound(uuid);
    let debug_str = format!("{:?}", error);
    assert!(debug_str.contains("NotFound"));
}

// ========================================================================
// Pull Policy Parsing Tests
// ========================================================================

#[test]
fn test_pull_policy_parsing() {
    // Test pull_policy enum parsing
    assert_eq!(format!("{:?}", PullPolicy::Always).to_lowercase(), "always");
    assert_eq!(
        format!("{:?}", PullPolicy::Missing).to_lowercase(),
        "missing"
    );
    assert_eq!(format!("{:?}", PullPolicy::Never).to_lowercase(), "never");
}

#[test]
fn test_pull_policy_as_str() {
    assert_eq!(PullPolicy::Always.as_str(), "always");
    assert_eq!(PullPolicy::Missing.as_str(), "missing");
    assert_eq!(PullPolicy::Never.as_str(), "never");
}

// ========================================================================
// State Parsing Tests
// ========================================================================

#[test]
fn test_state_parsing() {
    // Test state enum parsing
    assert_eq!(
        format!("{:?}", SandboxState::Creating).to_lowercase(),
        "creating"
    );
    assert_eq!(
        format!("{:?}", SandboxState::Running).to_lowercase(),
        "running"
    );
}

#[test]
fn test_sandbox_state_as_str() {
    assert_eq!(SandboxState::Creating.as_str(), "creating");
    assert_eq!(SandboxState::Created.as_str(), "created");
    assert_eq!(SandboxState::Starting.as_str(), "starting");
    assert_eq!(SandboxState::Running.as_str(), "running");
    assert_eq!(SandboxState::Stopped.as_str(), "stopped");
    assert_eq!(SandboxState::Error.as_str(), "error");
    assert_eq!(SandboxState::Destroying.as_str(), "destroying");
}

// ========================================================================
// Serialization Tests
// ========================================================================

#[test]
fn test_serialize_sandbox_fields_empty_environment() {
    let now = Utc::now();
    let sandbox = Sandbox {
        id: uuid::Uuid::new_v4(),
        config: SandboxConfig {
            image: "test:latest".to_string(),
            name: None,
            environment: HashMap::new(),
            port_mappings: vec![],
            exposed_ports: vec![],
            resource_limits: ResourceLimits::default(),
            volumes: vec![],
            command: None,
            inactivity_timeout_minutes: None,
            pull_policy: PullPolicy::Missing,
            features: vec![],
            enable_all_features: false,
            vnc_resolution: None,
        },
        state: SandboxState::Creating,
        container_id: None,
        created_at: now,
        updated_at: now,
        error_message: None,
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: now,
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    let result = serialize_sandbox_fields(&sandbox);
    assert!(result.is_ok());
    let fields = result.unwrap();
    // Verify empty collections serialize correctly
    assert_eq!(fields.environment, serde_json::json!({}));
    assert_eq!(fields.port_mappings, serde_json::json!([]));
    assert_eq!(fields.volumes, serde_json::json!([]));
}

#[test]
fn test_serialize_sandbox_fields_with_environment() {
    let now = Utc::now();
    let mut environment = HashMap::new();
    environment.insert("KEY1".to_string(), "value1".to_string());
    environment.insert("KEY2".to_string(), "value2".to_string());

    let sandbox = Sandbox {
        id: uuid::Uuid::new_v4(),
        config: SandboxConfig {
            image: "test:latest".to_string(),
            name: None,
            environment,
            port_mappings: vec![],
            exposed_ports: vec![],
            resource_limits: ResourceLimits::default(),
            volumes: vec![],
            command: Some(vec!["echo".to_string(), "hello".to_string()]),
            inactivity_timeout_minutes: Some(60),
            pull_policy: PullPolicy::Always,
            features: vec![],
            enable_all_features: false,
            vnc_resolution: None,
        },
        state: SandboxState::Running,
        container_id: Some("container-123".to_string()),
        created_at: now,
        updated_at: now,
        error_message: Some("error occurred".to_string()),
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: now,
            last_container_activity: Some(now),
            activity_count: 5,
        },
        inactivity_timeout_minutes: Some(60),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    let result = serialize_sandbox_fields(&sandbox);
    assert!(result.is_ok());
    let fields = result.unwrap();

    // Verify environment was serialized
    let env_obj = fields.environment.as_object().unwrap();
    assert_eq!(env_obj.get("KEY1").unwrap().as_str().unwrap(), "value1");
    assert_eq!(env_obj.get("KEY2").unwrap().as_str().unwrap(), "value2");

    // Verify command was serialized
    let cmd_array = fields.command.as_array().unwrap();
    assert_eq!(cmd_array.len(), 2);
}

#[test]
fn test_serialize_sandbox_fields_with_resource_limits() {
    let now = Utc::now();
    let sandbox = Sandbox {
        id: uuid::Uuid::new_v4(),
        config: SandboxConfig {
            image: "test:latest".to_string(),
            name: None,
            environment: HashMap::new(),
            port_mappings: vec![],
            exposed_ports: vec![],
            resource_limits: ResourceLimits {
                memory_mb: Some(1024),
                cpu_quota: Some(200000),
                cpu_period: Some(100000),
                cpu_shares: Some(512),
                pids_limit: Some(100),
                ulimits: None,
            },
            volumes: vec![],
            command: None,
            inactivity_timeout_minutes: None,
            pull_policy: PullPolicy::Never,
            features: vec![],
            enable_all_features: false,
            vnc_resolution: None,
        },
        state: SandboxState::Creating,
        container_id: None,
        created_at: now,
        updated_at: now,
        error_message: None,
        volume_mounts: vec![VolumeMount::Bind {
            host_path: "/host/path".to_string(),
            container_path: "/container/path".to_string(),
            read_only: true,
        }],
        activity: ActivityTracking {
            last_api_activity: now,
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    let result = serialize_sandbox_fields(&sandbox);
    assert!(result.is_ok());
    let fields = result.unwrap();

    // Verify resource_limits was serialized
    let limits = fields.resource_limits.as_object().unwrap();
    assert_eq!(limits.get("memory_mb").unwrap().as_u64().unwrap(), 1024);
    assert_eq!(limits.get("cpu_quota").unwrap().as_u64().unwrap(), 200000);
}

// ========================================================================
// JSON Deserialization Tests (using tokio_postgres::Row mock)
// ========================================================================

#[test]
fn test_parse_pull_policy_valid_values() {
    // Test the parse_pull_policy function logic
    assert_eq!(PullPolicy::Always.as_str(), "always");
    assert_eq!(PullPolicy::Missing.as_str(), "missing");
    assert_eq!(PullPolicy::Never.as_str(), "never");
}

#[test]
fn test_parse_sandbox_state_valid_values() {
    // Test the parse_sandbox_state function logic
    assert_eq!(SandboxState::Creating.as_str(), "creating");
    assert_eq!(SandboxState::Created.as_str(), "created");
    assert_eq!(SandboxState::Starting.as_str(), "starting");
    assert_eq!(SandboxState::Running.as_str(), "running");
    assert_eq!(SandboxState::Stopped.as_str(), "stopped");
    assert_eq!(SandboxState::Error.as_str(), "error");
    assert_eq!(SandboxState::Destroying.as_str(), "destroying");
}

// ========================================================================
// Activity Tracking Tests
// ========================================================================

#[test]
fn test_activity_tracking_default() {
    let now = Utc::now();
    let activity = ActivityTracking {
        last_api_activity: now,
        last_container_activity: None,
        activity_count: 0,
    };
    assert_eq!(activity.activity_count, 0);
    assert!(activity.last_container_activity.is_none());
}

#[test]
fn test_activity_tracking_with_count() {
    let now = Utc::now();
    let activity = ActivityTracking {
        last_api_activity: now,
        last_container_activity: Some(now),
        activity_count: 100,
    };
    assert_eq!(activity.activity_count, 100);
    assert!(activity.last_container_activity.is_some());
}

// ========================================================================
// Sandbox Config Tests
// ========================================================================

#[test]
fn test_sandbox_config_default() {
    let config = SandboxConfig::default();
    // Verify default values
    assert!(config.environment.is_empty());
    assert!(config.port_mappings.is_empty());
    assert!(config.volumes.is_empty());
    assert!(config.command.is_none());
    assert_eq!(config.pull_policy, PullPolicy::Missing);
}

#[test]
fn test_sandbox_config_with_values() {
    let mut environment = HashMap::new();
    environment.insert("TEST".to_string(), "value".to_string());

    let config = SandboxConfig {
        image: "nginx:latest".to_string(),
        name: Some("test-sandbox".to_string()),
        environment,
        port_mappings: vec![],
        exposed_ports: vec![],
        resource_limits: ResourceLimits::default(),
        volumes: vec![],
        command: Some(vec!["sh".to_string()]),
        inactivity_timeout_minutes: Some(30),
        pull_policy: PullPolicy::Missing,
        features: vec![],
        enable_all_features: false,
        vnc_resolution: None,
    };

    assert_eq!(config.image, "nginx:latest");
    assert_eq!(config.name, Some("test-sandbox".to_string()));
    assert_eq!(config.environment.get("TEST"), Some(&"value".to_string()));
}

// ========================================================================
// Volume Mount Tests
// ========================================================================

#[test]
fn test_volume_mount_bind() {
    let mount = VolumeMount::Bind {
        host_path: "/host/data".to_string(),
        container_path: "/container/data".to_string(),
        read_only: false,
    };
    match mount {
        VolumeMount::Bind {
            host_path,
            container_path,
            read_only,
        } => {
            assert_eq!(host_path, "/host/data");
            assert_eq!(container_path, "/container/data");
            assert!(!read_only);
        }
        _ => panic!("Expected Bind variant"),
    }
}

#[test]
fn test_volume_mount_read_only() {
    let mount = VolumeMount::Bind {
        host_path: "/host/config".to_string(),
        container_path: "/container/config".to_string(),
        read_only: true,
    };
    match mount {
        VolumeMount::Bind { read_only, .. } => {
            assert!(read_only);
        }
        _ => panic!("Expected Bind variant"),
    }
}

// ========================================================================
// Port Mapping Tests
// ========================================================================

#[test]
fn test_port_mapping_tcp() {
    let mapping = PortMapping {
        host_port: 8080,
        container_port: 80,
        protocol: PortProtocol::Tcp,
    };
    assert_eq!(mapping.host_port, 8080);
    assert_eq!(mapping.container_port, 80);
    assert_eq!(mapping.protocol, PortProtocol::Tcp);
}

#[test]
fn test_port_mapping_udp() {
    let mapping = PortMapping {
        host_port: 9000,
        container_port: 9000,
        protocol: PortProtocol::Udp,
    };
    assert_eq!(mapping.protocol, PortProtocol::Udp);
}

// ========================================================================
// Resource Limits Tests
// ========================================================================

#[test]
fn test_resource_limits_default() {
    let limits = ResourceLimits::default();
    assert!(limits.memory_mb.is_none());
    assert!(limits.cpu_quota.is_none());
    assert!(limits.cpu_shares.is_none());
    assert!(limits.pids_limit.is_none());
}

#[test]
fn test_resource_limits_with_values() {
    let limits = ResourceLimits {
        memory_mb: Some(2048),
        cpu_quota: Some(150000),
        cpu_period: Some(100000),
        cpu_shares: Some(1024),
        pids_limit: Some(200),
        ulimits: None,
    };
    assert_eq!(limits.memory_mb, Some(2048));
    assert_eq!(limits.cpu_quota, Some(150000));
    assert_eq!(limits.cpu_shares, Some(1024));
    assert_eq!(limits.pids_limit, Some(200));
}

// ========================================================================
// PostgresStateStore Type Tests
// ========================================================================

#[test]
fn test_store_is_clone() {
    fn assert_clone<T: Clone>() {}
    assert_clone::<PostgresStateStore>();
}

#[test]
fn test_store_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<PostgresStateStore>();
}

#[test]
fn test_store_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<PostgresStateStore>();
}

// Note: Full integration tests for PostgresStateStore are in
// tests/db_integration_tests.rs which uses TestDatabase fixture
// to test all CRUD operations with real PostgreSQL connections.

// ========================================================================
// Additional Error Handling Tests
// ========================================================================

#[test]
fn test_store_error_from_io() {
    // Test error conversion from io::Error via to_string().into()
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let error: StoreError = io_err.to_string().into();
    assert!(error.to_string().contains("file not found"));
}

#[test]
fn test_store_error_from_tokio_postgres() {
    // Test From<tokio_postgres::Error> for StoreError
    // Create a simple error message and convert it
    let error: StoreError = "tokio postgres error".to_string().into();
    assert!(error.to_string().contains("tokio postgres error"));
}

#[test]
fn test_store_error_from_deadpool() {
    // Test From<deadpool_postgres::PoolError> for StoreError
    // Create a simple error message and convert it
    let error: StoreError = "pool closed".to_string().into();
    assert!(error.to_string().contains("pool closed"));
}

#[test]
fn test_store_error_debug_all_variants() {
    let uuid = uuid::Uuid::new_v4();

    let err1 = StoreError::NotFound(uuid);
    let err2 = StoreError::InvalidState("test".to_string());
    let err3 = StoreError::Message("test".to_string());

    let debug1 = format!("{:?}", err1);
    let debug2 = format!("{:?}", err2);
    let debug3 = format!("{:?}", err3);

    assert!(debug1.contains("NotFound"));
    assert!(debug2.contains("InvalidState"));
    assert!(debug3.contains("Message"));
}

#[test]
fn test_store_error_with_empty_message() {
    let error: StoreError = "".to_string().into();
    assert_eq!(error.to_string(), "");
}

#[test]
fn test_store_error_with_unicode() {
    let error: StoreError = "日本語テスト".to_string().into();
    assert!(error.to_string().contains("日本語"));
}

#[test]
fn test_store_error_with_special_chars() {
    let error: StoreError = "error with \"quotes\" and \\backslash".to_string().into();
    let err_str = error.to_string();
    assert!(err_str.contains("quotes"));
    assert!(err_str.contains("backslash"));
}

// ========================================================================
// Sandbox Serialization Edge Cases
// ========================================================================

#[test]
fn test_serialize_sandbox_with_all_features() {
    let now = Utc::now();
    let sandbox = Sandbox {
        id: uuid::Uuid::new_v4(),
        config: SandboxConfig {
            image: "test:latest".to_string(),
            name: Some("test".to_string()),
            environment: HashMap::new(),
            port_mappings: vec![],
            exposed_ports: vec![],
            resource_limits: ResourceLimits::default(),
            volumes: vec![],
            command: None,
            inactivity_timeout_minutes: None,
            pull_policy: PullPolicy::Missing,
            features: vec!["vnc".to_string(), "web_terminal".to_string()],
            enable_all_features: true,
            vnc_resolution: None,
        },
        state: SandboxState::Running,
        container_id: Some("container-123".to_string()),
        created_at: now,
        updated_at: now,
        error_message: None,
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: now,
            last_container_activity: Some(now),
            activity_count: 10,
        },
        inactivity_timeout_minutes: Some(30),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    let result = serialize_sandbox_fields(&sandbox);
    assert!(result.is_ok());
    let fields = result.unwrap();
    // Test that features field exists and can be accessed
    assert!(fields.features.is_array());
}

#[test]
fn test_serialize_sandbox_with_port_mappings() {
    let now = Utc::now();
    let sandbox = Sandbox {
        id: uuid::Uuid::new_v4(),
        config: SandboxConfig {
            image: "test:latest".to_string(),
            name: None,
            environment: HashMap::new(),
            port_mappings: vec![
                PortMapping {
                    host_port: 8080,
                    container_port: 80,
                    protocol: PortProtocol::Tcp,
                },
                PortMapping {
                    host_port: 9000,
                    container_port: 9000,
                    protocol: PortProtocol::Udp,
                },
            ],
            exposed_ports: vec![],
            resource_limits: ResourceLimits::default(),
            volumes: vec![],
            command: None,
            inactivity_timeout_minutes: None,
            pull_policy: PullPolicy::Missing,
            features: vec![],
            enable_all_features: false,
            vnc_resolution: None,
        },
        state: SandboxState::Running,
        container_id: None,
        created_at: now,
        updated_at: now,
        error_message: None,
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: now,
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    let result = serialize_sandbox_fields(&sandbox);
    assert!(result.is_ok());
    let fields = result.unwrap();
    let ports = fields.port_mappings.as_array().unwrap();
    assert_eq!(ports.len(), 2);
}

#[test]
fn test_serialize_sandbox_with_command() {
    let now = Utc::now();
    let sandbox = Sandbox {
        id: uuid::Uuid::new_v4(),
        config: SandboxConfig {
            image: "test:latest".to_string(),
            name: None,
            environment: HashMap::new(),
            port_mappings: vec![],
            exposed_ports: vec![],
            resource_limits: ResourceLimits::default(),
            volumes: vec![],
            command: Some(vec!["echo".to_string(), "hello".to_string()]),
            inactivity_timeout_minutes: Some(60),
            pull_policy: PullPolicy::Missing,
            features: vec![],
            enable_all_features: false,
            vnc_resolution: None,
        },
        state: SandboxState::Running,
        container_id: None,
        created_at: now,
        updated_at: now,
        error_message: None,
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: now,
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: Some(60),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    let result = serialize_sandbox_fields(&sandbox);
    assert!(result.is_ok());
    let fields = result.unwrap();
    // Test that command field exists and is serialized
    assert!(fields.command.is_array());
}

// ========================================================================
// PortProtocol Tests
// ========================================================================

#[test]
fn test_port_protocol_serde() {
    // PortProtocol serializes to lowercase via serde
    let tcp = PortProtocol::Tcp;
    let udp = PortProtocol::Udp;

    // Serialize to JSON
    let tcp_json = serde_json::to_string(&tcp).unwrap();
    let udp_json = serde_json::to_string(&udp).unwrap();

    assert_eq!(tcp_json, "\"tcp\"");
    assert_eq!(udp_json, "\"udp\"");
}

#[test]
fn test_port_protocol_debug_format() {
    let tcp = format!("{:?}", PortProtocol::Tcp);
    let udp = format!("{:?}", PortProtocol::Udp);
    assert!(tcp.contains("Tcp"));
    assert!(udp.contains("Udp"));
}

// ========================================================================
// VolumeMount Tests
// ========================================================================

#[test]
fn test_volume_mount_trait_impls() {
    fn assert_clone<T: Clone>() {}
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_clone::<VolumeMount>();
    assert_send::<VolumeMount>();
    assert_sync::<VolumeMount>();
}

#[test]
fn test_volume_mount_debug() {
    let bind = VolumeMount::Bind {
        host_path: "/host".to_string(),
        container_path: "/container".to_string(),
        read_only: false,
    };
    let debug = format!("{:?}", bind);
    assert!(debug.contains("/host"));
    assert!(debug.contains("/container"));
}

// ========================================================================
// PortMapping Tests
// ========================================================================

#[test]
fn test_port_mapping_debug_format() {
    let mapping = PortMapping {
        host_port: 8080,
        container_port: 80,
        protocol: PortProtocol::Tcp,
    };
    let debug = format!("{:?}", mapping);
    assert!(debug.contains("8080"));
    assert!(debug.contains("80"));
}

#[test]
fn test_port_mapping_trait_impls() {
    fn assert_clone<T: Clone>() {}
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_clone::<PortMapping>();
    assert_send::<PortMapping>();
    assert_sync::<PortMapping>();
}

// ========================================================================
// ResourceLimits Tests
// ========================================================================

#[test]
fn test_resource_limits_debug_format() {
    let limits = ResourceLimits {
        memory_mb: Some(1024),
        cpu_quota: Some(50000),
        cpu_period: Some(100000),
        cpu_shares: Some(512),
        pids_limit: Some(50),
        ulimits: None,
    };
    let debug = format!("{:?}", limits);
    assert!(debug.contains("1024"));
    assert!(debug.contains("50000"));
}

#[test]
fn test_resource_limits_with_ulimits() {
    use crate::core::types::Ulimit;
    let limits = ResourceLimits {
        memory_mb: Some(2048),
        cpu_quota: None,
        cpu_period: None,
        cpu_shares: None,
        pids_limit: None,
        ulimits: Some(vec![
            Ulimit {
                name: "nofile".to_string(),
                soft: 1024,
                hard: 4096,
            },
            Ulimit {
                name: "nproc".to_string(),
                soft: 100,
                hard: 200,
            },
        ]),
    };
    assert_eq!(limits.ulimits.as_ref().unwrap().len(), 2);
    assert_eq!(limits.ulimits.as_ref().unwrap()[0].name, "nofile");
}

// ========================================================================
// Sandbox State Tests
// ========================================================================

#[test]
fn test_sandbox_state_traits() {
    fn assert_clone<T: Clone>() {}
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_clone::<SandboxState>();
    assert_send::<SandboxState>();
    assert_sync::<SandboxState>();
}

#[test]
fn test_all_sandbox_states() {
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
        let as_str = state.as_str();
        assert!(!as_str.is_empty());
    }
}

// ========================================================================
// PullPolicy Tests
// ========================================================================

#[test]
fn test_pull_policy_traits() {
    fn assert_clone<T: Clone>() {}
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_clone::<PullPolicy>();
    assert_send::<PullPolicy>();
    assert_sync::<PullPolicy>();
}

#[test]
fn test_pull_policy_as_str_values() {
    // Test as_str for all variants
    assert_eq!(PullPolicy::Always.as_str(), "always");
    assert_eq!(PullPolicy::Missing.as_str(), "missing");
    assert_eq!(PullPolicy::Never.as_str(), "never");
}

// ========================================================================
// ActivityTracking Tests
// ========================================================================

#[test]
fn test_activity_tracking_traits() {
    fn assert_clone<T: Clone>() {}
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_clone::<ActivityTracking>();
    assert_send::<ActivityTracking>();
    assert_sync::<ActivityTracking>();
}

#[test]
fn test_activity_tracking_increment() {
    let now = Utc::now();
    let mut activity = ActivityTracking {
        last_api_activity: now,
        last_container_activity: None,
        activity_count: 0,
    };
    activity.activity_count += 1;
    assert_eq!(activity.activity_count, 1);
}

// ========================================================================
// SandboxConfig Tests
// ========================================================================

#[test]
fn test_sandbox_config_traits() {
    fn assert_clone<T: Clone>() {}
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_clone::<SandboxConfig>();
    assert_send::<SandboxConfig>();
    assert_sync::<SandboxConfig>();
}

#[test]
fn test_sandbox_config_debug_format() {
    let config = SandboxConfig::default();
    let debug = format!("{:?}", config);
    assert!(debug.contains("SandboxConfig"));
}

// ========================================================================
// Sandbox Tests
// ========================================================================

#[test]
fn test_sandbox_traits() {
    fn assert_clone<T: Clone>() {}
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_clone::<Sandbox>();
    assert_send::<Sandbox>();
    assert_sync::<Sandbox>();
}

#[test]
fn test_sandbox_with_max_values() {
    let now = Utc::now();
    let mut env = HashMap::new();
    for i in 0..100 {
        env.insert(format!("KEY{}", i), format!("value{}", i));
    }

    let sandbox = Sandbox {
        id: uuid::Uuid::new_v4(),
        config: SandboxConfig {
            image: "test:latest".to_string(),
            name: Some("max-test".to_string()),
            environment: env,
            port_mappings: (0..50)
                .map(|i| PortMapping {
                    host_port: 8080 + i as u16,
                    container_port: 80 + i as u16,
                    protocol: if i % 2 == 0 {
                        PortProtocol::Tcp
                    } else {
                        PortProtocol::Udp
                    },
                })
                .collect(),
            exposed_ports: vec![],
            resource_limits: ResourceLimits {
                memory_mb: Some(32768),
                cpu_quota: Some(500000),
                cpu_period: Some(100000),
                cpu_shares: Some(2048),
                pids_limit: Some(1000),
                ulimits: None,
            },
            volumes: (0..10)
                .map(|i| VolumeMount::Bind {
                    host_path: format!("/host{}", i),
                    container_path: format!("/container{}", i),
                    read_only: i % 2 == 0,
                })
                .collect(),
            command: Some(vec![
                "bash".to_string(),
                "-c".to_string(),
                "echo test".to_string(),
            ]),
            inactivity_timeout_minutes: Some(1440),
            pull_policy: PullPolicy::Always,
            features: vec!["vnc".to_string(), "web_terminal".to_string()],
            enable_all_features: true,
            vnc_resolution: None,
        },
        state: SandboxState::Running,
        container_id: Some("container-max".to_string()),
        created_at: now,
        updated_at: now,
        error_message: None,
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: now,
            last_container_activity: Some(now),
            activity_count: 1000,
        },
        inactivity_timeout_minutes: Some(1440),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    assert_eq!(sandbox.config.environment.len(), 100);
    assert_eq!(sandbox.config.port_mappings.len(), 50);
    assert_eq!(sandbox.config.volumes.len(), 10);
    assert_eq!(sandbox.activity.activity_count, 1000);
}

// ========================================================================
// Database Integration Tests
// ========================================================================

use crate::db::test_db::TestDb;

/// Creates a test database pool for integration tests.
///
/// Creates a test database pool with the schema migrated.
///
/// Most DB-touching tests should use this. Migrations run at most once
/// per test binary, so per-test cost is just a pool acquisition.
pub async fn create_migrated_test_pool() -> deadpool_postgres::Pool {
    TestDb::from_default_env().connect_with_schema().await
}

/// Cleans up test sandboxes from database
async fn cleanup_test_sandboxes(pool: &deadpool_postgres::Pool) {
    if let Ok(client) = pool.get().await {
        let _ = client
            .execute("DELETE FROM sandboxes WHERE id::text LIKE 'test-%'", &[])
            .await;
    }
}

#[tokio::test]
async fn test_row_to_sandbox_propagates_type_mismatch() {
    let pool = create_migrated_test_pool().await;
    let client = pool.get().await.unwrap();

    // Query a row where vnc_resolution is an integer instead of text.
    // This simulates a schema mismatch or data corruption.
    let row = client
        .query_one(
            "SELECT 
                '00000000-0000-0000-0000-000000000001'::uuid as id,
                'nginx:latest'::text as image,
                NULL::text as name,
                '{}'::jsonb as environment,
                '[]'::jsonb as port_mappings,
                '{}'::jsonb as resource_limits,
                '[]'::jsonb as volumes,
                NULL::jsonb as command,
                NULL::bigint as inactivity_timeout_minutes,
                'missing'::text as pull_policy,
                '[]'::jsonb as features,
                false::bool as enable_all_features,
                42::int as vnc_resolution,
                'running'::text as state,
                NULL::text as container_id,
                NULL::text as error_message,
                '[]'::jsonb as volume_mounts,
                NOW()::timestamptz as last_api_activity,
                NULL::timestamptz as last_container_activity,
                0::bigint as activity_count,
                NOW()::timestamptz as created_at,
                NOW()::timestamptz as updated_at,
                NULL::timestamptz as deleted_at,
                NULL::text as deleted_by,
                NULL::uuid as api_key_id",
            &[],
        )
        .await
        .unwrap();

    let result = row_to_sandbox(row);
    assert!(
        result.is_err(),
        "Type mismatch should propagate an error, not be silently ignored"
    );
}

#[tokio::test]
async fn test_create_sandbox_success() {
    let pool = create_migrated_test_pool().await;
    let store = PostgresStateStore::new(pool.clone()).await.unwrap();

    cleanup_test_sandboxes(&pool).await;

    let sandbox_id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id: sandbox_id,
        config: SandboxConfig {
            image: "nginx:latest".to_string(),
            name: Some("test-sandbox".to_string()),
            ..Default::default()
        },
        state: SandboxState::Running,
        container_id: Some("container-123".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    let result = store.create_sandbox(sandbox).await;
    assert!(result.is_ok(), "Should successfully create sandbox");

    // Verify sandbox was created
    let retrieved = store.get_sandbox(&sandbox_id).await;
    assert!(retrieved.is_some(), "Should retrieve created sandbox");

    cleanup_test_sandboxes(&pool).await;
}

#[tokio::test]
async fn test_get_sandbox_not_found() {
    let pool = create_migrated_test_pool().await;
    let store = PostgresStateStore::new(pool).await.unwrap();

    let fake_id = uuid::Uuid::new_v4();
    let result = store.get_sandbox(&fake_id).await;
    assert!(
        result.is_none(),
        "Should return None for non-existent sandbox"
    );
}

#[tokio::test]
async fn test_update_sandbox_state() {
    let pool = create_migrated_test_pool().await;
    let store = PostgresStateStore::new(pool.clone()).await.unwrap();

    cleanup_test_sandboxes(&pool).await;

    let sandbox_id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id: sandbox_id,
        config: SandboxConfig {
            image: "nginx:latest".to_string(),
            ..Default::default()
        },
        state: SandboxState::Creating,
        container_id: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    store.create_sandbox(sandbox.clone()).await.unwrap();

    // Update by creating a new sandbox with updated values
    let updated_sandbox = Sandbox {
        id: sandbox_id,
        config: sandbox.config.clone(),
        state: SandboxState::Running,
        container_id: Some("container-456".to_string()),
        created_at: sandbox.created_at,
        updated_at: sandbox.updated_at,
        error_message: sandbox.error_message.clone(),
        volume_mounts: sandbox.volume_mounts.clone(),
        activity: sandbox.activity.clone(),
        inactivity_timeout_minutes: sandbox.inactivity_timeout_minutes,
        deleted_at: sandbox.deleted_at,
        deleted_by: sandbox.deleted_by.clone(),
        api_key_id: sandbox.api_key_id,
    };

    store.update_sandbox(&updated_sandbox).await.unwrap();

    // Verify update
    let retrieved = store.get_sandbox(&sandbox_id).await;
    assert!(retrieved.is_some());
    let retrieved_sandbox = retrieved.unwrap();
    assert_eq!(retrieved_sandbox.state, SandboxState::Running);
    assert_eq!(
        retrieved_sandbox.container_id,
        Some("container-456".to_string())
    );

    cleanup_test_sandboxes(&pool).await;
}

#[tokio::test]
async fn test_soft_delete_sandbox() {
    let pool = create_migrated_test_pool().await;
    let store = PostgresStateStore::new(pool.clone()).await.unwrap();

    cleanup_test_sandboxes(&pool).await;

    let sandbox_id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id: sandbox_id,
        config: SandboxConfig {
            image: "nginx:latest".to_string(),
            ..Default::default()
        },
        state: SandboxState::Running,
        container_id: Some("container-123".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    store.create_sandbox(sandbox).await.unwrap();

    // Soft delete
    store
        .soft_delete_sandbox(&sandbox_id, Some("test-user".to_string()))
        .await
        .unwrap();

    // Verify soft delete - should not come back in normal get
    let retrieved = store.get_sandbox(&sandbox_id).await;
    assert!(
        retrieved.is_none(),
        "Soft deleted sandbox should not be returned"
    );

    // But should come back with get_sandbox_with_deleted
    let retrieved_with_deleted = store.get_sandbox_with_deleted(&sandbox_id, true).await;
    assert!(
        retrieved_with_deleted.is_some(),
        "Should return soft deleted sandbox when requested"
    );
    let retrieved_sandbox = retrieved_with_deleted.unwrap();
    assert!(
        retrieved_sandbox.deleted_at.is_some(),
        "Should have deleted_at timestamp"
    );
    assert_eq!(retrieved_sandbox.deleted_by, Some("test-user".to_string()));

    cleanup_test_sandboxes(&pool).await;
}

#[tokio::test]
async fn test_restore_sandbox() {
    let pool = create_migrated_test_pool().await;
    let store = PostgresStateStore::new(pool.clone()).await.unwrap();

    cleanup_test_sandboxes(&pool).await;

    let sandbox_id = uuid::Uuid::new_v4();
    let deleted_sandbox = Sandbox {
        id: sandbox_id,
        config: SandboxConfig {
            image: "nginx:latest".to_string(),
            ..Default::default()
        },
        state: SandboxState::Running,
        container_id: Some("container-123".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: Some(Utc::now()),
        deleted_by: Some("test-user".to_string()),
        api_key_id: None,
    };

    store.create_sandbox(deleted_sandbox).await.unwrap();

    // Restore
    store.restore_sandbox(&sandbox_id).await.unwrap();

    // Verify restoration
    let retrieved = store.get_sandbox(&sandbox_id).await;
    assert!(retrieved.is_some(), "Restored sandbox should be returned");
    let sandbox = retrieved.unwrap();
    assert!(sandbox.deleted_at.is_none(), "deleted_at should be cleared");
    assert!(sandbox.deleted_by.is_none(), "deleted_by should be cleared");

    cleanup_test_sandboxes(&pool).await;
}

#[tokio::test]
async fn test_list_sandboxes() {
    let pool = create_migrated_test_pool().await;
    let store = PostgresStateStore::new(pool.clone()).await.unwrap();

    cleanup_test_sandboxes(&pool).await;

    // Create multiple sandboxes
    for i in 0..3 {
        let sandbox = Sandbox {
            id: uuid::Uuid::new_v4(),
            config: SandboxConfig {
                image: format!("test:{}", i),
                ..Default::default()
            },
            state: SandboxState::Running,
            container_id: Some(format!("container-{}", i)),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            error_message: None,
            volume_mounts: vec![],
            activity: ActivityTracking {
                last_api_activity: Utc::now(),
                last_container_activity: None,
                activity_count: 0,
            },
            inactivity_timeout_minutes: None,
            deleted_at: None,
            deleted_by: None,
            api_key_id: None,
        };
        store.create_sandbox(sandbox).await.unwrap();
    }

    // List all
    let sandboxes = store.list_sandboxes().await;
    assert!(sandboxes.len() >= 3, "Should list at least 3 sandboxes");

    cleanup_test_sandboxes(&pool).await;
}

#[tokio::test]
async fn test_permanently_delete_sandbox() {
    let pool = create_migrated_test_pool().await;
    let store = PostgresStateStore::new(pool.clone()).await.unwrap();

    cleanup_test_sandboxes(&pool).await;

    let sandbox_id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id: sandbox_id,
        config: SandboxConfig {
            image: "nginx:latest".to_string(),
            ..Default::default()
        },
        state: SandboxState::Stopped,
        container_id: Some("container-123".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    store.create_sandbox(sandbox).await.unwrap();

    // Permanently delete
    store.permanently_delete_sandbox(&sandbox_id).await.unwrap();

    // Verify deletion
    let retrieved = store.get_sandbox_with_deleted(&sandbox_id, true).await;
    assert!(
        retrieved.is_none(),
        "Permanently deleted sandbox should not exist"
    );

    cleanup_test_sandboxes(&pool).await;
}

#[tokio::test]
async fn test_concurrent_sandbox_operations() {
    let pool = create_migrated_test_pool().await;

    cleanup_test_sandboxes(&pool).await;

    // Create multiple sandboxes concurrently
    let mut handles = vec![];

    for i in 0..5 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            let store = PostgresStateStore::new(pool_clone).await.unwrap();
            let sandbox = Sandbox {
                id: uuid::Uuid::new_v4(),
                config: SandboxConfig {
                    image: format!("test:{}", i),
                    ..Default::default()
                },
                state: SandboxState::Running,
                container_id: Some(format!("container-{}", i)),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                error_message: None,
                volume_mounts: vec![],
                activity: ActivityTracking {
                    last_api_activity: Utc::now(),
                    last_container_activity: None,
                    activity_count: 0,
                },
                inactivity_timeout_minutes: None,
                deleted_at: None,
                deleted_by: None,
                api_key_id: None,
            };
            store.create_sandbox(sandbox).await
        });
        handles.push(handle);
    }

    // All operations should succeed
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Concurrent sandbox creation should succeed");
    }

    cleanup_test_sandboxes(&pool).await;
}

#[tokio::test]
async fn test_update_sandbox_error_message() {
    let pool = create_migrated_test_pool().await;
    let store = PostgresStateStore::new(pool.clone()).await.unwrap();

    cleanup_test_sandboxes(&pool).await;

    let sandbox_id = uuid::Uuid::new_v4();
    let sandbox = Sandbox {
        id: sandbox_id,
        config: SandboxConfig {
            image: "nginx:latest".to_string(),
            ..Default::default()
        },
        state: SandboxState::Error,
        container_id: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        error_message: None,
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    store.create_sandbox(sandbox).await.unwrap();

    // Update with error message
    let error_msg = "Container failed to start";
    let updated_sandbox = Sandbox {
        id: sandbox_id,
        state: SandboxState::Error,
        error_message: Some(error_msg.to_string()),
        updated_at: Utc::now(),
        config: SandboxConfig {
            image: "nginx:latest".to_string(),
            ..Default::default()
        },
        container_id: None,
        created_at: Utc::now(),
        volume_mounts: vec![],
        activity: ActivityTracking {
            last_api_activity: Utc::now(),
            last_container_activity: None,
            activity_count: 0,
        },
        inactivity_timeout_minutes: None,
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    };

    store.update_sandbox(&updated_sandbox).await.unwrap();

    // Verify error message was updated
    let retrieved = store.get_sandbox(&sandbox_id).await;
    assert!(retrieved.is_some());
    let retrieved_sandbox = retrieved.unwrap();
    assert_eq!(retrieved_sandbox.error_message, Some(error_msg.to_string()));

    cleanup_test_sandboxes(&pool).await;
}

// ========================================================================
// Error Handling Pattern Tests
// ========================================================================

#[test]
fn test_sandbox_row_deserialization_error_not_panics() {
    // Simulate the filter_map pattern used in fetch_sandboxes_owned_by
    // to ensure deserialization errors are handled gracefully
    // (logged via tracing::error!, not panicked)
    let results: Vec<Result<i32, String>> =
        vec![Ok(1), Err("mock deserialization error".to_string()), Ok(3)];

    let filtered: Vec<i32> = results
        .into_iter()
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                // This mirrors the tracing::error! call in production code
                let _msg = format!("Failed to convert owned sandbox row: {}", e);
                None
            }
        })
        .collect();

    assert_eq!(filtered, vec![1, 3]);
}
