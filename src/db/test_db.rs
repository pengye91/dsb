// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Test fixture for spinning up a real Postgres connection in unit tests.
//!
//! Several `#[cfg(test)] mod tests` blocks in this crate need a
//! `deadpool_postgres::Pool` to exercise the SQL-touching code paths
//! (state store, session token store, activity store, ...). The
//! production config-loading path has a known issue where
//! `DSB_DATABASE__*` env vars don't always reach the deadpool config,
//! so historically each test module rolled its own ad-hoc helper that
//! tried to thread the values through `config::load_for_tests()`. Those
//! helpers were the root cause of the 28 unit-test failures we kept
//! papering over with `|| true` in CI.
//!
//! This module is the single, shared fixture:
//!
//! - [`TestDb`] holds the connection parameters.
//! - [`TestDb::from_default_env`] reads `DSB_DATABASE__*` env vars
//!   directly (with sensible fallbacks for both inside-Docker and
//!   local-dev scenarios) — no `config` crate involved.
//! - [`TestDb::connect`] returns a pool, no migrations.
//! - [`TestDb::connect_with_schema`] returns a pool and ensures the
//!   schema exists. Migrations are idempotent and are run at most once
//!   per test binary via a process-wide static guard, so calling it
//!   from every test is cheap.
//!
//! Usage in a test module:
//!
//! ```ignore
//! use crate::db::test_db::TestDb;
//!
//! #[tokio::test]
//! async fn my_test() {
//!     let pool = TestDb::from_default_env().connect_with_schema().await;
//!     // ... use the pool
//! }
//! ```

use deadpool_postgres::Pool;

/// Connection parameters for a test database pool.
#[derive(Debug, Clone)]
pub struct TestDb {
    /// Postgres hostname (e.g. `127.0.0.1` or `postgres-test`).
    pub host: String,
    /// Postgres port.
    pub port: u16,
    /// Database name.
    pub name: String,
    /// Database user.
    pub user: String,
    /// Database password.
    pub password: String,
}

impl TestDb {
    /// Read connection parameters from `DSB_DATABASE__*` env vars, falling
    /// back to sensible defaults that match the project's
    /// `docker-compose.test.yml` when running inside Docker, or a
    /// locally-running Postgres otherwise.
    ///
    /// The detection logic is intentionally simple: presence of the
    /// `/.dockerenv` file (or the `INSIDE_DOCKER` env var) is taken as a
    /// strong signal that we are inside a container, in which case the
    /// default service name is `postgres-test` and the port is `5432`.
    /// Outside Docker we default to `127.0.0.1:5433`, which matches the
    /// `make test` setup (the host port mapped to the postgres-test
    /// container's `5432`).
    pub fn from_default_env() -> Self {
        let in_docker =
            std::env::var("INSIDE_DOCKER").is_ok() || std::path::Path::new("/.dockerenv").exists();

        let (default_host, default_port) = if in_docker {
            ("postgres-test", 5432)
        } else {
            ("127.0.0.1", 5433)
        };

        Self {
            host: std::env::var("DSB_DATABASE__HOST").unwrap_or_else(|_| default_host.to_string()),
            port: std::env::var("DSB_DATABASE__PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(default_port),
            name: std::env::var("DSB_DATABASE__NAME").unwrap_or_else(|_| "dsb_test".to_string()),
            user: std::env::var("DSB_DATABASE__USER")
                .unwrap_or_else(|_| "postgres_test".to_string()),
            password: std::env::var("DSB_DATABASE__PASSWORD")
                .unwrap_or_else(|_| "postgres_test_password".to_string()),
        }
    }

    /// Build a deadpool connection pool from these parameters.
    pub fn connect(&self) -> Pool {
        let mut pg_config = deadpool_postgres::Config::new();
        pg_config.host = Some(self.host.clone());
        pg_config.port = Some(self.port);
        pg_config.dbname = Some(self.name.clone());
        pg_config.user = Some(self.user.clone());
        pg_config.password = Some(self.password.clone());

        pg_config
            .create_pool(
                Some(deadpool_postgres::Runtime::Tokio1),
                tokio_postgres::NoTls,
            )
            .expect("Failed to create test pool")
    }

    /// Build a pool **and** ensure the schema exists. Migrations are
    /// idempotent (`CREATE TABLE IF NOT EXISTS`, `CREATE EXTENSION IF NOT
    /// EXISTS`) and are run at most once per test binary via a
    /// process-wide static guard, so calling this from every test is
    /// cheap.
    pub async fn connect_with_schema(&self) -> Pool {
        let pool = self.connect();
        static MIGRATED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
        MIGRATED
            .get_or_init(|| async {
                if let Err(e) = crate::db::migration::run_migrations(&pool).await {
                    panic!("Failed to run test migrations: {e}");
                }
            })
            .await;
        pool
    }
}
