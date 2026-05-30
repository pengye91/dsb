// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Centralized resource tracking for test cleanup
//!
//! This module provides the ResourceRegistry for tracking and cleaning up
//! test resources (containers, sandboxes, SSH sessions) with proper error
//! handling and timeout support.
//!
//! # Features
//!
//! - Automatic resource cleanup with timeout support
//! - Unique naming for test resources to avoid collisions
//! - Verification that cleanup succeeded
//! - Drop trait for emergency cleanup

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;
use uuid::Uuid;

/// Type of test resource being tracked
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // SshSession and Other are reserved for future use
pub enum ResourceType {
    /// Docker container
    Container,
    /// DSB sandbox
    Sandbox,
    /// SSH session (reserved for future use)
    SshSession,
    /// Database record
    DatabaseRecord,
    /// Generic resource (reserved for future use)
    Other,
}

/// Cleanup future type
pub type CleanupFuture = Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

/// A test resource that needs cleanup
struct TestResource {
    /// Unique identifier for this resource
    id: String,
    /// Type of resource
    resource_type: ResourceType,
    /// Human-readable description
    description: String,
    /// Cleanup function to call
    cleanup_fn: Box<dyn Fn() -> CleanupFuture + Send + Sync>,
}

/// Result of a cleanup operation
#[derive(Debug, Clone)]
pub struct CleanupResult {
    /// Number of resources successfully cleaned
    pub cleaned: usize,
    /// Resources that failed to clean (id -> error message)
    pub failed: Vec<(String, String)>,
    /// Resources that timed out during cleanup
    pub timed_out: Vec<String>,
    /// Total time taken for cleanup
    #[allow(dead_code)] // Reserved for future metrics/analytics
    pub duration_ms: u64,
}

impl CleanupResult {
    /// Returns true if all resources were cleaned up successfully
    pub fn is_success(&self) -> bool {
        self.failed.is_empty() && self.timed_out.is_empty()
    }
}

/// Centralized registry for tracking test resources
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::resource_registry::{ResourceRegistry, ResourceType};
///
/// #[tokio::test]
/// async fn test_with_cleanup() {
///     let registry = ResourceRegistry::new("test_with_cleanup");
///
///     // Register a container for cleanup
///     registry.register(
///         "container-123",
///         ResourceType::Container,
///         "test container".to_string(),
///         || Box::pin(async move {
///             // Cleanup logic here
///             Ok(())
///         })
///     );
///
///     // ... test code ...
///
///     // Clean up all resources
///     let result = registry.cleanup_all(30).await;
///     assert!(result.is_success(), "Cleanup failed: {:?}", result.failed);
/// }
/// ```
pub struct ResourceRegistry {
    /// Resources being tracked
    resources: Arc<Mutex<Vec<TestResource>>>,
    /// Name of the test using this registry
    test_name: String,
}

impl ResourceRegistry {
    /// Create a new resource registry for a test
    ///
    /// # Arguments
    ///
    /// * `test_name` - Name of the test (for logging/debugging)
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// let registry = ResourceRegistry::new("my_test_name");
    /// ```
    pub fn new(test_name: &str) -> Self {
        Self {
            resources: Arc::new(Mutex::new(Vec::new())),
            test_name: test_name.to_string(),
        }
    }

    /// Register a resource for cleanup
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for the resource
    /// * `resource_type` - Type of resource being registered
    /// * `description` - Human-readable description of the resource
    /// * `cleanup_fn` - Async function to call for cleanup (returns Result<(), String>)
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// registry.register(
    ///     "sandbox-abc-123",
    ///     ResourceType::Sandbox,
    ///     "test sandbox".to_string(),
    ///     || Box::pin(async move {
    ///         // Perform cleanup
    ///         delete_sandbox("sandbox-abc-123").await.map_err(|e| e.to_string())
    ///     })
    /// );
    /// ```
    pub fn register<F, Fut>(
        &self,
        id: String,
        resource_type: ResourceType,
        description: String,
        cleanup_fn: F,
    ) where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        let resource = TestResource {
            id,
            resource_type,
            description: description.clone(),
            cleanup_fn: Box::new(move || Box::pin(cleanup_fn()) as CleanupFuture),
        };

        // Register the resource (use try_lock to avoid deadlock in test context)
        if let Ok(mut resources) = self.resources.try_lock() {
            resources.push(resource);
        } else {
            // Log warning but don't fail - this is cleanup infrastructure
            eprintln!(
                "Warning: Failed to register resource '{}' in test '{}': mutex locked",
                description, self.test_name
            );
        }
    }

    /// Clean up all registered resources with a timeout
    ///
    /// # Arguments
    ///
    /// * `timeout_secs` - Maximum time to wait for each cleanup operation
    ///
    /// # Returns
    ///
    /// CleanupResult with statistics about the cleanup operation
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// let result = registry.cleanup_all(30).await;
    /// assert_eq!(result.failed.len(), 0, "Cleanup failures: {:?}", result.failed);
    /// assert_eq!(result.timed_out.len(), 0, "Cleanup timeouts: {:?}", result.timed_out);
    /// ```
    pub async fn cleanup_all(&self, timeout_secs: u64) -> CleanupResult {
        let start_time = std::time::Instant::now();

        // Take all resources (prevents new registrations during cleanup)
        let resources = {
            let mut lock = self.resources.lock().await;
            std::mem::take(&mut *lock)
        };

        let mut cleaned = 0;
        let mut failed = Vec::new();
        let mut timed_out = Vec::new();
        let cleanup_timeout = Duration::from_secs(timeout_secs);

        // Clean up each resource
        for resource in resources {
            let TestResource {
                id,
                resource_type,
                description,
                cleanup_fn,
            } = resource;

            // Attempt cleanup with timeout
            let cleanup_future = (cleanup_fn)();
            let cleanup_result = timeout(cleanup_timeout, cleanup_future).await;

            match cleanup_result {
                Ok(Ok(())) => {
                    cleaned += 1;
                    tracing::debug!(
                        test = %self.test_name,
                        resource_type = ?resource_type,
                        resource_id = %id,
                        "Cleaned up resource: {}",
                        description
                    );
                }
                Ok(Err(e)) => {
                    failed.push((id.clone(), e));
                    tracing::error!(
                        test = %self.test_name,
                        resource_type = ?resource_type,
                        resource_id = %id,
                        error = %failed.last().map(|f| &f.1).unwrap_or(&"unknown".to_string()),
                        "Failed to clean up resource: {}",
                        description
                    );
                }
                Err(_) => {
                    timed_out.push(id.clone());
                    tracing::error!(
                        test = %self.test_name,
                        resource_type = ?resource_type,
                        resource_id = %id,
                        "Timed out cleaning up resource: {} (timeout: {}s)",
                        description,
                        timeout_secs
                    );
                }
            }
        }

        let duration = start_time.elapsed();

        CleanupResult {
            cleaned,
            failed,
            timed_out,
            duration_ms: duration.as_millis() as u64,
        }
    }

    /// Get the number of currently registered resources
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// let count = registry.resource_count().await;
    /// println!("Tracking {} resources", count);
    /// ```
    #[allow(dead_code)] // Used in debugging scenarios
    pub async fn resource_count(&self) -> usize {
        self.resources.lock().await.len()
    }

    /// Get the test name this registry is associated with
    pub fn test_name(&self) -> &str {
        &self.test_name
    }

    /// Generate a unique resource name for testing
    ///
    /// This creates a unique name by combining the test name, resource type,
    /// and a UUID to prevent name collisions across test runs.
    ///
    /// # Arguments
    ///
    /// * `resource_type` - Type of resource (used in the name)
    ///
    /// # Returns
    ///
    /// A unique name string in the format: "test-{test_name}-{resource_type}-{uuid}"
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// let unique_name = registry.generate_unique_name("sandbox");
    /// // Returns: "test_my_test_name_sandbox_123e4567-e89b-12d3-a456-426614174000"
    /// ```
    pub fn generate_unique_name(&self, resource_type: &str) -> String {
        let uuid = Uuid::new_v4();
        format!(
            "test-{}-{}-{}",
            self.test_name.replace("::", "_").replace(" ", "_"),
            resource_type,
            uuid
        )
    }

    /// Generate a unique resource name with a custom suffix
    ///
    /// Like `generate_unique_name` but allows a custom suffix for additional context.
    ///
    /// # Arguments
    ///
    /// * `resource_type` - Type of resource (used in the name)
    /// * `suffix` - Additional suffix to append
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// let unique_name = registry.generate_unique_name_with_suffix("sandbox", "main");
    /// // Returns: "test_my_test_name_sandbox_main_123e4567..."
    /// ```
    pub fn generate_unique_name_with_suffix(&self, resource_type: &str, suffix: &str) -> String {
        let uuid = Uuid::new_v4();
        format!(
            "test-{}-{}-{}-{}",
            self.test_name.replace("::", "_").replace(" ", "_"),
            resource_type,
            suffix,
            uuid
        )
    }

    /// Verify that all resources have been cleaned up
    ///
    /// This checks if any resources are still registered. Should be called
    /// after cleanup_all() to ensure no resources leaked.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - All resources cleaned up
    /// * `Err(count)` - `count` resources still registered (potential leak)
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// registry.cleanup_all(30).await;
    /// registry.verify_cleanup().await.expect("Resources leaked!");
    /// ```
    pub async fn verify_cleanup(&self) -> Result<(), usize> {
        let count = self.resource_count().await;
        if count == 0 {
            Ok(())
        } else {
            tracing::error!(
                test = %self.test_name,
                leaked_count = count,
                "Resource verification failed: {} resources still registered",
                count
            );
            Err(count)
        }
    }

    /// Verify that all resources have been cleaned up with detailed report
    ///
    /// Like `verify_cleanup()` but returns detailed information about leaked resources.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - All resources cleaned up
    /// * `Err(LeakedResources)` - Details about leaked resources
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// registry.cleanup_all(30).await;
    /// if let Err(leaked) = registry.verify_cleanup_detailed().await {
    ///     eprintln!("Leaked resources: {:?}", leaked);
    /// }
    /// ```
    pub async fn verify_cleanup_detailed(&self) -> Result<(), Vec<String>> {
        let resources = self.resources.lock().await;
        if resources.is_empty() {
            Ok(())
        } else {
            let leaked: Vec<String> = resources
                .iter()
                .map(|r| format!("{}: {} ({:?})", r.id, r.description, r.resource_type))
                .collect();

            tracing::error!(
                test = %self.test_name,
                leaked_count = leaked.len(),
                leaked_resources = ?leaked,
                "Resource verification failed: resources still registered"
            );

            Err(leaked)
        }
    }
}

impl Drop for ResourceRegistry {
    /// Automatically cleanup all resources when registry is dropped
    ///
    /// This provides a safety net in case cleanup_all() is not explicitly called.
    /// However, explicit cleanup is preferred for better error reporting.
    fn drop(&mut self) {
        let test_name = self.test_name.clone();
        let resources = Arc::clone(&self.resources);

        // Spawn a background task to cleanup (since Drop is sync)
        let handle = tokio::runtime::Handle::try_current();

        if let Ok(runtime) = handle {
            runtime.spawn(async move {
                let count = resources.lock().await.len();
                if count > 0 {
                    tracing::warn!(
                        test = %test_name,
                        resource_count = count,
                        "ResourceRegistry dropped without explicit cleanup - performing emergency cleanup"
                    );

                    // Perform best-effort cleanup without timeout in Drop
                    let mut lock = resources.lock().await;
                    for resource in lock.drain(..) {
                        let cleanup_future = (resource.cleanup_fn)();
                        let _ = tokio::task::spawn(cleanup_future).await;
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_creation() {
        let registry = ResourceRegistry::new("test_creation");
        assert_eq!(registry.test_name(), "test_creation");
        assert_eq!(registry.resource_count().await, 0);
    }

    #[tokio::test]
    async fn test_register_and_count() {
        let registry = ResourceRegistry::new("test_register");

        registry.register(
            "resource-1".to_string(),
            ResourceType::Container,
            "test resource".to_string(),
            || Box::pin(async { Ok(()) }),
        );

        assert_eq!(registry.resource_count().await, 1);
    }

    #[tokio::test]
    async fn test_cleanup_all_success() {
        let registry = ResourceRegistry::new("test_cleanup_success");

        // Register 3 successful cleanups
        for i in 0..3 {
            registry.register(
                format!("resource-{}", i),
                ResourceType::Container,
                format!("test container {}", i),
                move || Box::pin(async move { Ok(()) }),
            );
        }

        let result = registry.cleanup_all(5).await;

        assert_eq!(result.cleaned, 3);
        assert_eq!(result.failed.len(), 0);
        assert_eq!(result.timed_out.len(), 0);
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn test_cleanup_with_failures() {
        let registry = ResourceRegistry::new("test_cleanup_failure");

        registry.register(
            "resource-ok".to_string(),
            ResourceType::Container,
            "ok resource".to_string(),
            || Box::pin(async { Ok(()) }),
        );

        registry.register(
            "resource-bad".to_string(),
            ResourceType::Sandbox,
            "bad resource".to_string(),
            || Box::pin(async { Err("cleanup failed".to_string()) }),
        );

        let result = registry.cleanup_all(5).await;

        assert_eq!(result.cleaned, 1);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].0, "resource-bad");
        assert!(!result.is_success());
    }

    #[tokio::test]
    async fn test_cleanup_with_timeout() {
        let registry = ResourceRegistry::new("test_cleanup_timeout");

        registry.register(
            "resource-slow".to_string(),
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

        let result = registry.cleanup_all(1).await;

        assert_eq!(result.timed_out.len(), 1);
        assert_eq!(result.timed_out[0], "resource-slow");
        assert!(!result.is_success());
    }

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

        registry.cleanup_all(5).await;

        let order = cleanup_order.lock().await;
        assert_eq!(*order, vec![0, 1, 2]);
    }

    #[tokio::test]
    async fn test_generate_unique_name() {
        let registry = ResourceRegistry::new("test_unique_name");

        let name1 = registry.generate_unique_name("sandbox");
        let name2 = registry.generate_unique_name("sandbox");

        // Names should be different (different UUIDs)
        assert_ne!(name1, name2);

        // Names should start with the expected prefix
        assert!(name1.starts_with("test-test_unique_name-sandbox-"));

        // Names should be valid for Docker containers
        assert!(name1.len() <= 128); // Docker container name limit
    }

    #[tokio::test]
    async fn test_generate_unique_name_with_suffix() {
        let registry = ResourceRegistry::new("test_suffix");

        let name1 = registry.generate_unique_name_with_suffix("sandbox", "main");
        let name2 = registry.generate_unique_name_with_suffix("sandbox", "main");

        // Names should be different
        assert_ne!(name1, name2);

        // Names should contain the suffix
        assert!(name1.contains("-main-"));
    }

    #[tokio::test]
    async fn test_verify_cleanup_success() {
        let registry = ResourceRegistry::new("test_verify_success");

        registry.register(
            "resource-1".to_string(),
            ResourceType::Container,
            "test resource".to_string(),
            || Box::pin(async { Ok(()) }),
        );

        // Clean up
        let result = registry.cleanup_all(5).await;
        assert!(result.is_success());

        // Verify cleanup
        let verify_result = registry.verify_cleanup().await;
        assert!(verify_result.is_ok());
    }

    #[tokio::test]
    async fn test_verify_cleanup_failure() {
        let registry = ResourceRegistry::new("test_verify_failure");

        registry.register(
            "resource-1".to_string(),
            ResourceType::Container,
            "test resource".to_string(),
            || Box::pin(async { Ok(()) }),
        );

        // Don't clean up - verify should fail
        let verify_result = registry.verify_cleanup().await;
        assert!(verify_result.is_err());
        assert_eq!(verify_result.unwrap_err(), 1);
    }

    #[tokio::test]
    async fn test_verify_cleanup_detailed() {
        let registry = ResourceRegistry::new("test_verify_detailed");

        registry.register(
            "container-123".to_string(),
            ResourceType::Container,
            "test container".to_string(),
            || Box::pin(async { Ok(()) }),
        );

        registry.register(
            "sandbox-456".to_string(),
            ResourceType::Sandbox,
            "test sandbox".to_string(),
            || Box::pin(async { Ok(()) }),
        );

        // Don't clean up - verify should fail with details
        let verify_result = registry.verify_cleanup_detailed().await;
        assert!(verify_result.is_err());

        let leaked = verify_result.unwrap_err();
        assert_eq!(leaked.len(), 2);
        assert!(leaked[0].contains("container-123"));
        assert!(leaked[1].contains("sandbox-456"));
    }
}
