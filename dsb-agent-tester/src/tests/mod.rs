// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Test modules for dsb-agent-tester
//!
//! This module contains integration and E2E tests for the DSB agent tester.

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
mod monorail_tests;

#[cfg(test)]
mod scenario_tests;

#[cfg(test)]
mod web_tools_tests;

// K8s E2E tests — run against a real Kubernetes cluster with MCP server deployed in-cluster.
// These are gated by the `k8s-e2e` feature flag so they don't run during normal `cargo test`.
#[cfg(all(test, feature = "k8s-e2e"))]
pub mod k8s_mod;

#[cfg(all(test, feature = "k8s-e2e"))]
mod k8s_concurrent_tools_tests;

#[cfg(all(test, feature = "k8s-e2e"))]
mod k8s_static_files_tests;

#[cfg(all(test, feature = "k8s-e2e"))]
mod k8s_dashboard_auth_tests;
#[cfg(test)]
mod k8s_douban_tests;
