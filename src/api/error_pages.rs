// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Error Page Rendering
//!
//! This module provides HTML error page templates for the dashboard and static file server.
//! Error pages are styled to match the DSB dashboard design.

use axum::http::StatusCode;
use axum::response::Html;

/// Error page data structure.
pub struct ErrorPageData {
    /// HTTP status code
    pub status_code: StatusCode,
    /// Error page title
    pub title: String,
    /// Error message
    pub message: String,
    /// Suggestion for resolving the error
    pub suggestion: String,
}

/// Get error page data for a given status code
pub fn get_error_page_data(status_code: StatusCode) -> ErrorPageData {
    match status_code {
        StatusCode::UNAUTHORIZED => ErrorPageData {
            status_code,
            title: "401 - Unauthorized".to_string(),
            message: "Authentication is required to access this resource.".to_string(),
            suggestion: "Please provide a valid API key using the X-API-Key header.".to_string(),
        },
        StatusCode::FORBIDDEN => ErrorPageData {
            status_code,
            title: "403 - Forbidden".to_string(),
            message: "You don't have permission to access this resource.".to_string(),
            suggestion: "Check your API key permissions or contact an administrator.".to_string(),
        },
        StatusCode::NOT_FOUND => ErrorPageData {
            status_code,
            title: "404 - Not Found".to_string(),
            message: "The requested resource could not be found.".to_string(),
            suggestion: "Verify the URL is correct and the resource exists.".to_string(),
        },
        StatusCode::INTERNAL_SERVER_ERROR => ErrorPageData {
            status_code,
            title: "500 - Internal Server Error".to_string(),
            message: "An unexpected error occurred on the server.".to_string(),
            suggestion: "Please try again later or contact support if the problem persists."
                .to_string(),
        },
        StatusCode::SERVICE_UNAVAILABLE => ErrorPageData {
            status_code,
            title: "503 - Service Unavailable".to_string(),
            message: "The service is temporarily unavailable.".to_string(),
            suggestion: "Please try again later. The server may be undergoing maintenance."
                .to_string(),
        },
        _ => ErrorPageData {
            status_code,
            title: format!("{} - Error", status_code.as_u16()),
            message: "An error occurred while processing your request.".to_string(),
            suggestion: "Please try again later.".to_string(),
        },
    }
}

/// Render an HTML error page
///
/// # Arguments
///
/// * `status_code` - HTTP status code
/// * `message` - Optional custom message (overrides default)
///
/// # Returns
///
/// HTML response with styled error page
pub fn render_error_page(status_code: StatusCode, message: Option<&str>) -> Html<String> {
    let data = get_error_page_data(status_code);
    let message = message.unwrap_or(&data.message);

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} - DSB</title>
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}
        body {{
            font-family: system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            color: #e4e4e7;
        }}
        .container {{
            text-align: center;
            padding: 2rem;
            max-width: 600px;
        }}
        .error-code {{
            font-size: 8rem;
            font-weight: 700;
            line-height: 1;
            background: linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            background-clip: text;
            margin-bottom: 1rem;
        }}
        .error-title {{
            font-size: 1.75rem;
            font-weight: 600;
            color: #f4f4f5;
            margin-bottom: 1rem;
        }}
        .error-message {{
            font-size: 1.125rem;
            color: #a1a1aa;
            margin-bottom: 0.5rem;
            line-height: 1.6;
        }}
        .error-suggestion {{
            font-size: 0.9375rem;
            color: #71717a;
            margin-bottom: 2rem;
            line-height: 1.6;
        }}
        .divider {{
            width: 60px;
            height: 4px;
            background: linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%);
            border-radius: 2px;
            margin: 2rem auto;
        }}
        .home-link {{
            display: inline-block;
            padding: 0.75rem 1.5rem;
            background: linear-gradient(135deg, #6366f1 0%, #8b5cf6 100%);
            color: white;
            text-decoration: none;
            border-radius: 0.5rem;
            font-weight: 500;
            transition: transform 0.2s, box-shadow 0.2s;
        }}
        .home-link:hover {{
            transform: translateY(-2px);
            box-shadow: 0 4px 20px rgba(99, 102, 241, 0.4);
        }}
        .footer {{
            margin-top: 3rem;
            font-size: 0.875rem;
            color: #52525b;
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="error-code">{status_code}</div>
        <div class="error-title">{title}</div>
        <div class="divider"></div>
        <p class="error-message">{message}</p>
        <p class="error-suggestion">{suggestion}</p>
        <a href="/dashboard" class="home-link">Go to Dashboard</a>
        <div class="footer">
            <p>Distributed Sandboxes</p>
        </div>
    </div>
</body>
</html>"#,
        title = escape_html(&data.title),
        status_code = status_code.as_u16(),
        message = escape_html(message),
        suggestion = escape_html(&data.suggestion),
    );

    Html(html)
}

/// Simple HTML escape helper
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Render a simple error page for JSON API requests
///
/// This returns a minimal JSON-compatible error response.
pub fn render_api_error_response(
    status_code: StatusCode,
    error: &str,
    hint: Option<&str>,
) -> String {
    let mut response = serde_json::json!({
        "error": error,
        "status": status_code.as_u16()
    });
    if let Some(hint) = hint {
        response["hint"] = serde_json::Value::String(hint.to_string());
    }
    serde_json::to_string(&response).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_page_data_401() {
        let data = get_error_page_data(StatusCode::UNAUTHORIZED);
        assert_eq!(data.title, "401 - Unauthorized");
        assert!(data.message.contains("Authentication"));
    }

    #[test]
    fn test_error_page_data_404() {
        let data = get_error_page_data(StatusCode::NOT_FOUND);
        assert_eq!(data.title, "404 - Not Found");
        assert!(data.message.contains("found"));
    }

    #[test]
    fn test_error_page_data_500() {
        let data = get_error_page_data(StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(data.title, "500 - Internal Server Error");
        assert!(data.message.contains("error"));
    }

    #[test]
    fn test_render_error_page_returns_html() {
        let response = render_error_page(StatusCode::NOT_FOUND, None);
        assert!(response.0.contains("<!DOCTYPE html>"));
        assert!(response.0.contains("404"));
        assert!(response.0.contains("Not Found"));
    }

    #[test]
    fn test_render_error_page_custom_message() {
        let response = render_error_page(
            StatusCode::NOT_FOUND,
            Some("Custom error message for testing"),
        );
        assert!(response.0.contains("Custom error message for testing"));
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<script>"), "&lt;script&gt;");
        assert_eq!(escape_html("&amp;"), "&amp;amp;");
        assert_eq!(escape_html("\"quote\""), "&quot;quote&quot;");
    }

    #[test]
    fn test_render_api_error_response() {
        let json = render_api_error_response(
            StatusCode::NOT_FOUND,
            "File not found",
            Some("Check the path"),
        );
        assert!(json.contains("File not found"));
        assert!(json.contains("Check the path"));
        assert!(json.contains("404"));
    }
}
