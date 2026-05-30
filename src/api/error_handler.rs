// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Error Handler Middleware
//!
//! This module provides error handling middleware that formats error responses
//! based on the request path - HTML for dashboard/static routes, JSON for API routes.

use axum::{
    http::{Request, StatusCode},
    response::Response,
};
use tower::{Layer, Service};
use tracing::error;

use crate::api::error_pages::{render_api_error_response, render_error_page};

/// Determine if the response should be HTML or JSON based on the request path.
///
/// Returns true for:
/// - `/dashboard/*` routes (dashboard SPA)
/// - `/static/*` routes (static file server)
/// - Root path `/`
fn should_return_html_for_request<B>(req: &Request<B>) -> bool {
    let path = req.uri().path();
    path.starts_with("/dashboard/")
        || path.starts_with("/static/")
        || path == "/"
        || path.is_empty()
}

/// Create the error handler layer
///
/// This layer transforms error responses to the appropriate format:
/// - HTML for dashboard and static file routes
/// - JSON for API routes
#[derive(Clone, Copy, Debug)]
pub struct ErrorHandlerLayer;

impl Default for ErrorHandlerLayer {
    fn default() -> Self {
        Self
    }
}

impl ErrorHandlerLayer {
    /// Create a new error handler layer
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for ErrorHandlerLayer {
    type Service = ErrorHandlerService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ErrorHandlerService { inner }
    }
}

/// Error handler service that wraps the inner service
#[derive(Clone, Debug)]
pub struct ErrorHandlerService<S> {
    inner: S,
}

impl<S, B> Service<Request<B>> for ErrorHandlerService<S>
where
    S: Service<Request<B>, Response = Response> + Clone + Send + 'static,
    S::Future: Send,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let should_return_html = should_return_html_for_request(&req);
        let mut inner = self.inner.clone();

        Box::pin(async move {
            match inner.call(req).await {
                Ok(response) => Ok(handle_response(response, should_return_html)),
                Err(_) => {
                    error!("Request failed");
                    let response = create_error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Internal server error",
                        None,
                        should_return_html,
                    );
                    Ok(response)
                }
            }
        })
    }
}

/// Handle a successful response and check for error status codes
fn handle_response(response: Response, should_return_html: bool) -> Response {
    let status = response.status();

    // Check if this is an error status code that needs formatting
    if !status.is_success() {
        // Get the error details from response extensions or use defaults
        let (error_msg, hint) = extract_error_details(&response);

        // Only transform if we're returning HTML and this is a route that should get HTML
        if should_return_html {
            return create_error_response(status, &error_msg, hint.as_deref(), true);
        }
    }

    response
}

/// Extract error details from response extensions
fn extract_error_details(response: &Response) -> (String, Option<String>) {
    // Check if we've stored error details in extensions
    if let Some(error_info) = response.extensions().get::<ErrorInfo>() {
        return (error_info.error.clone(), error_info.hint.clone());
    }

    // Default messages based on status code
    let (error_msg, hint) = match response.status() {
        StatusCode::UNAUTHORIZED => (
            "Unauthorized".to_string(),
            Some("Authentication required. Provide a valid API key.".to_string()),
        ),
        StatusCode::FORBIDDEN => (
            "Forbidden".to_string(),
            Some("You don't have permission to access this resource.".to_string()),
        ),
        StatusCode::NOT_FOUND => (
            "Not Found".to_string(),
            Some("The requested resource was not found.".to_string()),
        ),
        StatusCode::BAD_REQUEST => (
            "Bad Request".to_string(),
            Some("The request was invalid.".to_string()),
        ),
        StatusCode::INTERNAL_SERVER_ERROR => (
            "Internal Server Error".to_string(),
            Some("An unexpected error occurred.".to_string()),
        ),
        StatusCode::SERVICE_UNAVAILABLE => (
            "Service Unavailable".to_string(),
            Some("The service is temporarily unavailable.".to_string()),
        ),
        _ => (format!("Error {}", response.status().as_u16()), None),
    };

    (error_msg, hint)
}

/// Create an error response with appropriate format
fn create_error_response(
    status: StatusCode,
    error: &str,
    hint: Option<&str>,
    return_html: bool,
) -> Response {
    if return_html {
        let html = render_error_page(status, Some(error));
        Response::builder()
            .status(status)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(html.0.into())
            .unwrap_or_else(|_| Response::new(status.to_string().into()))
    } else {
        let json = render_api_error_response(status, error, hint);
        Response::builder()
            .status(status)
            .header("Content-Type", "application/json; charset=utf-8")
            .body(json.into())
            .unwrap_or_else(|_| Response::new(status.to_string().into()))
    }
}

/// Error information stored in request/response extensions.
#[derive(Debug, Clone)]
pub struct ErrorInfo {
    /// Error message
    pub error: String,
    /// Optional hint for resolving the error
    pub hint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;

    #[tokio::test]
    async fn test_should_return_html_for_dashboard() {
        let req = Request::builder()
            .uri("/dashboard/settings")
            .body(Body::empty())
            .unwrap();
        assert!(should_return_html_for_request(&req));
    }

    #[tokio::test]
    async fn test_should_return_html_for_static() {
        let req = Request::builder()
            .uri("/static/123/file.html")
            .body(Body::empty())
            .unwrap();
        assert!(should_return_html_for_request(&req));
    }

    #[tokio::test]
    async fn test_should_return_html_for_root() {
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        assert!(should_return_html_for_request(&req));
    }

    #[tokio::test]
    async fn test_should_not_return_html_for_api() {
        let req = Request::builder()
            .uri("/sandboxes")
            .body(Body::empty())
            .unwrap();
        assert!(!should_return_html_for_request(&req));
    }

    #[tokio::test]
    async fn test_should_not_return_html_for_admin() {
        let req = Request::builder()
            .uri("/admin/api-keys")
            .body(Body::empty())
            .unwrap();
        assert!(!should_return_html_for_request(&req));
    }

    #[tokio::test]
    async fn test_create_error_response_html() {
        let response = create_error_response(
            StatusCode::NOT_FOUND,
            "File not found",
            Some("Check the path"),
            true,
        );

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/html; charset=utf-8"
        );

        let body = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.contains("404"));
        assert!(body_str.contains("File not found"));
    }

    #[tokio::test]
    async fn test_create_error_response_json() {
        let response = create_error_response(
            StatusCode::NOT_FOUND,
            "File not found",
            Some("Check the path"),
            false,
        );

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json; charset=utf-8"
        );

        let body = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.contains("\"error\":\"File not found\""));
        assert!(body_str.contains("\"hint\":\"Check the path\""));
    }

    #[tokio::test]
    async fn test_should_return_html_for_root_path() {
        let req = Request::builder().uri("/").body(Body::empty()).unwrap();
        assert!(should_return_html_for_request(&req));
    }

    #[tokio::test]
    async fn test_should_return_html_for_nested_dashboard() {
        let req = Request::builder()
            .uri("/dashboard/nested/deep/path")
            .body(Body::empty())
            .unwrap();
        assert!(should_return_html_for_request(&req));
    }

    #[tokio::test]
    async fn test_should_not_return_html_for_api_sandboxes_id() {
        let req = Request::builder()
            .uri("/sandboxes/some-uuid-here")
            .body(Body::empty())
            .unwrap();
        assert!(!should_return_html_for_request(&req));
    }

    #[tokio::test]
    async fn test_should_not_return_html_for_activities() {
        let req = Request::builder()
            .uri("/activities")
            .body(Body::empty())
            .unwrap();
        assert!(!should_return_html_for_request(&req));
    }

    #[tokio::test]
    async fn test_should_not_return_html_for_images() {
        let req = Request::builder()
            .uri("/images/pull")
            .body(Body::empty())
            .unwrap();
        assert!(!should_return_html_for_request(&req));
    }

    #[tokio::test]
    async fn test_create_error_response_without_hint() {
        let response = create_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Something went wrong",
            None,
            false,
        );

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.contains("\"error\":\"Something went wrong\""));
    }

    #[tokio::test]
    async fn test_create_error_response_500_html() {
        let response = create_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error",
            Some("An error occurred"),
            true,
        );

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = to_bytes(response.into_body(), 1_000_000).await.unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.contains("500"));
    }

    #[test]
    fn test_error_handler_layer_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<ErrorHandlerLayer>();
    }

    #[test]
    fn test_error_handler_service_is_clone() {
        // ErrorHandlerService requires S to be Clone, which we can't easily test here
        // This is just to ensure the type compiles with the layer pattern
        let _ = ErrorHandlerLayer::new();
    }

    #[test]
    fn test_error_info_structure() {
        let info = ErrorInfo {
            error: "Test error".to_string(),
            hint: Some("Test hint".to_string()),
        };
        assert_eq!(info.error, "Test error");
        assert_eq!(info.hint, Some("Test hint".to_string()));
    }
}
