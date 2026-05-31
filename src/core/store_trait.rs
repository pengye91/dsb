// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # State Store Trait
//!
//! This module defines the trait for state store implementations.
//!
//! ## Overview
//!
//! The [`StateStoreTrait`] defines the interface that all state stores must implement.
//! This allows different storage backends (in-memory, PostgreSQL, etc.) to be used
//! interchangeably.
//!
//! ## Implementations
//!
//! - [`StateStore`](crate::core::state::StateStore) - In-memory store
//! - [`PostgresStateStore`](crate::db::PostgresStateStore) - PostgreSQL store

use crate::core::types::Sandbox;
use async_trait::async_trait;

/// Trait for state store implementations.
///
/// This trait defines the interface for storing and retrieving sandbox state.
/// Multiple implementations can exist (in-memory, PostgreSQL, etc.)
#[async_trait]
pub trait StateStoreTrait: Send + Sync {
    /// Creates a new sandbox in the store.
    ///
    /// # Arguments
    ///
    /// * `sandbox` - The sandbox instance to store
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the sandbox was stored successfully.
    ///
    /// # Errors
    ///
    /// Returns an error if the sandbox could not be stored.
    async fn create_sandbox(
        &self,
        sandbox: Sandbox,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Retrieves a sandbox by its ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to retrieve
    ///
    /// # Returns
    ///
    /// - `Some(Sandbox)` - If a sandbox with the given ID exists
    /// - `None` - If no sandbox with the given ID exists
    async fn get_sandbox(&self, id: &uuid::Uuid) -> Option<Sandbox>;

    /// Retrieves a sandbox by its ID, optionally including deleted sandboxes.
    ///
    /// This method is useful for cleanup operations that need to access
    /// soft-deleted sandboxes to remove their containers and volumes.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to retrieve
    /// * `include_deleted` - Whether to include soft-deleted sandboxes
    ///
    /// # Returns
    ///
    /// - `Some(Sandbox)` - If a sandbox with the given ID exists
    /// - `None` - If no sandbox with the given ID exists
    async fn get_sandbox_with_deleted(
        &self,
        id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Option<Sandbox>;

    /// Lists all sandboxes in the store.
    ///
    /// # Returns
    ///
    /// A vector containing all sandboxes in the store.
    async fn list_sandboxes(&self) -> Vec<Sandbox>;

    /// Updates an existing sandbox in the store.
    ///
    /// # Arguments
    ///
    /// * `sandbox` - The sandbox instance to store (must have a valid ID)
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the update was successful.
    ///
    /// # Errors
    ///
    /// Returns an error if the update failed.
    async fn update_sandbox(
        &self,
        sandbox: &Sandbox,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Deletes a sandbox from the store.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to delete
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` even if the sandbox doesn't exist (idempotent operation).
    ///
    /// # Errors
    ///
    /// Returns an error if the deletion failed.
    async fn delete_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    /// Lists sandboxes owned by a specific API key.
    ///
    /// Only returns non-deleted sandboxes where `api_key_id` matches.
    ///
    /// # Arguments
    ///
    /// * `api_key_id` - The API key ID to filter by
    ///
    /// # Arguments
    ///
    /// * `api_key_id` - The API key ID that must own the sandbox
    /// * `include_deleted` - Whether to include soft-deleted sandboxes
    ///
    /// # Returns
    ///
    /// A vector containing all sandboxes owned by the given API key.
    async fn list_sandboxes_owned_by(
        &self,
        api_key_id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Vec<Sandbox>;

    /// Retrieves a sandbox by ID, only if owned by the given API key.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to retrieve
    /// * `api_key_id` - The API key ID that must own the sandbox
    ///
    /// # Returns
    ///
    /// - `Some(Sandbox)` - If sandbox exists, is not deleted, and is owned by the API key
    /// - `None` - If sandbox doesn't exist, is deleted, or is owned by a different API key
    async fn get_sandbox_if_owned_by(
        &self,
        id: &uuid::Uuid,
        api_key_id: &uuid::Uuid,
    ) -> Option<Sandbox>;

    /// Retrieves a sandbox by ID owned by an API key, optionally including deleted.
    ///
    /// # Arguments
    ///
    /// * `id` - The UUID of the sandbox to retrieve
    /// * `api_key_id` - The API key ID that must own the sandbox
    /// * `include_deleted` - Whether to include soft-deleted sandboxes
    async fn get_sandbox_with_deleted_if_owned_by(
        &self,
        id: &uuid::Uuid,
        api_key_id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Option<Sandbox>;
}
