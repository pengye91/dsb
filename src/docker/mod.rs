// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! # Docker Integration
//!
//! This module provides Docker container management functionality.
//!
//! ## Modules
//!
//! - [`manager`](crate::docker::manager) - High-level Docker container management interface
//! - [`features`](crate::docker::features) - Feature detection from Docker image labels
//! - [`exec_proxy`](crate::docker::exec_proxy) - Docker exec with PTY support for SSH and terminal access
//! - [`docker_trait`](crate::docker::docker_trait) - Docker API trait abstraction for mocking and testing
//!
pub mod docker_trait;
pub mod exec_proxy;
pub mod features;
pub mod manager;

pub use docker_trait::{DockerError, DockerResult, DockerTrait};
pub use exec_proxy::{
    DockerExecProxy, DockerExecProxyTrait, ExecConfig, ExecMultiplexedStream, ExecProxyError,
    ExecReadStream, ExecWriteStream,
};
pub use manager::{DockerManager, DockerManagerError};
