// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # In-Memory State Store
//!
//! This module provides a thread-safe, in-memory store for managing sandbox state.
//!
//! ## Overview
//!
//! The [`StateStore`] is a simple, asynchronous key-value store built on top of
//! `tokio::sync::RwLock` and a `HashMap`. It provides CRUD operations for managing
//! sandbox instances with thread-safe concurrent access.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │         StateStore                  │
//! │  ┌───────────────────────────────┐ │
//! │  │ Arc<RwLock<HashMap<Uuid,      │ │
//! │  │            Sandbox>>>         │ │
//! │  └───────────────────────────────┘ │
//! │                                     │
//! │  - Thread-safe (Arc + RwLock)       │
//! │  - Async operations (.await)        │
//! │  - In-memory only (no persistence)  │
//! └─────────────────────────────────────┘
//! ```
//!
//! ## Usage Example
//!
//! ```rust,no_run,ignore
//! use dsb::core::{StateStore, Sandbox, SandboxConfig, SandboxState};
//! use dsb::core::types::{ActivityTracking, VolumeMount};
//! use chrono::Utc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let store = StateStore::new();
//!
//!     // Create a sandbox
//!     let sandbox = Sandbox {
//!         id: uuid::Uuid::new_v4(),
//!         config: SandboxConfig::default(),
//!         state: SandboxState::Creating,
//!         container_id: None,
//!         created_at: Utc::now(),
//!         updated_at: Utc::now(),
//!         error_message: None,
//!         volume_mounts: vec![],
//!         activity: ActivityTracking {
//!             last_api_activity: Utc::now(),
//!             last_container_activity: None,
//!             activity_count: 0,
//!         },
//!         inactivity_timeout_minutes: None,
//!     };
//!
//!     // Store it
//!     store.create_sandbox(sandbox.clone()).await.unwrap();
//!
//!     // Retrieve it
//!     let retrieved = store.get_sandbox(&sandbox.id).await.unwrap();
//!     assert_eq!(retrieved.id, sandbox.id);
//!
//!     // List all
//!     let all = store.list_sandboxes().await;
//!     assert_eq!(all.len(), 1);
//! }
//! ```
//!
//! ## Thread Safety
//!
//! The `StateStore` is designed to be safely shared across threads and async tasks.
//! It uses `Arc<RwLock<T>>` to allow:
//!
//! - Multiple concurrent readers (read operations don't block each other)
//! - Exclusive write access (writes block both reads and other writes)
//!
//! ## Limitations
//!
//! - **No persistence**: Data is lost when the process restarts
//! - **Memory-based**: All sandboxes are kept in memory
//! - **No eviction**: Old sandboxes are never automatically removed
//!
//! For production use, consider implementing persistence with a database.

use crate::core::store_trait::StateStoreTrait;
use crate::core::types::Sandbox;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Thread-safe in-memory store for managing sandbox state.
///
/// This struct provides CRUD (Create, Read, Update, Delete) operations for
/// sandbox instances with safe concurrent access across multiple async tasks.
///
/// # Type Parameters
///
/// The store is generic over the sandbox type but is typically used with
/// the [`Sandbox`] type from [`crate::core::types`].
///
/// # Example
///
/// ```rust
/// use dsb::core::StateStore;
///
/// // Create a new store
/// let store = StateStore::new();
///
/// // Clone is cheap - just clones the Arc
/// let store_clone = store.clone();
/// ```
#[derive(Clone)]
pub struct StateStore {
    /// Internal storage: Arc-wrapped RwLock for thread-safe concurrent access
    sandboxes: Arc<RwLock<HashMap<uuid::Uuid, Sandbox>>>,
}

impl StateStore {
    /// Creates a new empty state store.
    ///
    /// # Example
    ///
    /// ```rust
    /// use dsb::core::StateStore;
    ///
    /// let store = StateStore::new();
    /// ```
    pub fn new() -> Self {
        Self {
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Creates a new sandbox in the store.
    ///
    /// This method inserts a new sandbox into the store. If a sandbox with
    /// the same ID already exists, it will be overwritten.
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
    /// This method currently doesn't return errors but uses a Result type
    /// for future compatibility with persistent storage backends.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::{StateStore, Sandbox, SandboxConfig, SandboxState};
    /// # use dsb::core::types::ActivityTracking;
    /// # use chrono::Utc;
    /// # async fn example() {
    /// let store = StateStore::new();
    /// let now = Utc::now();
    ///
    /// let sandbox = Sandbox {
    ///     id: uuid::Uuid::new_v4(),
    ///     config: SandboxConfig::default(),
    ///     state: SandboxState::Creating,
    ///     container_id: None,
    ///     created_at: now,
    ///     updated_at: now,
    ///     error_message: None,
    ///     volume_mounts: vec![],
    ///     activity: ActivityTracking {
    ///         last_api_activity: now,
    ///         last_container_activity: None,
    ///         activity_count: 0,
    ///     },
    ///     inactivity_timeout_minutes: None,
    /// };
    ///
    /// store.create_sandbox(sandbox).await.unwrap();
    /// # }
    /// ```
    pub async fn create_sandbox(
        &self,
        sandbox: Sandbox,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.insert(sandbox.id, sandbox);
        Ok(())
    }

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
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::{StateStore, Sandbox, SandboxConfig, SandboxState};
    /// # use chrono::Utc;
    /// # use uuid::Uuid;
    /// # async fn example() {
    /// let store = StateStore::new();
    /// let id = Uuid::new_v4();
    ///
    /// // Try to get a non-existent sandbox
    /// let result = store.get_sandbox(&id).await;
    /// assert!(result.is_none());
    /// # }
    /// ```
    pub async fn get_sandbox(&self, id: &uuid::Uuid) -> Option<Sandbox> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.get(id).cloned()
    }

    /// Retrieves a sandbox by ID, optionally including deleted sandboxes.
    ///
    /// For the in-memory store, this is equivalent to `get_sandbox` since
    /// deleted sandboxes are kept in memory.
    pub async fn get_sandbox_with_deleted(
        &self,
        id: &uuid::Uuid,
        _include_deleted: bool,
    ) -> Option<Sandbox> {
        self.get_sandbox(id).await
    }

    /// Lists all sandboxes in the store.
    ///
    /// # Returns
    ///
    /// A vector containing all sandboxes in the store. The order is not guaranteed.
    ///
    /// # Performance Note
    ///
    /// This method clones all sandbox instances. For large numbers of sandboxes,
    /// this may be expensive. Consider pagination or streaming for production use.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::StateStore;
    /// # async fn example() {
    /// let store = StateStore::new();
    ///
    /// // List all sandboxes (empty store)
    /// let all = store.list_sandboxes().await;
    /// assert_eq!(all.len(), 0);
    /// # }
    /// ```
    pub async fn list_sandboxes(&self) -> Vec<Sandbox> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.values().cloned().collect()
    }

    /// Updates an existing sandbox in the store.
    ///
    /// This method replaces the existing sandbox with the provided one.
    /// If the sandbox doesn't exist, it will be created (upsert operation).
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
    /// This method currently doesn't return errors but uses a Result type
    /// for future compatibility.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::{StateStore, Sandbox, SandboxConfig, SandboxState};
    /// # use dsb::core::types::ActivityTracking;
    /// # use chrono::Utc;
    /// # async fn example() {
    /// let store = StateStore::new();
    /// let now = Utc::now();
    ///
    /// let mut sandbox = Sandbox {
    ///     id: uuid::Uuid::new_v4(),
    ///     config: SandboxConfig::default(),
    ///     state: SandboxState::Creating,
    ///     container_id: None,
    ///     created_at: now,
    ///     updated_at: now,
    ///     error_message: None,
    ///     volume_mounts: vec![],
    ///     activity: ActivityTracking {
    ///         last_api_activity: now,
    ///         last_container_activity: None,
    ///         activity_count: 0,
    ///     },
    ///     inactivity_timeout_minutes: None,
    /// };
    ///
    /// // Create initial state
    /// store.create_sandbox(sandbox.clone()).await.unwrap();
    ///
    /// // Update to running state
    /// sandbox.state = SandboxState::Running;
    /// sandbox.updated_at = Utc::now();
    /// store.update_sandbox(&sandbox).await.unwrap();
    /// # }
    /// ```
    pub async fn update_sandbox(
        &self,
        sandbox: &Sandbox,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.insert(sandbox.id, sandbox.clone());
        Ok(())
    }

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
    /// This method currently doesn't return errors but uses a Result type
    /// for future compatibility.
    ///
    /// # Example
    ///
    /// ```rust,no_run,ignore
    /// # use dsb::core::{StateStore, Sandbox, SandboxConfig, SandboxState};
    /// # use chrono::Utc;
    /// # async fn example() {
    /// let store = StateStore::new();
    /// let id = uuid::Uuid::new_v4();
    ///
    /// // Delete non-existent sandbox (no error)
    /// store.delete_sandbox(&id).await.unwrap();
    /// # }
    /// ```
    pub async fn delete_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.remove(id);
        Ok(())
    }
}

impl Default for StateStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Implement StateStoreTrait for StateStore.
#[async_trait]
impl StateStoreTrait for StateStore {
    async fn create_sandbox(
        &self,
        sandbox: Sandbox,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.insert(sandbox.id, sandbox);
        Ok(())
    }

    async fn get_sandbox(&self, id: &uuid::Uuid) -> Option<Sandbox> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.get(id).cloned()
    }

    async fn get_sandbox_with_deleted(
        &self,
        id: &uuid::Uuid,
        _include_deleted: bool,
    ) -> Option<Sandbox> {
        self.get_sandbox(id).await
    }

    async fn list_sandboxes(&self) -> Vec<Sandbox> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.values().cloned().collect()
    }

    async fn update_sandbox(
        &self,
        sandbox: &Sandbox,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.update_sandbox(sandbox).await
    }

    async fn delete_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.remove(id);
        Ok(())
    }

    async fn list_sandboxes_owned_by(
        &self,
        api_key_id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Vec<Sandbox> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes
            .values()
            .filter(|s| {
                s.api_key_id.as_ref() == Some(api_key_id)
                    && (include_deleted || s.deleted_at.is_none())
            })
            .cloned()
            .collect()
    }

    async fn get_sandbox_if_owned_by(
        &self,
        id: &uuid::Uuid,
        api_key_id: &uuid::Uuid,
    ) -> Option<Sandbox> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes
            .get(id)
            .filter(|s| s.api_key_id.as_ref() == Some(api_key_id) && s.deleted_at.is_none())
            .cloned()
    }

    async fn get_sandbox_with_deleted_if_owned_by(
        &self,
        id: &uuid::Uuid,
        api_key_id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Option<Sandbox> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes
            .get(id)
            .filter(|s| {
                s.api_key_id.as_ref() == Some(api_key_id)
                    && (include_deleted || s.deleted_at.is_none())
            })
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{SandboxConfig, SandboxState};

    fn create_test_sandbox() -> Sandbox {
        let now = chrono::Utc::now();
        Sandbox {
            id: uuid::Uuid::new_v4(),
            config: SandboxConfig::default(),
            state: SandboxState::Creating,
            container_id: None,
            created_at: now,
            updated_at: now,
            error_message: None,
            volume_mounts: vec![],
            activity: crate::core::types::ActivityTracking {
                last_api_activity: now,
                last_container_activity: None,
                activity_count: 0,
            },
            inactivity_timeout_minutes: None,
            deleted_at: None,
            deleted_by: None,
            api_key_id: None,
        }
    }

    #[tokio::test]
    async fn test_create_and_get_sandbox() {
        let store = StateStore::new();
        let sandbox = create_test_sandbox();
        let id = sandbox.id;

        store.create_sandbox(sandbox).await.unwrap();

        let retrieved = store.get_sandbox(&id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_get_nonexistent_sandbox() {
        let store = StateStore::new();
        let result = store.get_sandbox(&uuid::Uuid::new_v4()).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_sandboxes() {
        let store = StateStore::new();

        // Initially empty
        let list = store.list_sandboxes().await;
        assert_eq!(list.len(), 0);

        // Add two sandboxes
        store.create_sandbox(create_test_sandbox()).await.unwrap();
        store.create_sandbox(create_test_sandbox()).await.unwrap();

        // List should have two
        let list = store.list_sandboxes().await;
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_update_sandbox() {
        let store = StateStore::new();
        let mut sandbox = create_test_sandbox();
        let id = sandbox.id;

        // Create with initial state
        store.create_sandbox(sandbox.clone()).await.unwrap();

        // Update state
        sandbox.state = SandboxState::Running;
        store.update_sandbox(&sandbox).await.unwrap();

        // Verify update
        let retrieved = store.get_sandbox(&id).await.unwrap();
        assert_eq!(retrieved.state, SandboxState::Running);
    }

    #[tokio::test]
    async fn test_delete_sandbox() {
        let store = StateStore::new();
        let sandbox = create_test_sandbox();
        let id = sandbox.id;

        // Create
        store.create_sandbox(sandbox).await.unwrap();
        assert!(store.get_sandbox(&id).await.is_some());

        // Delete
        store.delete_sandbox(&id).await.unwrap();
        assert!(store.get_sandbox(&id).await.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_sandbox() {
        let store = StateStore::new();
        // Should not error
        store.delete_sandbox(&uuid::Uuid::new_v4()).await.unwrap();
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let store = Arc::new(StateStore::new());
        let mut handles = vec![];

        // Spawn 10 tasks, each creating a sandbox
        for _ in 0..10 {
            let store_clone = Arc::clone(&store);
            let handle = tokio::spawn(async move {
                let sandbox = create_test_sandbox();
                store_clone.create_sandbox(sandbox).await.unwrap();
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        // Should have 10 sandboxes
        let list = store.list_sandboxes().await;
        assert_eq!(list.len(), 10);
    }

    #[tokio::test]
    async fn test_default() {
        let store = StateStore::default();
        let list = store.list_sandboxes().await;
        assert_eq!(list.len(), 0);
    }
}
