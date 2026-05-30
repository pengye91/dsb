// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # HTTP Request/Response Logging Middleware
//!
//! Provides structured logging for all HTTP requests with:
//! - Request ID generation and propagation
//! - Method, path, status code logging
//! - Request timing measurement
//! - Error details

use axum::{extract::Request, http::HeaderMap, middleware::Next, response::Response};
use std::time::Instant;
use tracing::{debug, info, warn, Instrument};
use uuid::Uuid;

/// HTTP request logging middleware
///
/// Logs all requests with:
/// - Request ID (generated or from X-Request-ID header)
/// - HTTP method and path
/// - Response status code
/// - Request duration
/// - Client IP (if available via forwarded headers)
///
/// # Middleware Order
///
/// This should be applied **FIRST** in the middleware chain (which means it runs **LAST**
/// due to Axum's middleware reversal). This ensures all requests are logged regardless of
/// authentication or other middleware.
///
/// # Example
///
/// ```rust,no_run,ignore
/// use axum::{middleware, Router};
/// use dsb::api::logging::request_logging_middleware;
///
/// let app = Router::new()
///     .route("/", get(handler))
///     .layer(middleware::from_fn(request_logging_middleware));
/// ```
pub async fn request_logging_middleware(req: Request, next: Next) -> Response {
    let start = Instant::now();

    // Extract or generate request ID
    let request_id = extract_or_generate_request_id(req.headers());

    // Extract request details
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let client_ip = extract_client_ip(req.headers());

    // Add request ID to tracing span for this request
    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        path = %path,
        client_ip = %client_ip,
    );

    // Log request start at debug level
    debug!(
        request_id = %request_id,
        method = %method,
        path = %path,
        client_ip = %client_ip,
        "Request started"
    );

    // Execute request within span
    let response = next.run(req).instrument(span).await;

    // Calculate duration
    let duration = start.elapsed();
    let status = response.status();
    let status_code = status.as_u16();

    // Log response based on status code
    // Health checks are logged at DEBUG level to avoid spamming logs
    if path == "/health" || path == "/healthz" {
        debug!(
            request_id = %request_id,
            method = %method,
            path = %path,
            status = %status_code,
            duration_ms = duration.as_millis(),
            "Health check completed"
        );
    } else if status.is_success() {
        info!(
            request_id = %request_id,
            method = %method,
            path = %path,
            status = %status_code,
            duration_ms = duration.as_millis(),
            "Request completed"
        );
    } else if status.is_client_error() {
        // 4xx errors - client made a bad request
        warn!(
            request_id = %request_id,
            method = %method,
            path = %path,
            status = %status_code,
            duration_ms = duration.as_millis(),
            "Client error"
        );
    } else if status.is_server_error() {
        // 5xx errors - server failed
        tracing::error!(
            request_id = %request_id,
            method = %method,
            path = %path,
            status = %status_code,
            duration_ms = duration.as_millis(),
            "Server error"
        );
    } else {
        // Informational (1xx) or Redirection (3xx)
        info!(
            request_id = %request_id,
            method = %method,
            path = %path,
            status = %status_code,
            duration_ms = duration.as_millis(),
            "Request completed"
        );
    }

    // Inject request ID into response headers for client correlation
    let mut response = response;
    if let Ok(header_value) = request_id.parse() {
        response.headers_mut().insert("X-Request-ID", header_value);
    }

    response
}

/// Extract request ID from headers or generate new one
///
/// # Priority
///
/// 1. Check `X-Request-ID` header (client-provided)
/// 2. Generate new UUID v4 if not present
///
/// # Arguments
///
/// * `headers` - HTTP request headers
///
/// # Returns
///
/// Request ID string (UUID format)
fn extract_or_generate_request_id(headers: &HeaderMap) -> String {
    headers
        .get("X-Request-ID")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// Extract client IP from headers
///
/// # Priority
///
/// 1. Check `X-Forwarded-For` header (proxy/load balancer)
/// 2. Check `X-Real-IP` header (nginx/other reverse proxies)
/// 3. Return "unknown" if not found
///
/// # Arguments
///
/// * `headers` - HTTP request headers
///
/// # Returns
///
/// Client IP address string or "unknown"
fn extract_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("X-Forwarded-For")
        .or_else(|| headers.get("X-Real-IP"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;

    #[test]
    fn test_extract_or_generate_request_id_with_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Request-ID",
            HeaderValue::from_static("test-request-id-123"),
        );

        let request_id = extract_or_generate_request_id(&headers);
        assert_eq!(request_id, "test-request-id-123");
    }

    #[test]
    fn test_extract_or_generate_request_id_without_header() {
        let headers = HeaderMap::new();

        let request_id = extract_or_generate_request_id(&headers);
        // Should be a valid UUID
        assert!(Uuid::parse_str(&request_id).is_ok());
    }

    #[test]
    fn test_extract_client_ip_with_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Forwarded-For", HeaderValue::from_static("192.168.1.100"));

        let client_ip = extract_client_ip(&headers);
        assert_eq!(client_ip, "192.168.1.100");
    }

    #[test]
    fn test_extract_client_ip_with_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Real-IP", HeaderValue::from_static("10.0.0.50"));

        let client_ip = extract_client_ip(&headers);
        assert_eq!(client_ip, "10.0.0.50");
    }

    #[test]
    fn test_extract_client_ip_prefer_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Forwarded-For", HeaderValue::from_static("192.168.1.100"));
        headers.insert("X-Real-IP", HeaderValue::from_static("10.0.0.50"));

        let client_ip = extract_client_ip(&headers);
        // Should prefer X-Forwarded-For
        assert_eq!(client_ip, "192.168.1.100");
    }

    #[test]
    fn test_extract_client_ip_unknown() {
        let headers = HeaderMap::new();

        let client_ip = extract_client_ip(&headers);
        assert_eq!(client_ip, "unknown");
    }

    #[test]
    fn test_request_logging_middleware_injects_request_id() {
        // This is a basic compile-time test to ensure the middleware signature is correct
        // Full integration testing would require running an Axum server
        let _middleware = request_logging_middleware;
    }
}
