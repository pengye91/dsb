// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Configuration Handler
//!
//! This module provides the configuration endpoint for the frontend dashboard.
//!
//! The endpoint exposes non-sensitive configuration values that the frontend
//! needs to function correctly, such as default sandbox images, timeouts, etc.
//!
//! ## Security
//!
//! This endpoint is **protected by API key authentication** like all other endpoints.
//! It only exposes non-sensitive configuration values. Sensitive values like:
//! - API keys
//! - Database connection strings
//! - Secret tokens
//!
//! are intentionally excluded from the response.

use crate::core::SandboxService;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Frontend-safe configuration response
///
/// Only includes non-sensitive configuration that the frontend dashboard needs.
/// Sensitive values (API keys, secrets, etc.) are intentionally excluded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendConfig {
    /// Default sandbox image for the dashboard
    pub default_sandbox_image: String,

    /// Default inactivity timeout in minutes
    pub default_inactivity_timeout: u64,

    /// Whether authentication is required (for UI display purposes)
    pub authentication_required: bool,
}

impl From<&SandboxService> for FrontendConfig {
    fn from(service: &SandboxService) -> Self {
        Self {
            default_sandbox_image: service.default_sandbox_image.clone(),
            default_inactivity_timeout: service.default_inactivity_timeout,
            authentication_required: service.authentication_required,
        }
    }
}

impl From<&Arc<SandboxService>> for FrontendConfig {
    fn from(service: &Arc<SandboxService>) -> Self {
        Self {
            default_sandbox_image: service.default_sandbox_image.clone(),
            default_inactivity_timeout: service.default_inactivity_timeout,
            authentication_required: service.authentication_required,
        }
    }
}

/// Get frontend configuration
///
/// Returns non-sensitive configuration values for the frontend dashboard.
///
/// # Security
///
/// This endpoint is protected by API key authentication. It only exposes
/// safe configuration values - no secrets, API keys, or sensitive data.
///
/// # Response
///
/// Returns JSON with configuration values like default sandbox image, timeouts, etc.
///
/// # Example
///
/// ```bash
/// curl -H "X-API-Key: your-api-key" http://localhost:8080/config
/// ```
///
/// Returns:
/// ```json
/// {
///   "default_sandbox_image": "dsb/sandbox:latest",
///   "default_inactivity_timeout": 30,
///   "authentication_required": true
/// }
/// ```
pub async fn get_config(State(service): State<Arc<SandboxService>>) -> Json<FrontendConfig> {
    let frontend_config = FrontendConfig::from(&service);
    Json(frontend_config)
}
