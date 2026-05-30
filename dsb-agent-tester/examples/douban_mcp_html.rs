// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Call DSB MCP `web_fetch` (web) for Douban Top 250, build HTML, upload to sandbox `/public/`,
//! then verify via DSB static HTTP and optionally open the fetched bytes in the default browser.
//!
//! Usage:
//!   export DSB_API_KEY=...          # same key as DSB server (for MCP + static GET)
//!   export DSB_MCP_URL=http://127.0.0.1:3000/mcp/dsb/web
//!   cargo run -p dsb-agent-tester --example douban_mcp_html
//!
//! Optional:
//!   `DSB_MCP_SANDBOX_URL` — explicit sandbox MCP URL (default: `DSB_MCP_URL` with `/web` → `/sandbox`)
//!   `OUT_HTML` — also write merged HTML to this local path (default: ./douban_top250_mcp.html)
//!   `WEB_FETCH_MAX_LENGTH` (default 800000)
//!   `WEB_FETCH_URL` (default Douban Top 250)
//!   `DSB_MCP_SESSION` (default douban-top250-mcp)
//!   `STATIC_REL_PATH` — path segment after `/static/{id}/` (default douban_top250.html)
//!   `VERIFY_OPEN_BROWSER=1` — after HTTP verify, `open` the saved response (macOS); always runs on macOS if verify succeeds unless `VERIFY_OPEN_BROWSER=0`
//!
//! **Risk**: `file_upload` uses DSB exec; clusters that block exec return 403 and this example fails.
//! Mitigation: use a cluster where `/sandboxes/{id}/exec` is allowed, or rely on local Docker-only DSB.

use anyhow::Context;
use dsb_agent_tester::agents::MonorailAgent;
use rmcp::model::CallToolResult;
use serde_json::json;
use std::path::PathBuf;

fn extract_output_text(result: &CallToolResult) -> anyhow::Result<String> {
    let mut output = String::new();
    for content in &result.content {
        if let rmcp::model::RawContent::Text(text_content) = &content.raw {
            output.push_str(&text_content.text);
        }
    }
    if output.is_empty() {
        anyhow::bail!("No text output in MCP result");
    }
    Ok(output)
}

fn sandbox_mcp_url() -> anyhow::Result<String> {
    if let Ok(u) = std::env::var("DSB_MCP_SANDBOX_URL") {
        return Ok(u);
    }
    let web = std::env::var("DSB_MCP_URL")
        .or_else(|_| std::env::var("DSB_API_URL").map(|a| format!("{a}/mcp")))
        .context(
            "set DSB_MCP_URL (e.g. http://127.0.0.1:PORT/mcp/dsb/web) or DSB_MCP_SANDBOX_URL",
        )?;
    if web.contains("/mcp/dsb/web") {
        return Ok(web.replace("/mcp/dsb/web", "/mcp/dsb/sandbox"));
    }
    anyhow::bail!(
        "DSB_MCP_URL must contain /mcp/dsb/web to derive sandbox URL, or set DSB_MCP_SANDBOX_URL explicitly (got {web})"
    );
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let out = std::env::var("OUT_HTML").unwrap_or_else(|_| "douban_top250_mcp.html".into());
    let session = std::env::var("DSB_MCP_SESSION").unwrap_or_else(|_| "douban-top250-mcp".into());
    let fetch_url = std::env::var("WEB_FETCH_URL")
        .unwrap_or_else(|_| "https://movie.douban.com/top250".to_string());
    let max_length: usize = std::env::var("WEB_FETCH_MAX_LENGTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(800_000);
    let static_rel =
        std::env::var("STATIC_REL_PATH").unwrap_or_else(|_| "douban_top250.html".into());
    let public_path =
        std::env::var("SANDBOX_PUBLIC_PATH").unwrap_or_else(|_| format!("/public/{static_rel}"));

    let web_agent = MonorailAgent::new()
        .await
        .context("connect web MCP (DSB_MCP_URL)")?;
    let sandbox_url = sandbox_mcp_url()?;
    tracing::info!(%sandbox_url, "Connecting sandbox MCP…");
    let sandbox_agent = MonorailAgent::connect_to_url(sandbox_url)
        .await
        .context("connect sandbox MCP")?;

    tracing::info!(
        url = %fetch_url,
        max_length,
        "Calling web_fetch…"
    );
    let result = web_agent
        .call_tool(
            "web_fetch",
            Some(
                json!({
                    "session_id": session,
                    "url": fetch_url,
                    "format": "html",
                    "max_length": max_length
                })
                .as_object()
                .cloned()
                .unwrap(),
            ),
        )
        .await
        .context("web_fetch tool call failed")?;

    if result.is_error == Some(true) {
        let msg = extract_output_text(&result).unwrap_or_default();
        anyhow::bail!("web_fetch returned error: {msg}");
    }

    let text = extract_output_text(&result).context("empty MCP response")?;
    let parsed: serde_json::Value = serde_json::from_str(&text).context("web_fetch JSON parse")?;
    let inner = parsed
        .get("content")
        .and_then(|v| v.as_str())
        .context("missing .content in web_fetch JSON")?;

    let title = parsed
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Douban Top 250");

    let mut page = String::new();
    page.push_str("<!DOCTYPE html>\n<html lang=\"zh-CN\">\n<head>\n<meta charset=\"utf-8\"/>\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/>\n<title>");
    page.push_str(title);
    page.push_str(" — DSB MCP web_fetch + static</title>\n<style>\nbody{font-family:system-ui,sans-serif;margin:1rem;line-height:1.45;}\nheader{border-bottom:1px solid #ccc;margin-bottom:1rem;padding-bottom:0.5rem;}\n.src{color:#555;font-size:0.9rem;}\narticle{max-width:1200px;}\n</style>\n</head>\n<body>\n<header>\n<h1>");
    page.push_str(title);
    page.push_str("</h1>\n<p class=\"src\">Source: <a href=\"https://movie.douban.com/top250\">movie.douban.com/top250</a> — fetched with DSB MCP <code>web_fetch</code>, uploaded with <code>file_upload</code> under <code>/public/</code>, served via DSB static HTTP.</p>\n</header>\n<article>\n");
    page.push_str(inner);
    page.push_str("\n</article>\n</body>\n</html>\n");

    let path = PathBuf::from(&out);
    std::fs::write(&path, page.as_bytes()).with_context(|| format!("write {}", path.display()))?;
    tracing::info!("Wrote local copy {}", path.display());

    tracing::info!(%public_path, "file_upload to sandbox /public …");
    let up = sandbox_agent
        .call_tool(
            "file_upload",
            Some(
                json!({
                    "session_id": session,
                    "file_path": public_path,
                    "content": page,
                })
                .as_object()
                .cloned()
                .unwrap(),
            ),
        )
        .await
        .context("file_upload failed")?;

    if up.is_error == Some(true) {
        let msg = extract_output_text(&up).unwrap_or_default();
        anyhow::bail!("file_upload returned error: {msg}");
    }

    let url_json = sandbox_agent
        .call_tool(
            "get_static_file_url",
            Some(
                json!({
                    "session_id": session,
                    "file_path": static_rel,
                })
                .as_object()
                .cloned()
                .unwrap(),
            ),
        )
        .await
        .context("get_static_file_url failed")?;

    if url_json.is_error == Some(true) {
        let msg = extract_output_text(&url_json).unwrap_or_default();
        anyhow::bail!("get_static_file_url returned error: {msg}");
    }

    let url_text = extract_output_text(&url_json).context("get_static_file_url empty")?;
    let url_parsed: serde_json::Value =
        serde_json::from_str(&url_text).context("get_static_file_url JSON")?;
    let static_url = url_parsed
        .get("static_url")
        .and_then(|v| v.as_str())
        .context("missing static_url in MCP JSON")?;

    tracing::info!(%static_url, "GET static URL (same bytes the browser would get with X-API-Key)…");

    let mut rb = reqwest::Client::new().get(static_url);
    if let Ok(key) = std::env::var("DSB_API_KEY") {
        rb = rb.header("X-API-Key", key);
    }
    let resp = rb.send().await.context("HTTP GET static file")?;
    let status = resp.status();
    let body = resp.bytes().await.context("read static body")?;
    if !status.is_success() {
        anyhow::bail!("static GET failed: {status} body_len={}", body.len());
    }
    if body.len() < 100 {
        anyhow::bail!("static body suspiciously small: {} bytes", body.len());
    }
    tracing::info!(bytes = body.len(), "static file OK");

    let verify_path = std::env::var("VERIFY_HTML_PATH").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join("dsb_douban_static_verify.html")
            .to_string_lossy()
            .into_owned()
    });
    std::fs::write(&verify_path, &body)
        .with_context(|| format!("write verify snapshot {}", verify_path))?;
    tracing::info!(path = %verify_path, "Saved static response for inspection");

    let skip_browser = std::env::var("VERIFY_OPEN_BROWSER").ok().as_deref() == Some("0");
    let force_browser = std::env::var("VERIFY_OPEN_BROWSER").ok().as_deref() == Some("1");

    if cfg!(target_os = "macos") && !skip_browser {
        tracing::info!("Opening verified HTML in default browser (macOS `open`)…");
        let st = std::process::Command::new("open")
            .arg(&verify_path)
            .status()
            .context("run `open`")?;
        if !st.success() {
            tracing::warn!(?st, "`open` failed — open the verify path manually");
        }
    } else if force_browser && cfg!(target_os = "linux") {
        let st = std::process::Command::new("xdg-open")
            .arg(&verify_path)
            .status()
            .context("run xdg-open")?;
        if !st.success() {
            tracing::warn!(?st, "xdg-open failed");
        }
    } else if !cfg!(target_os = "macos") && !force_browser {
        tracing::info!("On macOS, unset VERIFY_OPEN_BROWSER to auto-open; elsewhere set VERIFY_OPEN_BROWSER=1 for xdg-open, or open {}", verify_path);
    }

    Ok(())
}
