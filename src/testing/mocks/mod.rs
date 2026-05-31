// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Mock implementations for testing
//!
//! This module provides mock implementations of external dependencies,
//! allowing comprehensive testing without requiring running services.

#[cfg(test)]
pub mod mock_docker;

#[cfg(test)]
pub use mock_docker::{MockContainer, MockContainerState, MockDocker};
