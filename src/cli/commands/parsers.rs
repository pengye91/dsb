// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie

pub(crate) fn parse_port_mapping(s: &str) -> Result<(u16, u16), String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err("Invalid port mapping format. Use HOST:CONTAINER".to_string());
    }

    let host_port: u16 = parts[0]
        .parse()
        .map_err(|_| "Invalid host port".to_string())?;
    let container_port: u16 = parts[1]
        .parse()
        .map_err(|_| "Invalid container port".to_string())?;

    Ok((host_port, container_port))
}

/// Parses a volume string into a VolumeMount enum.
/// Supports both bind mounts and named volumes:
/// - Bind mount: `/host/path:/container/path[:ro]`
/// - Named volume: `volume_name:/container/path[:ro]`
pub(crate) fn parse_volume_string(s: &str) -> Result<crate::core::types::VolumeMount, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 {
        return Err(
            "Invalid volume format. Use /host/path:/container/path or vol_name:/container/path[:ro]"
                .to_string(),
        );
    }

    // Determine if it's a named volume or bind mount
    // Named volumes don't start with / or . (relative path)
    let is_named_volume = !parts[0].starts_with('/') && !parts[0].starts_with('.');

    let read_only = parts.last().is_some_and(|p| *p == "ro");
    let container_path = if read_only && parts.len() > 2 {
        parts[parts.len() - 2]
    } else {
        parts[1]
    };

    if is_named_volume {
        Ok(crate::core::types::VolumeMount::Named {
            name: parts[0].to_string(),
            container_path: container_path.to_string(),
            read_only,
        })
    } else {
        // Reconstruct the host path (might contain colons on Windows)
        let host_path = if read_only && parts.len() > 2 {
            parts[0..parts.len() - 2].join(":")
        } else {
            parts[0..parts.len() - 1].join(":")
        };
        Ok(crate::core::types::VolumeMount::Bind {
            host_path,
            container_path: container_path.to_string(),
            read_only,
        })
    }
}

/// Parses a pull policy string into PullPolicy enum.
///
/// Accepted values (case-insensitive):
/// - "always" - Always pull the image before creating the container
/// - "missing" - Only pull if the image doesn't exist locally (default)
/// - "never" - Never pull, fail if image not present
pub(crate) fn parse_pull_policy(s: &str) -> Result<crate::core::types::PullPolicy, String> {
    match s.to_lowercase().as_str() {
        "always" => Ok(crate::core::types::PullPolicy::Always),
        "missing" => Ok(crate::core::types::PullPolicy::Missing),
        "never" => Ok(crate::core::types::PullPolicy::Never),
        _ => Err(format!(
            "Invalid pull policy '{}'. Valid options: always, missing, never",
            s
        )),
    }
}

/// Parses an environment variable string into a key-value pair.
///
/// Format: KEY=VALUE
pub(crate) fn parse_env_var(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err("Invalid env format. Use KEY=VALUE".to_string());
    }
    if parts[0].is_empty() {
        return Err("Environment variable key cannot be empty".to_string());
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

pub(crate) fn truncate_search_results(
    json_response: &mut serde_json::Value,
    result_limit: usize,
) -> Result<(), String> {
    if result_limit == 0 {
        return Err("num-results must be at least 1".to_string());
    }

    let results = json_response
        .get_mut("results")
        .and_then(|value| value.as_array_mut())
        .ok_or_else(|| "SearXNG response missing results array".to_string())?;

    results.truncate(result_limit);
    Ok(())
}

pub(crate) fn render_web_search_results(json_response: &serde_json::Value) -> String {
    let empty_vec = vec![];
    let results = json_response["results"].as_array().unwrap_or(&empty_vec);
    let mut output = String::new();

    for (index, result) in results.iter().enumerate() {
        let title = result["title"].as_str().unwrap_or("Untitled result");
        let url = result["url"].as_str().unwrap_or("");
        let snippet = result["snippet"]
            .as_str()
            .or_else(|| result["content"].as_str())
            .or_else(|| result["description"].as_str())
            .unwrap_or("");

        if url.is_empty() {
            output.push_str(&format!("{}. {}\n", index + 1, title));
        } else {
            output.push_str(&format!("{}. {} ({})\n", index + 1, title, url));
        }

        if !snippet.is_empty() {
            output.push_str(&format!("   {}\n", snippet));
        }

        output.push('\n');
    }

    if output.trim().is_empty() {
        "No search results found.".to_string()
    } else {
        output.trim_end().to_string()
    }
}

pub(crate) fn render_web_fetch_table(result: &serde_json::Value) -> String {
    let mut sections = Vec::new();

    if let Some(title) = result.get("title").and_then(|value| value.as_str()) {
        if !title.is_empty() {
            sections.push(format!("Title: {}", title));
        }
    }

    if let Some(url) = result.get("url").and_then(|value| value.as_str()) {
        if !url.is_empty() {
            sections.push(format!("URL: {}", url));
        }
    }

    if let Some(tab_info) = result.get("tab_info").and_then(|value| value.as_object()) {
        let page_id = tab_info
            .get("page_id")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let tab_title = tab_info
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        if !page_id.is_empty() || !tab_title.is_empty() {
            sections.push(format!(
                "Browser tab: {}{}",
                page_id,
                if tab_title.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", tab_title)
                }
            ));
        }
    }

    if let Some(path) = result
        .get("screenshot_path")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
    {
        sections.push(format!("Screenshot path: {}", path));
    } else if result
        .get("screenshot")
        .and_then(|value| value.as_str())
        .is_some()
    {
        sections
            .push("Screenshot: captured (use --output json to inspect base64 data)".to_string());
    }

    let content = result
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();

    if content.is_empty() {
        if sections.is_empty() {
            "No content returned.".to_string()
        } else {
            sections.join("\n")
        }
    } else if sections.is_empty() {
        content.to_string()
    } else {
        format!("{}\n\n{}", sections.join("\n"), content)
    }
}
