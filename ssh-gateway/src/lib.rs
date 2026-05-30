// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # DSB SSH Gateway Library
//!
//! This library provides SSH gateway functionality for DSB sandboxes.
//!
//! ## Overview
//!
//! The SSH gateway:
//! - Accepts SSH connections on a configured port
//! - Authenticates users via public key
//! - Authorizes sandbox access via DSB API
//! - Creates Docker exec instances with PTY
//! - Forwards data bidirectionally between SSH client and container
//!
//! ## Modules
//!
//! - [`ssh`] - SSH server implementation with russh
//! - [`docker`] - Docker exec proxy for container communication
//! - [`session`] - Session manager for DSB API integration

#![warn(missing_docs)]

pub mod docker;
pub mod k8s;
pub mod session;
pub mod ssh;

// Re-export commonly used types at the crate root
pub use docker::DockerExecProxy;
pub use k8s::K8sExecProxy;
pub use session::SessionManager;
pub use ssh::{ConnectionState, SshConfig, SshServer};
