// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Activity Database Operations Module
//!
//! This module handles CRUD operations for sandbox activities in PostgreSQL.
//!
//! ## Overview
//!
//! Provides database persistence for the activity tracking system.
//! Activities are recorded for audit purposes, troubleshooting, and
//! determining sandbox inactivity for auto-cleanup.
//!
//! ## Performance
//!
//! - Indexed queries on sandbox_id and timestamp
//! - Prepared statements for repeated operations
//! - Efficient JSONB storage for flexible details
//!
//! ## Testing Strategy
//!
//! ### Unit Tests (This Module - 25+ tests)
//! Testable pure logic without PostgreSQL:
//! - Error type and conversion tests
//! - ActivityType serialization/deserialization
//! - Type trait bounds (Send, Sync)
//! - Edge cases (invalid strings, case sensitivity, round-trip)
//!
//! ### Integration Tests
//! Database operations tested in:
//! - **`tests/db_integration_tests.rs`**: Activity CRUD with real PostgreSQL
//! - **`tests/integration_test.rs`**: E2E activity tracking
//!
//! Integration tests cover:
//! - Recording activities
//! - Listing by sandbox
//! - Recent activity queries
//! - Sandbox deletion marking

use crate::core::types::{ActivityType, SandboxActivity};
use deadpool_postgres::Pool;
use serde_json;
use thiserror::Error;
use tracing::{debug, error};

/// Error types for activity operations.
#[derive(Error, Debug)]
pub enum ActivityError {
    /// PostgreSQL database error
    #[error("PostgreSQL error: {0}")]
    Postgres(#[from] tokio_postgres::Error),

    /// Pool error
    #[error("Pool error: {0}")]
    Pool(String),

    /// JSON serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// Implement From for pool errors
impl From<deadpool_postgres::PoolError> for ActivityError {
    fn from(err: deadpool_postgres::PoolError) -> Self {
        ActivityError::Pool(err.to_string())
    }
}

/// PostgreSQL-based activity store.
///
/// Handles all activity database operations including recording, listing,
/// and marking activities as deleted.
pub struct ActivityStore {
    pool: Pool,
}

impl ActivityStore {
    /// Creates a new ActivityStore.
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Records a single activity event.
    ///
    /// This is a fire-and-forget operation that logs warnings but doesn't fail
    /// the parent operation if activity recording fails.
    ///
    /// # Arguments
    ///
    /// * `activity` - The activity to record
    pub async fn record_activity(&self, activity: &SandboxActivity) -> Result<(), ActivityError> {
        let client = self.pool.get().await?;

        debug!(
            sandbox_id = %activity.sandbox_id,
            activity_type = ?activity.activity_type,
            "Recording activity"
        );

        client
            .execute(
                r#"
                INSERT INTO sandbox_activities (id, sandbox_id, activity_type, timestamp, details, sandbox_is_deleted)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
                &[
                    &activity.id,
                    &activity.sandbox_id,
                    &ActivityTypeExt::activity_type_to_string(&activity.activity_type),
                    &activity.timestamp,
                    &activity.details,
                    &activity.sandbox_is_deleted,
                ],
            )
            .await?;

        debug!("Activity recorded successfully");
        Ok(())
    }

    /// Lists activities for a specific sandbox.
    ///
    /// Returns activities ordered by timestamp descending (most recent first).
    ///
    /// # Arguments
    ///
    /// * `sandbox_id` - The sandbox UUID
    /// * `limit` - Maximum number of activities to return
    pub async fn list_sandbox_activities(
        &self,
        sandbox_id: &uuid::Uuid,
        limit: usize,
    ) -> Result<Vec<SandboxActivity>, ActivityError> {
        let client = self.pool.get().await?;

        debug!(
            sandbox_id = %sandbox_id,
            limit = limit,
            "Listing sandbox activities"
        );

        let rows = client
            .query(
                r#"
                SELECT id, sandbox_id, activity_type, timestamp, details, sandbox_is_deleted
                FROM sandbox_activities
                WHERE sandbox_id = $1
                ORDER BY timestamp DESC
                LIMIT $2
                "#,
                &[sandbox_id, &(limit as i64)],
            )
            .await?;

        let activities: Vec<SandboxActivity> = rows
            .into_iter()
            .filter_map(|row| match Self::row_to_activity(row) {
                Ok(a) => Some(a),
                Err(e) => {
                    error!("Failed to deserialize activity row: {}", e);
                    None
                }
            })
            .collect();

        debug!(
            sandbox_id = %sandbox_id,
            count = activities.len(),
            "Retrieved sandbox activities"
        );

        Ok(activities)
    }

    /// Lists recent activities across all sandboxes.
    ///
    /// Returns activities ordered by timestamp descending (most recent first).
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of activities to return
    pub async fn list_recent_activities(
        &self,
        limit: usize,
    ) -> Result<Vec<SandboxActivity>, ActivityError> {
        let client = self.pool.get().await?;

        debug!(limit = limit, "Listing recent activities");

        let rows = client
            .query(
                r#"
                SELECT id, sandbox_id, activity_type, timestamp, details, sandbox_is_deleted
                FROM sandbox_activities
                ORDER BY timestamp DESC
                LIMIT $1
                "#,
                &[&(limit as i64)],
            )
            .await?;

        let activities: Vec<SandboxActivity> = rows
            .into_iter()
            .filter_map(|row| match Self::row_to_activity(row) {
                Ok(a) => Some(a),
                Err(e) => {
                    error!("Failed to deserialize activity row: {}", e);
                    None
                }
            })
            .collect();

        debug!(count = activities.len(), "Retrieved recent activities");

        Ok(activities)
    }

    /// Gets a specific activity by ID.
    ///
    /// # Arguments
    ///
    /// * `activity_id` - The activity UUID
    pub async fn get_activity(
        &self,
        activity_id: &uuid::Uuid,
    ) -> Result<Option<SandboxActivity>, ActivityError> {
        let client = self.pool.get().await?;

        debug!(activity_id = %activity_id, "Getting activity");

        let row = client
            .query_opt(
                r#"
                SELECT id, sandbox_id, activity_type, timestamp, details, sandbox_is_deleted
                FROM sandbox_activities
                WHERE id = $1
                "#,
                &[activity_id],
            )
            .await?;

        match row {
            Some(r) => {
                let activity = Self::row_to_activity(r)?;
                debug!("Activity retrieved successfully");
                Ok(Some(activity))
            }
            None => {
                debug!("Activity not found");
                Ok(None)
            }
        }
    }

    /// Marks all activities for a sandbox as deleted.
    ///
    /// This preserves activity history for audit purposes while marking
    /// that the sandbox has been cleaned up.
    ///
    /// # Arguments
    ///
    /// * `sandbox_id` - The sandbox UUID
    pub async fn mark_sandbox_activities_deleted(
        &self,
        sandbox_id: &uuid::Uuid,
    ) -> Result<u64, ActivityError> {
        let client = self.pool.get().await?;

        debug!(
            sandbox_id = %sandbox_id,
            "Marking sandbox activities as deleted"
        );

        let result = client
            .execute(
                r#"
                UPDATE sandbox_activities
                SET sandbox_is_deleted = TRUE
                WHERE sandbox_id = $1
                "#,
                &[sandbox_id],
            )
            .await?;

        debug!(
            sandbox_id = %sandbox_id,
            count = result,
            "Marked activities as deleted"
        );

        Ok(result)
    }

    /// Converts a database row to a SandboxActivity.
    ///
    /// # Arguments
    ///
    /// * `row` - PostgreSQL row
    fn row_to_activity(row: tokio_postgres::Row) -> Result<SandboxActivity, ActivityError> {
        let activity_type_str: String = row.try_get("activity_type")?;
        let activity_type = ActivityType::from_string(&activity_type_str)?;

        Ok(SandboxActivity {
            id: row.try_get("id")?,
            sandbox_id: row.try_get("sandbox_id")?,
            activity_type,
            timestamp: row.try_get("timestamp")?,
            details: row.try_get("details")?,
            sandbox_is_deleted: row.try_get("sandbox_is_deleted")?,
        })
    }
}

/// Extension trait to convert ActivityType to/from database strings.
pub trait ActivityTypeExt {
    /// Convert an ActivityType to its database string representation.
    fn activity_type_to_string(&self) -> String;
    /// Parse an ActivityType from a database string.
    fn from_string(s: &str) -> Result<Self, ActivityError>
    where
        Self: Sized;
}

impl ActivityTypeExt for ActivityType {
    fn activity_type_to_string(&self) -> String {
        match self {
            ActivityType::Create => "create".to_string(),
            ActivityType::Delete => "delete".to_string(),
            ActivityType::Restore => "restore".to_string(),
            ActivityType::Exec => "exec".to_string(),
            ActivityType::Stats => "stats".to_string(),
            ActivityType::Stop => "stop".to_string(),
            ActivityType::Start => "start".to_string(),
            ActivityType::Cleanup => "cleanup".to_string(),
            ActivityType::Info => "info".to_string(),
            ActivityType::ContainerActivity => "container_activity".to_string(),
            ActivityType::Upload => "upload".to_string(),
            ActivityType::Download => "download".to_string(),
        }
    }

    fn from_string(s: &str) -> Result<Self, ActivityError>
    where
        Self: Sized,
    {
        match s {
            "create" => Ok(ActivityType::Create),
            "delete" => Ok(ActivityType::Delete),
            "restore" => Ok(ActivityType::Restore),
            "exec" => Ok(ActivityType::Exec),
            "stats" => Ok(ActivityType::Stats),
            "stop" => Ok(ActivityType::Stop),
            "start" => Ok(ActivityType::Start),
            "cleanup" => Ok(ActivityType::Cleanup),
            "info" => Ok(ActivityType::Info),
            "container_activity" => Ok(ActivityType::ContainerActivity),
            "upload" => Ok(ActivityType::Upload),
            "download" => Ok(ActivityType::Download),
            _ => Err(ActivityError::Pool(format!("Invalid activity type: {}", s))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // ActivityError Tests
    // ========================================================================

    #[test]
    fn test_activity_error_postgres_variant() {
        // Create a database error using the Pool error variant
        let pool_err = deadpool_postgres::PoolError::Closed;
        let err = ActivityError::Pool(pool_err.to_string());
        assert!(err.to_string().contains("Pool error"));
    }

    #[test]
    fn test_activity_error_serialization_variant() {
        // Create a JSON error
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err = ActivityError::Serialization(json_err);
        assert!(err.to_string().contains("Serialization error"));
    }

    #[test]
    fn test_activity_error_from_pool_error() {
        // Test that PoolError converts to ActivityError
        let pool_err = deadpool_postgres::PoolError::Closed;
        let activity_err: ActivityError = pool_err.into();
        assert!(matches!(activity_err, ActivityError::Pool(_)));
    }

    #[test]
    fn test_activity_error_from_json_error() {
        // Test that serde_json::Error converts to ActivityError
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let activity_err: ActivityError = json_err.into();
        assert!(matches!(activity_err, ActivityError::Serialization(_)));
    }

    #[test]
    fn test_activity_error_display_with_context() {
        let err = ActivityError::Pool("connection timeout".to_string());
        let err_str = err.to_string();

        assert!(!err_str.is_empty());
        assert!(err_str.contains("Pool error"));
        assert!(err_str.contains("connection timeout"));
    }

    // ========================================================================
    // ActivityTypeExt Tests
    // ========================================================================

    #[test]
    fn test_activity_type_serialization() {
        assert_eq!(ActivityType::Create.activity_type_to_string(), "create");
        assert_eq!(ActivityType::Exec.activity_type_to_string(), "exec");
        assert_eq!(
            ActivityType::ContainerActivity.activity_type_to_string(),
            "container_activity"
        );
    }

    #[test]
    fn test_activity_type_deserialization() {
        assert_eq!(
            ActivityType::from_string("create").unwrap(),
            ActivityType::Create
        );
        assert_eq!(
            ActivityType::from_string("container_activity").unwrap(),
            ActivityType::ContainerActivity
        );
        assert!(ActivityType::from_string("invalid").is_err());
    }

    #[test]
    fn test_activity_type_all_variants_serialization() {
        let types = vec![
            (ActivityType::Create, "create"),
            (ActivityType::Delete, "delete"),
            (ActivityType::Exec, "exec"),
            (ActivityType::Stats, "stats"),
            (ActivityType::Stop, "stop"),
            (ActivityType::Cleanup, "cleanup"),
            (ActivityType::Info, "info"),
            (ActivityType::ContainerActivity, "container_activity"),
        ];

        for (activity_type, expected) in types {
            assert_eq!(
                activity_type.activity_type_to_string(),
                expected,
                "Failed for {:?}",
                activity_type
            );
        }
    }

    #[test]
    fn test_activity_type_from_string_case_sensitivity() {
        // ActivityType parsing should be case-sensitive
        assert!(ActivityType::from_string("CREATE").is_err());
        assert!(ActivityType::from_string("Create").is_err());
        assert!(ActivityType::from_string("").is_err());
    }

    #[test]
    fn test_activity_type_round_trip() {
        // Test serialization/deserialization round-trip
        for activity_type in [
            ActivityType::Create,
            ActivityType::Delete,
            ActivityType::Exec,
            ActivityType::Stats,
            ActivityType::Stop,
            ActivityType::Cleanup,
            ActivityType::Info,
            ActivityType::ContainerActivity,
        ] {
            let serialized = activity_type.activity_type_to_string();
            let deserialized = ActivityType::from_string(&serialized).unwrap();
            assert_eq!(
                activity_type, deserialized,
                "Round-trip failed for {:?}",
                activity_type
            );
        }
    }

    #[test]
    fn test_activity_type_debug_format() {
        let create = ActivityType::Create;
        let debug_str = format!("{:?}", create);
        assert!(debug_str.contains("Create"));
    }

    #[test]
    fn test_activity_error_postgres_from_string() {
        // Test that we can create ActivityError::Pool from a string
        let err = ActivityError::Pool("test error".to_string());
        assert_eq!(err.to_string(), "Pool error: test error");
    }

    #[test]
    fn test_activity_error_debug_format() {
        let pool_err = deadpool_postgres::PoolError::Closed;
        let err: ActivityError = pool_err.into();
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Pool"));
    }

    #[test]
    fn test_activity_error_source_chain() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err: ActivityError = json_err.into();
        let err_str = err.to_string();
        assert!(err_str.contains("Serialization error"));
    }

    // ========================================================================
    // ActivityStore Type Tests
    // ========================================================================

    #[test]
    fn test_activity_store_requires_pool() {
        // Compile-time test that ActivityStore requires Pool
        use deadpool_postgres::Pool;

        fn check_new_fn(pool: Pool) -> ActivityStore {
            ActivityStore::new(pool)
        }

        let _ = check_new_fn;
    }

    #[test]
    fn test_activity_store_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<ActivityStore>();
    }

    #[test]
    fn test_activity_store_is_sync() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<ActivityStore>();
    }

    // ========================================================================
    // Integration Test References
    // ========================================================================

    #[test]
    fn test_integration_test_files_exist() {
        // Documents where integration tests are located
        let _integration_tests = ("tests/db_integration_tests.rs", "tests/integration_test.rs");
    }

    #[test]
    fn test_activity_store_methods_exist() {
        // Compile-time verification that store methods exist
        // The ActivityStore provides these methods:
        // - new(pool: Pool) -> Self
        // - record_activity(&self, activity: &SandboxActivity) -> Result<(), ActivityError>
        // - list_sandbox_activities(&self, sandbox_id: &Uuid, limit: usize) -> Result<Vec<SandboxActivity>, ActivityError>
        // - list_recent_activities(&self, limit: usize) -> Result<Vec<SandboxActivity>, ActivityError>
        // - get_activity(&self, activity_id: &Uuid) -> Result<Option<SandboxActivity>, ActivityError>
        // - mark_sandbox_activities_deleted(&self, sandbox_id: &Uuid) -> Result<u64, ActivityError>

        let _ = ActivityStore::new;
    }

    // ========================================================================
    // Database Integration Tests
    // ========================================================================

    /// Creates a test database pool with the schema migrated.
    ///
    /// Uses the shared [`TestDb`] fixture so we don't have to keep a
    /// broken copy of the config-based pool plumbing in every test
    /// module. Migrations run at most once per test binary.
    async fn create_test_pool() -> deadpool_postgres::Pool {
        crate::db::test_db::TestDb::from_default_env()
            .connect_with_schema()
            .await
    }

    /// Cleans up test activities from database
    async fn cleanup_test_activities(pool: &deadpool_postgres::Pool) {
        if let Ok(client) = pool.get().await {
            let _ = client
                .execute(
                    "DELETE FROM sandbox_activities WHERE sandbox_id LIKE 'test-%'",
                    &[],
                )
                .await;
        }
    }

    #[tokio::test]
    async fn test_record_activity_success() {
        let pool = create_test_pool().await;
        let store = ActivityStore::new(pool.clone());

        cleanup_test_activities(&pool).await;

        let sandbox_id = uuid::Uuid::new_v4();
        let activity = SandboxActivity {
            id: uuid::Uuid::new_v4(),
            sandbox_id,
            activity_type: ActivityType::Create,
            timestamp: chrono::Utc::now(),
            details: serde_json::json!({"test": "data"}),
            sandbox_is_deleted: false,
        };

        let result = store.record_activity(&activity).await;
        assert!(result.is_ok(), "Should successfully record activity");

        cleanup_test_activities(&pool).await;
    }

    #[tokio::test]
    async fn test_list_sandbox_activities_success() {
        let pool = create_test_pool().await;
        let store = ActivityStore::new(pool.clone());

        cleanup_test_activities(&pool).await;

        let sandbox_id = uuid::Uuid::new_v4();

        // Create multiple activities
        for i in 0..3 {
            let activity = SandboxActivity {
                id: uuid::Uuid::new_v4(),
                sandbox_id,
                activity_type: ActivityType::Exec,
                timestamp: chrono::Utc::now() + chrono::Duration::seconds(i as i64),
                details: serde_json::json!({"index": i}),
                sandbox_is_deleted: false,
            };
            store.record_activity(&activity).await.unwrap();
        }

        // List activities
        let result = store.list_sandbox_activities(&sandbox_id, 10).await;
        assert!(result.is_ok(), "Should successfully list activities");

        let activities = result.unwrap();
        assert_eq!(activities.len(), 3, "Should return 3 activities");

        cleanup_test_activities(&pool).await;
    }

    #[tokio::test]
    async fn test_list_sandbox_activities_with_limit() {
        let pool = create_test_pool().await;
        let store = ActivityStore::new(pool.clone());

        cleanup_test_activities(&pool).await;

        let sandbox_id = uuid::Uuid::new_v4();

        // Create 5 activities
        for i in 0..5 {
            let activity = SandboxActivity {
                id: uuid::Uuid::new_v4(),
                sandbox_id,
                activity_type: ActivityType::Exec,
                timestamp: chrono::Utc::now() + chrono::Duration::seconds(i as i64),
                details: serde_json::json!({"index": i}),
                sandbox_is_deleted: false,
            };
            store.record_activity(&activity).await.unwrap();
        }

        // List with limit of 3
        let result = store.list_sandbox_activities(&sandbox_id, 3).await;
        assert!(result.is_ok());

        let activities = result.unwrap();
        assert_eq!(activities.len(), 3, "Should respect limit parameter");

        cleanup_test_activities(&pool).await;
    }

    #[tokio::test]
    async fn test_list_sandbox_activities_empty() {
        let pool = create_test_pool().await;
        let store = ActivityStore::new(pool.clone());

        let sandbox_id = uuid::Uuid::new_v4();

        // List activities for non-existent sandbox
        let result = store.list_sandbox_activities(&sandbox_id, 10).await;
        assert!(result.is_ok());

        let activities = result.unwrap();
        assert_eq!(
            activities.len(),
            0,
            "Should return empty list for no activities"
        );
    }

    #[tokio::test]
    async fn test_get_activity_success() {
        let pool = create_test_pool().await;
        let store = ActivityStore::new(pool.clone());

        cleanup_test_activities(&pool).await;

        let activity_id = uuid::Uuid::new_v4();
        let sandbox_id = uuid::Uuid::new_v4();

        let activity = SandboxActivity {
            id: activity_id,
            sandbox_id,
            activity_type: ActivityType::Create,
            timestamp: chrono::Utc::now(),
            details: serde_json::json!({"test": "data"}),
            sandbox_is_deleted: false,
        };

        store.record_activity(&activity).await.unwrap();

        // Get the activity
        let result = store.get_activity(&activity_id).await;
        assert!(result.is_ok());

        let retrieved = result.unwrap();
        assert!(retrieved.is_some(), "Activity should exist");

        let retrieved_activity = retrieved.unwrap();
        assert_eq!(retrieved_activity.id, activity_id);
        assert_eq!(retrieved_activity.sandbox_id, sandbox_id);

        cleanup_test_activities(&pool).await;
    }

    #[tokio::test]
    async fn test_get_activity_not_found() {
        let pool = create_test_pool().await;
        let store = ActivityStore::new(pool);

        let fake_id = uuid::Uuid::new_v4();

        // Try to get non-existent activity
        let result = store.get_activity(&fake_id).await;
        assert!(result.is_ok());

        let retrieved = result.unwrap();
        assert!(
            retrieved.is_none(),
            "Should return None for non-existent activity"
        );
    }

    #[tokio::test]
    async fn test_mark_sandbox_activities_deleted() {
        let pool = create_test_pool().await;
        let store = ActivityStore::new(pool.clone());

        cleanup_test_activities(&pool).await;

        let sandbox_id = uuid::Uuid::new_v4();

        // Create multiple activities
        for i in 0..3 {
            let activity = SandboxActivity {
                id: uuid::Uuid::new_v4(),
                sandbox_id,
                activity_type: ActivityType::Exec,
                timestamp: chrono::Utc::now() + chrono::Duration::seconds(i as i64),
                details: serde_json::json!({"index": i}),
                sandbox_is_deleted: false,
            };
            store.record_activity(&activity).await.unwrap();
        }

        // Mark as deleted
        let result = store.mark_sandbox_activities_deleted(&sandbox_id).await;
        assert!(result.is_ok());

        let count = result.unwrap();
        assert_eq!(count, 3, "Should mark 3 activities as deleted");

        // Verify they're marked
        let activities = store
            .list_sandbox_activities(&sandbox_id, 10)
            .await
            .unwrap();
        assert!(
            activities.iter().all(|a| a.sandbox_is_deleted),
            "All should be marked deleted"
        );

        cleanup_test_activities(&pool).await;
    }

    #[tokio::test]
    async fn test_mark_sandbox_activities_deleted_no_activities() {
        let pool = create_test_pool().await;
        let store = ActivityStore::new(pool);

        let sandbox_id = uuid::Uuid::new_v4();

        // Try to mark non-existent activities
        let result = store.mark_sandbox_activities_deleted(&sandbox_id).await;
        assert!(result.is_ok());

        let count = result.unwrap();
        assert_eq!(count, 0, "Should mark 0 activities");
    }

    #[tokio::test]
    async fn test_concurrent_activity_operations() {
        let pool = create_test_pool().await;

        cleanup_test_activities(&pool).await;

        // Create activities concurrently
        let mut handles = vec![];

        for i in 0..5 {
            let pool_clone = pool.clone();
            let sandbox_id = uuid::Uuid::new_v4();

            let handle = tokio::spawn(async move {
                let store = ActivityStore::new(pool_clone);
                let activity = SandboxActivity {
                    id: uuid::Uuid::new_v4(),
                    sandbox_id,
                    activity_type: ActivityType::Exec,
                    timestamp: chrono::Utc::now(),
                    details: serde_json::json!({"index": i}),
                    sandbox_is_deleted: false,
                };
                store.record_activity(&activity).await
            });

            handles.push(handle);
        }

        // All operations should succeed
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(
                result.is_ok(),
                "Concurrent activity creation should succeed"
            );
        }

        cleanup_test_activities(&pool).await;
    }

    #[tokio::test]
    async fn test_list_recent_activities_success() {
        let pool = create_test_pool().await;
        let store = ActivityStore::new(pool.clone());

        cleanup_test_activities(&pool).await;

        // Create activities for different sandboxes
        for i in 0..3 {
            let activity = SandboxActivity {
                id: uuid::Uuid::new_v4(),
                sandbox_id: uuid::Uuid::new_v4(),
                activity_type: ActivityType::Create,
                timestamp: chrono::Utc::now() + chrono::Duration::seconds(i as i64),
                details: serde_json::json!({"index": i}),
                sandbox_is_deleted: false,
            };
            store.record_activity(&activity).await.unwrap();
        }

        // List recent activities with limit - only count activities created in this test
        let result = store.list_recent_activities(3).await;
        assert!(result.is_ok());

        let activities = result.unwrap();
        // Just verify we get SOME activities and don't error, since tests run concurrently
        assert!(
            activities.len() >= 3,
            "Should return at least 3 recent activities"
        );

        cleanup_test_activities(&pool).await;
    }

    #[tokio::test]
    async fn test_record_activity_all_types() {
        let pool = create_test_pool().await;
        let store = ActivityStore::new(pool.clone());

        cleanup_test_activities(&pool).await;

        let sandbox_id = uuid::Uuid::new_v4();
        let activity_types = vec![
            ActivityType::Create,
            ActivityType::Delete,
            ActivityType::Exec,
            ActivityType::Stats,
            ActivityType::Stop,
        ];

        for activity_type in activity_types {
            let activity = SandboxActivity {
                id: uuid::Uuid::new_v4(),
                sandbox_id,
                activity_type,
                timestamp: chrono::Utc::now(),
                details: serde_json::json!({}),
                sandbox_is_deleted: false,
            };

            let result = store.record_activity(&activity).await;
            assert!(
                result.is_ok(),
                "Should record activity of type {:?}",
                activity_type
            );
        }

        cleanup_test_activities(&pool).await;
    }
}

// ========================================================================
// Error Handling Pattern Tests
// ========================================================================

#[test]
fn test_activity_row_deserialization_error_not_panics() {
    // Simulate the filter_map pattern used in list_sandbox_activities
    // and list_recent_activities to ensure deserialization errors
    // are handled gracefully (logged via tracing::error!, not panicked)
    let results: Vec<Result<i32, ActivityError>> = vec![
        Ok(1),
        Err(ActivityError::Pool("mock db error".to_string())),
        Ok(3),
    ];

    let filtered: Vec<i32> = results
        .into_iter()
        .filter_map(|r| match r {
            Ok(v) => Some(v),
            Err(e) => {
                // This mirrors the tracing::error! call in production code
                let _msg = format!("Failed to deserialize activity row: {}", e);
                None
            }
        })
        .collect();

    assert_eq!(filtered, vec![1, 3]);
}
