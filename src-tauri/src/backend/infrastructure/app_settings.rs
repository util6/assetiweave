use crate::backend::{
    infrastructure::{InfraError, InfraResult},
    store,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) const CONFIG_FILE_NAME: &str = "config.json";
pub(crate) const CONVERSATION_ADAPTER_DIR_NAME: &str = "conversation-adapters";
pub(crate) const SETTINGS_SCHEMA_VERSION: u32 = 4;
pub(crate) const DEFAULT_AI_RUNTIME_CLI: &str = "opencode";
pub(crate) const DEFAULT_CONVERSATION_FULL_SYNC_ON_STARTUP: bool = true;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AppLocale {
    Zh,
    En,
}

impl AppLocale {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Zh => "zh",
            Self::En => "en",
        }
    }
}

#[derive(Debug, Clone, Serialize)]

pub(crate) struct AppSettingsFile {
    pub(crate) config_dir: String,
    pub(crate) config_path: String,
    pub(crate) conversation_adapter_dir: String,
    pub(crate) display_config_dir: String,
    pub(crate) display_config_path: String,
    pub(crate) display_conversation_adapter_dir: String,
    pub(crate) settings: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSettingsDocument {
    pub(crate) schema_version: u32,
    pub(crate) settings: Value,
}

impl AppSettingsDocument {
    pub(crate) fn new(settings: Value) -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            settings,
        }
    }
}

fn default_recent_window_hours() -> u32 {
    48
}

fn default_watermark_time_1() -> String {
    "02:00".to_string()
}

fn default_watermark_time_2() -> String {
    "14:00".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemorySettings {
    #[serde(default = "default_true")]
    pub(crate) generation_enabled: bool,
    #[serde(default = "default_true")]
    pub(crate) usage_enabled: bool,
    #[serde(default = "default_recent_window_hours")]
    pub(crate) recent_window_hours: u32,
    #[serde(default = "default_watermark_time_1")]
    pub(crate) watermark_time_1: String,
    #[serde(default = "default_watermark_time_2")]
    pub(crate) watermark_time_2: String,
    #[serde(default)]
    pub(crate) generation_skill_asset_id: Option<String>,
    #[serde(default)]
    pub(crate) excluded_session_ids: Vec<String>,
    #[serde(default)]
    pub(crate) excluded_source_ids: Vec<String>,
}

fn is_valid_hh_mm(time_str: &str) -> bool {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    let Ok(h) = parts[0].parse::<u32>() else {
        return false;
    };
    let Ok(m) = parts[1].parse::<u32>() else {
        return false;
    };
    h < 24 && m < 60 && parts[0].len() == 2 && parts[1].len() == 2
}

impl MemorySettings {
    pub(crate) fn validate_schedule(&self) -> InfraResult<()> {
        if self.recent_window_hours != 24
            && self.recent_window_hours != 48
            && self.recent_window_hours != 72
        {
            return Err(InfraError::Validation(format!(
                "MEMORY_SCHEDULE_INVALID: invalid window hours {}, allowed: 24, 48, 72",
                self.recent_window_hours
            )));
        }
        if !is_valid_hh_mm(&self.watermark_time_1) {
            return Err(InfraError::Validation(format!(
                "MEMORY_SCHEDULE_INVALID: invalid watermark_time_1 format: {}",
                self.watermark_time_1
            )));
        }
        if !is_valid_hh_mm(&self.watermark_time_2) {
            return Err(InfraError::Validation(format!(
                "MEMORY_SCHEDULE_INVALID: invalid watermark_time_2 format: {}",
                self.watermark_time_2
            )));
        }
        if self.watermark_time_1 == self.watermark_time_2 {
            return Err(InfraError::Validation(
                "MEMORY_SCHEDULE_INVALID: watermark_time_1 and watermark_time_2 cannot be identical"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

fn default_true() -> bool {
    true
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            generation_enabled: true,
            usage_enabled: true,
            recent_window_hours: 48,
            watermark_time_1: "02:00".to_string(),
            watermark_time_2: "14:00".to_string(),
            generation_skill_asset_id: None,
            excluded_session_ids: Vec::new(),
            excluded_source_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConversationsSettings {
    #[serde(default = "default_conversation_full_sync")]
    pub(crate) auto_full_sync_on_startup: bool,
}

fn default_conversation_full_sync() -> bool {
    DEFAULT_CONVERSATION_FULL_SYNC_ON_STARTUP
}

impl Default for ConversationsSettings {
    fn default() -> Self {
        Self {
            auto_full_sync_on_startup: DEFAULT_CONVERSATION_FULL_SYNC_ON_STARTUP,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiRuntimeSettings {
    #[serde(default = "default_ai_runtime_cli")]
    pub(crate) cli: String,
    #[serde(default)]
    pub(crate) model: Option<String>,
}

fn default_ai_runtime_cli() -> String {
    DEFAULT_AI_RUNTIME_CLI.to_string()
}

impl Default for AiRuntimeSettings {
    fn default() -> Self {
        Self {
            cli: DEFAULT_AI_RUNTIME_CLI.to_string(),
            model: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentAssignmentSetting {
    pub(crate) agent_id: String,
    #[serde(default)]
    pub(crate) model_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackendSettings {
    #[serde(default)]
    pub(crate) memory: MemorySettings,
    #[serde(default)]
    pub(crate) conversations: ConversationsSettings,
    #[serde(default)]
    pub(crate) ai_runtime: AiRuntimeSettings,
    #[serde(default)]
    pub(crate) agent_assignments: std::collections::BTreeMap<String, AgentAssignmentSetting>,
    #[serde(default)]
    pub(crate) locale: Option<AppLocale>,
    #[serde(default)]
    pub(crate) column_layouts: std::collections::BTreeMap<String, Vec<f64>>,
}

impl BackendSettings {
    pub(crate) fn from_document(document: &AppSettingsDocument) -> InfraResult<Self> {
        Self::from_value(&document.settings)
    }

    pub(crate) fn from_value(value: &Value) -> InfraResult<Self> {
        serde_json::from_value(value.clone())
            .map_err(|error| InfraError::Validation(format!("invalid settings document: {error}")))
    }

    pub(crate) fn merge_into_document(
        &self,
        mut document: AppSettingsDocument,
    ) -> InfraResult<AppSettingsDocument> {
        let root = document.settings.as_object_mut().ok_or_else(|| {
            InfraError::Validation("settings root must be a JSON object".to_string())
        })?;

        root.insert(
            "memory".to_string(),
            serde_json::to_value(&self.memory).map_err(InfraError::external)?,
        );
        if root.contains_key("conversations")
            || self.conversations != ConversationsSettings::default()
        {
            root.insert(
                "conversations".to_string(),
                serde_json::to_value(&self.conversations).map_err(InfraError::external)?,
            );
        }
        root.insert(
            "aiRuntime".to_string(),
            serde_json::to_value(&self.ai_runtime).map_err(InfraError::external)?,
        );
        root.insert(
            "agentAssignments".to_string(),
            serde_json::to_value(&self.agent_assignments).map_err(InfraError::external)?,
        );
        root.insert(
            "locale".to_string(),
            serde_json::to_value(&self.locale).map_err(InfraError::external)?,
        );
        root.insert(
            "columnLayouts".to_string(),
            serde_json::to_value(&self.column_layouts).map_err(InfraError::external)?,
        );

        Ok(document)
    }

    pub(crate) fn is_memory_generation_enabled(&self) -> bool {
        self.memory.generation_enabled
    }

    pub(crate) fn is_memory_usage_enabled(&self) -> bool {
        self.memory.usage_enabled
    }

    pub(crate) fn is_session_excluded(&self, session_id: &str) -> bool {
        self.memory
            .excluded_session_ids
            .iter()
            .any(|id| id == session_id)
    }

    pub(crate) fn is_source_excluded(&self, source_id: &str) -> bool {
        self.memory
            .excluded_source_ids
            .iter()
            .any(|id| id == source_id)
    }

    pub(crate) fn auto_full_sync_on_startup(&self) -> bool {
        self.conversations.auto_full_sync_on_startup
    }

    pub(crate) fn resolve_agent_for_action(&self, action: &str) -> Option<(&str, Option<&str>)> {
        self.agent_assignments
            .get(action)
            .map(|assignment| (assignment.agent_id.as_str(), assignment.model_id.as_deref()))
    }
}

pub(crate) async fn get_app_settings_sqlx(pool: &sqlx::SqlitePool) -> InfraResult<AppSettingsFile> {
    let paths = app_settings_paths()?;
    ensure_settings_dirs(&paths)?;
    let settings = read_app_settings_value_sqlx(pool).await?;
    Ok(paths.into_file(settings))
}

pub(crate) async fn save_app_settings_sqlx(
    pool: &sqlx::SqlitePool,
    settings: Value,
) -> InfraResult<AppSettingsFile> {
    let paths = app_settings_paths()?;
    ensure_settings_dirs(&paths)?;
    let settings = canonicalize_settings(settings)?;
    // Validate known typed slices
    let typed = BackendSettings::from_value(&settings)?;
    // Merge known typed slices into the raw document to ensure unknown fields round-trip
    let document = AppSettingsDocument::new(settings);
    let merged = typed.merge_into_document(document)?;

    store::save_app_settings_sqlx(pool, SETTINGS_SCHEMA_VERSION, &merged.settings).await?;
    let persisted = read_app_settings_value_sqlx(pool).await?;
    let canonical = canonicalize_settings(persisted)?;
    Ok(paths.into_file(canonical))
}

pub(crate) async fn initialize_app_locale_sqlx(
    pool: &sqlx::SqlitePool,
    locale: AppLocale,
) -> InfraResult<AppSettingsFile> {
    let paths = app_settings_paths()?;
    ensure_settings_dirs(&paths)?;
    let _ = read_app_settings_value_sqlx(pool).await?;
    let settings = store::initialize_app_locale_sqlx(pool, locale.as_str()).await?;
    let canonical = canonicalize_settings(settings)?;
    Ok(paths.into_file(canonical))
}

pub(crate) async fn read_app_settings_value_sqlx(pool: &sqlx::SqlitePool) -> InfraResult<Value> {
    load_or_import_app_settings_sqlx(pool).await
}

/// Load the authoritative SQLite settings row. The legacy JSON document is
/// consulted exactly once, only when the row does not exist yet.
pub(crate) async fn load_or_import_app_settings_sqlx(
    pool: &sqlx::SqlitePool,
) -> InfraResult<Value> {
    if let Some((schema_version, stored)) = store::load_app_settings_sqlx(pool).await? {
        if schema_version > SETTINGS_SCHEMA_VERSION {
            return Err(InfraError::Validation(format!(
                "settings schema version {schema_version} is newer than supported version {SETTINGS_SCHEMA_VERSION}"
            )));
        }
        let settings = canonicalize_settings(stored)?;
        if schema_version < SETTINGS_SCHEMA_VERSION {
            store::save_app_settings_sqlx(pool, SETTINGS_SCHEMA_VERSION, &settings).await?;
        }
        return Ok(settings);
    }

    let paths = app_settings_paths()?;
    ensure_settings_dirs(&paths)?;
    let document = match read_settings_document(&paths.config_path) {
        Ok(doc) => doc,
        Err(err) => {
            tracing::warn!(
                "Failed to read legacy settings document from {:?}: {err}; falling back to default",
                paths.config_path
            );
            default_document()
        }
    };
    let imported = canonicalize_settings(document.settings)?;
    store::save_app_settings_sqlx(pool, SETTINGS_SCHEMA_VERSION, &imported).await?;
    Ok(imported)
}

pub(crate) use super::app_settings_canonical::*;
pub(crate) use super::app_settings_io::*;

#[cfg(test)]
#[path = "app_settings_tests.rs"]
mod tests;
