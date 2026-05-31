use async_trait::async_trait;
use crate::core::store_trait::StateStoreTrait;
use crate::db::store::PostgresStateStore;
use crate::db::store::Sandbox;

/// Implement StateStoreTrait for PostgresStateStore.
#[async_trait]
impl StateStoreTrait for PostgresStateStore {
    async fn create_sandbox(
        &self,
        sandbox: Sandbox,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.create_sandbox(sandbox).await
    }

    async fn get_sandbox(&self, id: &uuid::Uuid) -> Option<Sandbox> {
        self.get_sandbox_simple(id).await
    }

    async fn get_sandbox_with_deleted(
        &self,
        id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Option<Sandbox> {
        self.get_sandbox_with_deleted(id, include_deleted).await
    }

    async fn list_sandboxes(&self) -> Vec<Sandbox> {
        self.list_sandboxes().await
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
        self.delete_sandbox(id).await
    }

    async fn list_sandboxes_owned_by(
        &self,
        api_key_id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Vec<Sandbox> {
        self.fetch_sandboxes_owned_by(api_key_id, include_deleted)
            .await
    }

    async fn get_sandbox_if_owned_by(
        &self,
        id: &uuid::Uuid,
        api_key_id: &uuid::Uuid,
    ) -> Option<Sandbox> {
        self.fetch_sandbox_if_owned_by(id, api_key_id).await
    }

    async fn get_sandbox_with_deleted_if_owned_by(
        &self,
        id: &uuid::Uuid,
        api_key_id: &uuid::Uuid,
        include_deleted: bool,
    ) -> Option<Sandbox> {
        self.fetch_sandbox_with_deleted_if_owned_by(id, api_key_id, include_deleted)
            .await
    }
}
