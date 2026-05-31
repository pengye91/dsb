// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Kubernetes backend for DSB sandbox management.
//!
//! This module implements the [`SandboxManager`] trait using the Kubernetes API
//! via `kube-rs`. It manages sandbox Pods, Services, and Custom Resource Definitions.

pub mod crd;
pub mod manager;
pub mod operator;
pub mod types;
