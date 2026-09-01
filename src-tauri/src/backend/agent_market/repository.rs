use std::path::PathBuf;

use sqlx::{Row, SqlitePool};

use super::types::{
    AgentInstallation, AgentMarketProtocol, DistributionType, InstallationStatus, Ownership,
    ProtocolStatus, RuntimeStatus,
};

#[derive(Clone, Debug)]
pub(crate) struct AgentInstallationRepository {
    pool: SqlitePool,
}

impl AgentInstallationRepository {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(crate) async fn get(&self, agent_id: &str) -> Result<Option<AgentInstallation>, String> {
        let row = sqlx::query("SELECT * FROM app_agent_installations WHERE agent_id = ?1")
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| error.to_string())?;
        row.map(row_to_installation).transpose()
    }

    pub(crate) async fn list(&self) -> Result<Vec<AgentInstallation>, String> {
        let rows = sqlx::query("SELECT * FROM app_agent_installations ORDER BY agent_id")
            .fetch_all(&self.pool)
            .await
            .map_err(|error| error.to_string())?;
        rows.into_iter().map(row_to_installation).collect()
    }

    pub(crate) async fn list_registry_candidates(&self) -> Result<Vec<AgentInstallation>, String> {
        let rows = sqlx::query("SELECT * FROM app_agent_installations WHERE enabled = 1 AND installation_status = 'ready' AND runtime_status = 'ready' AND protocol_status = 'ready' AND (protocol != 'acp' OR model_status = 'ready') ORDER BY agent_id")
            .fetch_all(&self.pool).await.map_err(|error| error.to_string())?;
        rows.into_iter().map(row_to_installation).collect()
    }

    pub(crate) async fn upsert_active(
        &self,
        installation: &AgentInstallation,
    ) -> Result<(), String> {
        let args_json =
            serde_json::to_string(&installation.args).map_err(|error| error.to_string())?;
        let definition_json = installation.definition_json.to_string();
        let integrity_json = installation
            .integrity_json
            .as_ref()
            .map(ToString::to_string);
        sqlx::query(
            "INSERT INTO app_agent_installations (agent_id, installation_id, display_name, catalog_item_version, agent_version, protocol, distribution_id, distribution_type, ownership, install_dir, resolved_program, args_json, definition_json, integrity_json, source_registry, catalog_version, enabled, installation_status, runtime_status, runtime_error_code, runtime_error_message, runtime_checked_at, protocol_status, protocol_error_code, protocol_error_message, protocol_checked_at, model_status, model_error_code, model_checked_at, installed_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31) ON CONFLICT (agent_id) DO UPDATE SET installation_id=excluded.installation_id, display_name=excluded.display_name, catalog_item_version=excluded.catalog_item_version, agent_version=excluded.agent_version, protocol=excluded.protocol, distribution_id=excluded.distribution_id, distribution_type=excluded.distribution_type, ownership=excluded.ownership, install_dir=excluded.install_dir, resolved_program=excluded.resolved_program, args_json=excluded.args_json, definition_json=excluded.definition_json, integrity_json=excluded.integrity_json, source_registry=excluded.source_registry, catalog_version=excluded.catalog_version, enabled=excluded.enabled, installation_status=excluded.installation_status, runtime_status=excluded.runtime_status, runtime_error_code=excluded.runtime_error_code, runtime_error_message=excluded.runtime_error_message, runtime_checked_at=excluded.runtime_checked_at, protocol_status=excluded.protocol_status, protocol_error_code=excluded.protocol_error_code, protocol_error_message=excluded.protocol_error_message, protocol_checked_at=excluded.protocol_checked_at, model_status=excluded.model_status, model_error_code=excluded.model_error_code, model_checked_at=excluded.model_checked_at, installed_at=excluded.installed_at, updated_at=excluded.updated_at"
        )
        .bind(&installation.agent_id).bind(&installation.installation_id).bind(&installation.display_name)
        .bind(&installation.catalog_item_version).bind(&installation.agent_version).bind(installation.protocol.as_str())
        .bind(&installation.distribution_id).bind(installation.distribution_type.as_str()).bind(installation.ownership.as_str())
        .bind(installation.install_dir.as_ref().map(|path| path.to_string_lossy().to_string()))
        .bind(installation.resolved_program.to_string_lossy().to_string()).bind(args_json).bind(definition_json).bind(integrity_json)
        .bind(&installation.source_registry).bind(&installation.catalog_version).bind(installation.enabled as i64)
        .bind(installation.installation_status.as_str()).bind(installation.runtime_status.as_str())
        .bind(&installation.runtime_error_code).bind(&installation.runtime_error_message).bind(&installation.runtime_checked_at)
        .bind(installation.protocol_status.as_str()).bind(&installation.protocol_error_code).bind(&installation.protocol_error_message)
        .bind(&installation.protocol_checked_at).bind(&installation.model_status).bind(&installation.model_error_code).bind(&installation.model_checked_at)
        .bind(&installation.installed_at).bind(&installation.updated_at)
        .execute(&self.pool).await.map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) async fn update_enabled(
        &self,
        agent_id: &str,
        enabled: bool,
        updated_at: &str,
    ) -> Result<(), String> {
        sqlx::query(
            "UPDATE app_agent_installations SET enabled = ?1, updated_at = ?2 WHERE agent_id = ?3",
        )
        .bind(enabled as i64)
        .bind(updated_at)
        .bind(agent_id)
        .execute(&self.pool)
        .await
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) async fn update_health(
        &self,
        installation: &AgentInstallation,
    ) -> Result<(), String> {
        sqlx::query("UPDATE app_agent_installations SET installation_status = ?1, runtime_status = ?2, runtime_error_code = ?3, runtime_error_message = ?4, runtime_checked_at = ?5, protocol_status = ?6, protocol_error_code = ?7, protocol_error_message = ?8, protocol_checked_at = ?9, model_status = ?10, model_error_code = ?11, model_checked_at = ?12, updated_at = ?13 WHERE agent_id = ?14")
            .bind(installation.installation_status.as_str())
            .bind(installation.runtime_status.as_str()).bind(&installation.runtime_error_code).bind(&installation.runtime_error_message).bind(&installation.runtime_checked_at)
            .bind(installation.protocol_status.as_str()).bind(&installation.protocol_error_code).bind(&installation.protocol_error_message).bind(&installation.protocol_checked_at)
            .bind(&installation.model_status).bind(&installation.model_error_code).bind(&installation.model_checked_at).bind(&installation.updated_at)
            .bind(&installation.agent_id).execute(&self.pool).await.map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) async fn mark_health_unchecked(&self, updated_at: &str) -> Result<u64, String> {
        let result = sqlx::query("UPDATE app_agent_installations SET protocol_status = 'unchecked', protocol_error_code = NULL, protocol_error_message = NULL, protocol_checked_at = NULL, model_status = 'unchecked', model_error_code = NULL, model_checked_at = NULL, updated_at = ?1 WHERE enabled = 1 AND installation_status IN ('ready', 'broken')")
            .bind(updated_at)
            .execute(&self.pool)
            .await
            .map_err(|error| error.to_string())?;
        Ok(result.rows_affected())
    }

    pub(crate) async fn mark_broken(
        &self,
        agent_id: &str,
        runtime_status: RuntimeStatus,
        error_code: &str,
        error_message: &str,
        updated_at: &str,
    ) -> Result<(), String> {
        sqlx::query("UPDATE app_agent_installations SET installation_status = 'broken', runtime_status = ?1, runtime_error_code = ?2, runtime_error_message = ?3, runtime_checked_at = ?4, updated_at = ?5 WHERE agent_id = ?6")
            .bind(runtime_status.as_str())
            .bind(error_code)
            .bind(error_message)
            .bind(updated_at)
            .bind(updated_at)
            .bind(agent_id)
            .execute(&self.pool)
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) async fn delete(&self, agent_id: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM app_agent_installations WHERE agent_id = ?1")
            .bind(agent_id)
            .execute(&self.pool)
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn row_to_installation(row: sqlx::sqlite::SqliteRow) -> Result<AgentInstallation, String> {
    let protocol_value: String = row.try_get("protocol").map_err(|error| error.to_string())?;
    let distribution_value: String = row
        .try_get("distribution_type")
        .map_err(|error| error.to_string())?;
    let ownership_value: String = row
        .try_get("ownership")
        .map_err(|error| error.to_string())?;
    let installation_status_value: String = row
        .try_get("installation_status")
        .map_err(|error| error.to_string())?;
    let runtime_status_value: String = row
        .try_get("runtime_status")
        .map_err(|error| error.to_string())?;
    let protocol_status_value: String = row
        .try_get("protocol_status")
        .map_err(|error| error.to_string())?;
    let protocol = match protocol_value.as_str() {
        "acp" => AgentMarketProtocol::Acp,
        "native" => AgentMarketProtocol::Native,
        value => return Err(format!("invalid installation protocol: {value}")),
    };
    let distribution_type = parse_distribution(distribution_value.as_str())?;
    let ownership = match ownership_value.as_str() {
        "system" => Ownership::System,
        "managed" => Ownership::Managed,
        value => return Err(format!("invalid ownership: {value}")),
    };
    let installation_status = match installation_status_value.as_str() {
        "ready" => InstallationStatus::Ready,
        "incompatible" => InstallationStatus::Incompatible,
        "broken" => InstallationStatus::Broken,
        value => return Err(format!("invalid installation status: {value}")),
    };
    let runtime_status = match runtime_status_value.as_str() {
        "unchecked" => RuntimeStatus::Unchecked,
        "ready" => RuntimeStatus::Ready,
        "runtime_missing" => RuntimeStatus::RuntimeMissing,
        "entry_missing" => RuntimeStatus::EntryMissing,
        "failed" => RuntimeStatus::Failed,
        value => return Err(format!("invalid runtime status: {value}")),
    };
    let protocol_status = match protocol_status_value.as_str() {
        "unchecked" => ProtocolStatus::Unchecked,
        "ready" => ProtocolStatus::Ready,
        "auth_required" => ProtocolStatus::AuthRequired,
        "failed" => ProtocolStatus::Failed,
        "unsupported" => ProtocolStatus::Unsupported,
        value => return Err(format!("invalid protocol status: {value}")),
    };
    let args_json: String = row
        .try_get("args_json")
        .map_err(|error| error.to_string())?;
    let definition_json_string: String = row
        .try_get("definition_json")
        .map_err(|error| error.to_string())?;
    let args = serde_json::from_str(&args_json).map_err(|error| error.to_string())?;
    let definition_json =
        serde_json::from_str(&definition_json_string).map_err(|error| error.to_string())?;
    let integrity_string: Option<String> = row
        .try_get("integrity_json")
        .map_err(|error| error.to_string())?;
    let integrity_json = integrity_string
        .map(|value| serde_json::from_str(&value).map_err(|error| error.to_string()))
        .transpose()?;
    let agent_id: String = row.try_get("agent_id").map_err(|error| error.to_string())?;
    let installation_id: String = row
        .try_get("installation_id")
        .map_err(|error| error.to_string())?;
    let display_name: String = row
        .try_get("display_name")
        .map_err(|error| error.to_string())?;
    let catalog_item_version: String = row
        .try_get("catalog_item_version")
        .map_err(|error| error.to_string())?;
    let agent_version: String = row
        .try_get("agent_version")
        .map_err(|error| error.to_string())?;
    let distribution_id: String = row
        .try_get("distribution_id")
        .map_err(|error| error.to_string())?;
    let install_dir_string: Option<String> = row
        .try_get("install_dir")
        .map_err(|error| error.to_string())?;
    let resolved_program: String = row
        .try_get("resolved_program")
        .map_err(|error| error.to_string())?;
    let source_registry: String = row
        .try_get("source_registry")
        .map_err(|error| error.to_string())?;
    let catalog_version: String = row
        .try_get("catalog_version")
        .map_err(|error| error.to_string())?;
    let enabled: i64 = row.try_get("enabled").map_err(|error| error.to_string())?;
    let runtime_error_code: Option<String> = row
        .try_get("runtime_error_code")
        .map_err(|error| error.to_string())?;
    let runtime_error_message: Option<String> = row
        .try_get("runtime_error_message")
        .map_err(|error| error.to_string())?;
    let runtime_checked_at: Option<String> = row
        .try_get("runtime_checked_at")
        .map_err(|error| error.to_string())?;
    let protocol_error_code: Option<String> = row
        .try_get("protocol_error_code")
        .map_err(|error| error.to_string())?;
    let protocol_error_message: Option<String> = row
        .try_get("protocol_error_message")
        .map_err(|error| error.to_string())?;
    let protocol_checked_at: Option<String> = row
        .try_get("protocol_checked_at")
        .map_err(|error| error.to_string())?;
    let model_status: Option<String> = row
        .try_get("model_status")
        .map_err(|error| error.to_string())?;
    let model_error_code: Option<String> = row
        .try_get("model_error_code")
        .map_err(|error| error.to_string())?;
    let model_checked_at: Option<String> = row
        .try_get("model_checked_at")
        .map_err(|error| error.to_string())?;
    let installed_at: String = row
        .try_get("installed_at")
        .map_err(|error| error.to_string())?;
    let updated_at: String = row
        .try_get("updated_at")
        .map_err(|error| error.to_string())?;
    Ok(AgentInstallation {
        agent_id,
        installation_id,
        display_name,
        catalog_item_version,
        agent_version,
        protocol,
        distribution_id,
        distribution_type,
        ownership,
        install_dir: install_dir_string.map(PathBuf::from),
        resolved_program: PathBuf::from(resolved_program),
        args,
        definition_json,
        integrity_json,
        source_registry,
        catalog_version,
        enabled: enabled != 0,
        installation_status,
        runtime_status,
        runtime_error_code,
        runtime_error_message,
        runtime_checked_at,
        protocol_status,
        protocol_error_code,
        protocol_error_message,
        protocol_checked_at,
        model_status,
        model_error_code,
        model_checked_at,
        installed_at,
        updated_at,
    })
}

fn parse_distribution(value: &str) -> Result<DistributionType, String> {
    match value {
        "system" => Ok(DistributionType::System),
        "binary" => Ok(DistributionType::Binary),
        "npx" => Ok(DistributionType::Npx),
        "uvx" => Ok(DistributionType::Uvx),
        value => Err(format!("invalid distribution type: {value}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::store::Database;
    use std::time::SystemTime;

    #[test]
    fn repository_is_application_scoped_and_upsert_keeps_one_current_row() {
        let path = std::env::temp_dir().join(format!(
            "assetiweave-agent-repo-{}.db",
            uuid::Uuid::new_v4()
        ));
        let database = Database::open_initialized(&path).expect("database");
        let repository = AgentInstallationRepository::new(database.pool().clone());
        let now = format!("{:?}", SystemTime::now());
        let installation = fixture("agent", "installation", &now);
        database
            .block_on(repository.upsert_active(&installation))
            .expect("upsert");
        database
            .block_on(repository.upsert_active(&AgentInstallation {
                installation_id: "replacement".to_string(),
                ..installation.clone()
            }))
            .expect("replacement");
        let rows = database.block_on(repository.list()).expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].installation_id, "replacement");
        let _ = std::fs::remove_file(path);
    }

    fn fixture(agent_id: &str, installation_id: &str, now: &str) -> AgentInstallation {
        AgentInstallation {
            agent_id: agent_id.to_string(),
            installation_id: installation_id.to_string(),
            display_name: "Agent".to_string(),
            catalog_item_version: "1.0.0".to_string(),
            agent_version: "1.0.0".to_string(),
            protocol: AgentMarketProtocol::Acp,
            distribution_id: "system".to_string(),
            distribution_type: DistributionType::System,
            ownership: Ownership::System,
            install_dir: None,
            resolved_program: PathBuf::from("/usr/bin/agent"),
            args: Vec::new(),
            definition_json: serde_json::json!({}),
            integrity_json: None,
            source_registry: "agent".to_string(),
            catalog_version: "2026.08.16.1".to_string(),
            enabled: true,
            installation_status: InstallationStatus::Ready,
            runtime_status: RuntimeStatus::Ready,
            runtime_error_code: None,
            runtime_error_message: None,
            runtime_checked_at: None,
            protocol_status: ProtocolStatus::Ready,
            protocol_error_code: None,
            protocol_error_message: None,
            protocol_checked_at: None,
            model_status: None,
            model_error_code: None,
            model_checked_at: None,
            installed_at: now.to_string(),
            updated_at: now.to_string(),
        }
    }
}
