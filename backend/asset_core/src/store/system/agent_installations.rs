use std::path::PathBuf;

use sqlx::SqlitePool;

use super::agent_installation_mappers::{row_to_installation, AgentCapabilityMeta};
use crate::backend::domain::agents::{
    AgentInstallation, AgentMarketProtocol, DistributionType, InstallationStatus, Ownership,
    ProtocolStatus, RuntimeStatus,
};
use crate::backend::store::StoreResult;

#[derive(Clone, Debug)]
pub struct AgentInstallationRepository {
    pool: SqlitePool,
}

impl AgentInstallationRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn get(&self, agent_id: &str) -> StoreResult<Option<AgentInstallation>> {
        let row = sqlx::query(
            r#"
            SELECT
                extension_id, capability_kind, target_id, display_name, active_version,
                distribution_type, install_dir, enabled, installation_status, runtime_status,
                error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
            FROM app_extensions
            WHERE capability_kind = 'agent' AND extension_id = ?1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_installation).transpose()
    }

    pub async fn list(&self) -> StoreResult<Vec<AgentInstallation>> {
        let rows = sqlx::query(
            r#"
            SELECT
                extension_id, capability_kind, target_id, display_name, active_version,
                distribution_type, install_dir, enabled, installation_status, runtime_status,
                error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
            FROM app_extensions
            WHERE capability_kind = 'agent'
            ORDER BY target_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_installation).collect()
    }

    pub async fn list_registry_candidates(&self) -> StoreResult<Vec<AgentInstallation>> {
        let rows = sqlx::query(
            r#"
            SELECT
                extension_id, capability_kind, target_id, display_name, active_version,
                distribution_type, install_dir, enabled, installation_status, runtime_status,
                error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
            FROM app_extensions
            WHERE capability_kind = 'agent'
              AND enabled = 1
              AND installation_status = 'ready'
              AND runtime_status = 'ready'
              AND json_extract(capability_meta_json, '$.protocol_status') = 'ready'
            ORDER BY target_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_installation).collect()
    }

    pub async fn upsert_active(&self, installation: &AgentInstallation) -> StoreResult<()> {
        let meta = AgentCapabilityMeta::from(installation);
        let meta_json = crate::backend::store::codec::encode_json(&meta)?;
        let install_dir_str = installation.install_dir.as_ref().map(|path| {
            let s = path.to_string_lossy();
            crate::backend::store::normalize_path_for_storage(&s).unwrap_or_else(|_| s.to_string())
        });
        let error_code = installation
            .runtime_error_code
            .as_deref()
            .or(installation.protocol_error_code.as_deref());
        let error_message = installation
            .runtime_error_message
            .as_deref()
            .or(installation.protocol_error_message.as_deref());
        let checked_at = installation
            .runtime_checked_at
            .as_deref()
            .or(installation.protocol_checked_at.as_deref());

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO app_extensions (
                extension_id, capability_kind, target_id, display_name, active_version,
                distribution_type, install_dir, enabled, installation_status, runtime_status,
                error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
            ) VALUES (
                ?1, 'agent', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15
            )
            ON CONFLICT (extension_id) DO UPDATE SET
                capability_kind = excluded.capability_kind,
                target_id = excluded.target_id,
                display_name = excluded.display_name,
                active_version = excluded.active_version,
                distribution_type = excluded.distribution_type,
                install_dir = excluded.install_dir,
                enabled = excluded.enabled,
                installation_status = excluded.installation_status,
                runtime_status = excluded.runtime_status,
                error_code = excluded.error_code,
                error_message = excluded.error_message,
                checked_at = excluded.checked_at,
                installed_at = excluded.installed_at,
                updated_at = excluded.updated_at,
                capability_meta_json = excluded.capability_meta_json
            "#,
        )
        .bind(&installation.agent_id)
        .bind(&installation.agent_id)
        .bind(&installation.display_name)
        .bind(&installation.agent_version)
        .bind(installation.distribution_type.as_str())
        .bind(&install_dir_str)
        .bind(installation.enabled as i64)
        .bind(installation.installation_status.as_str())
        .bind(installation.runtime_status.as_str())
        .bind(error_code)
        .bind(error_message)
        .bind(checked_at)
        .bind(&installation.installed_at)
        .bind(&installation.updated_at)
        .bind(&meta_json)
        .execute(&mut *tx)
        .await?;

        let artifact_sha256 = installation
            .integrity_json
            .as_ref()
            .and_then(|v| v.get("sha256").and_then(serde_json::Value::as_str));
        let content_hash = installation
            .integrity_json
            .as_ref()
            .and_then(|v| v.get("content_hash").and_then(serde_json::Value::as_str))
            .unwrap_or_default();

        sqlx::query(
            r#"
            INSERT INTO app_extension_versions (
                extension_id, version, install_dir, artifact_sha256, content_hash,
                verification_status, installed_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7
            )
            ON CONFLICT (extension_id, version) DO UPDATE SET
                install_dir = excluded.install_dir,
                artifact_sha256 = COALESCE(excluded.artifact_sha256, app_extension_versions.artifact_sha256),
                content_hash = COALESCE(excluded.content_hash, app_extension_versions.content_hash),
                verification_status = COALESCE(excluded.verification_status, app_extension_versions.verification_status),
                installed_at = excluded.installed_at
            "#,
        )
        .bind(&installation.agent_id)
        .bind(&installation.agent_version)
        .bind(install_dir_str.as_deref().unwrap_or_default())
        .bind(artifact_sha256)
        .bind(content_hash)
        .bind("tested")
        .bind(&installation.installed_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn update_enabled(
        &self,
        agent_id: &str,
        enabled: bool,
        updated_at: &str,
    ) -> StoreResult<()> {
        sqlx::query(
            "UPDATE app_extensions SET enabled = ?1, updated_at = ?2 WHERE capability_kind = 'agent' AND extension_id = ?3",
        )
        .bind(enabled as i64)
        .bind(updated_at)
        .bind(agent_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_health(&self, installation: &AgentInstallation) -> StoreResult<()> {
        let error_code = installation
            .runtime_error_code
            .as_deref()
            .or(installation.protocol_error_code.as_deref());
        let error_message = installation
            .runtime_error_message
            .as_deref()
            .or(installation.protocol_error_message.as_deref());
        let checked_at = installation
            .runtime_checked_at
            .as_deref()
            .or(installation.protocol_checked_at.as_deref());

        sqlx::query(
            r#"
            UPDATE app_extensions
            SET installation_status = ?1,
                runtime_status = ?2,
                error_code = ?3,
                error_message = ?4,
                checked_at = ?5,
                updated_at = ?6,
                capability_meta_json = json_set(
                    capability_meta_json,
                    '$.protocol_status', ?7,
                    '$.protocol_error_code', ?8,
                    '$.protocol_error_message', ?9,
                    '$.protocol_checked_at', ?10,
                    '$.model_status', ?11,
                    '$.model_error_code', ?12,
                    '$.model_checked_at', ?13,
                    '$.runtime_error_code', ?14,
                    '$.runtime_error_message', ?15,
                    '$.runtime_checked_at', ?16
                )
            WHERE capability_kind = 'agent' AND extension_id = ?17
            "#,
        )
        .bind(installation.installation_status.as_str())
        .bind(installation.runtime_status.as_str())
        .bind(error_code)
        .bind(error_message)
        .bind(checked_at)
        .bind(&installation.updated_at)
        .bind(installation.protocol_status.as_str())
        .bind(&installation.protocol_error_code)
        .bind(&installation.protocol_error_message)
        .bind(&installation.protocol_checked_at)
        .bind(&installation.model_status)
        .bind(&installation.model_error_code)
        .bind(&installation.model_checked_at)
        .bind(&installation.runtime_error_code)
        .bind(&installation.runtime_error_message)
        .bind(&installation.runtime_checked_at)
        .bind(&installation.agent_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_health_unchecked(&self, updated_at: &str) -> StoreResult<u64> {
        let result = sqlx::query(
            r#"
            UPDATE app_extensions
            SET updated_at = ?1,
                capability_meta_json = json_set(
                    capability_meta_json,
                    '$.protocol_status', 'unchecked',
                    '$.protocol_error_code', NULL,
                    '$.protocol_error_message', NULL,
                    '$.protocol_checked_at', NULL,
                    '$.model_status', 'unchecked',
                    '$.model_error_code', NULL,
                    '$.model_checked_at', NULL
                )
            WHERE capability_kind = 'agent'
              AND enabled = 1
              AND installation_status IN ('ready', 'broken')
            "#,
        )
        .bind(updated_at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn mark_broken(
        &self,
        agent_id: &str,
        runtime_status: RuntimeStatus,
        error_code: &str,
        error_message: &str,
        updated_at: &str,
    ) -> StoreResult<()> {
        sqlx::query(
            r#"
            UPDATE app_extensions
            SET installation_status = 'broken',
                runtime_status = ?1,
                error_code = ?2,
                error_message = ?3,
                checked_at = ?4,
                updated_at = ?5,
                capability_meta_json = json_set(
                    capability_meta_json,
                    '$.runtime_error_code', ?2,
                    '$.runtime_error_message', ?3,
                    '$.runtime_checked_at', ?4
                )
            WHERE capability_kind = 'agent' AND extension_id = ?6
            "#,
        )
        .bind(runtime_status.as_str())
        .bind(error_code)
        .bind(error_message)
        .bind(updated_at)
        .bind(updated_at)
        .bind(agent_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_incompatible(
        &self,
        agent_id: &str,
        error_code: &str,
        error_message: &str,
        updated_at: &str,
    ) -> StoreResult<()> {
        sqlx::query(
            r#"
            UPDATE app_extensions
            SET installation_status = 'incompatible',
                error_code = ?1,
                error_message = ?2,
                checked_at = ?3,
                updated_at = ?4,
                capability_meta_json = json_set(
                    capability_meta_json,
                    '$.runtime_error_code', ?1,
                    '$.runtime_error_message', ?2,
                    '$.runtime_checked_at', ?3
                )
            WHERE capability_kind = 'agent' AND extension_id = ?5
            "#,
        )
        .bind(error_code)
        .bind(error_message)
        .bind(updated_at)
        .bind(updated_at)
        .bind(agent_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete(&self, agent_id: &str) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM app_extension_versions WHERE extension_id = ?1")
            .bind(agent_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "DELETE FROM app_extensions WHERE capability_kind = 'agent' AND extension_id = ?1",
        )
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "agent_installations_tests.rs"]
mod tests;
