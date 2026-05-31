// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Test Database Setup
//!
//! Provides PostgreSQL test database using docker-compose services.
//!
//! This module connects to the postgres-test service from docker-compose.test.yml,
//! eliminating the need for testcontainers and preventing container leaks.

use deadpool_postgres::{Config as DeadpoolConfig, Pool, Runtime};
use std::error::Error;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_postgres::NoTls;

/// All integration tests share one Postgres database; serialize migration runs so parallel
/// `TestDatabase::new()` calls do not race `run_migrations` (sqlx migration errors).
static TEST_DB_MIGRATION_LOCK: Mutex<()> = Mutex::const_new(());

/// A PostgreSQL test database connected to docker-compose service
pub struct TestDatabase {
    /// Connection pool
    pub pool: Pool,
}

impl TestDatabase {
    /// Creates a connection to docker-compose postgres-test service
    ///
    /// # Prerequisites
    ///
    /// Docker compose services must be running:
    /// ```bash
    /// docker compose -f docker-compose.test.yml up -d postgres-test
    /// ```
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tests::db_test_setup::TestDatabase;
    ///
    /// #[tokio::test]
    /// async fn test_with_database() {
    ///     let db = TestDatabase::new().await.expect("Failed to create test DB");
    ///     // Use db.pool for queries
    /// }
    /// ```
    ///
    /// # Environment Variables
    ///
    /// You can customize the connection using `TEST_DATABASE_URL`:
    ///
    /// ```bash
    /// export TEST_DATABASE_URL="postgresql://user:pass@host:port/db"
    /// ```
    ///
    /// Or use DSB configuration variables (takes precedence):
    ///
    /// ```bash
    /// export DSB_DATABASE__HOST=localhost
    /// export DSB_DATABASE__PORT=5432
    /// export DSB_DATABASE__NAME=dsb_test
    /// export DSB_DATABASE__USER=postgres_test
    /// export DSB_DATABASE__PASSWORD=postgres_test_password
    /// ```
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Load test configuration to get database credentials
        // This will load .env.test file
        let _config = dsb::config::load_for_tests();

        // Try TEST_DATABASE_URL first, then fall back to test config logic
        let connection_string = if let Ok(url) = std::env::var("TEST_DATABASE_URL") {
            url
        } else {
            crate::common::test_config::get_test_database_url()
        };

        // Create connection pool
        let mut cfg = DeadpoolConfig::new();
        cfg.url = Some(connection_string.clone());
        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;

        // Wait for database to be ready
        Self::wait_for_ready(&pool).await?;

        // Run DSB database migrations (only if not already applied)
        Self::ensure_migrations(&pool).await?;

        Ok(TestDatabase { pool })
    }

    /// Waits for database to be ready to accept connections
    async fn wait_for_ready(pool: &Pool) -> Result<(), Box<dyn std::error::Error>> {
        let mut attempts = 0;
        let max_attempts = 30;

        while attempts < max_attempts {
            match pool.get().await {
                Ok(_) => return Ok(()),
                Err(_) if attempts < max_attempts - 1 => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    attempts += 1;
                }
                Err(e) => return Err(Box::new(e)),
            }
        }

        Err("Database failed to become ready. \
            Make sure docker-compose services are running: \
            `docker compose -f docker-compose.test.yml up -d postgres-test`"
            .into())
    }

    /// Ensures database migrations have been applied
    ///
    /// This checks if migrations have already been applied to avoid
    /// redundant migration runs, which significantly improves test
    /// performance when multiple TestDatabase instances are created.
    async fn ensure_migrations(pool: &Pool) -> Result<(), Box<dyn std::error::Error>> {
        let _migration_guard = TEST_DB_MIGRATION_LOCK.lock().await;

        // Check if migrations have already been applied
        if let Ok(client) = pool.get().await {
            // Check if the migrations table exists and has data
            let check_result = client
                .query_one(
                    "SELECT EXISTS (
                        SELECT FROM information_schema.tables
                        WHERE table_schema = 'public'
                        AND table_name = '_sqlx_migrations'
                    )",
                    &[],
                )
                .await;

            if let Ok(row) = check_result {
                let migrations_exist: bool = row.get(0);
                if migrations_exist {
                    // Check if we have any migrations applied
                    let count_result = client
                        .query_one("SELECT COUNT(*) FROM _sqlx_migrations", &[])
                        .await;

                    if let Ok(count_row) = count_result {
                        let count: i64 = count_row.get(0);
                        if count > 0 {
                            tracing::debug!(
                                migrations_count = count,
                                "Migrations already applied, skipping"
                            );
                            return Ok(());
                        }
                    }
                }
            }
        }

        // Migrations not applied or couldn't check, run them
        tracing::debug!("Running database migrations...");
        dsb::db::migration::run_migrations(pool)
            .await
            .map_err(|e| format!("Migration error: {}", e))?;
        tracing::debug!("Database migrations completed");

        Ok(())
    }

    /// Cleans up test data between tests
    ///
    /// Uses CASCADE to automatically handle foreign key dependencies,
    /// making cleanup more robust and future-proof.
    pub async fn cleanup_data(&self) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.pool.get().await?;

        tracing::debug!("Starting database cleanup");

        // Use TRUNCATE with CASCADE for efficient, complete cleanup
        // CASCADE handles foreign key dependencies automatically
        let tables = vec!["ssh_sessions", "activities", "sandboxes", "api_keys"];

        for table in &tables {
            match client
                .execute(&format!("TRUNCATE TABLE {} CASCADE", table), &[])
                .await
            {
                Ok(rows_affected) => {
                    tracing::debug!(
                        table = %table,
                        rows_affected = rows_affected,
                        "Truncated table"
                    );
                }
                Err(e) => {
                    // Check if it's a "table does not exist" error (PostgreSQL error code 42P01)
                    // We can detect this by checking if the error source is a DbError with code 42P01
                    if let Some(db_err) = e.source() {
                        let err_str = format!("{}", db_err);
                        if err_str.contains("does not exist") || err_str.contains("42P01") {
                            tracing::debug!(
                                table = %table,
                                "Table does not exist, skipping"
                            );
                            continue;
                        }
                    }
                    tracing::error!(
                        table = %table,
                        error = %e,
                        "Failed to truncate table"
                    );
                    return Err(Box::new(e));
                }
            }
        }

        tracing::debug!("Database cleanup completed successfully");
        Ok(())
    }

    /// Drops all tables and recreates schema (for clean slate)
    #[allow(dead_code)]
    pub async fn reset_schema(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.cleanup_data().await?;
        Ok(())
    }

    /// Gets a fresh database connection from the pool
    #[allow(dead_code)]
    pub async fn get_connection(
        &self,
    ) -> Result<deadpool_postgres::Object, Box<dyn std::error::Error>> {
        Ok(self.pool.get().await?)
    }

    /// Register this database's cleanup with a ResourceRegistry
    ///
    /// This method registers the database cleanup function with a
    /// ResourceRegistry so that it will be cleaned up along with
    /// other test resources.
    ///
    /// # Arguments
    ///
    /// * `registry` - ResourceRegistry to register with
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tests::common::resource_registry::ResourceRegistry;
    /// use tests::common::db_test_setup::TestDatabase;
    ///
    /// #[tokio::test]
    /// async fn test_with_registry() {
    ///     let registry = ResourceRegistry::new("test_name");
    ///     let db = TestDatabase::new().await.unwrap();
    ///     db.register_with_registry(&registry);
    ///
    ///     // ... test code ...
    ///
    ///     let result = registry.cleanup_all(30).await;
    ///     assert!(result.is_success());
    /// }
    /// ```
    #[allow(dead_code)]
    pub fn register_with_registry(
        &self,
        registry: &crate::common::resource_registry::ResourceRegistry,
    ) {
        let pool = self.pool.clone();

        registry.register(
            format!("testdb-{}", uuid::Uuid::new_v4()),
            crate::common::resource_registry::ResourceType::DatabaseRecord,
            "TestDatabase connection pool".to_string(),
            move || {
                let pool = pool.clone();
                Box::pin(async move {
                    // Run cleanup
                    if let Ok(client) = pool.get().await {
                        let tables = vec!["ssh_sessions", "activities", "sandboxes"];

                        for table in &tables {
                            if let Err(e) = client
                                .execute(&format!("TRUNCATE TABLE {} CASCADE", table), &[])
                                .await
                            {
                                return Err(format!("Failed to truncate {}: {}", table, e));
                            }
                        }
                    }
                    Ok(())
                })
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[serial_test::serial]
    async fn test_create_test_database() {
        if crate::common::using_external_api() {
            eprintln!("Skipping test_create_test_database: requires local Postgres container");
            return;
        }
        let db = TestDatabase::new().await.expect("Failed to create test DB");

        // Verify we can query
        let client = db.pool.get().await.expect("Failed to get connection");
        let rows = client.query("SELECT 1", &[]).await.expect("Query failed");
        let value: i32 = rows.first().expect("Failed to get row").get(0);

        assert_eq!(value, 1);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_cleanup_data() {
        if crate::common::using_external_api() {
            eprintln!("Skipping test_cleanup_data: requires local Postgres container");
            return;
        }
        let db = TestDatabase::new().await.expect("Failed to create test DB");

        // Insert some test data
        let client = db.pool.get().await.expect("Failed to get connection");
        client.execute(
            "INSERT INTO sandboxes (id, name, image, state, pull_policy, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
            &[&uuid::Uuid::new_v4(), &"test".to_string(), &"nginx:latest", &"running", &"missing"]
        ).await.expect("Failed to insert");

        // Cleanup
        db.cleanup_data().await.expect("Failed to cleanup");

        // Verify no data
        let rows = client
            .query("SELECT COUNT(*) FROM sandboxes", &[])
            .await
            .expect("Query failed");
        let count: i64 = rows.first().expect("Failed to get count").get(0);
        assert_eq!(count, 0);
    }
}
