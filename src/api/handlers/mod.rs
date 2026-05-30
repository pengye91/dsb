// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # HTTP Request Handlers
//!
//! This module contains the HTTP request handlers for the API.
//!
//! ## Modules
//!
//! - [`health`] - Health check endpoint
//! - [`config`] - Configuration endpoint for frontend
//! - [`sandbox`] - Sandbox CRUD operations, stats, streaming, and cleanup
//! - [`activities`] - Activity tracking endpoints
//! - [`ssh`] - SSH session management endpoints
//! - [`static_files`] - Static file serving and management
//! - [`admin`] - Admin API endpoints for API key management

pub mod activities;
pub mod admin;
pub mod config;
pub mod health;
pub mod images;
pub mod sandbox;
pub mod ssh;
pub mod static_files;

pub use activities::*;
pub use admin::*;
pub use config::*;
pub use health::*;
pub use images::*;
pub use sandbox::*;
pub use ssh::*;
