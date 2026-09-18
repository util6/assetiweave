use crate::backend::{runtime::AppError, runtime::AppResult, store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

const CONFIG_FILE_NAME: &str = "config.json";
const CONVERSATION_ADAPTER_DIR_NAME: &str = "conversation-adapters";
pub(crate) const SETTINGS_SCHEMA_VERSION: u32 = 4;
const DEFAULT_AI_RUNTIME_CLI: &str = "opencode";
const DEFAULT_CONVERSATION_FULL_SYNC_ON_STARTUP: bool = true;

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
    pub(crate) fn validate_schedule(&self) -> AppResult<()> {
        if self.recent_window_hours != 24
            && self.recent_window_hours != 48
            && self.recent_window_hours != 72
        {
            return Err(AppError::Validation(format!(
                "MEMORY_SCHEDULE_INVALID: invalid window hours {}, allowed: 24, 48, 72",
                self.recent_window_hours
            )));
        }
        if !is_valid_hh_mm(&self.watermark_time_1) {
            return Err(AppError::Validation(format!(
                "MEMORY_SCHEDULE_INVALID: invalid watermark_time_1 format: {}",
                self.watermark_time_1
            )));
        }
        if !is_valid_hh_mm(&self.watermark_time_2) {
            return Err(AppError::Validation(format!(
                "MEMORY_SCHEDULE_INVALID: invalid watermark_time_2 format: {}",
                self.watermark_time_2
            )));
        }
        if self.watermark_time_1 == self.watermark_time_2 {
            return Err(AppError::Validation(
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
    pub(crate) fn from_document(document: &AppSettingsDocument) -> AppResult<Self> {
        Self::from_value(&document.settings)
    }

    pub(crate) fn from_value(value: &Value) -> AppResult<Self> {
        serde_json::from_value(value.clone())
            .map_err(|error| AppError::Validation(format!("invalid settings document: {error}")))
    }

    pub(crate) fn merge_into_document(
        &self,
        mut document: AppSettingsDocument,
    ) -> AppResult<AppSettingsDocument> {
        let root = document.settings.as_object_mut().ok_or_else(|| {
            AppError::Validation("settings root must be a JSON object".to_string())
        })?;

        root.insert(
            "memory".to_string(),
            serde_json::to_value(&self.memory).map_err(AppError::external)?,
        );
        if root.contains_key("conversations")
            || self.conversations != ConversationsSettings::default()
        {
            root.insert(
                "conversations".to_string(),
                serde_json::to_value(&self.conversations).map_err(AppError::external)?,
            );
        }
        root.insert(
            "aiRuntime".to_string(),
            serde_json::to_value(&self.ai_runtime).map_err(AppError::external)?,
        );
        root.insert(
            "agentAssignments".to_string(),
            serde_json::to_value(&self.agent_assignments).map_err(AppError::external)?,
        );
        root.insert(
            "locale".to_string(),
            serde_json::to_value(&self.locale).map_err(AppError::external)?,
        );
        root.insert(
            "columnLayouts".to_string(),
            serde_json::to_value(&self.column_layouts).map_err(AppError::external)?,
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

pub(crate) async fn get_app_settings_sqlx(pool: &sqlx::SqlitePool) -> AppResult<AppSettingsFile> {
    let paths = app_settings_paths()?;
    ensure_settings_dirs(&paths)?;
    let settings = read_app_settings_value_sqlx(pool).await?;
    Ok(paths.into_file(settings))
}

pub(crate) async fn save_app_settings_sqlx(
    pool: &sqlx::SqlitePool,
    settings: Value,
) -> AppResult<AppSettingsFile> {
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
) -> AppResult<AppSettingsFile> {
    let paths = app_settings_paths()?;
    ensure_settings_dirs(&paths)?;
    let _ = read_app_settings_value_sqlx(pool).await?;
    let settings = store::initialize_app_locale_sqlx(pool, locale).await?;
    let canonical = canonicalize_settings(settings)?;
    Ok(paths.into_file(canonical))
}

pub(crate) async fn read_app_settings_value_sqlx(pool: &sqlx::SqlitePool) -> AppResult<Value> {
    load_or_import_app_settings_sqlx(pool).await
}

/// Load the authoritative SQLite settings row. The legacy JSON document is
/// consulted exactly once, only when the row does not exist yet.
pub(crate) async fn load_or_import_app_settings_sqlx(pool: &sqlx::SqlitePool) -> AppResult<Value> {
    if let Some((schema_version, stored)) = store::load_app_settings_sqlx(pool).await? {
        if schema_version > SETTINGS_SCHEMA_VERSION {
            return Err(AppError::Validation(format!(
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
    let imported = canonicalize_settings(read_settings_document(&paths.config_path)?.settings)?;
    store::save_app_settings_sqlx(pool, SETTINGS_SCHEMA_VERSION, &imported).await?;
    Ok(imported)
}

pub(crate) fn conversation_adapter_dir() -> AppResult<PathBuf> {
    Ok(app_settings_paths()?.conversation_adapter_dir)
}

struct AppSettingsPaths {
    config_dir: PathBuf,
    config_path: PathBuf,
    conversation_adapter_dir: PathBuf,
}

impl AppSettingsPaths {
    fn into_file(self, settings: Value) -> AppSettingsFile {
        let config_dir = self.config_dir.to_string_lossy().to_string();
        let config_path = self.config_path.to_string_lossy().to_string();
        let conversation_adapter_dir = self.conversation_adapter_dir.to_string_lossy().to_string();
        AppSettingsFile {
            display_config_dir: crate::backend::path_utils::display_path_or_original(&config_dir),
            display_config_path: crate::backend::path_utils::display_path_or_original(&config_path),
            display_conversation_adapter_dir: crate::backend::path_utils::display_path_or_original(
                &conversation_adapter_dir,
            ),
            config_dir,
            config_path,
            conversation_adapter_dir,
            settings,
        }
    }
}

fn app_settings_paths() -> AppResult<AppSettingsPaths> {
    let config_dir = app_config_dir()?;
    Ok(AppSettingsPaths {
        config_path: config_dir.join(CONFIG_FILE_NAME),
        conversation_adapter_dir: config_dir.join(CONVERSATION_ADAPTER_DIR_NAME),
        config_dir,
    })
}

fn app_config_dir() -> AppResult<PathBuf> {
    Ok(crate::backend::runtime::config::runtime_config()?
        .home_dir
        .clone())
}

fn ensure_settings_dirs(paths: &AppSettingsPaths) -> AppResult<()> {
    fs::create_dir_all(&paths.config_dir).map_err(AppError::external)?;
    Ok(fs::create_dir_all(&paths.conversation_adapter_dir).map_err(AppError::external)?)
}

fn read_settings_document(path: &Path) -> AppResult<AppSettingsDocument> {
    if !path.exists() {
        let document = default_document();
        write_settings_document(path, &document)?;
        return Ok(document);
    }

    let content = fs::read_to_string(path).map_err(AppError::external)?;
    let parsed: Value = serde_json::from_str(&content)
        .map_err(|error| format!("解析设置文件失败: {} ({error})", path.to_string_lossy()))
        .map_err(AppError::external)?;
    Ok(normalize_document(parsed))
}

fn normalize_settings_paths(mut settings: Value) -> AppResult<Value> {
    normalize_shared_ai_settings(&mut settings);
    for path in [
        &["dataBackup", "customDirectory"][..],
        &["conversationRuntimeOverrides", "bash"][..],
        &["conversationRuntimeOverrides", "node"][..],
        &["conversationRuntimeOverrides", "python"][..],
    ] {
        normalize_json_path_setting(&mut settings, path)?;
    }
    Ok(settings)
}

pub(crate) fn canonicalize_settings(settings: Value) -> AppResult<Value> {
    let mut settings = normalize_settings_paths(settings)?;
    let Some(root) = settings.as_object_mut() else {
        return Ok(settings);
    };
    // These maps were migration inputs. Canonical action assignments contain
    // the complete agent/model selection and are the only execution source.
    root.remove("agentCapabilityAssignments");
    root.remove("agentModels");
    if let Some(translation) = root
        .get_mut("conversationTranslation")
        .and_then(Value::as_object_mut)
    {
        translation.remove("cli");
        translation.remove("model");
    }

    if let Some(locale_val) = root.get("locale") {
        if !locale_val.is_null() {
            match locale_val.as_str() {
                Some("zh") | Some("en") => {}
                _ => {
                    return Err(AppError::Validation(format!(
                        "invalid locale value: {locale_val}"
                    )));
                }
            }
        }
    } else {
        root.insert("locale".to_string(), Value::Null);
    }

    if let Some(layouts_val) = root.get("columnLayouts") {
        if let Some(layouts_obj) = layouts_val.as_object() {
            for (key, array_val) in layouts_obj {
                let Some(arr) = array_val.as_array() else {
                    return Err(AppError::Validation(format!(
                        "columnLayouts entry '{key}' must be an array"
                    )));
                };
                if arr.len() < 2 || arr.len() > 16 {
                    return Err(AppError::Validation(format!(
                        "columnLayouts entry '{key}' must have between 2 and 16 elements"
                    )));
                }
                for item in arr {
                    let Some(num) = item.as_f64() else {
                        return Err(AppError::Validation(format!(
                            "columnLayouts entry '{key}' elements must be positive numbers"
                        )));
                    };
                    if !num.is_finite() || num <= 0.0 {
                        return Err(AppError::Validation(format!(
                            "columnLayouts entry '{key}' elements must be positive finite numbers"
                        )));
                    }
                }
            }
        } else {
            return Err(AppError::Validation(
                "columnLayouts must be an object".to_string(),
            ));
        }
    } else {
        root.insert("columnLayouts".to_string(), json!({}));
    }

    Ok(settings)
}

fn normalize_shared_ai_settings(settings: &mut Value) {
    let Some(root) = settings.as_object_mut() else {
        return;
    };

    let legacy_translation = root
        .get("conversationTranslation")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let stored_runtime = root
        .get("aiRuntime")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let cli = normalize_ai_runtime_cli(
        stored_runtime
            .get("cli")
            .or_else(|| legacy_translation.get("cli")),
    );
    let model = normalize_ai_runtime_model(
        stored_runtime
            .get("model")
            .or_else(|| legacy_translation.get("model")),
    );
    root.insert(
        "aiRuntime".to_string(),
        json!({ "cli": cli, "model": model }),
    );
    let mut agent_capabilities = root
        .get("agentCapabilityAssignments")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let supported_capabilities = [
        "cardTranslation",
        "memory",
        "memory.extraction",
        "memory.generation",
        "memory.project",
        "memory.global",
        "promptOptimization",
    ];
    let unknown_capabilities = agent_capabilities
        .keys()
        .filter(|key| !supported_capabilities.contains(&key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    for key in unknown_capabilities {
        agent_capabilities.remove(&key);
        tracing::warn!(
            action = "settings.agent_capability",
            capability = %key,
            "未知的 Agent capability 已禁用"
        );
    }
    for service_id in ["cardTranslation", "memory", "promptOptimization"] {
        let agent_id = normalize_agent_capability_agent_id(agent_capabilities.get(service_id), cli);
        agent_capabilities.insert(service_id.to_string(), Value::String(agent_id));
    }
    let memory_agent = agent_capabilities
        .get("memory")
        .and_then(Value::as_str)
        .unwrap_or(cli)
        .to_string();
    for service_id in [
        "memory.extraction",
        "memory.generation",
        "memory.project",
        "memory.global",
        "memory.recall",
    ] {
        let agent_id =
            normalize_agent_capability_agent_id(agent_capabilities.get(service_id), &memory_agent);
        agent_capabilities.insert(service_id.to_string(), Value::String(agent_id));
    }
    root.insert(
        "agentCapabilityAssignments".to_string(),
        Value::Object(agent_capabilities),
    );
    root.insert(
        "agentAssignments".to_string(),
        normalize_canonical_agent_assignments(root, cli, &model),
    );
    root.insert("settingsSchemaVersion".to_string(), json!(3));

    let mut translation = legacy_translation;
    translation.remove("cli");
    translation.remove("model");
    root.insert(
        "conversationTranslation".to_string(),
        Value::Object(translation),
    );

    let mut memory = root
        .get("memory")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let generation_enabled = memory
        .get("generationEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let usage_enabled = memory
        .get("usageEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let recent_window_hours = memory
        .get("recentWindowHours")
        .and_then(Value::as_u64)
        .unwrap_or(48);
    let watermark_time_1 = memory
        .get("watermarkTime1")
        .and_then(Value::as_str)
        .unwrap_or("02:00")
        .to_string();
    let watermark_time_2 = memory
        .get("watermarkTime2")
        .and_then(Value::as_str)
        .unwrap_or("14:00")
        .to_string();
    let generation_skill_asset_id = memory
        .get("generationSkillAssetId")
        .cloned()
        .unwrap_or(Value::Null);

    memory.insert("generationEnabled".to_string(), json!(generation_enabled));
    memory.insert("usageEnabled".to_string(), json!(usage_enabled));
    memory.insert("recentWindowHours".to_string(), json!(recent_window_hours));
    memory.insert("watermarkTime1".to_string(), json!(watermark_time_1));
    memory.insert("watermarkTime2".to_string(), json!(watermark_time_2));
    memory.insert(
        "generationSkillAssetId".to_string(),
        generation_skill_asset_id,
    );
    for key in ["excludedSessionIds", "excludedSourceIds"] {
        let values = memory
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        memory.insert(key.to_string(), Value::Array(values));
    }
    root.insert("memory".to_string(), Value::Object(memory));
}

fn normalize_canonical_agent_assignments(
    root: &serde_json::Map<String, Value>,
    default_agent: &str,
    runtime_model: &str,
) -> Value {
    let legacy = root
        .get("agentCapabilityAssignments")
        .and_then(Value::as_object);
    let agent_models = root.get("agentModels").and_then(Value::as_object);
    let existing = root.get("agentAssignments").and_then(Value::as_object);
    let has_canonical_assignments = existing.is_some();
    let action_sources = [
        ("translation.card", "cardTranslation"),
        ("memory.extraction", "memory.extraction"),
        ("memory.generation", "memory.generation"),
        ("memory.project", "memory.project"),
        ("memory.global", "memory.global"),
        ("memory.recall", "memory.recall"),
        ("prompt.optimization", "promptOptimization"),
    ];
    let mut assignments = serde_json::Map::new();
    for (action_id, legacy_id) in action_sources {
        let existing_assignment = existing
            .and_then(|values| values.get(action_id))
            .and_then(Value::as_object);
        if has_canonical_assignments
            && existing_assignment.is_none()
            && !matches!(
                action_id,
                "memory.generation" | "memory.project" | "memory.global" | "memory.recall"
            )
        {
            continue;
        }
        let fallback_agent = if action_id == "memory.generation" {
            legacy
                .and_then(|values| values.get("memory"))
                .and_then(Value::as_str)
                .unwrap_or(default_agent)
        } else {
            default_agent
        };
        let legacy_agent = legacy
            .and_then(|values| values.get(legacy_id))
            .and_then(Value::as_str)
            .unwrap_or(fallback_agent);
        let agent_id = existing_assignment
            .and_then(|assignment| assignment.get("agentId"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(legacy_agent);
        let model_id = existing_assignment
            .and_then(|assignment| assignment.get("modelId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                agent_models
                    .and_then(|models| models.get(agent_id))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .or_else(|| (agent_id == default_agent).then_some(runtime_model))
            .filter(|value| !value.is_empty());
        assignments.insert(
            action_id.to_string(),
            json!({ "agentId": agent_id, "modelId": model_id }),
        );
    }
    if let Some(existing) = existing {
        for key in existing.keys() {
            if !assignments.contains_key(key) {
                tracing::warn!(
                    action = "settings.agent_assignment",
                    action_id = %key,
                    "未知的 Agent action assignment 已隔离"
                );
            }
        }
    }
    Value::Object(assignments)
}

fn normalize_ai_runtime_cli(value: Option<&Value>) -> &'static str {
    if value.and_then(Value::as_str) == Some("gemini") {
        "gemini"
    } else {
        DEFAULT_AI_RUNTIME_CLI
    }
}

fn normalize_ai_runtime_model(value: Option<&Value>) -> String {
    let Some(value) = value.and_then(Value::as_str) else {
        return String::new();
    };
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.len() <= 120 {
        normalized
    } else {
        String::new()
    }
}

fn normalize_agent_capability_agent_id(value: Option<&Value>, fallback: &str) -> String {
    let Some(value) = value.and_then(Value::as_str) else {
        return fallback.to_string();
    };
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() || normalized.len() > 128 {
        fallback.to_string()
    } else {
        normalized
    }
}

fn normalize_json_path_setting(value: &mut Value, path: &[&str]) -> AppResult<()> {
    let Some((key, parents)) = path.split_last() else {
        return Ok(());
    };
    let mut current = value;
    for parent in parents {
        let Some(next) = current.get_mut(*parent) else {
            return Ok(());
        };
        current = next;
    }
    let Some(raw) = current
        .get(*key)
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(());
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(());
    }
    let normalized = crate::backend::path_utils::normalize_path_for_storage(raw)?;
    current[*key] = Value::String(normalized);
    Ok(())
}

fn write_settings_document(path: &Path, document: &AppSettingsDocument) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| "设置文件缺少父目录".to_string())
        .map_err(AppError::external)?;
    fs::create_dir_all(parent).map_err(AppError::external)?;
    let content = serde_json::to_string_pretty(document).map_err(AppError::external)?;
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, format!("{content}\n")).map_err(AppError::external)?;
    Ok(fs::rename(&temp_path, path).map_err(AppError::external)?)
}

fn default_document() -> AppSettingsDocument {
    AppSettingsDocument {
        schema_version: SETTINGS_SCHEMA_VERSION,
        settings: json!({}),
    }
}

fn normalize_document(value: Value) -> AppSettingsDocument {
    if value.get("settings").is_some() {
        return serde_json::from_value(value).unwrap_or_else(|_| default_document());
    }

    AppSettingsDocument {
        schema_version: SETTINGS_SCHEMA_VERSION,
        settings: value,
    }
}

#[cfg(test)]
#[path = "app_settings_tests.rs"]
mod tests;
