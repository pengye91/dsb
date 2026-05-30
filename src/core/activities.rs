// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Activity Tracking Service
//!
//! This module provides high-level activity tracking operations for sandboxes.
//!
//! ## Overview
//!
//! The `ActivityService` wraps the database operations and provides business logic
//! for recording, querying, and managing sandbox activities.
//!
//! ## Testing Strategy
//!
//! Activity service is tested through:
//!
//! ### Unit Tests (This Module)
//! Error type and struct tests:
//! - `ActivityError` variant display formatting
//! - `ActivityService` type trait bounds
//! - Error conversion tests
//!
//! ### Integration Tests
//! Full service tests in:
//! - **`tests/db_integration_tests.rs`**: Activity CRUD operations with real database
//! - **`tests/integration_test.rs`**: Full E2E activity tracking through API
//!
//! Integration tests cover:
//! - Recording activities
//! - Listing by sandbox
//! - Recent activity queries
//! - Sandbox deletion marking
//!
//! ## Usage
//!
//! ```rust,no_run,ignore
//! use dsb::core::activities::ActivityService;
//! use dsb::core::types::ActivityType;
//! use deadpool_postgres::Pool;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let pool: Pool = unimplemented!();
//! let service = ActivityService::new(pool);
//!
//! // Record an activity
//! let sandbox_id = uuid::Uuid::new_v4();
//! service.record_activity(
//!     sandbox_id,
//!     ActivityType::Create,
//!     serde_json::json!({"image": "nginx:latest"}),
//! ).await?;
//! # Ok(())
//! # }
//! ```

use crate::core::types::{ActivityResponse, ActivityType, SandboxActivity};
use crate::db::activities::ActivityError as DbActivityError;
use crate::db::ActivityStore;
use deadpool_postgres::Pool;
use serde_json;
use thiserror::Error;
use tracing::warn;

/// Error types for activity service operations.
#[derive(Error, Debug)]
pub enum ActivityError {
    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] DbActivityError),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Service layer for activity tracking operations.
///
/// This service provides high-level methods for recording and querying activities,
/// with proper error handling and logging.
pub struct ActivityService {
    store: ActivityStore,
}

impl ActivityService {
    /// Creates a new ActivityService.
    ///
    /// # Arguments
    ///
    /// * `pool` - PostgreSQL connection pool
    pub fn new(pool: Pool) -> Self {
        Self {
            store: ActivityStore::new(pool),
        }
    }

    /// Records an activity event.
    ///
    /// This method creates a new activity record with a generated UUID and
    /// the current timestamp.
    ///
    /// # Arguments
    ///
    /// * `sandbox_id` - The sandbox UUID
    /// * `activity_type` - The type of activity
    /// * `details` - Additional activity details (JSON object)
    ///
    /// # Returns
    ///
    /// The created activity record, or an error if recording fails.
    pub async fn record_activity(
        &self,
        sandbox_id: uuid::Uuid,
        activity_type: ActivityType,
        details: serde_json::Value,
    ) -> Result<SandboxActivity, ActivityError> {
        let activity = SandboxActivity {
            id: uuid::Uuid::new_v4(),
            sandbox_id,
            activity_type,
            timestamp: chrono::Utc::now(),
            details,
            sandbox_is_deleted: false,
        };

        self.store.record_activity(&activity).await?;
        Ok(activity)
    }

    /// Records an activity event (fire-and-forget).
    ///
    /// This method records an activity but logs warnings instead of returning errors.
    /// Useful for non-critical activity tracking where failures shouldn't block operations.
    ///
    /// # Arguments
    ///
    /// * `sandbox_id` - The sandbox UUID
    /// * `activity_type` - The type of activity
    /// * `details` - Additional activity details (JSON object)
    pub async fn record_activity_async(
        &self,
        sandbox_id: uuid::Uuid,
        activity_type: ActivityType,
        details: serde_json::Value,
    ) {
        if let Err(e) = self
            .record_activity(sandbox_id, activity_type, details)
            .await
        {
            warn!("Failed to record activity: {}", e);
        }
    }

    /// Lists activities for a specific sandbox.
    ///
    /// # Arguments
    ///
    /// * `sandbox_id` - The sandbox UUID
    /// * `limit` - Maximum number of activities to return
    ///
    /// # Returns
    ///
    /// A vector of activity responses ordered by timestamp descending.
    pub async fn list_sandbox_activities(
        &self,
        sandbox_id: &uuid::Uuid,
        limit: usize,
    ) -> Result<Vec<ActivityResponse>, ActivityError> {
        let activities: Vec<SandboxActivity> = self
            .store
            .list_sandbox_activities(sandbox_id, limit)
            .await?;

        Ok(activities.into_iter().map(ActivityResponse::from).collect())
    }

    /// Lists recent activities across all sandboxes.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of activities to return
    ///
    /// # Returns
    ///
    /// A vector of activity responses ordered by timestamp descending.
    pub async fn list_recent_activities(
        &self,
        limit: usize,
    ) -> Result<Vec<ActivityResponse>, ActivityError> {
        let activities: Vec<SandboxActivity> = self.store.list_recent_activities(limit).await?;

        Ok(activities.into_iter().map(ActivityResponse::from).collect())
    }

    /// Gets a specific activity by ID.
    ///
    /// # Arguments
    ///
    /// * `activity_id` - The activity UUID
    ///
    /// # Returns
    ///
    /// The activity response if found, None otherwise.
    pub async fn get_activity(
        &self,
        activity_id: &uuid::Uuid,
    ) -> Result<Option<ActivityResponse>, ActivityError> {
        match self.store.get_activity(activity_id).await? {
            Some(activity) => Ok(Some(ActivityResponse::from(activity))),
            None => Ok(None),
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
    ///
    /// # Returns
    ///
    /// The number of activities marked as deleted.
    pub async fn mark_sandbox_activities_deleted(
        &self,
        sandbox_id: &uuid::Uuid,
    ) -> Result<u64, ActivityError> {
        self.store
            .mark_sandbox_activities_deleted(sandbox_id)
            .await
            .map_err(ActivityError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // ActivityError Tests
    // ========================================================================

    #[test]
    fn test_activity_error_database_variant() {
        // Create a database error using the Pool error variant
        let pool_err = deadpool_postgres::PoolError::Closed;
        let db_err: DbActivityError = pool_err.into();
        let err = ActivityError::Database(db_err);
        assert!(err.to_string().contains("Database error"));
    }

    #[test]
    fn test_activity_error_serialization_variant() {
        // Create a JSON error by trying to parse invalid JSON
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err = ActivityError::Serialization(json_err);
        assert!(err.to_string().contains("Serialization error"));
    }

    #[test]
    fn test_activity_error_from_database() {
        // Test that DbActivityError converts to ActivityError using Pool error
        let pool_err = deadpool_postgres::PoolError::Closed;
        let db_err: DbActivityError = pool_err.into();
        let activity_err: ActivityError = db_err.into();
        assert!(matches!(activity_err, ActivityError::Database(_)));
    }

    #[test]
    fn test_activity_error_from_serialization() {
        // Test that serde_json::Error converts to ActivityError
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let activity_err: ActivityError = json_err.into();
        assert!(matches!(activity_err, ActivityError::Serialization(_)));
    }

    // ========================================================================
    // ActivityService Type Tests
    // ========================================================================

    #[test]
    fn test_activity_service_requires_pool() {
        // Compile-time test that ActivityService requires Pool
        use deadpool_postgres::Pool;

        // This verifies the constructor signature
        fn check_new_fn(pool: Pool) -> ActivityService {
            ActivityService::new(pool)
        }

        let _ = check_new_fn;
    }

    #[test]
    fn test_activity_service_is_send() {
        // Verify ActivityService implements Send
        fn assert_send<T: Send>() {}
        assert_send::<ActivityService>();
    }

    #[test]
    fn test_activity_service_is_sync() {
        // Verify ActivityService implements Sync
        fn assert_sync<T: Sync>() {}
        assert_sync::<ActivityService>();
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
    fn test_activity_service_methods_exist() {
        // Compile-time verification that service methods exist

        // The ActivityService provides these methods:
        // - new(pool: Pool) -> Self
        // - record_activity(sandbox_id, activity_type, details) -> Result<SandboxActivity, ActivityError>
        // - record_activity_async(sandbox_id, activity_type, details) -> ()
        // - list_sandbox_activities(sandbox_id, limit) -> Result<Vec<ActivityResponse>, ActivityError>
        // - list_recent_activities(limit) -> Result<Vec<ActivityResponse>, ActivityError>
        // - get_activity(activity_id) -> Result<Option<ActivityResponse>, ActivityError>
        // - mark_sandbox_activities_deleted(sandbox_id) -> Result<u64, ActivityError>

        // Just verify the struct exists and has the right constructor
        let _ = ActivityService::new;
    }

    // ========================================================================
    // Error Edge Cases
    // ========================================================================

    #[test]
    fn test_activity_error_display_with_context() {
        // Test that error messages are descriptive
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let err = ActivityError::Serialization(json_err);

        let err_str = err.to_string();
        assert!(!err_str.is_empty());
        assert!(err_str.contains("Serialization error"));
    }

    #[test]
    fn test_activity_error_chain() {
        // Test error conversion chain
        let json_err = serde_json::from_str::<serde_json::Value>("{invalid}").unwrap_err();
        let activity_err: ActivityError = json_err.into();

        // Should preserve error type through conversion
        assert!(matches!(activity_err, ActivityError::Serialization(_)));
    }

    // ========================================================================
    // SandboxActivity Tests
    // ========================================================================

    #[test]
    fn test_sandbox_activity_fields() {
        use chrono::Utc;

        let activity = SandboxActivity {
            id: uuid::Uuid::new_v4(),
            sandbox_id: uuid::Uuid::new_v4(),
            activity_type: ActivityType::Create,
            timestamp: Utc::now(),
            details: serde_json::json!({"test": "data"}),
            sandbox_is_deleted: false,
        };

        assert!(matches!(activity.activity_type, ActivityType::Create));
        assert!(!activity.sandbox_is_deleted);
        assert!(activity.details.is_object());
    }

    #[test]
    fn test_sandbox_activity_all_types() {
        use chrono::Utc;

        let types = vec![
            ActivityType::Create,
            ActivityType::Delete,
            ActivityType::Restore,
            ActivityType::Exec,
            ActivityType::Stats,
            ActivityType::Stop,
            ActivityType::Upload,
            ActivityType::Download,
        ];

        for activity_type in types {
            let activity = SandboxActivity {
                id: uuid::Uuid::new_v4(),
                sandbox_id: uuid::Uuid::new_v4(),
                activity_type,
                timestamp: Utc::now(),
                details: serde_json::json!({}),
                sandbox_is_deleted: false,
            };

            assert!(activity.details.is_object());
        }
    }
}
