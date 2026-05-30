// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! K8s E2E tests: Static file server under concurrent load.
//!
//! These tests validate that the static file upload, download, and listing
//! operations work correctly under high concurrent load on a real K8s cluster.
//!
//! NOTE: Static file serving in K8s mode proxies file operations through
//! `exec` commands into the sandbox pod, since there is no shared filesystem
//! between the DSB server pod and sandbox pods.
//!
//! Prerequisites:
//!   - MCP server deployed in the k8s cluster
//!   - Port-forward to MCP services
//!   - `DSB_API_KEY` set

use crate::agents::MonorailAgent;
use crate::tests::k8s_mod::{
    k8s_dsb_api_url, k8s_mcp_sandbox_url, k8s_sandbox_image, k8s_session_id,
};
use crate::tests::test_utils::{call_tool_with_retry, extract_output_text, extract_sandbox_id};
use anyhow::Context;
use std::future::Future;
use tracing::info;

// ============================================================================
// Helpers
// ============================================================================

async fn create_sandbox_session(
    test_name: &str,
) -> anyhow::Result<(MonorailAgent, String, String)> {
    let sandbox_agent = MonorailAgent::connect_to_url(k8s_mcp_sandbox_url())
        .await
        .context("connect to sandbox MCP")?;

    let session_id = k8s_session_id(test_name);
    let image = k8s_sandbox_image();

    let create_result = call_tool_with_retry(
        &sandbox_agent,
        "create_sandbox",
        serde_json::json!({
            "session_id": session_id.clone(),
            "image": image
        })
        .as_object()
        .cloned(),
    )
    .await
    .context("create_sandbox failed")?;

    let sandbox_id = extract_sandbox_id(&create_result)?;
    info!(%sandbox_id, "Sandbox created for static file tests");

    Ok((sandbox_agent, session_id, sandbox_id))
}

async fn destroy_sandbox_session(session_id: &str) {
    let sandbox_agent = match MonorailAgent::connect_to_url(k8s_mcp_sandbox_url()).await {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!("Failed to connect for cleanup: {}", e);
            return;
        }
    };
    let _ = sandbox_agent
        .call_tool(
            "destroy_sandbox",
            serde_json::json!({"session_id": session_id})
                .as_object()
                .cloned(),
        )
        .await;
}

async fn run_with_sandbox_session<F, Fut>(test_name: &str, test_body: F) -> anyhow::Result<()>
where
    F: FnOnce(MonorailAgent, String, String) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let (agent, session_id, sandbox_id) = create_sandbox_session(test_name).await?;
    let session_for_cleanup = session_id.clone();

    let join_result = tokio::task::spawn(test_body(agent, session_id, sandbox_id)).await;

    destroy_sandbox_session(&session_for_cleanup).await;

    match join_result {
        Ok(r) => r,
        Err(e) => std::panic::resume_unwind(e.into_panic()),
    }
}

/// Generate deterministic test file content.
fn gen_file_content(name: &str, size: usize) -> String {
    let prefix = format!("# Test file: {}\n", name);
    let suffix = format!("\n# End of file: {}\n", name);
    let body_needed = size.saturating_sub(prefix.len() + suffix.len());
    let mut body = String::with_capacity(body_needed);
    while body.len() < body_needed {
        body.push_str("0123456789abcdef");
    }
    body.truncate(body_needed);
    format!("{}{}{}", prefix, body, suffix)
}

// ============================================================================
// Category 1: Concurrent file uploads
// ============================================================================

/// Test: Upload 10 files, 5 at a time, then verify each via listing.
#[tokio::test]
async fn test_k8s_concurrent_file_uploads() -> anyhow::Result<()> {
    run_with_sandbox_session(
        "static-uploads",
        |agent, session_id, _sandbox_id| async move {
            let files: Vec<(String, String)> = (0..10)
                .map(|i| {
                    let name = format!("test_file_{}.txt", i);
                    let content = gen_file_content(&name, 10000);
                    (name, content)
                })
                .collect();

            // Upload 5 at a time (2 batches)
            for batch in files.chunks(5) {
                let uploads: Vec<_> = batch
                    .iter()
                    .map(|(name, content)| {
                        let agent_ref = &agent;
                        let session = session_id.clone();
                        let name = name.clone();
                        let content = content.clone();
                        async move {
                            let result = call_tool_with_retry(
                                agent_ref,
                                "file_upload",
                                serde_json::json!({
                                    "session_id": session,
                                    "file_path": format!("/public/{}", name),
                                    "content": content
                                })
                                .as_object()
                                .cloned(),
                            )
                            .await?;

                            if result.is_error == Some(true) {
                                let msg = extract_output_text(&result).unwrap_or_default();
                                anyhow::bail!("upload {} error: {}", name, msg);
                            }
                            info!("Uploaded {}", name);
                            Ok(())
                        }
                    })
                    .collect();

                let results = futures::future::join_all(uploads).await;
                for (i, result) in results.iter().enumerate() {
                    result
                        .as_ref()
                        .map_err(|e| anyhow::anyhow!("upload {}: {}", i, e))?;
                }
            }

            // Verify via listing
            let list_result = call_tool_with_retry(
                &agent,
                "list_static_files",
                serde_json::json!({"session_id": session_id.clone()})
                    .as_object()
                    .cloned(),
            )
            .await
            .context("list_static_files")?;

            let list_text = extract_output_text(&list_result)?;
            info!("Static file listing: {} chars", list_text.len());

            // Verify all 10 files appear in listing
            for (name, _) in &files {
                anyhow::ensure!(
                    list_text.contains(name),
                    "File {} not found in listing",
                    name
                );
            }

            info!(
                "All {} concurrent uploads verified via listing",
                files.len()
            );
            Ok(())
        },
    )
    .await
}

// ============================================================================
// Category 2: Concurrent downloads via HTTP GET
// ============================================================================

/// Test: Upload files then download them concurrently via DSB static API.
#[tokio::test]
async fn test_k8s_concurrent_file_downloads() -> anyhow::Result<()> {
    run_with_sandbox_session(
        "static-downloads",
        |agent, session_id, sandbox_id| async move {
            // Upload a test file (keep under 20KB to avoid kube-exec URI limits)
            let file_name = "download_test.txt";
            let content = gen_file_content(file_name, 10000);

            let upload_result = call_tool_with_retry(
                &agent,
                "file_upload",
                serde_json::json!({
                    "session_id": session_id.clone(),
                    "file_path": format!("/public/{}", file_name),
                    "content": content.clone()
                })
                .as_object()
                .cloned(),
            )
            .await
            .context("upload file for download test")?;

            if upload_result.is_error == Some(true) {
                let msg = extract_output_text(&upload_result).unwrap_or_default();
                anyhow::bail!("upload error: {}", msg);
            }

            // Download 20 times concurrently via HTTP
            let api_key = std::env::var("DSB_API_KEY").unwrap_or_default();
            let base_url = k8s_dsb_api_url();
            let client = reqwest::Client::new();
            let download_url = format!("{}/static/{}/{}", base_url, sandbox_id, file_name);

            let downloads: Vec<_> = (0..20)
                .map(|i| {
                    let client_ref = client.clone();
                    let url = download_url.clone();
                    let key = api_key.clone();
                    let expected = content.clone();
                    async move {
                        let resp = client_ref
                            .get(&url)
                            .header("x-api-key", key)
                            .send()
                            .await
                            .context(format!("download request {}", i))?;

                        anyhow::ensure!(
                            resp.status().is_success(),
                            "download {} returned status {}",
                            i,
                            resp.status()
                        );

                        let body = resp.text().await.context("download body")?;
                        anyhow::ensure!(
                            body == expected,
                            "download {} content mismatch: expected {} bytes, got {} bytes",
                            i,
                            expected.len(),
                            body.len()
                        );

                        info!("Download {} verified: {} bytes", i, body.len());
                        Ok(())
                    }
                })
                .collect();

            let results = futures::future::join_all(downloads).await;
            for (i, result) in results.iter().enumerate() {
                result
                    .as_ref()
                    .map_err(|e| anyhow::anyhow!("download {}: {}", i, e))?;
            }

            info!("All 20 concurrent downloads verified");
            Ok(())
        },
    )
    .await
}

// ============================================================================
// Category 3: Mixed upload/download storm
// ============================================================================

/// Test: Upload and download simultaneously.
#[tokio::test]
async fn test_k8s_mixed_upload_download_storm() -> anyhow::Result<()> {
    run_with_sandbox_session("static-storm", |agent, session_id, sandbox_id| async move {
        let api_key = std::env::var("DSB_API_KEY").unwrap_or_default();
        let base_url = k8s_dsb_api_url();
        let client = reqwest::Client::new();

        // Pre-upload one file (keep under 20KB to avoid kube-exec URI limits)
        let file_name = "storm_test.txt";
        let content = gen_file_content(file_name, 10000);
        let upload_result = call_tool_with_retry(
            &agent,
            "file_upload",
            serde_json::json!({
                "session_id": session_id.clone(),
                "file_path": format!("/public/{}", file_name),
                "content": content.clone()
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("pre-upload")?;

        if upload_result.is_error == Some(true) {
            let msg = extract_output_text(&upload_result).unwrap_or_default();
            anyhow::bail!("pre-upload error: {}", msg);
        }

        let download_url = format!("{}/static/{}/{}", base_url, sandbox_id, file_name);

        // 5 uploads + 10 downloads simultaneously
        let mut tasks: Vec<std::pin::Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>> =
            Vec::with_capacity(15);

        for i in 0..5 {
            let i = i;
            let session = session_id.clone();
            let name = format!("storm_upload_{}.txt", i);
            let data = gen_file_content(&name, 5000);
            let sandbox_url = k8s_mcp_sandbox_url();
            tasks.push(Box::pin(async move {
                let agent = MonorailAgent::connect_to_url(sandbox_url)
                    .await
                    .context("connect sandbox MCP for storm upload")?;
                let result = call_tool_with_retry(
                    &agent,
                    "file_upload",
                    serde_json::json!({
                        "session_id": session,
                        "file_path": format!("/public/{}", name),
                        "content": data
                    })
                    .as_object()
                    .cloned(),
                )
                .await?;

                if result.is_error == Some(true) {
                    let msg = extract_output_text(&result).unwrap_or_default();
                    anyhow::bail!("storm upload {} error: {}", i, msg);
                }
                info!("Storm upload {} succeeded", i);
                Ok(())
            }));
        }

        for i in 0..10 {
            let i = i;
            let client_ref = client.clone();
            let url = download_url.clone();
            let key = api_key.clone();
            let expected = content.clone();
            tasks.push(Box::pin(async move {
                let resp = client_ref
                    .get(&url)
                    .header("x-api-key", key)
                    .send()
                    .await
                    .context(format!("storm download {}", i))?;

                anyhow::ensure!(
                    resp.status().is_success(),
                    "storm download {} status {}",
                    i,
                    resp.status()
                );

                let body = resp.text().await.context("storm download body")?;
                anyhow::ensure!(body == expected, "storm download {} content mismatch", i);
                info!("Storm download {} verified", i);
                Ok(())
            }));
        }

        let results = futures::future::join_all(tasks).await;
        for (i, result) in results.iter().enumerate() {
            result
                .as_ref()
                .map_err(|e| anyhow::anyhow!("storm task {} failed: {}", i, e))?;
        }

        info!("Mixed upload/download storm test passed");
        Ok(())
    })
    .await
}

// ============================================================================
// Category 4: Large file test
// ============================================================================

/// Test: Upload a 5MB file and verify download integrity.
#[tokio::test]
async fn test_k8s_large_file_upload_download() -> anyhow::Result<()> {
    run_with_sandbox_session("static-large", |agent, session_id, sandbox_id| async move {
        let file_name = "large_test.txt";
        let size = 5 * 1024 * 1024; // 5MB
        let content = gen_file_content(file_name, size);

        info!("Uploading {} byte file...", content.len());
        let upload_result = call_tool_with_retry(
            &agent,
            "file_upload",
            serde_json::json!({
                "session_id": session_id.clone(),
                "file_path": format!("/public/{}", file_name),
                "content": content.clone()
            })
            .as_object()
            .cloned(),
        )
        .await
        .context("large file upload")?;

        if upload_result.is_error == Some(true) {
            let msg = extract_output_text(&upload_result).unwrap_or_default();
            anyhow::bail!("large upload error: {}", msg);
        }
        info!("Large file uploaded successfully");

        // Download via HTTP
        let api_key = std::env::var("DSB_API_KEY").unwrap_or_default();
        let base_url = k8s_dsb_api_url();
        let download_url = format!("{}/static/{}/{}", base_url, sandbox_id, file_name);

        let resp = reqwest::Client::new()
            .get(&download_url)
            .header("x-api-key", api_key)
            .send()
            .await
            .context("large file download request")?;

        anyhow::ensure!(
            resp.status().is_success(),
            "large file download status: {}",
            resp.status()
        );

        let body = resp.text().await.context("large file download body")?;
        anyhow::ensure!(
            body == content,
            "Large file content mismatch: expected {} bytes, got {} bytes",
            content.len(),
            body.len()
        );

        info!(
            "Large file {} bytes uploaded and downloaded successfully",
            content.len()
        );
        Ok(())
    })
    .await
}
