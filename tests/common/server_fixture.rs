// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Self-contained DSB server fixture for integration tests
//!
//! Each test binary starts its own in-memory DSB server on a random OS-assigned
//! port, optionally backed by an ephemeral PostgreSQL container via Bollard.
//! No shared state, no port conflicts, no `--test-threads=1`.

#![allow(dead_code)]

use dsb::config::Config;
use serde_json::json;
use std::time::Duration;
use tokio::time::sleep;

/// A running DSB server for a single test binary.
pub struct ServerFixture {
    /// Base URL (e.g., `http://127.0.0.1:54321`)
    pub base_url: String,
    /// HTTP client pre-configured with the base URL and API key
    pub client: TestClient,
    /// Handle to abort the server task
    #[allow(dead_code)]
    shutdown: tokio::sync::oneshot::Sender<()>,
}

impl ServerFixture {
    /// Start a fresh DSB server with an in-memory state store.
    ///
    /// Admin API and API key store are disabled (requires PostgreSQL).
    pub async fn start_in_memory() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = default_test_config();
        config.database.url = None;
        config.database.password = None;
        config.server.require_auth = false;
        Self::start_with_config(config).await
    }

    /// Start a fresh DSB server with an in-memory state store and auth enabled.
    pub async fn start_in_memory_with_auth(
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = default_test_config();
        config.database.url = None;
        config.database.password = None;
        config.server.require_auth = true;
        Self::start_with_config(config).await
    }

    /// Start a fresh DSB server with an ephemeral PostgreSQL database.
    ///
    /// All features enabled: admin API, API key store, activity tracking, SSH sessions.
    pub async fn start_with_postgres() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        use crate::common::testcontainers_postgres::EphemeralPostgres;

        let pg = EphemeralPostgres::start().await?;

        let mut config = default_test_config();
        config.database.url = None;
        config.database.password = Some("postgres".to_string());
        config.database.host = "127.0.0.1".to_string();
        config.database.port = pg.host_port;
        config.database.name = "dsb_test".to_string();
        config.database.user = "postgres".to_string();
        config.server.require_auth = true;

        let mut fixture = Self::start_with_config(config).await?;
        fixture.client.pg = Some(pg);
        Ok(fixture)
    }

    /// Start a server with the given configuration.
    pub async fn start_with_config(
        mut config: Config,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let port = Self::find_free_port().await?;
        config.server.port = port;
        config.server.host = "127.0.0.1".to_string();

        let base_url = format!("http://127.0.0.1:{}", port);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            tokio::select! {
                result = dsb::api::start_server(&config) => {
                    if let Err(e) = result {
                        tracing::error!("Test server exited with error: {}", e);
                    }
                }
                _ = shutdown_rx => {
                    tracing::info!("Test server shutting down gracefully");
                }
            }
        });

        let raw_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let mut last_err = None;

        while tokio::time::Instant::now() < deadline {
            match raw_client.get(format!("{}/health", base_url)).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let client = TestClient::new(raw_client, base_url.clone());
                    return Ok(ServerFixture {
                        base_url,
                        client,
                        shutdown: shutdown_tx,
                    });
                }
                Ok(resp) => last_err = Some(format!("HTTP {}", resp.status())),
                Err(e) => last_err = Some(e.to_string()),
            }
            sleep(Duration::from_millis(200)).await;
        }

        Err(format!(
            "Test server failed to start on {} within 30s. Last error: {:?}",
            base_url, last_err
        )
        .into())
    }

    /// Connect to an external API instead of starting a local server.
    ///
    /// Reads the target URL and API key from [`TestInfraConfig`] and
    /// returns a fixture whose client points at the external deployment.
    /// Use this when `using_external_api()` is `true`.
    pub async fn connect_external() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let config = crate::common::test_config::TestInfraConfig::from_env();
        let base_url = config.api_base_url;

        let raw_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        let mut last_err = None;

        while tokio::time::Instant::now() < deadline {
            match raw_client.get(format!("{}/health", base_url)).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let client = TestClient::new(raw_client, base_url.clone());
                    // Dummy channel — never used, external server is not shut down by us
                    let (shutdown_tx, _) = tokio::sync::oneshot::channel();
                    return Ok(ServerFixture {
                        base_url,
                        client,
                        shutdown: shutdown_tx,
                    });
                }
                Ok(resp) => last_err = Some(format!("HTTP {}", resp.status())),
                Err(e) => last_err = Some(e.to_string()),
            }
            sleep(Duration::from_millis(200)).await;
        }

        Err(format!(
            "External API at {} not reachable within 30s. Last error: {:?}",
            base_url, last_err
        )
        .into())
    }

    /// Find a free TCP port on localhost.
    async fn find_free_port() -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        drop(listener);
        Ok(addr.port())
    }
}

/// HTTP client wrapper for test convenience.
pub struct TestClient {
    raw: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    #[allow(dead_code)]
    pub pg: Option<crate::common::testcontainers_postgres::EphemeralPostgres>,
}

impl TestClient {
    fn new(raw: reqwest::Client, base_url: String) -> Self {
        let config = crate::common::test_config::TestInfraConfig::from_env();
        Self {
            raw,
            base_url,
            api_key: Some(config.api_key),
            pg: None,
        }
    }

    /// Set or clear the API key for subsequent requests.
    pub fn with_api_key(mut self, key: Option<String>) -> Self {
        self.api_key = key;
        self
    }

    fn build_request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.raw.request(method, &url);
        if let Some(ref key) = self.api_key {
            req = req.header("x-api-key", key);
        }
        req
    }

    pub async fn get(&self, path: &str) -> reqwest::Response {
        self.build_request(reqwest::Method::GET, path)
            .send()
            .await
            .expect("GET request failed")
    }

    pub async fn post(&self, path: &str) -> reqwest::Response {
        self.build_request(reqwest::Method::POST, path)
            .send()
            .await
            .expect("POST request failed")
    }

    pub async fn post_json<T: serde::Serialize>(&self, path: &str, body: &T) -> reqwest::Response {
        self.build_request(reqwest::Method::POST, path)
            .json(body)
            .send()
            .await
            .expect("POST JSON request failed")
    }

    pub async fn delete(&self, path: &str) -> reqwest::Response {
        self.build_request(reqwest::Method::DELETE, path)
            .send()
            .await
            .expect("DELETE request failed")
    }

    /// Create a sandbox and return its ID.
    /// The name is automatically suffixed with a UUID to avoid Docker name collisions.
    /// Retries once on transient server errors (5xx).
    pub async fn create_sandbox(&self, image: &str, name: &str) -> String {
        let unique_name = format!("{}-{}", name, uuid::Uuid::new_v4());
        let mut last_status = reqwest::StatusCode::OK;
        let mut last_body = String::new();
        for attempt in 0..3 {
            let resp = self
                .post_json("/sandboxes", &json!({"image": image, "name": unique_name}))
                .await;
            let status = resp.status();
            if status == 201 {
                let body: serde_json::Value =
                    resp.json().await.expect("Failed to parse sandbox response");
                return body["id"].as_str().expect("Missing sandbox id").to_string();
            }
            last_status = status;
            last_body = resp.text().await.unwrap_or_default();
            if status.is_server_error() && attempt < 2 {
                eprintln!(
                    "Sandbox creation attempt {} failed ({}): {}. Retrying...",
                    attempt + 1,
                    status,
                    last_body
                );
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
            break;
        }
        panic!(
            "Failed to create sandbox after 3 attempts (status: {}): {}",
            last_status, last_body,
        );
    }

    /// Poll until the sandbox reaches "running" state.
    pub async fn wait_for_running(&self, sandbox_id: &str, timeout_secs: u64) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
        while tokio::time::Instant::now() < deadline {
            let resp = self.get(&format!("/sandboxes/{}", sandbox_id)).await;
            if resp.status().is_success() {
                let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
                if let Some(state) = body["state"].as_str() {
                    match state {
                        "running" => return,
                        "error" | "stopped" => {
                            panic!("Sandbox {} reached unexpected state: {}", sandbox_id, state)
                        }
                        _ => {}
                    }
                }
            }
            sleep(Duration::from_millis(200)).await;
        }
        panic!(
            "Sandbox {} did not reach running state within {}s",
            sandbox_id, timeout_secs
        );
    }

    /// Delete a sandbox and wait for it to be gone.
    pub async fn delete_sandbox(&self, sandbox_id: &str) {
        let resp = self.delete(&format!("/sandboxes/{}", sandbox_id)).await;
        assert!(
            resp.status().is_success() || resp.status() == 404,
            "Failed to delete sandbox: {}",
            resp.status()
        );
    }
}

/// Build a default test configuration suitable for integration tests.
fn default_test_config() -> Config {
    Config {
        server: dsb::config::ServerConfig {
            port: 0,
            host: "127.0.0.1".to_string(),
            api_key: None,
            web_terminal_api_key: None,
            ssh_gateway_api_key: None,
            vnc_api_key: None,
            vnc_require_auth: false,
            vnc_token_ttl_secs: 3600,
            require_auth: false,
            admin_api_key: Some(crate::common::test_config::TestInfraConfig::from_env().api_key),
            session_token_ttl_secs: 300,
            cookie_secret: Some("test-secret-32-bytes-long-!!!".to_string()),
        },
        database: dsb::config::DatabaseConfig {
            url: None,
            host: "localhost".to_string(),
            port: 5432,
            name: "dsb_test".to_string(),
            user: "postgres".to_string(),
            password: None,
            pool_max_size: Some(5),
        },
        docker: dsb::config::DockerConfig {
            registry: std::env::var("DSB_DOCKER__REGISTRY").unwrap_or_default(),
            host: None,
            default_image: "nginx:latest".to_string(),
            test_image: {
                let config = crate::common::test_config::TestInfraConfig::from_env();
                config.sandbox_image
            },
            network: None,
            home_dir: None,
            http_client: dsb::config::DockerConfig::default().http_client,
            proxy_env: std::collections::HashMap::new(),
        },
        sandbox: dsb::config::SandboxConfig {
            backend: {
                let config = crate::common::test_config::TestInfraConfig::from_env();
                if config.backend == "kubernetes" {
                    dsb::config::BackendType::Kubernetes
                } else {
                    dsb::config::BackendType::Docker
                }
            },
            default_inactivity_timeout: 5,
            cleanup_dry_run: false,
            state_monitor_interval: 60,
            deleted_sandbox_retention_days: 15,
            default_vnc_resolution: "2560x1440".to_string(),
            vnc_port: 5901,
            max_browser_tabs: 5,
            tool_timeouts: dsb::config::ToolTimeoutConfig::default(),
            default_resource_limits: dsb::config::DefaultResourceLimits::default(),
            kubernetes: dsb::config::KubernetesConfig::default(),
        },
        ssh: dsb::config::SshConfig {
            port: 2222,
            api_url: "http://localhost:8080".to_string(),
            api_key: None,
            host_key_path: None,
            cleanup_check_interval: 30,
            session_timeout: 300,
            termination_timeout: 60,
            backend: "docker".to_string(),
            kubernetes_namespace: "dsb-sandboxes".to_string(),
        },
        logging: dsb::config::LoggingConfig {
            level: "warn".to_string(),
            format: "pretty".to_string(),
            file: None,
            max_file_size_mb: 10,
            max_files: 5,
            ansi: false,
            filters: None,
        },
        static_server: dsb::config::StaticServerConfig::default(),
    }
}
