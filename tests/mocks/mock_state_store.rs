// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Mock StateStore for testing
//!
//! Provides an in-memory implementation of StateStoreTrait for testing
//! without requiring a database connection.

use dsb::core::{store_trait::StateStoreTrait, Sandbox};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct MockStateStore {
    inner: Arc<RwLock<MockStateStoreInner>>,
}

struct MockStateStoreInner {
    sandboxes: HashMap<uuid::Uuid, Sandbox>,
}

impl MockStateStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MockStateStoreInner {
                sandboxes: HashMap::new(),
            })),
        }
    }

    /// Helper method for tests to directly add a sandbox
    pub async fn add_sandbox(&self, sandbox: Sandbox) {
        let mut inner = self.inner.write().await;
        inner.sandboxes.insert(sandbox.id, sandbox);
    }

    /// Helper method for tests to get all sandboxes
    pub async fn get_all_sandboxes(&self) -> Vec<Sandbox> {
        let inner = self.inner.read().await;
        inner.sandboxes.values().cloned().collect()
    }
}

impl Default for MockStateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl StateStoreTrait for MockStateStore {
    async fn create_sandbox(
        &self,
        sandbox: Sandbox,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut inner = self.inner.write().await;
        inner.sandboxes.insert(sandbox.id, sandbox);
        Ok(())
    }

    async fn get_sandbox(&self, id: &uuid::Uuid) -> Option<Sandbox> {
        let inner = self.inner.read().await;
        inner.sandboxes.get(id).cloned()
    }

    async fn get_sandbox_with_deleted(
        &self,
        id: &uuid::Uuid,
        _include_deleted: bool,
    ) -> Option<Sandbox> {
        self.get_sandbox(id).await
    }

    async fn list_sandboxes(&self) -> Vec<Sandbox> {
        let inner = self.inner.read().await;
        inner.sandboxes.values().cloned().collect()
    }

    async fn update_sandbox(
        &self,
        sandbox: &Sandbox,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut inner = self.inner.write().await;
        inner.sandboxes.insert(sandbox.id, sandbox.clone());
        Ok(())
    }

    async fn delete_sandbox(
        &self,
        id: &uuid::Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut inner = self.inner.write().await;
        inner.sandboxes.remove(id);
        Ok(())
    }

    async fn list_sandboxes_owned_by(
        &self,
        api_key_id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Vec<Sandbox> {
        let inner = self.inner.read().await;
        inner
            .sandboxes
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
        let inner = self.inner.read().await;
        inner
            .sandboxes
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
        let inner = self.inner.read().await;
        inner
            .sandboxes
            .get(id)
            .filter(|s| {
                s.api_key_id.as_ref() == Some(api_key_id)
                    && (include_deleted || s.deleted_at.is_none())
            })
            .cloned()
    }
}

///////////////////////////////////////////////////////////////////////////////
// Tests
///////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;
    use dsb::core::types::{ActivityTracking, SandboxConfig};

    fn create_test_sandbox(id: uuid::Uuid, state: dsb::core::types::SandboxState) -> Sandbox {
        let config = SandboxConfig {
            image: "nginx:latest".to_string(),
            ..Default::default()
        };

        let now = chrono::Utc::now();

        Sandbox {
            id,
            state,
            config,
            container_id: Some("container-123".to_string()),
            created_at: now,
            updated_at: now,
            error_message: None,
            volume_mounts: vec![],
            activity: ActivityTracking {
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
    async fn test_mock_state_store_create_and_get() {
        let store = MockStateStore::new();

        let id = uuid::Uuid::new_v4();
        let sandbox = create_test_sandbox(id, dsb::core::types::SandboxState::Running);

        store.create_sandbox(sandbox.clone()).await.unwrap();

        let retrieved = store.get_sandbox(&sandbox.id).await;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, sandbox.id);
        assert_eq!(retrieved.state, dsb::core::types::SandboxState::Running);
    }

    #[tokio::test]
    async fn test_mock_state_store_list() {
        let store = MockStateStore::new();

        let sandbox1 = create_test_sandbox(
            uuid::Uuid::new_v4(),
            dsb::core::types::SandboxState::Running,
        );
        let sandbox2 = create_test_sandbox(
            uuid::Uuid::new_v4(),
            dsb::core::types::SandboxState::Stopped,
        );

        store.create_sandbox(sandbox1.clone()).await.unwrap();
        store.create_sandbox(sandbox2.clone()).await.unwrap();

        let sandboxes = store.list_sandboxes().await;
        assert_eq!(sandboxes.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_state_store_update() {
        let store = MockStateStore::new();

        let id = uuid::Uuid::new_v4();
        let mut sandbox = create_test_sandbox(id, dsb::core::types::SandboxState::Running);

        store.create_sandbox(sandbox.clone()).await.unwrap();

        // Update state
        sandbox.state = dsb::core::types::SandboxState::Stopped;
        sandbox.updated_at = chrono::Utc::now();
        store.update_sandbox(&sandbox).await.unwrap();

        let retrieved = store.get_sandbox(&sandbox.id).await.unwrap();
        assert_eq!(retrieved.state, dsb::core::types::SandboxState::Stopped);
    }

    #[tokio::test]
    async fn test_mock_state_store_delete() {
        let store = MockStateStore::new();

        let id = uuid::Uuid::new_v4();
        let sandbox = create_test_sandbox(id, dsb::core::types::SandboxState::Running);

        store.create_sandbox(sandbox.clone()).await.unwrap();
        store.delete_sandbox(&sandbox.id).await.unwrap();

        let retrieved = store.get_sandbox(&sandbox.id).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_mock_state_store_helper_methods() {
        let store = MockStateStore::new();

        let id = uuid::Uuid::new_v4();
        let sandbox = create_test_sandbox(id, dsb::core::types::SandboxState::Running);

        store.add_sandbox(sandbox.clone()).await;

        let all = store.get_all_sandboxes().await;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, sandbox.id);
    }
}
