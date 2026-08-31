use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::backend::runtime::{AppError, AppResult};

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct PersistentExecutionBinding {
    pub(crate) tenant_id: String,
    pub(crate) execution_context_key: String,
    pub(crate) provider_session_id: String,
    pub(crate) agent_id: String,
    pub(crate) installation_id: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) workspace_path: String,
    pub(crate) binding_version: i64,
    pub(crate) provider_metadata_json: String,
}

impl std::fmt::Debug for PersistentExecutionBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PersistentExecutionBinding")
            .field("tenant_id", &self.tenant_id)
            .field("execution_context_key", &self.execution_context_key)
            .field("provider_session_id", &"<redacted>")
            .field("agent_id", &self.agent_id)
            .field("installation_id", &self.installation_id)
            .field("model", &self.model.as_ref().map(|_| "<redacted>"))
            .field("workspace_path", &"<redacted>")
            .field("binding_version", &self.binding_version)
            .field("provider_metadata_json", &"<redacted>")
            .finish()
    }
}

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
    ) -> AppResult<Option<PersistentExecutionBinding>> {
        let row = sqlx::query("SELECT tenant_id, execution_context_key, provider_session_id, agent_id, installation_id, model, workspace_path, binding_version, provider_metadata_json FROM agent_execution_bindings WHERE tenant_id = ?1 AND execution_context_key = ?2")
            .bind(tenant_id)
            .bind(execution_context_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::external)?;
        row.map(|row| {
            Ok(PersistentExecutionBinding {
                tenant_id: row.try_get("tenant_id").map_err(AppError::external)?,
                execution_context_key: row
                    .try_get("execution_context_key")
                    .map_err(AppError::external)?,
                provider_session_id: row
                    .try_get("provider_session_id")
                    .map_err(AppError::external)?,
                agent_id: row.try_get("agent_id").map_err(AppError::external)?,
                installation_id: row.try_get("installation_id").map_err(AppError::external)?,
                model: row.try_get("model").map_err(AppError::external)?,
                workspace_path: row.try_get("workspace_path").map_err(AppError::external)?,
                binding_version: row.try_get("binding_version").map_err(AppError::external)?,
                provider_metadata_json: row
                    .try_get("provider_metadata_json")
                    .map_err(AppError::external)?,
            })
        })
        .transpose()
    }

    pub(crate) async fn save(&self, binding: &PersistentExecutionBinding) -> AppResult<()> {
        if binding.tenant_id.trim().is_empty()
            || binding.execution_context_key.trim().is_empty()
            || binding.provider_session_id.trim().is_empty()
            || binding.workspace_path.trim().is_empty()
        {
            return Err(AppError::Validation(
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
            .map_err(AppError::external)?;
        Ok(())
    }
}
