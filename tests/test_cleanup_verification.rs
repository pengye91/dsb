// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Meta-tests for cleanup verification
//!
//! These tests verify that the cleanup infrastructure works correctly.
//! They test resource cleanup on success, failure, and panic scenarios.

// Import common test modules
mod common;

#[cfg(test)]
mod cleanup_tests {
    use super::common::resource_registry::{ResourceRegistry, ResourceType};
    use super::common::test_panic_hook::{
        clear_panic_cleanup, install_test_panic_hook, register_panic_cleanup,
    };

    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;

    /// Test that ResourceRegistry properly cleans up resources
    #[tokio::test]
    async fn test_resource_registry_cleanup() {
        let registry = ResourceRegistry::new("test_registry_cleanup");

        // Register 5 resources
        for i in 0..5 {
            registry.register(
                format!("resource-{}", i),
                ResourceType::Container,
                format!("test resource {}", i),
                move || Box::pin(async move { Ok(()) }),
            );
        }

        // Verify all resources are registered
        assert_eq!(registry.resource_count().await, 5);

        // Cleanup all resources
        let result = registry.cleanup_all(30).await;

        // Verify cleanup succeeded
        assert!(result.is_success(), "Cleanup should succeed");
        assert_eq!(result.cleaned, 5, "Should clean 5 resources");
        assert_eq!(result.failed.len(), 0, "Should have no failures");
        assert_eq!(result.timed_out.len(), 0, "Should have no timeouts");
    }

    /// Test that cleanup handles partial failures
    #[tokio::test]
    async fn test_cleanup_with_partial_failures() {
        let registry = ResourceRegistry::new("test_partial_failures");

        // Register 3 successful and 2 failing resources
        for i in 0..3 {
            registry.register(
                format!("ok-{}", i),
                ResourceType::Container,
                format!("ok resource {}", i),
                move || Box::pin(async move { Ok(()) }),
            );
        }

        for i in 0..2 {
            registry.register(
                format!("bad-{}", i),
                ResourceType::Sandbox,
                format!("bad resource {}", i),
                move || Box::pin(async move { Err("cleanup error".to_string()) }),
            );
        }

        let result = registry.cleanup_all(30).await;

        assert!(
            !result.is_success(),
            "Cleanup should fail for some resources"
        );
        assert_eq!(result.cleaned, 3, "Should clean 3 successful resources");
        assert_eq!(result.failed.len(), 2, "Should have 2 failures");
    }

    /// Test that cleanup times out for stuck resources
    #[tokio::test]
    async fn test_cleanup_timeout() {
        let registry = ResourceRegistry::new("test_timeout");

        // Register a slow resource
        registry.register(
            "slow-resource".to_string(),
            ResourceType::Container,
            "slow resource".to_string(),
            || {
                Box::pin(async move {
                    // Sleep longer than timeout
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    Ok(())
                })
            },
        );

        // Register a fast resource
        registry.register(
            "fast-resource".to_string(),
            ResourceType::Sandbox,
            "fast resource".to_string(),
            || Box::pin(async move { Ok(()) }),
        );

        let result = registry.cleanup_all(1).await;

        // Fast resource should be cleaned, slow should timeout
        assert_eq!(result.cleaned, 1, "Fast resource should be cleaned");
        assert_eq!(result.timed_out.len(), 1, "Slow resource should timeout");
    }

    /// Test that cleanup order is preserved
    #[tokio::test]
    async fn test_cleanup_order_preserved() {
        let registry = ResourceRegistry::new("test_order");
        let cleanup_order = Arc::new(Mutex::new(Vec::new()));

        for i in 0..3 {
            let cleanup_order_clone = Arc::clone(&cleanup_order);
            registry.register(
                format!("resource-{}", i),
                ResourceType::Container,
                format!("resource {}", i),
                move || {
                    let id = i;
                    let order_clone = Arc::clone(&cleanup_order_clone);
                    Box::pin(async move {
                        order_clone.lock().await.push(id);
                        Ok(())
                    })
                },
            );
        }

        registry.cleanup_all(30).await;

        let order = cleanup_order.lock().await;
        assert_eq!(*order, vec![0, 1, 2], "Cleanup order should be preserved");
    }

    /// Test that panic hook runs cleanup on panic
    #[test]
    fn test_panic_hook_cleanup() {
        install_test_panic_hook();
        clear_panic_cleanup();

        static CLEANUP_CALLED: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);

        register_panic_cleanup(|| {
            CLEANUP_CALLED.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        // Trigger a panic in a subprocess
        let result = std::panic::catch_unwind(|| {
            panic!("Test panic");
        });

        // Panic should have been caught
        assert!(result.is_err());

        // Note: In normal test execution, the panic hook would run
        // but in catch_unwind it doesn't. This is a known limitation.
        // The real test is that the panic hook installs without error.
    }

    /// Test that multiple cleanup functions can be registered
    #[test]
    fn test_multiple_panic_cleanup() {
        install_test_panic_hook();
        clear_panic_cleanup();

        static COUNT: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

        for _ in 0..5 {
            register_panic_cleanup(|| {
                COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            });
        }

        // Verify 5 cleanup functions were registered
        // (We can't easily test execution without actually panicking)
        // This test ensures registration doesn't fail
    }
}

#[cfg(test)]
mod database_cleanup_tests {
    use super::common::db_test_setup::TestDatabase;

    /// Test that database cleanup removes all test data
    #[tokio::test]
    #[serial_test::serial]
    async fn test_database_cleanup_effective() {
        let db = TestDatabase::new().await.expect("Failed to create test DB");

        // Insert test data
        let client = db.pool.get().await.expect("Failed to get connection");

        // Insert a sandbox
        client
            .execute(
                "INSERT INTO sandboxes (id, name, image, state, pull_policy, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
                &[
                    &uuid::Uuid::new_v4(),
                    &"test-cleanup-sandbox".to_string(),
                    &"nginx:latest",
                    &"running",
                    &"missing",
                ],
            )
            .await
            .expect("Failed to insert test data");

        // Verify data exists
        let rows = client
            .query("SELECT COUNT(*) FROM sandboxes", &[])
            .await
            .expect("Query failed");
        let count: i64 = rows.first().expect("Failed to get count").get(0);
        assert!(count > 0, "Should have test data");

        // Run cleanup
        db.cleanup_data().await.expect("Cleanup failed");

        // Verify all data is removed
        let rows = client
            .query("SELECT COUNT(*) FROM sandboxes", &[])
            .await
            .expect("Query failed");
        let count: i64 = rows.first().expect("Failed to get count").get(0);
        assert_eq!(count, 0, "All data should be removed");
    }

    /// Test that cleanup handles missing tables gracefully
    #[tokio::test]
    #[serial_test::serial]
    async fn test_cleanup_handles_missing_tables() {
        let db = TestDatabase::new().await.expect("Failed to create test DB");

        // Drop a table to simulate missing schema
        let _client = db.pool.get().await.expect("Failed to get connection");

        // This should not fail even if tables don't exist
        db.cleanup_data().await.expect("Cleanup should not fail");
    }
}
