// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # VNC Token Authentication Module
//!
//! This module provides session token-based authentication for VNC connections.
//!
//! ## Architecture
//!
//! ```text
//! Token Generation (HTTP API)          Token Validation (WebSocket)
//! POST /auth/vnc/tokens                WS /vnc/{sandbox_id}?token=xxx
//!        │                                      │
//!        ▼                                      ▼
//! VncTokenService                      VncTokenService
//!   │                                              │
//!   ├─► Validate sandbox exists                    ├─► Extract token from query
//!   ├─► Generate secure token                      ├─► Validate against Postgres
//!   ├─► Store in PostgreSQL                        ├─► Check sandbox_id matches
//!   └─► Audit logging                              └─► Return 401 if invalid
//! ```
//!
//! ## Components
//!
//! - **types**: Token data structures
//! - **token_store**: Storage abstraction (Postgres/in-memory)
//! - **token_service**: Business logic layer
//!
//! ## Usage
//!
//! ```rust,no_run,ignore
//! use dsb::auth::tokens::{VncTokenService, CreateVncTokenRequest};
//!
//! let service = VncTokenService::new(store, sandbox_service, config);
//!
//! // Generate token
//! let request = CreateVncTokenRequest {
//!     sandbox_id: uuid::Uuid::new_v4(),
//!     ttl_secs: 3600,
//! };
//! let token = service.create_token(request, None).await?;
//!
//! // Validate token
//! let result = service.validate_token(&token.token).await?;
//! ```

pub mod token_service;
pub mod token_store;
pub mod types;

pub use token_service::VncTokenService;
pub use token_store::{VncTokenStore, POSTGRES_VNC_TOKEN_STORE};
pub use types::{CreateVncTokenRequest, TokenValidationResult, VncSessionToken};
