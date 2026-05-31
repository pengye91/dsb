// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Shared constants and helpers for k8s E2E tests.
//!
//! These tests run against a real Kubernetes cluster with the MCP server deployed
//! in-cluster. They use the modern MCP service endpoints (`/mcp/dsb/web`,
//! `/mcp/dsb/browser`, `/mcp/dsb/sandbox`) instead of the legacy `/mcp` endpoint.

use std::env;

/// Default MCP web service URL for k8s tests.
/// Override with `K8S_MCP_WEB_URL` env var.
pub fn k8s_mcp_web_url() -> String {
    env::var("K8S_MCP_WEB_URL")
        .or_else(|_| env::var("DSB_MCP_URL"))
        .unwrap_or_else(|_| "http://localhost:3333/mcp/dsb/web".to_string())
}

/// Default MCP browser service URL for k8s tests.
/// Override with `K8S_MCP_BROWSER_URL` env var.
pub fn k8s_mcp_browser_url() -> String {
    env::var("K8S_MCP_BROWSER_URL")
        .unwrap_or_else(|_| "http://localhost:3333/mcp/dsb/browser".to_string())
}

/// Default MCP sandbox service URL for k8s tests.
/// Override with `K8S_MCP_SANDBOX_URL` env var.
pub fn k8s_mcp_sandbox_url() -> String {
    env::var("K8S_MCP_SANDBOX_URL")
        .unwrap_or_else(|_| "http://localhost:3333/mcp/dsb/sandbox".to_string())
}

/// DSB API URL for direct HTTP calls (dashboard auth tests).
/// Override with `K8S_DSB_API_URL` or `DSB_K8S_API_URL` env var.
pub fn k8s_dsb_api_url() -> String {
    env::var("K8S_DSB_API_URL")
        .or_else(|_| env::var("DSB_K8S_API_URL"))
        .unwrap_or_else(|_| "http://localhost:18080".to_string())
}

/// API key for authentication.
/// Reads from `DSB_API_KEY` env var.
pub fn k8s_api_key() -> String {
    env::var("DSB_API_KEY").unwrap_or_default()
}

/// Sandbox image to use for k8s tests.
/// Defaults to the registry k8s-v0.0.5 tag.
pub fn k8s_sandbox_image() -> String {
    env::var("DSB_TEST_SANDBOX_IMAGE")
        .unwrap_or_else(|_| "ghcr.io/dsb/sandbox:k8s-v0.0.5".to_string())
}

/// Session ID prefix for k8s tests.
pub fn k8s_session_id(test_name: &str) -> String {
    format!(
        "k8s-e2e-{}-{}",
        test_name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    )
}

/// Wikipedia pages used for real-world web fetch/browser tests.
pub const WIKIPEDIA_PAGES: &[(&str, &[&str])] = &[
    (
        "https://en.wikipedia.org/wiki/Rust_(programming_language)",
        &["Rust", "programming", "memory safety", "Mozilla"],
    ),
    (
        "https://en.wikipedia.org/wiki/Kubernetes",
        &["Kubernetes", "container", "orchestration", "Google"],
    ),
    (
        "https://en.wikipedia.org/wiki/World_Wide_Web",
        &["World Wide Web", "Tim Berners-Lee", "HTTP", "HTML"],
    ),
    (
        "https://en.wikipedia.org/wiki/Artificial_intelligence",
        &[
            "artificial intelligence",
            "machine learning",
            "neural network",
        ],
    ),
    (
        "https://en.wikipedia.org/wiki/Quantum_computing",
        &[
            "quantum computing",
            "qubit",
            "superposition",
            "entanglement",
        ],
    ),
];
