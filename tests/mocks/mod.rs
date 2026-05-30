// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Mock implementations for testing
//!
//! This module provides mock implementations of external dependencies,
//! allowing comprehensive testing without requiring running services.

pub mod mock_docker;
pub mod mock_http_client;
pub mod mock_state_store;

#[allow(unused_imports)]
pub use mock_docker::{MockContainer, MockContainerState, MockDocker};
#[allow(unused_imports)]
pub use mock_http_client::{HttpClientTrait, HttpError, HttpResponse, MockHttpClient};
