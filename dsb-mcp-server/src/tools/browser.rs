// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Browser automation tools (using agent_browser_tools.py via tool_proxy.py)
//!
//! This module provides browser automation using agent-browser CLI.
//! Key features:
//! - Ref-based element selection (@e1, @e2) from accessibility snapshots
//! - AI-native design for deterministic automation
//! - Connects to Chromium via CDP on port 9222

use crate::dsb_client::DSBClient;
use serde_json::json;

/// Helper: Call agent_browser_tools.py via tool_proxy.py HTTP endpoint
pub(crate) async fn call_browser_tool(
    dsb_client: &DSBClient,
    sandbox_id: &str,
    action: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let id = uuid::Uuid::parse_str(sandbox_id).map_err(|e| format!("Invalid sandbox ID: {}", e))?;

    dsb_client
        .execute_tool(
            id,
            "python",
            "/opt/tools/agent_browser_tools.py",
            action,
            Some(args),
            None,
        )
        .await
        .map_err(|e| format!("Browser tool execution failed: {}", e))
}

/// Navigate to URL in browser
pub async fn navigate(
    dsb_client: &DSBClient,
    sandbox_id: String,
    url: String,
) -> Result<String, String> {
    // Parse UUID first for clear error messages
    let _id =
        uuid::Uuid::parse_str(&sandbox_id).map_err(|e| format!("Invalid sandbox ID: {}", e))?;

    // Validate URL for SSRF prevention
    crate::tools::web::validate_url_secure(&url)?;

    let args = json!({"url": url});
    let result = call_browser_tool(dsb_client, &sandbox_id, "browser_navigate", args).await?;

    Ok(format!(
        "**Navigated to:** {}\n\n**Message:** {}",
        result
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown"),
        result.get("message").and_then(|v| v.as_str()).unwrap_or("")
    ))
}

/// Get accessibility snapshot with refs (@e1, @e2)
pub async fn snapshot(
    dsb_client: &DSBClient,
    sandbox_id: String,
    interactive: bool,
) -> Result<String, String> {
    let args = json!({"interactive": interactive});
    let result = call_browser_tool(dsb_client, &sandbox_id, "browser_snapshot", args).await?;

    // Return the snapshot content
    if let Some(snapshot) = result.get("snapshot").and_then(|v| v.as_str()) {
        Ok(snapshot.to_string())
    } else {
        Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
    }
}

/// Click element by ref or selector
pub async fn click(
    dsb_client: &DSBClient,
    sandbox_id: String,
    r#ref: Option<String>,
    selector: Option<String>,
) -> Result<String, String> {
    let mut args = json!({});

    if let Some(r) = r#ref {
        args["ref"] = json!(r);
    } else if let Some(s) = selector {
        args["selector"] = json!(s);
    } else {
        return Err("Must provide either ref or selector".to_string());
    }

    call_browser_tool(dsb_client, &sandbox_id, "browser_click", args).await?;
    Ok("**Clicked element successfully**".to_string())
}

/// Fill form field by ref or selector
pub async fn fill(
    dsb_client: &DSBClient,
    sandbox_id: String,
    r#ref: Option<String>,
    selector: Option<String>,
    value: String,
    clear: Option<bool>,
) -> Result<String, String> {
    let mut args = json!({"value": value});

    if let Some(r) = r#ref {
        args["ref"] = json!(r);
    } else if let Some(s) = selector {
        args["selector"] = json!(s);
    } else {
        return Err("Must provide either ref or selector".to_string());
    }

    if let Some(c) = clear {
        args["clear"] = json!(c);
    }

    call_browser_tool(dsb_client, &sandbox_id, "browser_fill", args).await?;
    Ok("**Filled form field successfully**".to_string())
}

/// Take screenshot
pub async fn screenshot(
    dsb_client: &DSBClient,
    sandbox_id: String,
    full_page: bool,
    name: Option<String>,
    selector: Option<String>,
) -> Result<String, String> {
    let mut args = json!({"fullPage": full_page});

    if let Some(n) = name {
        args["name"] = json!(n);
    }
    if let Some(s) = selector {
        args["selector"] = json!(s);
    }

    let result = call_browser_tool(dsb_client, &sandbox_id, "browser_screenshot", args).await?;

    Ok(format!(
        "**Screenshot saved to:** {}",
        result
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("/tmp/screenshot.png")
    ))
}

/// Evaluate JavaScript
pub async fn evaluate(
    dsb_client: &DSBClient,
    sandbox_id: String,
    script: String,
) -> Result<String, String> {
    let args = json!({"script": script});
    let result = call_browser_tool(dsb_client, &sandbox_id, "browser_evaluate", args).await?;

    Ok(format!(
        "**Result:** {}",
        result
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("null")
    ))
}

/// Scroll page
pub async fn scroll(
    dsb_client: &DSBClient,
    sandbox_id: String,
    direction: String,
    amount: i32,
) -> Result<String, String> {
    let args = json!({"direction": direction, "amount": amount});
    call_browser_tool(dsb_client, &sandbox_id, "browser_scroll", args).await?;
    Ok("**Scrolled successfully**".to_string())
}

/// Go back in browser history
pub async fn go_back(dsb_client: &DSBClient, sandbox_id: String) -> Result<String, String> {
    let args = json!({});
    let result = call_browser_tool(dsb_client, &sandbox_id, "browser_go_back", args).await?;

    Ok(format!(
        "**Went back, current URL:** {}",
        result.get("url").and_then(|v| v.as_str()).unwrap_or("")
    ))
}

/// Go forward in browser history
pub async fn go_forward(dsb_client: &DSBClient, sandbox_id: String) -> Result<String, String> {
    let args = json!({});
    let result = call_browser_tool(dsb_client, &sandbox_id, "browser_go_forward", args).await?;

    Ok(format!(
        "**Went forward, current URL:** {}",
        result.get("url").and_then(|v| v.as_str()).unwrap_or("")
    ))
}

/// Wait for element or text
pub async fn wait(
    dsb_client: &DSBClient,
    sandbox_id: String,
    selector: Option<String>,
    text: Option<String>,
    time_ms: Option<i32>,
) -> Result<String, String> {
    let mut args = json!({});

    if let Some(s) = selector {
        args["selector"] = json!(s);
    } else if let Some(t) = text {
        args["text"] = json!(t);
    } else if let Some(t) = time_ms {
        args["time"] = json!(t);
    } else {
        return Err("Must provide selector, text, or time".to_string());
    }

    call_browser_tool(dsb_client, &sandbox_id, "browser_wait", args).await?;
    Ok("**Wait completed**".to_string())
}

/// Press a key
pub async fn press_key(
    dsb_client: &DSBClient,
    sandbox_id: String,
    key: String,
) -> Result<String, String> {
    let args = json!({"key": key});
    call_browser_tool(dsb_client, &sandbox_id, "browser_press_key", args).await?;
    Ok(format!("**Pressed key:** {}", key))
}

/// Manage tabs (list, new, select, close)
pub async fn tabs(
    dsb_client: &DSBClient,
    sandbox_id: String,
    action: String,
    index: Option<i32>,
    url: Option<String>,
) -> Result<String, String> {
    let mut args = json!({"action": action});

    if let Some(i) = index {
        args["index"] = json!(i);
    }
    if let Some(u) = url {
        args["url"] = json!(u);
    }

    let result = call_browser_tool(dsb_client, &sandbox_id, "browser_tabs", args).await?;

    if action == "list" {
        let tabs = result
            .get("tabs")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let mut output = String::from("**Open Tabs:**\n\n");
        for (i, tab) in tabs.iter().enumerate() {
            let title = tab.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let url = tab.get("url").and_then(|v| v.as_str()).unwrap_or("");
            output.push_str(&format!("{}. **{}** - {}\n", i, title, url));
        }
        Ok(output)
    } else {
        Ok(format!("**Tab action '{}' completed**", action))
    }
}

/// Legacy function for backward compatibility
pub async fn automate_browser(
    dsb_client: &crate::dsb_client::DSBClient,
    sandbox_id: String,
    action: String,
    selector: Option<String>,
    value: Option<String>,
    url: Option<String>,
) -> Result<String, String> {
    // Map legacy actions to new actions
    match action.as_str() {
        "navigate" => navigate(dsb_client, sandbox_id, url.unwrap_or_default()).await,
        "click" => click(dsb_client, sandbox_id, None, selector).await,
        "fill" | "type" => {
            fill(
                dsb_client,
                sandbox_id,
                None,
                selector,
                value.unwrap_or_default(),
                None,
            )
            .await
        }
        "screenshot" => screenshot(dsb_client, sandbox_id, false, None, None).await,
        "back" => go_back(dsb_client, sandbox_id).await,
        "forward" => go_forward(dsb_client, sandbox_id).await,
        _ => Err(format!("Unknown browser action: {}", action)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[tokio::test]
    async fn test_browser_automate_valid_uuid() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result = automate_browser(
            &client,
            "123e4567-e89b-12d3-a456-426614174000".to_string(),
            "navigate".to_string(),
            None,
            None,
            Some("https://example.com".to_string()),
        )
        .await;

        // Will fail on connection but UUID parsing should work
        if let Err(e) = result {
            assert!(
                e.contains("connection") || e.contains("error") || e.contains("Failed"),
                "Expected connection/failure error, got: {}",
                e
            );
        }
    }

    #[tokio::test]
    async fn test_browser_automate_invalid_uuid() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result = automate_browser(
            &client,
            "invalid-uuid".to_string(),
            "navigate".to_string(),
            None,
            None,
            None,
        )
        .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid sandbox ID"));
    }

    #[tokio::test]
    async fn test_browser_automate_with_url() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result = automate_browser(
            &client,
            "123e4567-e89b-12d3-a456-426614174000".to_string(),
            "navigate".to_string(),
            None,
            None,
            Some("https://example.com".to_string()),
        )
        .await;

        // Will fail on connection but UUID parsing should work
        if let Err(e) = result {
            assert!(
                e.contains("connection") || e.contains("error") || e.contains("Failed"),
                "Expected connection/failure error, got: {}",
                e
            );
        }
    }

    #[tokio::test]
    async fn test_browser_automate_without_url() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result = automate_browser(
            &client,
            "123e4567-e89b-12d3-a456-426614174000".to_string(),
            "screenshot".to_string(),
            None,
            None,
            None,
        )
        .await;

        // Will fail on connection but UUID parsing should work
        if let Err(e) = result {
            assert!(
                e.contains("connection") || e.contains("error") || e.contains("Failed"),
                "Expected connection/failure error, got: {}",
                e
            );
        }
    }

    #[tokio::test]
    async fn test_browser_automate_with_selector() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result = automate_browser(
            &client,
            "123e4567-e89b-12d3-a456-426614174000".to_string(),
            "click".to_string(),
            Some("#button".to_string()),
            None,
            None,
        )
        .await;

        // Will fail on connection but UUID parsing should work
        if let Err(e) = result {
            assert!(
                e.contains("connection") || e.contains("error") || e.contains("Failed"),
                "Expected connection/failure error, got: {}",
                e
            );
        }
    }

    #[tokio::test]
    async fn test_browser_automate_with_value() {
        let settings = Settings::default();
        let client = crate::dsb_client::DSBClient::new(settings).unwrap();

        let result = automate_browser(
            &client,
            "123e4567-e89b-12d3-a456-426614174000".to_string(),
            "type".to_string(),
            Some("#input".to_string()),
            Some("test value".to_string()),
            None,
        )
        .await;

        // Will fail on connection but UUID parsing should work
        if let Err(e) = result {
            assert!(
                e.contains("connection") || e.contains("error") || e.contains("Failed"),
                "Expected connection/failure error, got: {}",
                e
            );
        }
    }
}
