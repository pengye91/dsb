// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! E2E Sandbox Lifecycle Tests
//!
//! Comprehensive end-to-end tests for sandbox CRUD operations.
//! Each test starts its own fresh DSB server on a random port with an
//! in-memory state store — no shared state, no `--test-threads=1`.
//!
//! ```bash
//! # Run this test suite
//! cargo test --test e2e_sandbox_lifecycle
//!
//! # Run a specific test
//! cargo test --test e2e_sandbox_lifecycle test_health_check
//! ```

mod common;
use common::server_fixture::ServerFixture;
use common::sandbox_image;
use common::using_external_api;

#[tokio::test]
async fn test_health_check() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };

    let resp = fixture.client.get("/health").await;

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
    assert_eq!(body.get("status").and_then(|s| s.as_str()), Some("ok"));
}

#[tokio::test]
async fn test_create_list_get_delete_sandbox() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;
    let image = sandbox_image();

    // 1. Create sandbox (use simple command to avoid heavy supervisord startup in test env)
    let create_resp = client
        .post_json(
            "/sandboxes",
            &serde_json::json!({
                "image": image,
                "name": format!("e2e-test-sandbox-{}", uuid::Uuid::new_v4()),
                "command": ["sleep", "3600"]
            }),
        )
        .await;

    if !create_resp.status().is_success() {
        let status = create_resp.status();
        let err_text = create_resp.text().await.unwrap_or_default();
        eprintln!("Create failed with body: {}", err_text);
        panic!("Create failed with status {}", status);
    }
    let create_body: serde_json::Value = create_resp.json().await.expect("Failed to parse create response");
    let sandbox_id = create_body
        .get("id")
        .and_then(|v| v.as_str())
        .expect("Missing sandbox id")
        .to_string();

    // 2. Wait for running
    client.wait_for_running(&sandbox_id, 60).await;

    // 3. List sandboxes
    let list_resp = client.get("/sandboxes").await;
    assert_eq!(list_resp.status(), 200);
    let list_body: serde_json::Value = list_resp.json().await.expect("Failed to parse list");
    let sandboxes = list_body
        .get("data")
        .and_then(|v| v.as_array())
        .expect("Missing data array");
    assert!(
        sandboxes.iter().any(|s| s.get("id").and_then(|v| v.as_str()) == Some(&sandbox_id)),
        "Created sandbox not found in list"
    );

    // 4. Get sandbox
    let get_resp = client.get(&format!("/sandboxes/{}", sandbox_id)).await;
    assert_eq!(get_resp.status(), 200);
    let get_body: serde_json::Value = get_resp.json().await.expect("Failed to parse get");
    assert_eq!(
        get_body.get("id").and_then(|v| v.as_str()),
        Some(sandbox_id.as_str())
    );

    // 5. Execute command (retry up to 3 times in case the container is briefly unstable)
    let mut exec_resp = None;
    for attempt in 0..3 {
        let resp = client
            .post_json(
                &format!("/sandboxes/{}/exec", sandbox_id),
                &serde_json::json!({
                    "command": ["echo", "hello-from-e2e"]
                }),
            )
            .await;
        if resp.status().is_success() {
            exec_resp = Some(resp);
            break;
        }
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        } else {
            panic!("Exec failed after 3 attempts: {:?}", resp);
        }
    }
    let exec_resp = exec_resp.expect("Exec should have succeeded");
    let exec_body: serde_json::Value = exec_resp.json().await.expect("Failed to parse exec");
    let output = exec_body
        .get("output")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        output.contains("hello-from-e2e"),
        "Unexpected exec output: {}", output
    );

    // 6. Delete sandbox
    let del_resp = client.delete(&format!("/sandboxes/{}", sandbox_id)).await;
    assert!(del_resp.status().is_success(), "Delete failed: {:?}", del_resp);

    // 7. Verify soft-deleted (still retrievable with deleted_at set)
    let get_after_del = client.get(&format!("/sandboxes/{}?include_deleted=true", sandbox_id)).await;
    assert_eq!(get_after_del.status(), 200);
    let body_after_del: serde_json::Value = get_after_del.json().await.expect("Parse failed");
    assert!(
        body_after_del.get("deleted_at").is_some(),
        "Expected sandbox to be soft-deleted"
    );
}

#[tokio::test]
async fn test_sandbox_isolation_between_tests() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    // Create a sandbox using the helper (unique name is auto-generated)
    let id = client.create_sandbox(&sandbox_image(), "isolation-test").await;

    // Verify it exists in the list (on external there may be other sandboxes)
    let list = client
        .get("/sandboxes")
        .await
        .json::<serde_json::Value>()
        .await
        .expect("Parse failed");
    let sandboxes = list
        .get("data")
        .and_then(|v| v.as_array())
        .expect("Missing data array");
    assert!(
        sandboxes.iter().any(|s| s.get("id").and_then(|v| v.as_str()) == Some(&id)),
        "Created sandbox should appear in the list"
    );

    // Clean up
    let _ = client.delete(&format!("/sandboxes/{}", id)).await;
}

#[tokio::test]
async fn test_create_sandbox_with_custom_command() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let resp = client
        .post_json(
            "/sandboxes",
            &serde_json::json!({
                "image": sandbox_image(),
                "name": format!("custom-cmd-{}", uuid::Uuid::new_v4()),
                "command": ["echo", "hello"]
            }),
        )
        .await;

    assert_eq!(resp.status(), 201, "Create with custom command should succeed");
    let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
    assert!(body.get("id").is_some(), "Expected sandbox id in response");
}

#[tokio::test]
async fn test_stop_sandbox() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let id = client
        .create_sandbox(
            &sandbox_image(),
            "stop-test",
        )
        .await;
    client.wait_for_running(&id, 60).await;

    let resp = client.post(&format!("/sandboxes/{}/stop", id)).await;
    assert_eq!(resp.status(), 200, "Stop should return 200");

    let get_resp = client.get(&format!("/sandboxes/{}", id)).await;
    assert!(get_resp.status().is_success());
    let body: serde_json::Value = get_resp.json().await.expect("Failed to parse JSON");
    assert_eq!(
        body.get("state").and_then(|s| s.as_str()),
        Some("stopped"),
        "Expected sandbox state to be stopped"
    );

    client.delete_sandbox(&id).await;
}

#[tokio::test]
async fn test_get_nonexistent_sandbox_returns_404() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };

    let fake_id = uuid::Uuid::new_v4();
    let resp = fixture.client.get(&format!("/sandboxes/{}", fake_id)).await;

    assert_eq!(
        resp.status(),
        404,
        "Expected 404 for nonexistent sandbox"
    );
}

#[tokio::test]
async fn test_invalid_sandbox_create_request_returns_400() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };

    let resp = fixture
        .client
        .post_json(
            "/sandboxes",
            &serde_json::json!({
                "command": ["echo", "hello"]
                // missing "image" field
            }),
        )
        .await;

    assert!(
        resp.status().is_client_error(),
        "Expected 4xx for missing image field, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_concurrent_sandbox_creation() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let base = fixture.base_url.clone();

    let mut handles = vec![];

    for i in 0..5 {
        let base = base.clone();
        let api_key = common::test_config::TestInfraConfig::from_env().api_key;
        let handle = tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("Failed to build client");
            let mut req = client
                .post(format!("{}/sandboxes", base))
                .json(&serde_json::json!({
                    "image": sandbox_image(),
                    "name": format!("concurrent-{}-{}", i, uuid::Uuid::new_v4()),
                    "command": ["sleep", "60"]
                }));
            if !api_key.is_empty() {
                req = req.header("x-api-key", &api_key);
            }
            let resp = req.send().await.expect("Request failed");
            resp.status()
        });
        handles.push(handle);
    }

    let mut success_count = 0;
    for handle in handles {
        let status = handle.await.expect("Task panicked");
        if status == 201 {
            success_count += 1;
        } else if status.is_server_error() {
            eprintln!("Concurrent request failed with {} (acceptable)", status);
        } else {
            panic!("Unexpected status in concurrent test: {}", status);
        }
    }

    assert!(
        success_count >= 3,
        "Expected at least 3 successful sandbox creations, got {}",
        success_count
    );
}

#[tokio::test]
async fn test_exec_command_in_sandbox() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let resp = client
        .post_json(
            "/sandboxes",
            &serde_json::json!({
                "image": sandbox_image(),
                "name": format!("exec-test-{}", uuid::Uuid::new_v4()),
                "command": ["sleep", "60"]
            }),
        )
        .await;

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
    let id = body["id"].as_str().expect("Missing id").to_string();

    client.wait_for_running(&id, 30).await;

    let exec_resp = client
        .post_json(
            &format!("/sandboxes/{}/exec", id),
            &serde_json::json!({
                "command": ["echo", "hello"]
            }),
        )
        .await;

    assert!(
        exec_resp.status().is_success(),
        "Exec failed: {:?}",
        exec_resp
    );
    let exec_body: serde_json::Value = exec_resp.json().await.expect("Failed to parse exec");
    let output = exec_body
        .get("output")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        output.contains("hello"),
        "Expected output to contain 'hello', got: {}",
        output
    );

    client.delete_sandbox(&id).await;
}

#[tokio::test]
async fn test_sandbox_stats() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let id = client
        .create_sandbox(
            &sandbox_image(),
            "stats-test",
        )
        .await;

    client.wait_for_running(&id, 60).await;

    let stats_resp = client.get(&format!("/sandboxes/{}/stats", id)).await;
    assert_eq!(stats_resp.status(), 200, "Stats endpoint should return 200");

    let stats: serde_json::Value = stats_resp.json().await.expect("Failed to parse stats");
    assert!(!stats.is_null(), "Stats should not be null");

    client.delete_sandbox(&id).await;
}

#[tokio::test]
async fn test_full_sandbox_lifecycle() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let resp = client
        .post_json(
            "/sandboxes",
            &serde_json::json!({
                "image": sandbox_image(),
                "name": format!("lifecycle-test-{}", uuid::Uuid::new_v4()),
                "command": ["sleep", "60"]
            }),
        )
        .await;

    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
    let id = body["id"].as_str().expect("Missing id").to_string();

    // Wait for running
    client.wait_for_running(&id, 30).await;

    // Exec
    let exec_resp = client
        .post_json(
            &format!("/sandboxes/{}/exec", id),
            &serde_json::json!({
                "command": ["echo", "lifecycle"]
            }),
        )
        .await;
    assert!(exec_resp.status().is_success(), "Exec should succeed");

    // Stop
    let stop_resp = client.post(&format!("/sandboxes/{}/stop", id)).await;
    assert!(stop_resp.status().is_success(), "Stop should succeed");

    // Delete
    let del_resp = client.delete(&format!("/sandboxes/{}", id)).await;
    assert!(del_resp.status().is_success(), "Delete should succeed");

    // Verify 404 or soft-delete
    let get_resp = client.get(&format!("/sandboxes/{}", id)).await;
    if get_resp.status() == 404 {
        // Hard delete
    } else {
        assert!(get_resp.status().is_success());
        let get_body: serde_json::Value = get_resp.json().await.expect("Failed to parse");
        assert!(
            get_body.get("deleted_at").is_some(),
            "Expected sandbox to be soft-deleted"
        );
    }
}

#[tokio::test]
async fn test_create_sandbox_without_name() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };
    let client = &fixture.client;

    let resp = client
        .post_json(
            "/sandboxes",
            &serde_json::json!({
                "image": sandbox_image()
            }),
        )
        .await;

    assert_eq!(resp.status(), 201, "Create without name should succeed");
    let body: serde_json::Value = resp.json().await.expect("Failed to parse JSON");
    let name = body
        .get("config")
        .and_then(|c| c.get("name"))
        .and_then(|n| n.as_str());
    assert!(name.is_some(), "Expected auto-generated name in config");
    assert!(
        !name.unwrap().is_empty(),
        "Auto-generated name should not be empty"
    );
}

#[tokio::test]
async fn test_delete_nonexistent_sandbox() {
    let fixture = if using_external_api() {
        ServerFixture::connect_external()
            .await
            .expect("Failed to connect to external API")
    } else {
        ServerFixture::start_in_memory()
            .await
            .expect("Failed to start server")
    };

    let fake_id = uuid::Uuid::new_v4();
    let resp = fixture.client.delete(&format!("/sandboxes/{}", fake_id)).await;

    assert!(
        resp.status() == 404 || resp.status() == 204,
        "Expected 404 or 204 for deleting nonexistent sandbox, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn test_list_sandboxes_empty() {
    if using_external_api() {
        eprintln!("Skipping test_list_sandboxes_empty: requires isolated server state");
        return;
    }

    let fixture = ServerFixture::start_in_memory()
        .await
        .expect("Failed to start server");

    let resp = fixture.client.get("/sandboxes").await;
    assert_eq!(resp.status(), 200, "List should return 200");

    let body: serde_json::Value = resp.json().await.expect("Failed to parse list");
    let sandboxes = body
        .get("data")
        .and_then(|d| d.as_array())
        .expect("Missing data array");
    assert!(
        sandboxes.is_empty(),
        "Expected empty sandbox list on fresh server, got {} items",
        sandboxes.len()
    );
}
