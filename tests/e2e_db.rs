// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Consolidated database integration tests.
//!
//! These tests exercise the DATABASE LAYER directly — no HTTP server needed.
//!
//! **Local mode**: each test gets its own fresh PostgreSQL container via
//! EphemeralPostgres.
//!
//! **External (EKS) mode**: tests connect to the shared external database.
//! Count-sensitive assertions are relaxed to account for shared state.

mod common;
use common::testcontainers_postgres::EphemeralPostgres;
use common::using_external_api;

use chrono::Utc;
use dsb::core::store_trait::StateStoreTrait;
use dsb::core::types::{
    ActivityTracking, ActivityType, CreateSshSessionRequest,
    PullPolicy, ResourceLimits, Sandbox, SandboxActivity, SandboxConfig, SandboxState,
    SshAuthMethod, SshSessionFilters, SshSessionState,
};
use dsb::db::store::SandboxListFilters;
use dsb::db::{
    ActivityStore, PostgresStateStore, PostgresSshSessionStore, SshSessionStoreTrait,
};
use dsb::core::ssh_service::SshSessionService;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

///////////////////////////////////////////////////////////////////////////////
// Test Database Setup
///////////////////////////////////////////////////////////////////////////////

/// Test database fixture that keeps the backing store alive.
///
/// In local mode: holds an `EphemeralPostgres` (and its container).
/// In external mode: just wraps a pool to the shared database.
struct TestDb {
    pool: deadpool_postgres::Pool,
    #[allow(dead_code)]
    _pg: Option<EphemeralPostgres>,
}

impl TestDb {
    fn pool(&self) -> deadpool_postgres::Pool {
        self.pool.clone()
    }
}

/// Returns a test database fixture.
///
/// In local mode: spins up an ephemeral PostgreSQL container.
/// In external mode: connects to the shared database configured via
/// `DSB_TEST_DATABASE_URL`.
async fn setup_test_db() -> TestDb {
    if common::using_external_api() {
        use deadpool_postgres::Runtime;
        use tokio_postgres::NoTls;

        let database_url = common::test_config::get_test_database_url();
        let mut config = deadpool_postgres::Config::new();
        config.url = Some(database_url);

        let pool = config
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .expect("Failed to create DB pool");
        TestDb { pool, _pg: None }
    } else {
        let pg = EphemeralPostgres::start()
            .await
            .expect("Failed to start Postgres");
        let pool = pg.pool.clone();
        TestDb {
            pool,
            _pg: Some(pg),
        }
    }
}

///////////////////////////////////////////////////////////////////////////////
// Helper Functions
///////////////////////////////////////////////////////////////////////////////

fn make_test_sandbox(id: Uuid) -> Sandbox {
    let now = Utc::now();
    Sandbox {
        id,
        config: SandboxConfig {
            image: "nginx:latest".to_string(),
            name: Some(format!("test-{}", id)),
            environment: HashMap::new(),
            port_mappings: vec![],
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

fn make_owned_test_sandbox(id: Uuid, api_key_id: Uuid) -> Sandbox {
    let mut sandbox = make_test_sandbox(id);
    sandbox.api_key_id = Some(api_key_id);
    sandbox
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
            ON CONFLICT (id) DO NOTHING
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

/// Insert a sandbox row directly for SSH session tests (FK constraint).
async fn insert_test_sandbox_for_ssh(
    pool: &deadpool_postgres::Pool,
    name: &str,
) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
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
        ON CONFLICT (id) DO NOTHING
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
                &Option::<serde_json::Value>::None,
                &Option::<i64>::None,
                &"missing",
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

///////////////////////////////////////////////////////////////////////////////
// Pool Tests
///////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn test_pool_create_from_url() {
    let db = setup_test_db().await;
    let pool = db.pool();

    let client = pool.get().await.expect("Failed to get connection");
    let result = client
        .query_one("SELECT 1 as result", &[])
        .await
        .expect("Query failed");
    let value: i32 = result.get("result");
    assert_eq!(value, 1);
}

#[tokio::test]
async fn test_pool_connection_reuse() {
    let db = setup_test_db().await;
    let pool = db.pool();

    let client1 = pool.get().await.expect("Failed to get connection 1");
    let client2 = pool.get().await.expect("Failed to get connection 2");

    let result1 = client1.query_one("SELECT 1 as result", &[]).await.unwrap();
    let result2 = client2.query_one("SELECT 2 as result", &[]).await.unwrap();

    assert_eq!(result1.get::<_, i32>("result"), 1);
    assert_eq!(result2.get::<_, i32>("result"), 2);
}

///////////////////////////////////////////////////////////////////////////////
// StateStore CRUD
///////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn test_store_create_sandbox() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let store = PostgresStateStore::new(pool.clone())
        .await
        .expect("Failed to create store");

    let id = Uuid::new_v4();
    let sandbox = make_test_sandbox(id);

    store
        .create_sandbox(sandbox.clone())
        .await
        .expect("Failed to create sandbox");

    let retrieved = store.get_sandbox(&id).await;
    assert!(retrieved.is_some(), "Sandbox should exist");

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, id);
    assert_eq!(retrieved.config.image, "nginx:latest");
    assert_eq!(retrieved.state, SandboxState::Creating);
}

#[tokio::test]
async fn test_store_get_sandbox_not_found() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let store = PostgresStateStore::new(pool.clone())
        .await
        .expect("Failed to create store");

    let result = store.get_sandbox(&Uuid::new_v4()).await;
    assert!(result.is_none(), "Non-existent sandbox should return None");
}

#[tokio::test]
async fn test_store_list_sandboxes() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let store = PostgresStateStore::new(pool.clone())
        .await
        .expect("Failed to create store");

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    store.create_sandbox(make_test_sandbox(id1)).await.unwrap();
    store.create_sandbox(make_test_sandbox(id2)).await.unwrap();
    store.create_sandbox(make_test_sandbox(id3)).await.unwrap();

    let list = store.list_sandboxes().await;
    let ids: Vec<_> = list.iter().map(|s| s.id).collect();
    assert!(ids.contains(&id1), "List should contain sandbox 1");
    assert!(ids.contains(&id2), "List should contain sandbox 2");
    assert!(ids.contains(&id3), "List should contain sandbox 3");
}

#[tokio::test]
async fn test_store_update_sandbox() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let store = PostgresStateStore::new(pool.clone())
        .await
        .expect("Failed to create store");

    let id = Uuid::new_v4();
    let mut sandbox = make_test_sandbox(id);
    store.create_sandbox(sandbox.clone()).await.unwrap();

    sandbox.state = SandboxState::Running;
    sandbox.container_id = Some("container-123".to_string());
    sandbox.config.image = "nginx:alpine".to_string();
    sandbox.updated_at = Utc::now();

    store
        .update_sandbox(&sandbox)
        .await
        .expect("Failed to update sandbox");

    let retrieved = store.get_sandbox(&id).await.expect("Sandbox should exist");
    assert_eq!(retrieved.state, SandboxState::Running);
    assert_eq!(retrieved.container_id, Some("container-123".to_string()));
    assert_eq!(retrieved.config.image, "nginx:alpine");
}

#[tokio::test]
async fn test_store_delete_sandbox() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let store = PostgresStateStore::new(pool.clone())
        .await
        .expect("Failed to create store");

    let id = Uuid::new_v4();
    store.create_sandbox(make_test_sandbox(id)).await.unwrap();

    assert!(store.get_sandbox(&id).await.is_some());

    store
        .delete_sandbox(&id)
        .await
        .expect("Failed to delete sandbox");

    assert!(store.get_sandbox(&id).await.is_none());
}

#[tokio::test]
async fn test_store_update_nonexistent_sandbox() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let store = PostgresStateStore::new(pool.clone())
        .await
        .expect("Failed to create store");

    let id = Uuid::new_v4();
    let sandbox = make_test_sandbox(id);

    let result = store.update_sandbox(&sandbox).await;
    assert!(result.is_err(), "Updating non-existent sandbox should fail");
}

///////////////////////////////////////////////////////////////////////////////
// ActivityStore
///////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn test_activities_record_and_list() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let activity_store = ActivityStore::new(pool.clone());
    let store = PostgresStateStore::new(pool.clone())
        .await
        .expect("Failed to create store");

    let sandbox_id = Uuid::new_v4();
    store
        .create_sandbox(make_test_sandbox(sandbox_id))
        .await
        .unwrap();

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

    let activities = activity_store
        .list_sandbox_activities(&sandbox_id, 10)
        .await
        .expect("Failed to list activities");

    assert_eq!(activities.len(), 2, "Should have 2 activities");
    assert_eq!(activities[0].activity_type, ActivityType::Exec);
    assert_eq!(activities[1].activity_type, ActivityType::Create);
}

#[tokio::test]
async fn test_activities_list_recent() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let activity_store = ActivityStore::new(pool.clone());
    let store = PostgresStateStore::new(pool.clone())
        .await
        .expect("Failed to create store");

    let sandbox_id1 = Uuid::new_v4();
    let sandbox_id2 = Uuid::new_v4();
    store
        .create_sandbox(make_test_sandbox(sandbox_id1))
        .await
        .unwrap();
    store
        .create_sandbox(make_test_sandbox(sandbox_id2))
        .await
        .unwrap();

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

    let recent = activity_store
        .list_recent_activities(10)
        .await
        .expect("Failed to list recent");

    // On a shared DB there may be more activities; just verify our 8 are present.
    assert!(
        recent.len() >= 8,
        "Should return at least 8 recent activities, got {}",
        recent.len()
    );
}

#[tokio::test]
async fn test_activities_mark_sandbox_deleted() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let activity_store = ActivityStore::new(pool.clone());
    let store = PostgresStateStore::new(pool.clone())
        .await
        .expect("Failed to create store");

    let sandbox_id = Uuid::new_v4();
    store
        .create_sandbox(make_test_sandbox(sandbox_id))
        .await
        .unwrap();

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

    activity_store
        .mark_sandbox_activities_deleted(&sandbox_id)
        .await
        .expect("Failed to mark as deleted");

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

///////////////////////////////////////////////////////////////////////////////
// Filtering & Ownership
///////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn test_store_list_sandboxes_filtered_with_include_deleted() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let store = PostgresStateStore::new(pool.clone())
        .await
        .expect("Failed to create store");

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    store.create_sandbox(make_test_sandbox(id1)).await.unwrap();
    store.create_sandbox(make_test_sandbox(id2)).await.unwrap();

    let id3 = Uuid::new_v4();
    let mut sandbox3 = make_test_sandbox(id3);
    store.create_sandbox(sandbox3.clone()).await.unwrap();
    sandbox3.state = SandboxState::Destroyed;
    sandbox3.deleted_at = Some(Utc::now());
    store
        .update_sandbox(&sandbox3)
        .await
        .expect("Failed to update sandbox to destroyed state");

    let filters_no_deleted = SandboxListFilters {
        include_deleted: false,
        ..Default::default()
    };
    let list_no_deleted = store
        .list_sandboxes_filtered(Some(filters_no_deleted))
        .await
        .expect("Failed to list sandboxes");
    let ids: Vec<_> = list_no_deleted.data.iter().map(|s| s.id).collect();
    assert!(ids.contains(&id1), "Should include active sandbox 1");
    assert!(ids.contains(&id2), "Should include active sandbox 2");
    assert!(!ids.contains(&id3), "Should not include destroyed sandbox");

    let filters_with_deleted = SandboxListFilters {
        include_deleted: true,
        ..Default::default()
    };
    let list_with_deleted = store
        .list_sandboxes_filtered(Some(filters_with_deleted))
        .await
        .expect("Failed to list sandboxes");
    let ids: Vec<_> = list_with_deleted.data.iter().map(|s| s.id).collect();
    assert!(ids.contains(&id1), "Should include sandbox 1");
    assert!(ids.contains(&id2), "Should include sandbox 2");
    assert!(ids.contains(&id3), "Should include destroyed sandbox");
}

#[tokio::test]
async fn test_store_list_sandboxes_owned_by_filters_by_owner() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let api_key_a = Uuid::new_v4();
    let api_key_b = Uuid::new_v4();
    insert_test_api_key(&pool, api_key_a, "owner-a").await;
    insert_test_api_key(&pool, api_key_b, "owner-b").await;
    let store = PostgresStateStore::new(pool.clone())
        .await
        .expect("Failed to create store");

    let sandbox_a1 = make_owned_test_sandbox(Uuid::new_v4(), api_key_a);
    let sandbox_a2 = make_owned_test_sandbox(Uuid::new_v4(), api_key_a);
    let sandbox_b = make_owned_test_sandbox(Uuid::new_v4(), api_key_b);
    let mut deleted_a = make_owned_test_sandbox(Uuid::new_v4(), api_key_a);
    deleted_a.deleted_at = Some(Utc::now());
    deleted_a.state = SandboxState::Destroyed;

    store.create_sandbox(sandbox_a1.clone()).await.unwrap();
    store.create_sandbox(sandbox_a2.clone()).await.unwrap();
    store.create_sandbox(sandbox_b).await.unwrap();
    store.create_sandbox(deleted_a).await.unwrap();

    let owned = store.list_sandboxes_owned_by(&api_key_a, false).await;
    let ids: Vec<_> = owned.iter().map(|sandbox| sandbox.id).collect();

    assert!(
        ids.len() >= 2,
        "Should return at least 2 active sandboxes for owner A"
    );
    assert!(ids.contains(&sandbox_a1.id));
    assert!(ids.contains(&sandbox_a2.id));
    assert!(
        owned.iter().all(|sandbox| sandbox.api_key_id == Some(api_key_a)),
        "All returned sandboxes should belong to owner A"
    );
}

///////////////////////////////////////////////////////////////////////////////
// SSH Session Statistics Queries
///////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn test_ssh_session_statistics_query() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let store = PostgresSshSessionStore::new(pool.clone());

    let stats = store
        .get_session_statistics()
        .await
        .expect("Failed to get session statistics");

    // On a shared DB stats may not be zero — just verify the query succeeds
    // and the struct fields are populated.
    assert!(
        stats.total_sessions >= 0,
        "total_sessions should be non-negative"
    );
    assert!(
        stats.active_sessions >= 0,
        "active_sessions should be non-negative"
    );
}

#[tokio::test]
async fn test_stuck_connecting_sessions_detection() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let store = PostgresSshSessionStore::new(pool.clone());

    let stuck_sessions = store
        .get_stuck_connecting_sessions(30)
        .await
        .expect("Failed to get stuck connecting sessions");

    // On a shared DB there may be stuck sessions; just verify query succeeds.
    let _count = stuck_sessions.len();
}

#[tokio::test]
async fn test_orphaned_sessions_detection() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let store = PostgresSshSessionStore::new(pool.clone());

    let orphaned_sessions = store
        .get_orphaned_sessions()
        .await
        .expect("Failed to get orphaned sessions");

    // On a shared DB there may be orphaned sessions; just verify query succeeds.
    let _count = orphaned_sessions.len();
}

#[tokio::test]
async fn test_stale_sessions_detection() {
    let db = setup_test_db().await;
    let pool = db.pool();
    let store = PostgresSshSessionStore::new(pool.clone());

    let stale_sessions = store
        .get_stale_sessions(300)
        .await
        .expect("Failed to get stale sessions");

    // On a shared DB there may be stale sessions; just verify query succeeds.
    let _count = stale_sessions.len();
}

///////////////////////////////////////////////////////////////////////////////
// SSH Session Lifecycle (DB parts only)
///////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn test_ssh_session_lifecycle() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let db = setup_test_db().await;
    let pool = db.pool();
    let ssh_store = Arc::new(PostgresSshSessionStore::new(pool.clone())) as Arc<dyn SshSessionStoreTrait>;
    let ssh_service = SshSessionService::new(ssh_store);

    let sandbox_id = insert_test_sandbox_for_ssh(&pool, "test-ssh-lifecycle").await?;

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

    ssh_service.update_activity(session.id, 1024, 2048).await?;

    let updated_session = ssh_service.get_session(session.id).await?;
    assert_eq!(updated_session.bytes_sent, 1024);
    assert_eq!(updated_session.bytes_received, 2048);

    ssh_service.disconnect_session(session.id).await?;

    let disconnected_session = ssh_service.get_session(session.id).await?;
    assert_eq!(disconnected_session.state, SshSessionState::Disconnected);
    assert!(disconnected_session.disconnected_at.is_some());
    assert!(disconnected_session.duration_seconds.is_some());

    Ok(())
}

#[tokio::test]
async fn test_list_ssh_sessions_with_filters() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let db = setup_test_db().await;
    let pool = db.pool();
    let ssh_store = Arc::new(PostgresSshSessionStore::new(pool.clone())) as Arc<dyn SshSessionStoreTrait>;
    let ssh_service = SshSessionService::new(ssh_store);

    let sandbox1_id = insert_test_sandbox_for_ssh(&pool, "test-list-sessions-1").await?;
    let sandbox2_id = insert_test_sandbox_for_ssh(&pool, "test-list-sessions-2").await?;

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

    let _session2 = ssh_service
        .create_session(CreateSshSessionRequest {
            sandbox_id: sandbox2_id,
            client_ip: "192.168.1.101".to_string(),
            ssh_version: Some("SSH-2.0-OpenSSH_9.0".to_string()),
            auth_method: SshAuthMethod::ApiKey,
            username: None,
            public_key: None,
        })
        .await?;

    let all_sessions = ssh_service
        .list_sessions(SshSessionFilters::default())
        .await;
    assert!(all_sessions.len() >= 2);

    let sandbox1_sessions = ssh_service
        .list_sessions(SshSessionFilters {
            sandbox_id: Some(sandbox1_id),
            ..Default::default()
        })
        .await;
    assert_eq!(sandbox1_sessions.len(), 1);
    assert_eq!(sandbox1_sessions[0].id, session1.id);

    let connecting_sessions = ssh_service
        .list_sessions(SshSessionFilters {
            state: Some(SshSessionState::Connecting),
            ..Default::default()
        })
        .await;
    assert!(connecting_sessions.len() >= 2);

    Ok(())
}

#[tokio::test]
async fn test_terminate_session_with_reason() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let db = setup_test_db().await;
    let pool = db.pool();
    let ssh_store = Arc::new(PostgresSshSessionStore::new(pool.clone())) as Arc<dyn SshSessionStoreTrait>;
    let ssh_service = SshSessionService::new(ssh_store);

    let sandbox_id = insert_test_sandbox_for_ssh(&pool, "test-terminate-session").await?;

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

    Ok(())
}
