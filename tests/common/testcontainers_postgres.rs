// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Ephemeral PostgreSQL for integration tests via Bollard (Docker API)
//!
//! Replaces the shared docker-compose postgres-test service with a fresh
//! container per test binary. No port conflicts, no shared state, no
//! `docker compose up` prerequisite.
//!
//! Uses Bollard (already a project dependency) instead of testcontainers-rs
//! to avoid version conflicts with bollard-stubs.

#![allow(dead_code)]

use bollard::models::ContainerCreateBody;
use bollard::models::HostConfig;
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, RemoveContainerOptionsBuilder,
};
use bollard::Docker;
use deadpool_postgres::{Config as DeadpoolConfig, Pool, Runtime};
use tokio_postgres::NoTls;

/// An ephemeral PostgreSQL container for a single test binary.
pub struct EphemeralPostgres {
    /// Deadpool connection pool
    pub pool: Pool,
    /// Docker container ID (kept alive for the test duration)
    #[allow(dead_code)]
    container_id: String,
    /// Docker client handle
    #[allow(dead_code)]
    docker: Docker,
    /// Host port mapped to Postgres
    pub host_port: u16,
}

impl EphemeralPostgres {
    /// Start a fresh PostgreSQL container and return a connection pool.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// use tests::common::testcontainers_postgres::EphemeralPostgres;
    ///
    /// #[tokio::test]
    /// async fn test_with_db() {
    ///     let db = EphemeralPostgres::start().await.expect("Failed to start Postgres");
    ///     // Use db.pool for queries
    /// }
    /// ```
    pub async fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let docker = Docker::connect_with_local_defaults()?;

        // Pull image if not present
        let image = "postgres:16-alpine";
        Self::ensure_image(&docker, image).await?;

        let container_name = format!("dsb-test-postgres-{}", uuid::Uuid::new_v4());

        let container = docker
            .create_container(
                Some(
                    CreateContainerOptionsBuilder::new()
                        .name(&container_name)
                        .build(),
                ),
                ContainerCreateBody {
                    image: Some(image.to_string()),
                    env: Some(vec![
                        "POSTGRES_DB=dsb_test".to_string(),
                        "POSTGRES_USER=postgres".to_string(),
                        "POSTGRES_PASSWORD=postgres".to_string(),
                    ]),
                    host_config: Some(HostConfig {
                        port_bindings: Some({
                            let mut bindings = std::collections::HashMap::new();
                            bindings.insert(
                                "5432/tcp".to_string(),
                                Some(vec![bollard::models::PortBinding {
                                    host_ip: Some("127.0.0.1".to_string()),
                                    host_port: Some("0".to_string()), // random port
                                }]),
                            );
                            bindings
                        }),
                        auto_remove: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )
            .await?;

        docker
            .start_container(
                &container.id,
                None::<bollard::query_parameters::StartContainerOptions>,
            )
            .await?;

        // Find the assigned host port
        let inspect = docker
            .inspect_container(
                &container.id,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await?;
        let host_port = inspect
            .network_settings
            .as_ref()
            .and_then(|ns| ns.ports.as_ref())
            .and_then(|ports| ports.get("5432/tcp"))
            .and_then(|binding| binding.as_ref())
            .and_then(|bindings| bindings.first())
            .and_then(|b| b.host_port.as_ref())
            .and_then(|p| p.parse::<u16>().ok())
            .ok_or("Failed to get host port for Postgres container")?;

        let connection_string = format!(
            "postgresql://postgres:postgres@127.0.0.1:{}/dsb_test",
            host_port
        );

        let mut cfg = DeadpoolConfig::new();
        cfg.url = Some(connection_string);
        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;

        // Wait for database to be ready
        Self::wait_for_ready(&pool).await?;

        // Run DSB migrations
        dsb::db::migration::run_migrations(&pool)
            .await
            .map_err(|e| format!("Migration error: {}", e))?;

        Ok(EphemeralPostgres {
            pool,
            container_id: container.id,
            docker,
            host_port,
        })
    }

    /// Wait for the database to accept connections.
    async fn wait_for_ready(pool: &Pool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut attempts = 0;
        let max_attempts = 60;

        while attempts < max_attempts {
            match pool.get().await {
                Ok(_) => return Ok(()),
                Err(_) if attempts < max_attempts - 1 => {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    attempts += 1;
                }
                Err(e) => return Err(Box::new(e)),
            }
        }

        Err("Database failed to become ready within timeout".into())
    }

    /// Pull the image if it doesn't exist locally.
    async fn ensure_image(
        docker: &Docker,
        image: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let image_exists = docker.inspect_image(image).await.is_ok();
        if image_exists {
            return Ok(());
        }

        tracing::info!("Pulling image {} for test database...", image);
        use futures_util::StreamExt;

        let mut stream = docker.create_image(
            Some(CreateImageOptionsBuilder::new().from_image(image).build()),
            None,
            None,
        );

        while let Some(result) = stream.next().await {
            match result {
                Ok(_) => {}
                Err(e) => return Err(format!("Failed to pull image: {}", e).into()),
            }
        }

        Ok(())
    }

    /// Truncate all test tables for a clean slate between tests.
    pub async fn cleanup(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.pool.get().await?;
        let tables = vec!["ssh_sessions", "activities", "sandboxes", "api_keys"];

        for table in &tables {
            if let Err(e) = client
                .execute(&format!("TRUNCATE TABLE {} CASCADE", table), &[])
                .await
            {
                let err_str = format!("{}", e);
                if !err_str.contains("does not exist") && !err_str.contains("42P01") {
                    return Err(Box::new(e));
                }
            }
        }

        Ok(())
    }
}

impl Drop for EphemeralPostgres {
    fn drop(&mut self) {
        // Best-effort container removal on drop
        let docker = self.docker.clone();
        let container_id = self.container_id.clone();
        tokio::spawn(async move {
            let _ = docker
                .remove_container(
                    &container_id,
                    Some(RemoveContainerOptionsBuilder::new().force(true).build()),
                )
                .await;
        });
    }
}
