// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Panic hook for cleanup on test failure
//!
//! This module provides a panic hook that ensures cleanup functions run
//! even when tests panic. It's particularly useful for integration tests
//! that create resources (containers, sandboxes, etc.) that need cleanup.
//!
//! # Features
//!
//! - Run cleanup functions on test panic
//! - Verify cleanup succeeded
//! - Log cleanup failures

use std::sync::{Arc, Mutex};

/// Cleanup function to run on panic
type PanicCleanupFn = Box<dyn FnMut() + Send + 'static>;

/// Global registry of cleanup functions to run on panic
static PANIC_CLEANUP_REGISTRY: Mutex<Vec<PanicCleanupFn>> = Mutex::new(Vec::new());

/// Install the test panic hook
///
/// This function replaces the default panic hook with one that runs
/// registered cleanup functions before calling the original panic handler.
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::test_panic_hook::install_test_panic_hook;
///
/// #[tokio::test]
/// async fn my_test() {
///     install_test_panic_hook();
///
///     // Register cleanup function
///     register_panic_cleanup(|| {
///         println!("Cleaning up after panic!");
///     });
///
///     // Test code that might panic
///     panic!("Oh no!");
///     // Cleanup will still run!
/// }
/// ```
pub fn install_test_panic_hook() {
    let original_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        // Run all registered cleanup functions
        let cleanup_functions = {
            let mut registry = PANIC_CLEANUP_REGISTRY.lock().unwrap();
            std::mem::take(&mut *registry)
        };

        tracing::error!(
            panic = ?panic_info,
            "Test panicked - running {} cleanup functions",
            cleanup_functions.len()
        );

        for mut cleanup_fn in cleanup_functions {
            // Run cleanup, ignoring errors (we're already in panic)
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cleanup_fn();
            }));
        }

        // Call original panic hook
        original_hook(panic_info);
    }));
}

/// Register a cleanup function to run on panic
///
/// The cleanup function will be called if the test panics. It should
/// perform best-effort cleanup without panicking itself.
///
/// # Arguments
///
/// * `cleanup_fn` - Function to call on panic
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::test_panic_hook::register_panic_cleanup;
///
/// #[tokio::test]
/// async fn test_with_cleanup() {
///     let container_id = start_container().await;
///
///     // Register cleanup to run even if we panic
///     register_panic_cleanup(|| {
///         // Note: This will run synchronously, so use blocking operations
///         // or spawn background tasks as needed
///         std::println!("Cleaning up container {}", container_id);
///     });
///
///     // Test code that might panic
///     assert!(false, "This will panic!");
/// }
/// ```
pub fn register_panic_cleanup<F>(cleanup_fn: F)
where
    F: FnMut() + Send + 'static,
{
    let mut registry = PANIC_CLEANUP_REGISTRY.lock().unwrap();
    registry.push(Box::new(cleanup_fn));
}

/// Clear all registered panic cleanup functions
///
/// This is useful for cleanup between tests or when you want to
/// explicitly remove cleanup functions.
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::test_panic_hook::clear_panic_cleanup;
///
/// fn after_test() {
///     clear_panic_cleanup();
/// }
/// ```
pub fn clear_panic_cleanup() {
    let mut registry = PANIC_CLEANUP_REGISTRY.lock().unwrap();
    registry.clear();
}

/// Register cleanup function that requires async runtime
///
/// Since panic hooks run synchronously, this spawns a background task
/// to run the async cleanup function.
///
/// # Arguments
///
/// * `cleanup_fn` - Async function to call on panic (accepts Handle)
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::test_panic_hook::register_async_panic_cleanup;
///
/// #[tokio::test]
/// async fn test_with_async_cleanup() {
///     let sandbox_id = create_sandbox().await;
///
///     register_async_panic_cleanup(move |handle| {
///         let sandbox_id = sandbox_id.clone();
///         async move {
///             // Async cleanup here
///             delete_sandbox(&sandbox_id).await;
///         }
///     });
/// }
/// ```
#[allow(dead_code)] // Reserved for future use with async cleanup scenarios
pub fn register_async_panic_cleanup<F, Fut>(cleanup_fn: F)
where
    F: FnOnce(tokio::runtime::Handle) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    // Wrap FnOnce in Option to make it callable multiple times (but only execute once)
    let mut cleanup_fn_opt = Some(cleanup_fn);
    let sync_wrapper = move || {
        if let Some(cleanup_fn) = cleanup_fn_opt.take() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                // Clone the handle to avoid moving it while borrowed
                handle.spawn(cleanup_fn(handle.clone()));
            } else {
                tracing::error!("No tokio runtime available for async panic cleanup");
            }
        }
    };

    register_panic_cleanup(sync_wrapper);
}

/// Wrapper for ResourceRegistry cleanup on panic
///
/// This creates a panic cleanup function that will call cleanup_all()
/// on a ResourceRegistry when a panic occurs.
///
/// # Arguments
///
/// * `registry` - ResourceRegistry to clean up (wrapped in Arc for sharing)
/// * `timeout_secs` - Timeout for cleanup operations
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::resource_registry::ResourceRegistry;
/// use tests::common::test_panic_hook::register_registry_cleanup;
/// use std::sync::Arc;
///
/// #[tokio::test]
/// async fn test_with_registry() {
///     let registry = Arc::new(tokio::sync::Mutex::new(ResourceRegistry::new("my_test")));
///
///     // Register cleanup on panic
///     register_registry_cleanup(registry.clone(), 30);
///
///     // ... register resources ...
///     // ... test code that might panic ...
///
///     // Normal cleanup
///     let result = registry.lock().await.cleanup_all(30).await;
///     assert!(result.is_success());
/// }
/// ```
#[allow(dead_code)]
pub fn register_registry_cleanup(
    registry: Arc<tokio::sync::Mutex<super::resource_registry::ResourceRegistry>>,
    timeout_secs: u64,
) {
    register_async_panic_cleanup(move |_handle| {
        let registry = Arc::clone(&registry);
        let timeout = timeout_secs;

        async move {
            if let Ok(registry_guard) = registry.try_lock() {
                let test_name = registry_guard.test_name().to_string();
                tracing::warn!(
                    test = %test_name,
                    "Running panic cleanup for ResourceRegistry"
                );

                let result = registry_guard.cleanup_all(timeout).await;

                if !result.is_success() {
                    log_cleanup_failures(&result);
                } else {
                    tracing::info!(
                        test = %test_name,
                        cleaned = result.cleaned,
                        "Panic cleanup completed successfully"
                    );
                }
            }
        }
    });
}

/// Wrapper for ResourceRegistry cleanup on panic with verification
///
/// This is like `register_registry_cleanup` but also verifies that all
/// resources were cleaned up after the panic.
///
/// # Arguments
///
/// * `registry` - ResourceRegistry to clean up (wrapped in Arc for sharing)
/// * `timeout_secs` - Timeout for cleanup operations
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::resource_registry::ResourceRegistry;
/// use tests::common::test_panic_hook::register_registry_cleanup_with_verification;
/// use std::sync::Arc;
///
/// #[tokio::test]
/// async fn test_with_registry_verification() {
///     let registry = Arc::new(tokio::sync::Mutex::new(ResourceRegistry::new("my_test")));
///
///     // Register cleanup with verification on panic
///     register_registry_cleanup_with_verification(registry.clone(), 30);
///
///     // ... register resources ...
///     // ... test code that might panic ...
/// }
/// ```
#[allow(dead_code)]
pub fn register_registry_cleanup_with_verification(
    registry: Arc<tokio::sync::Mutex<super::resource_registry::ResourceRegistry>>,
    timeout_secs: u64,
) {
    register_async_panic_cleanup(move |_handle| {
        let registry = Arc::clone(&registry);
        let timeout = timeout_secs;

        async move {
            if let Ok(registry_guard) = registry.try_lock() {
                let test_name = registry_guard.test_name().to_string();
                tracing::warn!(
                    test = %test_name,
                    "Running panic cleanup with verification for ResourceRegistry"
                );

                let result = registry_guard.cleanup_all(timeout).await;

                if !result.is_success() {
                    log_cleanup_failures(&result);
                } else {
                    tracing::info!(
                        test = %test_name,
                        cleaned = result.cleaned,
                        "Panic cleanup completed successfully"
                    );
                }

                // Verify cleanup succeeded
                match registry_guard.verify_cleanup_detailed().await {
                    Ok(()) => {
                        tracing::info!(test = %test_name, "Panic cleanup verification passed");
                    }
                    Err(leaked) => {
                        tracing::error!(
                            test = %test_name,
                            leaked_count = leaked.len(),
                            leaked_resources = ?leaked,
                            "Panic cleanup verification FAILED - resources leaked!"
                        );
                        eprintln!("WARNING: Resources leaked after panic:");
                        for resource in leaked {
                            eprintln!("  - {}", resource);
                        }
                    }
                }
            }
        }
    });
}

/// Log cleanup failures from a CleanupResult
///
/// This is a helper function to log detailed information about cleanup failures.
///
/// # Arguments
///
/// * `result` - CleanupResult to log
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::test_panic_hook::log_cleanup_failures;
///
/// let result = registry.cleanup_all(30).await;
/// if !result.is_success() {
///     log_cleanup_failures(&result);
/// }
/// ```
#[allow(dead_code)]
pub fn log_cleanup_failures(result: &super::resource_registry::CleanupResult) {
    if !result.failed.is_empty() {
        tracing::error!(
            failed_count = result.failed.len(),
            failures = ?result.failed,
            "Cleanup failed for {} resources",
            result.failed.len()
        );
        eprintln!("FAILED to clean up {} resources:", result.failed.len());
        for (id, error) in &result.failed {
            eprintln!("  - {}: {}", id, error);
        }
    }

    if !result.timed_out.is_empty() {
        tracing::error!(
            timeout_count = result.timed_out.len(),
            timed_out = ?result.timed_out,
            "Cleanup timed out for {} resources",
            result.timed_out.len()
        );
        eprintln!(
            "TIMED OUT cleaning up {} resources:",
            result.timed_out.len()
        );
        for id in &result.timed_out {
            eprintln!("  - {}", id);
        }
    }
}

/// Setup panic hook with ResourceRegistry verification
///
/// This is a convenience function that installs the panic hook and registers
/// cleanup with verification in one call.
///
/// # Arguments
///
/// * `registry` - ResourceRegistry to clean up (wrapped in Arc for sharing)
/// * `timeout_secs` - Timeout for cleanup operations
///
/// # Example
///
/// ```rust,no_run,ignore
/// use tests::common::resource_registry::ResourceRegistry;
/// use tests::common::test_panic_hook::setup_panic_hook_with_verification;
/// use std::sync::Arc;
///
/// #[tokio::test]
/// async fn test_with_setup() {
///     let registry = Arc::new(tokio::sync::Mutex::new(ResourceRegistry::new("my_test")));
///
///     // Setup panic hook with verification in one call
///     setup_panic_hook_with_verification(registry.clone(), 30);
///
///     // ... register resources ...
///     // ... test code ...
/// }
/// ```
#[allow(dead_code)]
pub fn setup_panic_hook_with_verification(
    registry: Arc<tokio::sync::Mutex<super::resource_registry::ResourceRegistry>>,
    timeout_secs: u64,
) {
    install_test_panic_hook();
    register_registry_cleanup_with_verification(registry, timeout_secs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_panic_hook() {
        // Should not panic
        install_test_panic_hook();
    }

    #[test]
    fn test_register_cleanup() {
        install_test_panic_hook();

        static CLEANUP_CALLED: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);

        register_panic_cleanup(|| {
            CLEANUP_CALLED.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        clear_panic_cleanup();
    }

    #[test]
    fn test_clear_cleanup() {
        install_test_panic_hook();

        register_panic_cleanup(|| {});
        register_panic_cleanup(|| {});

        clear_panic_cleanup();

        let registry = PANIC_CLEANUP_REGISTRY.lock().unwrap();
        assert_eq!(registry.len(), 0);
    }
}
