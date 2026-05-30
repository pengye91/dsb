// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Activity API Handlers
//!
//! HTTP handlers for activity tracking endpoints.

use crate::{
    api::{
        auth::{ApiKeyIdentity, ApiKeyType},
        ApiError, ErrorCode,
    },
    core::{ActivityService, SandboxService},
};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Query parameters for listing activities.
#[derive(Debug, Deserialize)]
pub struct ListActivitiesQuery {
    /// Filter by sandbox ID (optional)
    pub sandbox_id: Option<uuid::Uuid>,

    /// Filter by activity type (optional)
    pub activity_type: Option<String>,

    /// Maximum number of activities to return
    pub limit: Option<usize>,
}

/// Query parameters for cleanup operations.
#[derive(Debug, Deserialize)]
pub struct CleanupQuery {
    /// Dry-run mode (don't actually delete)
    pub dry_run: Option<bool>,

    /// Inactivity timeout in minutes
    pub timeout: Option<u64>,
}

/// Response for cleanup operations.
#[derive(Debug, Serialize, Deserialize)]
pub struct CleanupResponse {
    /// Human-readable status message
    pub message: String,
    /// Number of sandboxes cleaned up
    pub cleaned: u64,
    /// Whether this was a dry-run (no actual deletions)
    pub dry_run: bool,
}

fn privileged_only_response(message: &str) -> Response {
    ApiError::Validation {
        message: message.to_string(),
        field: None,
        code: ErrorCode::AuthorizationInsufficientPermissions,
    }
    .into_response()
}

fn is_privileged(identity: &ApiKeyIdentity) -> bool {
    matches!(identity.key_type, ApiKeyType::Privileged)
}

/// Lists all activities or activities for a specific sandbox.
///
/// # Query Parameters
///
/// - `sandbox_id`: Optional UUID to filter by sandbox
/// - `limit`: Maximum number of activities to return (default: 100)
/// - `activity_type`: Optional filter by activity type
pub async fn list_activities(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Query(params): Query<ListActivitiesQuery>,
) -> Response {
    if let Some(sandbox_id) = params.sandbox_id {
        if !is_privileged(&identity) {
            if let Err(e) = service
                .check_sandbox_ownership(&identity, &sandbox_id)
                .await
            {
                return e.into_response();
            }
        }
    } else if !is_privileged(&identity) {
        return privileged_only_response("Listing all activities requires a privileged API key");
    }

    // Get activity service if available
    let activity_service: &Arc<ActivityService> = match service.get_activity_service() {
        Some(service) => service,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Activity tracking requires PostgreSQL backend",
            )
                .into_response();
        }
    };

    let limit = params.limit.unwrap_or(100);

    let result = if let Some(sandbox_id) = params.sandbox_id {
        activity_service
            .list_sandbox_activities(&sandbox_id, limit)
            .await
    } else {
        activity_service.list_recent_activities(limit).await
    };

    match result {
        Ok(activities) => (StatusCode::OK, Json(activities)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to list activities: {}", e)})),
        )
            .into_response(),
    }
}

/// Gets details of a specific activity.
pub async fn get_activity(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
) -> Response {
    let activity_service: &Arc<ActivityService> = match service.get_activity_service() {
        Some(service) => service,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Activity tracking requires PostgreSQL backend",
            )
                .into_response();
        }
    };

    match activity_service.get_activity(&id).await {
        Ok(Some(activity)) => {
            if !is_privileged(&identity) {
                if let Err(e) = service
                    .check_sandbox_ownership(&identity, &activity.sandbox_id)
                    .await
                {
                    return e.into_response();
                }
            }

            (StatusCode::OK, Json(activity)).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Activity not found").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to get activity: {}", e)})),
        )
            .into_response(),
    }
}

/// Lists activities for a specific sandbox.
pub async fn list_sandbox_activities(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Path(id): Path<uuid::Uuid>,
    Query(params): Query<ListActivitiesQuery>,
) -> Response {
    if !is_privileged(&identity) {
        if let Err(e) = service.check_sandbox_ownership(&identity, &id).await {
            return e.into_response();
        }
    }

    let activity_service: &Arc<ActivityService> = match service.get_activity_service() {
        Some(service) => service,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Activity tracking requires PostgreSQL backend",
            )
                .into_response();
        }
    };

    let limit = params.limit.unwrap_or(100);

    match activity_service.list_sandbox_activities(&id, limit).await {
        Ok(activities) => (StatusCode::OK, Json(activities)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to list sandbox activities: {}", e)})),
        )
            .into_response(),
    }
}

/// Cleans up inactive sandboxes (with optional dry-run).
///
/// # Query Parameters
///
/// - `dry_run`: If true, only report what would be cleaned up
/// - `timeout`: Inactivity threshold in minutes
pub async fn cleanup_inactive_sandboxes(
    State(service): State<Arc<SandboxService>>,
    Extension(identity): Extension<ApiKeyIdentity>,
    Query(params): Query<CleanupQuery>,
) -> Response {
    if !is_privileged(&identity) {
        return privileged_only_response("Cleaning up all sandboxes requires a privileged API key");
    }

    let dry_run = params.dry_run.unwrap_or(false);
    let timeout_minutes = params.timeout.unwrap_or(30);

    // List all sandboxes
    let sandboxes = service.list_sandboxes().await;
    let now = chrono::Utc::now();
    let mut cleaned_count = 0u64;

    for sandbox in sandboxes {
        // Calculate inactivity using MAX of API and container activity
        let last_api = sandbox.activity.last_api_activity;
        let last_container = sandbox.activity.last_container_activity;

        let last_activity = match last_container {
            Some(container_time) if container_time > last_api => container_time,
            _ => last_api,
        };

        let elapsed = now.signed_duration_since(last_activity);
        let elapsed_minutes = elapsed.num_minutes() as u64;

        if elapsed_minutes >= timeout_minutes {
            if dry_run {
                tracing::info!(
                    "[DRY RUN] Would cleanup sandbox {} (inactive for {} minutes)",
                    sandbox.id,
                    elapsed_minutes
                );
            } else {
                tracing::info!(
                    "Cleaning sandbox {} (inactive for {} minutes)",
                    sandbox.id,
                    elapsed_minutes
                );

                if service.cleanup_sandbox(&sandbox.id).await.is_ok() {
                    cleaned_count += 1;
                }
            }
        }
    }

    let response = CleanupResponse {
        message: if dry_run {
            format!(
                "Dry run complete: {} sandboxes would be cleaned up",
                cleaned_count
            )
        } else {
            format!("Cleanup complete: {} sandboxes cleaned", cleaned_count)
        },
        cleaned: cleaned_count,
        dry_run,
    };

    (StatusCode::OK, Json(response)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_list_activities_query_default() {
        let query = ListActivitiesQuery {
            sandbox_id: None,
            activity_type: None,
            limit: None,
        };

        assert!(query.sandbox_id.is_none());
        assert!(query.activity_type.is_none());
        assert!(query.limit.is_none());
    }

    #[test]
    fn test_list_activities_query_with_sandbox_id() {
        let uuid = uuid::Uuid::new_v4();
        let query = ListActivitiesQuery {
            sandbox_id: Some(uuid),
            activity_type: None,
            limit: None,
        };

        assert_eq!(query.sandbox_id, Some(uuid));
    }

    #[test]
    fn test_list_activities_query_serialization() {
        let json = r#"{"sandbox_id":"550e8400-e29b-41d4-a716-446655440000","activity_type":"exec","limit":50}"#;
        let query: ListActivitiesQuery = serde_json::from_str(json).unwrap();

        assert!(query.sandbox_id.is_some());
        assert_eq!(query.activity_type, Some("exec".to_string()));
        assert_eq!(query.limit, Some(50));
    }

    #[test]
    fn test_list_activities_query_with_all_fields() {
        let uuid = uuid::Uuid::new_v4();
        let query = ListActivitiesQuery {
            sandbox_id: Some(uuid),
            activity_type: Some("create".to_string()),
            limit: Some(100),
        };

        assert!(query.sandbox_id.is_some());
        assert_eq!(query.activity_type, Some("create".to_string()));
        assert_eq!(query.limit, Some(100));
    }

    #[test]
    fn test_cleanup_query_default() {
        let query = CleanupQuery {
            dry_run: None,
            timeout: None,
        };

        assert!(query.dry_run.is_none());
        assert!(query.timeout.is_none());
    }

    #[test]
    fn test_cleanup_query_with_dry_run() {
        let query = CleanupQuery {
            dry_run: Some(true),
            timeout: None,
        };

        assert_eq!(query.dry_run, Some(true));
    }

    #[test]
    fn test_cleanup_query_serialization() {
        let json = r#"{"dry_run":true,"timeout":60}"#;
        let query: CleanupQuery = serde_json::from_str(json).unwrap();

        assert_eq!(query.dry_run, Some(true));
        assert_eq!(query.timeout, Some(60));
    }

    #[test]
    fn test_cleanup_response_serialization() {
        let response = CleanupResponse {
            message: "Test cleanup".to_string(),
            cleaned: 5,
            dry_run: false,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Test cleanup"));
        assert!(json.contains("5"));
        assert!(json.contains("false"));
    }

    #[test]
    fn test_cleanup_response_with_dry_run() {
        let response = CleanupResponse {
            message: "Dry run complete".to_string(),
            cleaned: 10,
            dry_run: true,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Dry run complete"));
        assert!(json.contains("10"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_cleanup_response_can_be_deserialized() {
        let json_str = r#"{"message":"Cleanup complete","cleaned":3,"dry_run":false}"#;
        let response: CleanupResponse = serde_json::from_str(json_str).unwrap();

        assert_eq!(response.message, "Cleanup complete");
        assert_eq!(response.cleaned, 3);
        assert!(!response.dry_run);
    }

    #[test]
    fn test_cleanup_response_zero_cleaned() {
        let response = CleanupResponse {
            message: "Nothing to clean".to_string(),
            cleaned: 0,
            dry_run: true,
        };

        assert_eq!(response.cleaned, 0);
        assert!(response.dry_run);
    }

    #[test]
    fn test_cleanup_response_large_numbers() {
        let response = CleanupResponse {
            message: "Large cleanup".to_string(),
            cleaned: u64::MAX,
            dry_run: false,
        };

        assert_eq!(response.cleaned, u64::MAX);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(&u64::MAX.to_string()));
    }

    #[test]
    fn test_list_activities_query_empty_activity_type() {
        let query = ListActivitiesQuery {
            sandbox_id: None,
            activity_type: Some("".to_string()),
            limit: Some(10),
        };

        assert_eq!(query.activity_type, Some("".to_string()));
        assert_eq!(query.limit, Some(10));
    }

    #[test]
    fn test_list_activities_query_zero_limit() {
        let query = ListActivitiesQuery {
            sandbox_id: None,
            activity_type: None,
            limit: Some(0),
        };

        assert_eq!(query.limit, Some(0));
    }

    #[test]
    fn test_list_activities_query_large_limit() {
        let query = ListActivitiesQuery {
            sandbox_id: None,
            activity_type: None,
            limit: Some(usize::MAX),
        };

        assert_eq!(query.limit, Some(usize::MAX));
    }

    #[test]
    fn test_cleanup_query_timeout_boundary_values() {
        let test_cases = vec![0, 1, 30, 60, 1440, u64::MAX];

        for timeout in test_cases {
            let query = CleanupQuery {
                dry_run: None,
                timeout: Some(timeout),
            };

            assert_eq!(query.timeout, Some(timeout));
        }
    }

    #[test]
    fn test_cleanup_response_with_unicode() {
        let response = CleanupResponse {
            message: "清理完成 ✅".to_string(),
            cleaned: 5,
            dry_run: false,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("清理完成"));
        assert!(json.contains("✅"));
    }

    #[test]
    fn test_cleanup_response_with_newlines() {
        let response = CleanupResponse {
            message: "Line 1\nLine 2\nLine 3".to_string(),
            cleaned: 3,
            dry_run: true,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Line 1"));
        assert!(json.contains("Line 2"));
        assert!(json.contains("Line 3"));
    }

    #[test]
    fn test_list_activities_query_invalid_uuid() {
        let json = r#"{"sandbox_id":"not-a-uuid","activity_type":"exec","limit":10}"#;
        let result: Result<ListActivitiesQuery, _> = serde_json::from_str(json);

        // Should fail to deserialize invalid UUID
        assert!(result.is_err());
    }

    #[test]
    fn test_cleanup_query_negative_timeout() {
        let json = r#"{"timeout":-1}"#;
        let result: Result<CleanupQuery, _> = serde_json::from_str(json);

        // u64 can't be negative, so deserialization should fail
        assert!(result.is_err());
    }

    // ========================================================================
    // Additional Edge Case Tests
    // ========================================================================

    #[test]
    fn test_list_activities_query_very_large_limit() {
        let json = r#"{"limit":999999}"#;
        let result: Result<ListActivitiesQuery, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let query = result.unwrap();
        assert_eq!(query.limit, Some(999999));
    }

    #[test]
    fn test_list_activities_query_all_activity_types() {
        let activity_types = vec![
            "create", "delete", "exec", "stats", "stop", "cleanup", "info",
        ];

        for activity_type in activity_types {
            let json = format!(r#"{{"activity_type":"{}"}}"#, activity_type);
            let result: Result<ListActivitiesQuery, _> = serde_json::from_str(&json);
            assert!(
                result.is_ok(),
                "Should deserialize activity type: {}",
                activity_type
            );
        }
    }

    #[test]
    fn test_cleanup_query_with_timeout() {
        let json = r#"{"timeout":300}"#;
        let result: Result<CleanupQuery, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let query = result.unwrap();
        assert_eq!(query.timeout, Some(300));
    }

    #[test]
    fn test_cleanup_query_very_large_timeout() {
        let json = r#"{"timeout":86400}"#; // 24 hours
        let result: Result<CleanupQuery, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let query = result.unwrap();
        assert_eq!(query.timeout, Some(86400));
    }

    #[test]
    fn test_list_activities_query_with_only_sandbox_id() {
        let uuid = uuid::Uuid::new_v4();
        let json = format!(r#"{{"sandbox_id":"{}"}}"#, uuid);
        let result: Result<ListActivitiesQuery, _> = serde_json::from_str(&json);
        assert!(result.is_ok());

        let query = result.unwrap();
        assert!(query.sandbox_id.is_some());
        assert!(query.activity_type.is_none());
        assert!(query.limit.is_none());
    }

    #[test]
    fn test_list_activities_query_with_only_activity_type() {
        let json = r#"{"activity_type":"delete"}"#;
        let result: Result<ListActivitiesQuery, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let query = result.unwrap();
        assert!(query.sandbox_id.is_none());
        assert_eq!(query.activity_type, Some("delete".to_string()));
        assert!(query.limit.is_none());
    }

    #[test]
    fn test_list_activities_query_with_only_limit() {
        let json = r#"{"limit":25}"#;
        let result: Result<ListActivitiesQuery, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let query = result.unwrap();
        assert!(query.sandbox_id.is_none());
        assert!(query.activity_type.is_none());
        assert_eq!(query.limit, Some(25));
    }
}
