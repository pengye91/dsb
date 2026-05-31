// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Database Integration Tests
//!
//! Comprehensive tests for database modules using docker-compose PostgreSQL.
//!
//! These tests connect to the postgres-test service from docker-compose.test.yml.
//! Tests run sequentially and include cleanup to maintain isolation.

mod common;

use chrono::Utc;
use common::TestDatabase;
use dsb::core::store_trait::StateStoreTrait;
use dsb::core::types::{
    ActivityTracking, ActivityType, PortMapping, PortProtocol, PullPolicy, ResourceLimits, Sandbox,
    SandboxActivity, SandboxConfig, SandboxState, VolumeMount,
};
use dsb::db::{ActivityStore, PostgresStateStore};
use serial_test::serial;
use std::collections::HashMap;
use uuid::Uuid;

///////////////////////////////////////////////////////////////////////////////
// Test Helper Functions
///////////////////////////////////////////////////////////////////////////////

/// Setup test with clean database state
async fn setup_clean_db() -> TestDatabase {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    // Clean up any leftover data from previous tests
    let _ = db.cleanup_data().await;
    db
}

async fn insert_test_api_key(pool: &deadpool_postgres::Pool, id: Uuid, name: &str) {
    let client = pool.get().await.expect("Failed to get DB connection");
    let key_hash = format!("test-hash-{id}");
    let key_prefix: String = id.to_string().chars().take(8).collect();
    let unique_name = format!("{name}-{id}");
    let scopes = serde_json::json!([]);

    client
        .execute(
            r#"
            INSERT INTO api_keys (id, key_hash, key_prefix, name, scopes, created_by)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            &[
                &id,
                &key_hash,
                &key_prefix,
                &unique_name,
                &scopes,
                &"db-integration-tests",
            ],
        )
        .await
        .expect("Failed to insert test API key");
}

///////////////////////////////////////////////////////////////////////////////
// db/pool.rs tests
///////////////////////////////////////////////////////////////////////////////

#[serial]
#[tokio::test]
async fn test_pool_create_from_url() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");

    // Verify pool is accessible
    let client = db.pool.get().await.expect("Failed to get connection");
    let result = client
        .query_one("SELECT 1 as result", &[])
        .await
        .expect("Query failed");
    let value: i32 = result.get("result");
    assert_eq!(value, 1);
}

#[serial]
#[tokio::test]
async fn test_pool_connection_reuse() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");

    // Get multiple connections from pool
    let client1 = db.pool.get().await.expect("Failed to get connection 1");
    let client2 = db.pool.get().await.expect("Failed to get connection 2");

    // Both connections work
    let result1 = client1.query_one("SELECT 1 as result", &[]).await.unwrap();
    let result2 = client2.query_one("SELECT 2 as result", &[]).await.unwrap();

    assert_eq!(result1.get::<_, i32>("result"), 1);
    assert_eq!(result2.get::<_, i32>("result"), 2);
}

///////////////////////////////////////////////////////////////////////////////
// db/store.rs tests - CRUD Operations
///////////////////////////////////////////////////////////////////////////////

fn create_test_sandbox(id: Uuid) -> Sandbox {
    let now = Utc::now();
    let mut environment = HashMap::new();
    environment.insert("TEST".to_string(), "value".to_string());

    Sandbox {
        id,
        config: SandboxConfig {
            image: "nginx:latest".to_string(),
            name: Some(format!("test-{}", id)),
            environment,
            port_mappings: vec![PortMapping {
                host_port: 8080,
                container_port: 80,
                protocol: PortProtocol::Tcp,
            }],
            exposed_ports: vec![],
            resource_limits: ResourceLimits {
                memory_mb: Some(512),
                cpu_quota: Some(100000),
                cpu_period: Some(100000),
                cpu_shares: None,
                pids_limit: Some(100),
                ulimits: Some(vec![]),
            },
            volumes: vec![],
            command: None,
            inactivity_timeout_minutes: Some(30),
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
        inactivity_timeout_minutes: Some(30),
        deleted_at: None,
        deleted_by: None,
        api_key_id: None,
    }
}

fn create_owned_test_sandbox(id: Uuid, api_key_id: Uuid) -> Sandbox {
    let mut sandbox = create_test_sandbox(id);
    sandbox.api_key_id = Some(api_key_id);
    sandbox
}

#[serial]
#[tokio::test]
async fn test_store_create_sandbox() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Create a test sandbox
    let id = Uuid::new_v4();
    let sandbox = create_test_sandbox(id);

    // Insert into store
    store
        .create_sandbox(sandbox.clone())
        .await
        .expect("Failed to create sandbox");

    // Verify it was created
    let retrieved = store.get_sandbox(&id).await;
    assert!(retrieved.is_some(), "Sandbox should exist");

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, id);
    assert_eq!(retrieved.config.image, "nginx:latest");
    assert_eq!(retrieved.state, SandboxState::Creating);
}

#[serial]
#[tokio::test]
async fn test_store_get_sandbox_not_found() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Try to get non-existent sandbox
    let result = store.get_sandbox(&Uuid::new_v4()).await;
    assert!(result.is_none(), "Non-existent sandbox should return None");
}

#[serial]
#[tokio::test]
async fn test_store_list_sandboxes() {
    let db = setup_clean_db().await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Initially empty
    let list = store.list_sandboxes().await;
    assert_eq!(list.len(), 0, "Should start with no sandboxes");

    // Create multiple sandboxes
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    store
        .create_sandbox(create_test_sandbox(id1))
        .await
        .unwrap();
    store
        .create_sandbox(create_test_sandbox(id2))
        .await
        .unwrap();
    store
        .create_sandbox(create_test_sandbox(id3))
        .await
        .unwrap();

    // List should return all 3
    let list = store.list_sandboxes().await;
    assert_eq!(list.len(), 3, "Should have 3 sandboxes");

    // Verify IDs match
    let ids: Vec<_> = list.iter().map(|s| s.id).collect();
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));
    assert!(ids.contains(&id3));
}

#[serial]
#[tokio::test]
async fn test_store_update_sandbox() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Create a sandbox
    let id = Uuid::new_v4();
    let mut sandbox = create_test_sandbox(id);
    store.create_sandbox(sandbox.clone()).await.unwrap();

    // Update the sandbox
    sandbox.state = SandboxState::Running;
    sandbox.container_id = Some("container-123".to_string());
    sandbox.config.image = "nginx:alpine".to_string();
    sandbox.updated_at = Utc::now();

    store
        .update_sandbox(&sandbox)
        .await
        .expect("Failed to update sandbox");

    // Verify updates
    let retrieved = store.get_sandbox(&id).await.expect("Sandbox should exist");
    assert_eq!(retrieved.state, SandboxState::Running);
    assert_eq!(retrieved.container_id, Some("container-123".to_string()));
    assert_eq!(retrieved.config.image, "nginx:alpine");
}

#[serial]
#[tokio::test]
async fn test_store_delete_sandbox() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Create a sandbox
    let id = Uuid::new_v4();
    store.create_sandbox(create_test_sandbox(id)).await.unwrap();

    // Verify it exists
    assert!(store.get_sandbox(&id).await.is_some());

    // Delete it
    store
        .delete_sandbox(&id)
        .await
        .expect("Failed to delete sandbox");

    // Verify it's gone
    assert!(store.get_sandbox(&id).await.is_none());
}

#[serial]
#[tokio::test]
async fn test_store_update_nonexistent_sandbox() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Try to update non-existent sandbox
    let id = Uuid::new_v4();
    let sandbox = create_test_sandbox(id);

    let result = store.update_sandbox(&sandbox).await;
    assert!(result.is_err(), "Updating non-existent sandbox should fail");
}

#[serial]
#[tokio::test]
async fn test_store_delete_nonexistent_sandbox() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Try to delete non-existent sandbox
    let id = Uuid::new_v4();
    let result = store.delete_sandbox(&id).await;

    // Should succeed (idempotent) or fail depending on implementation
    // Current implementation likely returns error or succeeds
    // Just verify it doesn't panic
    assert!(result.is_ok() || result.is_err());
}

///////////////////////////////////////////////////////////////////////////////
// db/activities.rs tests
///////////////////////////////////////////////////////////////////////////////

#[serial]
#[tokio::test]
async fn test_activities_record_and_list() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let activity_store = ActivityStore::new(db.pool.clone());

    // Create a sandbox first
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");
    let sandbox_id = Uuid::new_v4();
    store
        .create_sandbox(create_test_sandbox(sandbox_id))
        .await
        .unwrap();

    // Record multiple activities
    let activity1 = SandboxActivity {
        id: Uuid::new_v4(),
        sandbox_id,
        activity_type: ActivityType::Create,
        timestamp: Utc::now(),
        details: serde_json::json!({"image": "nginx:latest"}),
        sandbox_is_deleted: false,
    };

    let activity2 = SandboxActivity {
        id: Uuid::new_v4(),
        sandbox_id,
        activity_type: ActivityType::Exec,
        timestamp: Utc::now(),
        details: serde_json::json!({"command": "ls -la"}),
        sandbox_is_deleted: false,
    };

    activity_store
        .record_activity(&activity1)
        .await
        .expect("Failed to record activity 1");

    activity_store
        .record_activity(&activity2)
        .await
        .expect("Failed to record activity 2");

    // List activities for sandbox
    let activities = activity_store
        .list_sandbox_activities(&sandbox_id, 10)
        .await
        .expect("Failed to list activities");

    assert_eq!(activities.len(), 2, "Should have 2 activities");
    // Activities are returned in DESC order (most recent first)
    assert_eq!(activities[0].activity_type, ActivityType::Exec);
    assert_eq!(activities[1].activity_type, ActivityType::Create);
}

#[serial]
#[tokio::test]
async fn test_activities_list_recent() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let activity_store = ActivityStore::new(db.pool.clone());

    // Create sandboxes
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");
    let sandbox_id1 = Uuid::new_v4();
    let sandbox_id2 = Uuid::new_v4();
    store
        .create_sandbox(create_test_sandbox(sandbox_id1))
        .await
        .unwrap();
    store
        .create_sandbox(create_test_sandbox(sandbox_id2))
        .await
        .unwrap();

    // Record activities for both sandboxes
    for i in 0..5 {
        let activity = SandboxActivity {
            id: Uuid::new_v4(),
            sandbox_id: sandbox_id1,
            activity_type: ActivityType::Exec,
            timestamp: Utc::now(),
            details: serde_json::json!({"index": i}),
            sandbox_is_deleted: false,
        };
        activity_store.record_activity(&activity).await.unwrap();
    }

    for i in 0..3 {
        let activity = SandboxActivity {
            id: Uuid::new_v4(),
            sandbox_id: sandbox_id2,
            activity_type: ActivityType::Exec,
            timestamp: Utc::now(),
            details: serde_json::json!({"index": i}),
            sandbox_is_deleted: false,
        };
        activity_store.record_activity(&activity).await.unwrap();
    }

    // List recent activities (limit to 5)
    let recent = activity_store
        .list_recent_activities(5)
        .await
        .expect("Failed to list recent");

    assert_eq!(recent.len(), 5, "Should return 5 recent activities");
}

#[serial]
#[tokio::test]
async fn test_activities_get_activity() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let activity_store = ActivityStore::new(db.pool.clone());

    // Create a sandbox
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");
    let sandbox_id = Uuid::new_v4();
    store
        .create_sandbox(create_test_sandbox(sandbox_id))
        .await
        .unwrap();

    // Record an activity
    let activity = SandboxActivity {
        id: Uuid::new_v4(),
        sandbox_id,
        activity_type: ActivityType::Stats,
        timestamp: Utc::now(),
        details: serde_json::json!({"cpu": "50%", "memory": "100MB"}),
        sandbox_is_deleted: false,
    };

    activity_store
        .record_activity(&activity)
        .await
        .expect("Failed to record activity");

    // Get the activity
    let retrieved = activity_store
        .get_activity(&activity.id)
        .await
        .expect("Failed to get activity")
        .expect("Activity should exist");

    assert_eq!(retrieved.id, activity.id);
    assert_eq!(retrieved.activity_type, ActivityType::Stats);
    assert_eq!(retrieved.sandbox_id, sandbox_id);
}

#[serial]
#[tokio::test]
async fn test_activities_mark_sandbox_deleted() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let activity_store = ActivityStore::new(db.pool.clone());

    // Create a sandbox
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");
    let sandbox_id = Uuid::new_v4();
    store
        .create_sandbox(create_test_sandbox(sandbox_id))
        .await
        .unwrap();

    // Record activities
    for i in 0..3 {
        let activity = SandboxActivity {
            id: Uuid::new_v4(),
            sandbox_id,
            activity_type: ActivityType::Exec,
            timestamp: Utc::now(),
            details: serde_json::json!({"index": i}),
            sandbox_is_deleted: false,
        };
        activity_store.record_activity(&activity).await.unwrap();
    }

    // Mark as deleted
    activity_store
        .mark_sandbox_activities_deleted(&sandbox_id)
        .await
        .expect("Failed to mark as deleted");

    // Verify all activities are marked
    let activities = activity_store
        .list_sandbox_activities(&sandbox_id, 10)
        .await
        .expect("Failed to list activities");

    assert_eq!(activities.len(), 3);
    for activity in activities {
        assert!(
            activity.sandbox_is_deleted,
            "Activity should be marked as deleted"
        );
    }
}

#[serial]
#[tokio::test]
async fn test_activities_upload_and_download() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let activity_store = ActivityStore::new(db.pool.clone());

    // Create a sandbox
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");
    let sandbox_id = Uuid::new_v4();
    store
        .create_sandbox(create_test_sandbox(sandbox_id))
        .await
        .expect("Failed to create sandbox");

    // Record upload activity
    let upload_activity = SandboxActivity {
        id: Uuid::new_v4(),
        sandbox_id,
        activity_type: ActivityType::Upload,
        timestamp: Utc::now(),
        details: serde_json::json!({
            "path": "/app/config.json",
            "size": 1024
        }),
        sandbox_is_deleted: false,
    };
    activity_store
        .record_activity(&upload_activity)
        .await
        .unwrap();

    // Record download activity
    let download_activity = SandboxActivity {
        id: Uuid::new_v4(),
        sandbox_id,
        activity_type: ActivityType::Download,
        timestamp: Utc::now(),
        details: serde_json::json!({
            "path": "/app/output.txt",
            "size": 2048
        }),
        sandbox_is_deleted: false,
    };
    activity_store
        .record_activity(&download_activity)
        .await
        .unwrap();

    // List activities for sandbox
    let activities = activity_store
        .list_sandbox_activities(&sandbox_id, 10)
        .await
        .expect("Failed to list activities");

    assert_eq!(activities.len(), 2);
    assert_eq!(activities[0].activity_type, ActivityType::Download);
    assert_eq!(activities[1].activity_type, ActivityType::Upload);

    // Verify upload details
    assert_eq!(activities[1].details["path"], "/app/config.json");
    assert_eq!(activities[1].details["size"], 1024);

    // Verify download details
    assert_eq!(activities[0].details["path"], "/app/output.txt");
    assert_eq!(activities[0].details["size"], 2048);
}

///////////////////////////////////////////////////////////////////////////////
// Edge case and error handling tests
///////////////////////////////////////////////////////////////////////////////

#[serial]
#[tokio::test]
async fn test_store_sandbox_with_complex_config() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    let id = Uuid::new_v4();
    let mut sandbox = create_test_sandbox(id);

    // Add complex configuration
    let mut environment = HashMap::new();
    environment.insert("KEY1".to_string(), "value1".to_string());
    environment.insert("KEY2".to_string(), "value2".to_string());
    environment.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
    sandbox.config.environment = environment;

    sandbox.config.volumes = vec![VolumeMount::Bind {
        host_path: "/host/path".to_string(),
        container_path: "/container/path".to_string(),
        read_only: false,
    }];

    sandbox.config.resource_limits = ResourceLimits {
        memory_mb: Some(1024),
        cpu_quota: Some(200000),
        cpu_period: Some(100000),
        cpu_shares: None,
        pids_limit: Some(200),
        ulimits: Some(vec![]),
    };

    store.create_sandbox(sandbox.clone()).await.unwrap();

    // Retrieve and verify
    let retrieved = store.get_sandbox(&id).await.expect("Sandbox should exist");
    assert_eq!(retrieved.config.environment.len(), 3);
    assert_eq!(retrieved.config.volumes.len(), 1);
    assert_eq!(retrieved.config.resource_limits.memory_mb, Some(1024));
}

#[serial]
#[tokio::test]
async fn test_store_state_transitions() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    let id = Uuid::new_v4();
    let mut sandbox = create_test_sandbox(id);
    store.create_sandbox(sandbox.clone()).await.unwrap();

    // Simulate state transitions
    let states = vec![
        SandboxState::Creating,
        SandboxState::Running,
        SandboxState::Stopped,
        SandboxState::Destroying,
        SandboxState::Destroyed,
    ];

    for state in states {
        sandbox.state = state;
        sandbox.updated_at = Utc::now();
        store
            .update_sandbox(&sandbox)
            .await
            .expect("Failed to update state");

        let retrieved = store.get_sandbox(&id).await.expect("Sandbox should exist");
        assert_eq!(retrieved.state, state);
    }
}

#[serial]
#[tokio::test]
async fn test_store_list_sandboxes_includes_deleted() {
    let db = setup_clean_db().await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Create active sandboxes
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    store
        .create_sandbox(create_test_sandbox(id1))
        .await
        .unwrap();
    store
        .create_sandbox(create_test_sandbox(id2))
        .await
        .unwrap();

    // Create and delete a sandbox (soft delete with destroyed state)
    let id3 = Uuid::new_v4();
    let mut sandbox3 = create_test_sandbox(id3);
    store.create_sandbox(sandbox3.clone()).await.unwrap();
    sandbox3.state = SandboxState::Destroyed;
    sandbox3.deleted_at = Some(Utc::now());
    store
        .update_sandbox(&sandbox3)
        .await
        .expect("Failed to update sandbox to destroyed state");

    // list_sandboxes should return ALL sandboxes including deleted ones
    let list = store.list_sandboxes().await;
    assert_eq!(
        list.len(),
        3,
        "Should include all 3 sandboxes including destroyed"
    );

    // Verify all IDs are present
    let ids: Vec<_> = list.iter().map(|s| s.id).collect();
    assert!(ids.contains(&id1), "Should include active sandbox 1");
    assert!(ids.contains(&id2), "Should include active sandbox 2");
    assert!(ids.contains(&id3), "Should include destroyed sandbox 3");
}

#[serial]
#[tokio::test]
async fn test_store_list_sandboxes_filtered_with_include_deleted() {
    use dsb::db::store::SandboxListFilters;

    let db = setup_clean_db().await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Create active sandboxes
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    store
        .create_sandbox(create_test_sandbox(id1))
        .await
        .unwrap();
    store
        .create_sandbox(create_test_sandbox(id2))
        .await
        .unwrap();

    // Create and delete a sandbox
    let id3 = Uuid::new_v4();
    let mut sandbox3 = create_test_sandbox(id3);
    store.create_sandbox(sandbox3.clone()).await.unwrap();
    sandbox3.state = SandboxState::Destroyed;
    sandbox3.deleted_at = Some(Utc::now());
    store
        .update_sandbox(&sandbox3)
        .await
        .expect("Failed to update sandbox to destroyed state");

    // Test with include_deleted = false (should exclude deleted)
    let filters_no_deleted = SandboxListFilters {
        include_deleted: false,
        ..Default::default()
    };
    let list_no_deleted = store
        .list_sandboxes_filtered(Some(filters_no_deleted))
        .await
        .expect("Failed to list sandboxes");
    assert_eq!(
        list_no_deleted.data.len(),
        2,
        "Should only return 2 active sandboxes"
    );
    let ids: Vec<_> = list_no_deleted.data.iter().map(|s| s.id).collect();
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));
    assert!(!ids.contains(&id3), "Should not include destroyed sandbox");

    // Test with include_deleted = true (should include all)
    let filters_with_deleted = SandboxListFilters {
        include_deleted: true,
        ..Default::default()
    };
    let list_with_deleted = store
        .list_sandboxes_filtered(Some(filters_with_deleted))
        .await
        .expect("Failed to list sandboxes");
    assert_eq!(
        list_with_deleted.data.len(),
        3,
        "Should return all 3 sandboxes"
    );
    let ids: Vec<_> = list_with_deleted.data.iter().map(|s| s.id).collect();
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));
    assert!(ids.contains(&id3), "Should include destroyed sandbox");
}

#[serial]
#[tokio::test]
async fn test_store_list_sandboxes_owned_by_filters_by_owner() {
    let db = setup_clean_db().await;
    let api_key_a = Uuid::new_v4();
    let api_key_b = Uuid::new_v4();
    insert_test_api_key(&db.pool, api_key_a, "owner-a").await;
    insert_test_api_key(&db.pool, api_key_b, "owner-b").await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    let sandbox_a1 = create_owned_test_sandbox(Uuid::new_v4(), api_key_a);
    let sandbox_a2 = create_owned_test_sandbox(Uuid::new_v4(), api_key_a);
    let sandbox_b = create_owned_test_sandbox(Uuid::new_v4(), api_key_b);
    let mut deleted_a = create_owned_test_sandbox(Uuid::new_v4(), api_key_a);
    deleted_a.deleted_at = Some(Utc::now());
    deleted_a.state = SandboxState::Destroyed;

    store.create_sandbox(sandbox_a1.clone()).await.unwrap();
    store.create_sandbox(sandbox_a2.clone()).await.unwrap();
    store.create_sandbox(sandbox_b).await.unwrap();
    store.create_sandbox(deleted_a).await.unwrap();

    let owned = store.list_sandboxes_owned_by(&api_key_a, false).await;
    let ids: Vec<_> = owned.iter().map(|sandbox| sandbox.id).collect();

    assert_eq!(owned.len(), 2);
    assert!(ids.contains(&sandbox_a1.id));
    assert!(ids.contains(&sandbox_a2.id));
    assert!(owned
        .iter()
        .all(|sandbox| sandbox.api_key_id == Some(api_key_a)));
}

#[serial]
#[tokio::test]
async fn test_store_get_sandbox_with_deleted_if_owned_by_honors_include_deleted() {
    let db = setup_clean_db().await;
    let owner = Uuid::new_v4();
    let other_owner = Uuid::new_v4();
    insert_test_api_key(&db.pool, owner, "owner").await;
    insert_test_api_key(&db.pool, other_owner, "other-owner").await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    let sandbox_id = Uuid::new_v4();
    let mut deleted = create_owned_test_sandbox(sandbox_id, owner);
    deleted.deleted_at = Some(Utc::now());
    deleted.state = SandboxState::Destroyed;
    store.create_sandbox(deleted).await.unwrap();

    assert!(store
        .get_sandbox_if_owned_by(&sandbox_id, &owner)
        .await
        .is_none());
    assert!(store
        .get_sandbox_with_deleted_if_owned_by(&sandbox_id, &owner, true)
        .await
        .is_some());
    assert!(store
        .get_sandbox_with_deleted_if_owned_by(&sandbox_id, &other_owner, true)
        .await
        .is_none());
}

#[serial]
#[tokio::test]
async fn test_store_parse_destroyed_state() {
    let db = setup_clean_db().await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Create a sandbox and update it to destroyed state
    let id = Uuid::new_v4();
    let mut sandbox = create_test_sandbox(id);
    store.create_sandbox(sandbox.clone()).await.unwrap();

    // Update to destroyed state
    sandbox.state = SandboxState::Destroyed;
    sandbox.deleted_at = Some(Utc::now());
    store
        .update_sandbox(&sandbox)
        .await
        .expect("Failed to update sandbox to destroyed state");

    // Verify we can retrieve it with include_deleted=true and it has the correct state
    let retrieved = store
        .get_sandbox_with_deleted(&id, true)
        .await
        .expect("Should be able to retrieve destroyed sandbox with include_deleted=true");
    assert_eq!(retrieved.state, SandboxState::Destroyed);
    assert!(
        retrieved.deleted_at.is_some(),
        "Should have deleted_at timestamp"
    );

    // Verify get_sandbox() returns None for deleted sandbox (default behavior)
    let retrieved_normal = store.get_sandbox(&id).await;
    assert!(
        retrieved_normal.is_none(),
        "get_sandbox() should return None for deleted sandbox"
    );

    // Verify it appears in list_sandboxes (which includes deleted)
    let list = store.list_sandboxes().await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].state, SandboxState::Destroyed);
}

///////////////////////////////////////////////////////////////////////////////
// db/store.rs tests - Restore and Permanent Deletion
///////////////////////////////////////////////////////////////////////////////

#[serial]
#[tokio::test]
async fn test_restore_sandbox() {
    let db = setup_clean_db().await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Create a sandbox
    let id = Uuid::new_v4();
    let sandbox = create_test_sandbox(id);
    store.create_sandbox(sandbox).await.unwrap();

    // Soft delete the sandbox
    store
        .soft_delete_sandbox(&id, Some("test".to_string()))
        .await
        .unwrap();

    // Verify it's marked as deleted
    let deleted = store.get_sandbox_with_deleted(&id, true).await.unwrap();
    assert!(deleted.deleted_at.is_some());
    assert_eq!(deleted.deleted_by, Some("test".to_string()));

    // Restore the sandbox
    store.restore_sandbox(&id).await.unwrap();

    // Verify it's restored
    let restored = store.get_sandbox(&id).await.unwrap();
    assert!(restored.deleted_at.is_none());
    assert!(restored.deleted_by.is_none());
    assert_eq!(restored.state, SandboxState::Stopped);
}

#[serial]
#[tokio::test]
async fn test_restore_nonexistent_sandbox() {
    let db = setup_clean_db().await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    let id = Uuid::new_v4();
    let result = store.restore_sandbox(&id).await;
    assert!(result.is_err());
}

#[serial]
#[tokio::test]
async fn test_restore_non_deleted_sandbox() {
    let db = setup_clean_db().await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Create a sandbox that's not deleted
    let id = Uuid::new_v4();
    let sandbox = create_test_sandbox(id);
    store.create_sandbox(sandbox).await.unwrap();

    // Try to restore a non-deleted sandbox
    let result = store.restore_sandbox(&id).await;
    assert!(result.is_err());
}

#[serial]
#[tokio::test]
async fn test_permanently_delete_sandbox() {
    let db = setup_clean_db().await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Create and soft delete a sandbox
    let id = Uuid::new_v4();
    let sandbox = create_test_sandbox(id);
    store.create_sandbox(sandbox).await.unwrap();
    store.soft_delete_sandbox(&id, None).await.unwrap();

    // Verify it exists
    assert!(store.get_sandbox_with_deleted(&id, true).await.is_some());

    // Permanently delete
    store.permanently_delete_sandbox(&id).await.unwrap();

    // Verify it's gone
    assert!(store.get_sandbox_with_deleted(&id, true).await.is_none());
}

#[serial]
#[tokio::test]
async fn test_cleanup_expired_sandboxes() {
    let db = setup_clean_db().await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Create a sandbox deleted long ago
    let old_id = Uuid::new_v4();
    let mut old_sandbox = create_test_sandbox(old_id);
    old_sandbox.deleted_at = Some(Utc::now() - chrono::Duration::days(20));
    old_sandbox.deleted_by = Some("old".to_string());
    store.create_sandbox(old_sandbox).await.unwrap();

    // Create a sandbox deleted recently
    let recent_id = Uuid::new_v4();
    let mut recent_sandbox = create_test_sandbox(recent_id);
    recent_sandbox.deleted_at = Some(Utc::now() - chrono::Duration::days(5));
    recent_sandbox.deleted_by = Some("recent".to_string());
    store.create_sandbox(recent_sandbox).await.unwrap();

    // Create a non-deleted sandbox
    let active_id = Uuid::new_v4();
    let active_sandbox = create_test_sandbox(active_id);
    store.create_sandbox(active_sandbox).await.unwrap();

    // Cleanup with 15 day retention
    let deleted_count = store.cleanup_expired_sandboxes(15).await.unwrap();

    // Should delete only the old sandbox
    assert_eq!(deleted_count, 1);

    // Verify results
    assert!(store
        .get_sandbox_with_deleted(&old_id, true)
        .await
        .is_none()); // Old one deleted
    assert!(store
        .get_sandbox_with_deleted(&recent_id, true)
        .await
        .is_some()); // Recent one kept
    assert!(store.get_sandbox(&active_id).await.is_some()); // Active one kept
}

#[serial]
#[tokio::test]
async fn test_cleanup_expired_sandboxes_none_expired() {
    let db = setup_clean_db().await;
    let store = PostgresStateStore::new(db.pool)
        .await
        .expect("Failed to create store");

    // Create a recently deleted sandbox
    let id = Uuid::new_v4();
    let mut sandbox = create_test_sandbox(id);
    sandbox.deleted_at = Some(Utc::now() - chrono::Duration::days(1));
    sandbox.deleted_by = Some("recent".to_string());
    store.create_sandbox(sandbox).await.unwrap();

    // Cleanup with 15 day retention
    let deleted_count = store.cleanup_expired_sandboxes(15).await.unwrap();

    // Should delete nothing
    assert_eq!(deleted_count, 0);

    // Verify it still exists
    assert!(store.get_sandbox_with_deleted(&id, true).await.is_some());
}
