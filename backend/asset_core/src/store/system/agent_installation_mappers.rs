use std::path::PathBuf;

use sqlx::Row;

use crate::backend::domain::agents::{
    AgentInstallation, AgentMarketProtocol, DistributionType, InstallationStatus, Ownership,
    ProtocolStatus, RuntimeStatus,
};
use crate::backend::store::path_codec::{expand_path, normalize_path_for_storage};
use crate::backend::store::{StoreError, StoreResult};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct AgentCapabilityMeta {
    pub installation_id: String,
    pub catalog_item_version: String,
    pub protocol: String,
    pub distribution_id: String,
    pub ownership: String,
    pub resolved_program: String,
    pub args: Vec<String>,
    pub definition_json: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity_json: Option<serde_json::Value>,
    pub source_registry: String,
    pub catalog_version: String,
    pub protocol_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_checked_at: Option<String>,
}

impl From<&AgentInstallation> for AgentCapabilityMeta {
    fn from(inst: &AgentInstallation) -> Self {
        let program_raw = inst.resolved_program.to_string_lossy();
        let resolved_program =
            normalize_path_for_storage(&program_raw).unwrap_or_else(|_| program_raw.to_string());
        let mut definition_json = inst.definition_json.clone();
        if let Some(prog) = definition_json.get("program").and_then(|v| v.as_str()) {
            if let Ok(normalized) = normalize_path_for_storage(prog) {
                definition_json["program"] = serde_json::Value::String(normalized);
            }
        }

        Self {
            installation_id: inst.installation_id.clone(),
            catalog_item_version: inst.catalog_item_version.clone(),
            protocol: inst.protocol.as_str().to_string(),
            distribution_id: inst.distribution_id.clone(),
            ownership: inst.ownership.as_str().to_string(),
            resolved_program,
            args: inst.args.clone(),
            definition_json,
            integrity_json: inst.integrity_json.clone(),
            source_registry: inst.source_registry.clone(),
            catalog_version: inst.catalog_version.clone(),
            protocol_status: inst.protocol_status.as_str().to_string(),
            protocol_error_code: inst.protocol_error_code.clone(),
            protocol_error_message: inst.protocol_error_message.clone(),
            protocol_checked_at: inst.protocol_checked_at.clone(),
            model_status: inst.model_status.clone(),
            model_error_code: inst.model_error_code.clone(),
            model_checked_at: inst.model_checked_at.clone(),
            runtime_error_code: inst.runtime_error_code.clone(),
            runtime_error_message: inst.runtime_error_message.clone(),
            runtime_checked_at: inst.runtime_checked_at.clone(),
        }
    }
}

pub(super) fn row_to_installation(row: sqlx::sqlite::SqliteRow) -> StoreResult<AgentInstallation> {
    let extension_id: String = row.try_get("extension_id")?;
    let target_id: String = row.try_get("target_id")?;
    let display_name: String = row.try_get("display_name")?;
    let active_version: String = row.try_get("active_version")?;
    let distribution_value: String = row.try_get("distribution_type")?;
    let install_dir_string: Option<String> = row.try_get("install_dir")?;
    let enabled: i64 = row.try_get("enabled")?;
    let installation_status_value: String = row.try_get("installation_status")?;
    let runtime_status_value: String = row.try_get("runtime_status")?;
    let row_error_code: Option<String> = row.try_get("error_code")?;
    let row_error_message: Option<String> = row.try_get("error_message")?;
    let row_checked_at: Option<String> = row.try_get("checked_at")?;
    let installed_at: String = row.try_get("installed_at")?;
    let updated_at: String = row.try_get("updated_at")?;
    let capability_meta_json: String = row.try_get("capability_meta_json")?;

    let meta: serde_json::Value = crate::backend::store::codec::decode_json(&capability_meta_json)?;

    let installation_status = match installation_status_value.as_str() {
        "ready" => InstallationStatus::Ready,
        "incompatible" => InstallationStatus::Incompatible,
        "broken" => InstallationStatus::Broken,
        value => {
            return Err(StoreError::Storage(format!(
                "invalid installation status: {value}"
            )))
        }
    };

    let runtime_status = match runtime_status_value.as_str() {
        "unchecked" => RuntimeStatus::Unchecked,
        "ready" => RuntimeStatus::Ready,
        "runtime_missing" => RuntimeStatus::RuntimeMissing,
        "entry_missing" => RuntimeStatus::EntryMissing,
        "failed" => RuntimeStatus::Failed,
        value => {
            return Err(StoreError::Storage(format!(
                "invalid runtime status: {value}"
            )))
        }
    };

    let protocol = match meta
        .get("protocol")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("acp")
    {
        "acp" => AgentMarketProtocol::Acp,
        "native" => AgentMarketProtocol::Acp, // 平滑降级兼容历史 native 标记
        value => {
            return Err(StoreError::Storage(format!(
                "invalid installation protocol: {value}"
            )))
        }
    };

    let distribution_type = parse_distribution(&distribution_value).map_err(StoreError::Storage)?;

    let ownership = match meta
        .get("ownership")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("managed")
    {
        "system" => Ownership::System,
        "managed" => Ownership::Managed,
        value => return Err(StoreError::Storage(format!("invalid ownership: {value}"))),
    };

    let protocol_status = match meta
        .get("protocol_status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unchecked")
    {
        "unchecked" => ProtocolStatus::Unchecked,
        "ready" => ProtocolStatus::Ready,
        "auth_required" => ProtocolStatus::AuthRequired,
        "failed" => ProtocolStatus::Failed,
        "unsupported" => ProtocolStatus::Unsupported,
        _ => {
            if installation_status == InstallationStatus::Incompatible {
                ProtocolStatus::Failed
            } else {
                ProtocolStatus::Unchecked
            }
        }
    };

    let agent_id = target_id;
    let installation_id = meta
        .get("installation_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&extension_id)
        .to_string();
    let catalog_item_version = meta
        .get("catalog_item_version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&active_version)
        .to_string();
    let distribution_id = meta
        .get("distribution_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("binary")
        .to_string();
    let resolved_program_raw = meta
        .get("resolved_program")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let resolved_program = if resolved_program_raw.is_empty() {
        PathBuf::new()
    } else {
        expand_path(resolved_program_raw).unwrap_or_else(|_| PathBuf::from(resolved_program_raw))
    };
    let args: Vec<String> = meta
        .get("args")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| StoreError::Storage(e.to_string()))?
        .unwrap_or_default();
    let mut definition_json = meta
        .get("definition_json")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(prog) = definition_json.get("program").and_then(|v| v.as_str()) {
        if let Ok(expanded) = expand_path(prog) {
            definition_json["program"] =
                serde_json::Value::String(expanded.to_string_lossy().to_string());
        }
    }
    let integrity_json = meta.get("integrity_json").cloned().filter(|v| !v.is_null());
    let source_registry = meta
        .get("source_registry")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("curated")
        .to_string();
    let catalog_version = meta
        .get("catalog_version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();

    let runtime_error_code = meta
        .get("runtime_error_code")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .or_else(|| row_error_code.clone());
    let runtime_error_message = meta
        .get("runtime_error_message")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .or_else(|| row_error_message.clone());
    let runtime_checked_at = meta
        .get("runtime_checked_at")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .or_else(|| row_checked_at.clone());

    let protocol_error_code = meta
        .get("protocol_error_code")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let protocol_error_message = meta
        .get("protocol_error_message")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let protocol_checked_at = meta
        .get("protocol_checked_at")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);

    let model_status = meta
        .get("model_status")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let model_error_code = meta
        .get("model_error_code")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let model_checked_at = meta
        .get("model_checked_at")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);

    Ok(AgentInstallation {
        agent_id,
        installation_id,
        display_name,
        catalog_item_version,
        agent_version: active_version,
        protocol,
        distribution_id,
        distribution_type,
        ownership,
        install_dir: install_dir_string
            .map(|s| expand_path(&s).unwrap_or_else(|_| PathBuf::from(&s))),
        resolved_program,
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

pub(super) fn parse_distribution(value: &str) -> Result<DistributionType, String> {
    match value {
        "binary" | "system" | "archive" | "workspace" => Ok(DistributionType::Binary),
        "npx" | "uvx" => Ok(DistributionType::Npx),
        value => Err(format!("invalid distribution type: {value}")),
    }
}
