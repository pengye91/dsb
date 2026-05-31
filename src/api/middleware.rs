// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Request ID Middleware
//!
//! This module provides middleware for generating and tracking unique request identifiers.
//!
//! ## Features
//!
//! - **Unique request IDs**: Generates a UUID for each incoming request
//! - **Request tracking**: Adds `X-Request-ID` header to responses
//! - **Error context**: Request IDs are included in error responses for troubleshooting
//!
//! ## Usage
//!
//! The middleware is automatically applied to all requests and makes the request ID
//! available via request extensions for use in handlers and error formatting.

use axum::{extract::Request, middleware::Next, response::Response};
use uuid::Uuid;

/// Request ID wrapper type
///
/// This type wraps a unique request identifier that can be extracted from
/// request extensions in handlers.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Middleware to add unique request IDs to all incoming requests
///
/// This middleware:
/// 1. Generates a unique UUID for each request
/// 2. Stores it in request extensions for handler access
/// 3. Adds it to the response as `X-Request-ID` header
///
/// # Example
///
/// ```rust,no_run,ignore
/// use axum::{Router, routing::get};
/// use dsb::api::middleware::request_id_middleware;
///
/// let app = Router::new()
///     .route("/", get(handler))
///     .layer(axum::middleware::from_fn(request_id_middleware));
/// ```
pub async fn request_id_middleware(req: Request, next: Next) -> Response {
    let request_id = Uuid::new_v4().to_string();

    let mut req = req;
    req.extensions_mut().insert(RequestId(request_id.clone()));

    let mut response = next.run(req).await;

    response
        .headers_mut()
        .insert("X-Request-ID", request_id.parse().unwrap());

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[test]
    fn test_request_id_structure() {
        let id = RequestId("test-123".to_string());
        assert_eq!(id.0, "test-123");
    }

    #[test]
    fn test_request_id_clone() {
        let id1 = RequestId("test-123".to_string());
        let id2 = id1.clone();
        assert_eq!(id1.0, id2.0);
    }

    #[test]
    fn test_request_id_debug() {
        let id = RequestId("test-123".to_string());
        let debug_str = format!("{:?}", id);
        assert!(debug_str.contains("test-123"));
    }

    #[test]
    fn test_request_id_empty() {
        let id = RequestId("".to_string());
        assert_eq!(id.0, "");
    }

    #[test]
    fn test_request_id_with_uuid() {
        let uuid_str = Uuid::new_v4().to_string();
        let id = RequestId(uuid_str.clone());
        assert_eq!(id.0, uuid_str);
    }

    #[tokio::test]
    async fn test_request_id_middleware_adds_header() {
        // Create a simple handler that returns 200
        async fn handler() -> &'static str {
            "OK"
        }

        let app = Router::new()
            .route("/test", get(handler))
            .layer(axum::middleware::from_fn(request_id_middleware));

        // Make a request
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check response has X-Request-ID header
        assert_eq!(response.status(), StatusCode::OK);
        let request_id_header = response
            .headers()
            .get("X-Request-ID")
            .expect("X-Request-ID header should be present");

        // Verify it's a valid UUID format
        let request_id_str = request_id_header.to_str().unwrap();
        assert!(Uuid::parse_str(request_id_str).is_ok());
    }

    #[tokio::test]
    async fn test_request_id_middleware_unique_ids() {
        async fn handler() -> &'static str {
            "OK"
        }

        let app = Router::new()
            .route("/test", get(handler))
            .layer(axum::middleware::from_fn(request_id_middleware));

        // Make multiple requests and collect IDs
        let mut ids = std::collections::HashSet::new();
        for _ in 0..10 {
            let response = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/test")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            let request_id_header = response.headers().get("X-Request-ID").unwrap();
            let request_id_str = request_id_header.to_str().unwrap();
            ids.insert(request_id_str.to_string());
        }

        // All IDs should be unique
        assert_eq!(ids.len(), 10);
    }

    #[tokio::test]
    async fn test_request_id_middleware_preserves_response() {
        async fn handler() -> &'static str {
            "Hello, World!"
        }

        let app = Router::new()
            .route("/test", get(handler))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check response body is preserved
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"Hello, World!");
    }

    #[tokio::test]
    async fn test_request_id_middleware_with_request_headers() {
        async fn handler() -> &'static str {
            "OK"
        }

        let app = Router::new()
            .route("/test", get(handler))
            .layer(axum::middleware::from_fn(request_id_middleware));

        // Make a request with custom headers
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/test")
                    .header("X-Custom-Header", "custom-value")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check response still has X-Request-ID
        assert!(response.headers().get("X-Request-ID").is_some());
    }

    #[tokio::test]
    async fn test_request_id_middleware_stores_in_extensions() {
        // Handler that extracts RequestId from request extensions
        use axum::extract::Extension;

        async fn handler(Extension(request_id): Extension<RequestId>) -> String {
            request_id.0
        }

        let app = Router::new()
            .route("/test", get(handler))
            .layer(axum::middleware::from_fn(request_id_middleware));

        // Make a request
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check response body contains the request ID
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let request_id_str = String::from_utf8(body.to_vec()).unwrap();

        // Verify it's a valid UUID
        assert!(Uuid::parse_str(&request_id_str).is_ok());
    }

    #[tokio::test]
    async fn test_request_id_middleware_with_error_response() {
        // Handler that returns an error
        async fn handler() -> Result<&'static str, StatusCode> {
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }

        let app = Router::new()
            .route("/test", get(handler))
            .layer(axum::middleware::from_fn(request_id_middleware));

        // Make a request
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check error response still has X-Request-ID
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(response.headers().get("X-Request-ID").is_some());
    }

    #[tokio::test]
    async fn test_request_id_middleware_concurrent_requests() {
        async fn handler() -> &'static str {
            "OK"
        }

        let app = Router::new()
            .route("/test", get(handler))
            .layer(axum::middleware::from_fn(request_id_middleware));

        // Make concurrent requests
        let mut handles = vec![];
        for _ in 0..20 {
            let app_clone = app.clone();
            let handle = tokio::spawn(async move {
                app_clone
                    .oneshot(
                        axum::http::Request::builder()
                            .uri("/test")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap()
            });
            handles.push(handle);
        }

        // Collect all request IDs
        let mut ids = std::collections::HashSet::new();
        for handle in handles {
            let response = handle.await.unwrap();
            let request_id_header = response.headers().get("X-Request-ID").unwrap();
            let request_id_str = request_id_header.to_str().unwrap();
            ids.insert(request_id_str.to_string());
        }

        // All IDs should be unique even under concurrent load
        assert_eq!(ids.len(), 20);
    }

    #[test]
    fn test_request_id_multiple_clones() {
        let id1 = RequestId("original".to_string());
        let id2 = id1.clone();
        let id3 = id2.clone();

        assert_eq!(id1.0, "original");
        assert_eq!(id2.0, "original");
        assert_eq!(id3.0, "original");
    }

    #[test]
    fn test_request_id_long_string() {
        let long_string = "a".repeat(1000);
        let id = RequestId(long_string.clone());
        assert_eq!(id.0, long_string);
    }

    #[test]
    fn test_request_id_with_special_characters() {
        let special = "test-id_123!@#$%^&*()".to_string();
        let id = RequestId(special.clone());
        assert_eq!(id.0, special);
    }

    #[test]
    fn test_request_id_with_unicode() {
        let unicode = "测试-id-🚀".to_string();
        let id = RequestId(unicode.clone());
        assert_eq!(id.0, unicode);
    }
}
