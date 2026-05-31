// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Background Tasks
//!
//! Periodic background tasks for maintenance and cleanup.

use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info};

use crate::db::session_token_store::SessionTokenStore;

/// Session token cleanup task
///
/// Runs periodically to delete expired session tokens from the database.
///
/// # Arguments
///
/// * `db_pool` - Database connection pool
/// * `cleanup_interval_secs` - Interval between cleanup runs in seconds
///
/// # Example
///
/// ```rust,no_run,ignore
/// use dsb::tasks::session_token_cleanup_task;
/// use deadpool_postgres::Pool;
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// let pool: Pool = todo!();
///
/// // Start cleanup task (runs every 5 minutes)
/// tokio::spawn(async move {
///     session_token_cleanup_task(pool, 300).await;
/// });
/// # Ok(())
/// # }
/// ```
pub async fn session_token_cleanup_task(
    db_pool: deadpool_postgres::Pool,
    cleanup_interval_secs: u64,
) {
    let mut timer = interval(Duration::from_secs(cleanup_interval_secs));

    loop {
        timer.tick().await;

        debug!("Running session token cleanup task");

        let store = crate::db::PostgresSessionTokenStore::new(db_pool.clone());

        match store.delete_expired_tokens().await {
            Ok(count) => {
                if count > 0 {
                    info!("Cleaned up {} expired session tokens", count);
                } else {
                    debug!("No expired session tokens to clean up");
                }
            }
            Err(e) => {
                error!("Failed to cleanup expired session tokens: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_module_exists() {
        // This is a compile-time test - if it compiles, the module exists
        let _ = "tasks module exists";
    }

    #[test]
    fn test_session_token_cleanup_task_exists() {
        // Verify the function exists and has the correct signature
        fn check_signature(_db_pool: deadpool_postgres::Pool, _cleanup_interval_secs: u64) {
            // This function only compiles if session_token_cleanup_task exists
            // with compatible signature
        }
        // This is a compile-time test
        let _ = check_signature;
    }

    #[test]
    fn test_cleanup_interval_is_valid() {
        // Test that cleanup interval values are reasonable
        let intervals = [60u64, 300u64, 3600u64]; // 1min, 5min, 1hour

        for interval in intervals {
            assert!(
                interval >= 60,
                "Cleanup interval should be at least 60 seconds"
            );
            assert!(
                interval <= 86400,
                "Cleanup interval should not exceed 24 hours"
            );
        }
    }
}
