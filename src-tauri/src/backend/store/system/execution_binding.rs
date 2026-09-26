use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::backend::domain::agents::PersistentExecutionBinding;
use crate::backend::store::{StoreError, StoreResult};

#[derive(Clone)]
pub(crate) struct PersistentBindingStore {
    pool: SqlitePool,
}

impl PersistentBindingStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(crate) async fn load(
        &self,
        tenant_id: &str,
        execution_context_key: &str,
    ) -> StoreResult<Option<PersistentExecutionBinding>> {
        let row = sqlx::query("SELECT tenant_id, execution_context_key, provider_session_id, agent_id, installation_id, model, workspace_path, binding_version, provider_metadata_json FROM agent_execution_bindings WHERE tenant_id = ?1 AND execution_context_key = ?2")
            .bind(tenant_id)
            .bind(execution_context_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::external)?;
        row.map(|row| {
            Ok(PersistentExecutionBinding {
                tenant_id: row.try_get("tenant_id").map_err(StoreError::external)?,
                execution_context_key: row
                    .try_get("execution_context_key")
                    .map_err(StoreError::external)?,
                provider_session_id: row
                    .try_get("provider_session_id")
                    .map_err(StoreError::external)?,
                agent_id: row.try_get("agent_id").map_err(StoreError::external)?,
                installation_id: row
                    .try_get("installation_id")
                    .map_err(StoreError::external)?,
                model: row.try_get("model").map_err(StoreError::external)?,
                workspace_path: row
                    .try_get("workspace_path")
                    .map_err(StoreError::external)?,
                binding_version: row
                    .try_get("binding_version")
                    .map_err(StoreError::external)?,
                provider_metadata_json: row
                    .try_get("provider_metadata_json")
                    .map_err(StoreError::external)?,
            })
        })
        .transpose()
    }

    pub(crate) async fn save(&self, binding: &PersistentExecutionBinding) -> StoreResult<()> {
        if binding.tenant_id.trim().is_empty()
            || binding.execution_context_key.trim().is_empty()
            || binding.provider_session_id.trim().is_empty()
            || binding.workspace_path.trim().is_empty()
        {
            return Err(StoreError::Validation(
                "Persistent execution binding is incomplete".to_string(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO agent_execution_bindings (tenant_id, execution_context_key, provider_session_id, agent_id, installation_id, model, workspace_path, binding_version, provider_metadata_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10) ON CONFLICT (tenant_id, execution_context_key) DO UPDATE SET provider_session_id = excluded.provider_session_id, agent_id = excluded.agent_id, installation_id = excluded.installation_id, model = excluded.model, workspace_path = excluded.workspace_path, binding_version = excluded.binding_version, provider_metadata_json = excluded.provider_metadata_json, updated_at = excluded.updated_at")
            .bind(&binding.tenant_id)
            .bind(&binding.execution_context_key)
            .bind(&binding.provider_session_id)
            .bind(&binding.agent_id)
            .bind(&binding.installation_id)
            .bind(&binding.model)
            .bind(&binding.workspace_path)
            .bind(binding.binding_version)
            .bind(&binding.provider_metadata_json)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(StoreError::external)?;
        Ok(())
    }

    pub(crate) async fn delete(
        &self,
        tenant_id: &str,
        execution_context_key: &str,
    ) -> StoreResult<()> {
        sqlx::query("DELETE FROM agent_execution_bindings WHERE tenant_id = ?1 AND execution_context_key = ?2")
            .bind(tenant_id)
            .bind(execution_context_key)
            .execute(&self.pool)
            .await
            .map_err(StoreError::external)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "execution_binding_tests.rs"]
mod tests;
