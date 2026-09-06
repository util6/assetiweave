use crate::backend::{
    runtime::AppError,
    runtime::AppResult,
    store::{self, Database},
};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemorySettings {
    #[serde(default = "default_true")]
    pub(crate) generation_enabled: bool,
    #[serde(default = "default_true")]
    pub(crate) usage_enabled: bool,
    #[serde(default)]
    pub(crate) excluded_session_ids: Vec<String>,
    #[serde(default)]
    pub(crate) excluded_source_ids: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            generation_enabled: true,
            usage_enabled: true,
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

pub(crate) fn load_backend_settings_for_database(_db: &Database) -> AppResult<BackendSettings> {
    if let Some(runtime) = crate::backend::runtime::current_process_runtime() {
        return BackendSettings::from_value(&runtime.app_settings_value());
    }
    let paths = app_settings_paths()?;
    let doc = read_settings_document(&paths.config_path)?;
    BackendSettings::from_document(&doc)
}

pub(crate) fn conversation_full_sync_on_startup_enabled_for_database(
    db: &Database,
) -> AppResult<bool> {
    Ok(load_backend_settings_for_database(db)?.auto_full_sync_on_startup())
}

pub(crate) fn memory_generation_enabled_for_database(db: &Database) -> AppResult<bool> {
    Ok(load_backend_settings_for_database(db)?.is_memory_generation_enabled())
}

pub(crate) fn memory_usage_enabled_for_database(db: &Database) -> AppResult<bool> {
    Ok(load_backend_settings_for_database(db)?.is_memory_usage_enabled())
}

pub(crate) fn memory_session_excluded_for_database(
    db: &Database,
    session_id: &str,
) -> AppResult<bool> {
    Ok(load_backend_settings_for_database(db)?.is_session_excluded(session_id))
}

pub(crate) fn memory_source_excluded_for_database(
    db: &Database,
    source_id: &str,
) -> AppResult<bool> {
    Ok(load_backend_settings_for_database(db)?.is_source_excluded(source_id))
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

#[cfg(test)]
fn conversation_full_sync_on_startup_enabled_from_value(settings: &Value) -> bool {
    settings
        .get("conversations")
        .and_then(Value::as_object)
        .and_then(|conversations| conversations.get("autoFullSyncOnStartup"))
        .and_then(Value::as_bool)
        .unwrap_or(DEFAULT_CONVERSATION_FULL_SYNC_ON_STARTUP)
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
    memory.insert("generationEnabled".to_string(), json!(generation_enabled));
    memory.insert("usageEnabled".to_string(), json!(usage_enabled));
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
                "memory.project" | "memory.global" | "memory.recall"
            )
        {
            continue;
        }
        let legacy_agent = legacy
            .and_then(|values| values.get(legacy_id))
            .and_then(Value::as_str)
            .unwrap_or(default_agent);
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
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn settings_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn legacy_top_level_settings_are_wrapped() {
        let document = normalize_document(json!({ "density": "compact" }));

        assert_eq!(document.schema_version, SETTINGS_SCHEMA_VERSION);
        assert_eq!(document.settings["density"], "compact");
    }

    #[test]
    fn current_document_shape_is_preserved() {
        let document = normalize_document(json!({
            "schemaVersion": 1,
            "settings": { "density": "compact" }
        }));

        assert_eq!(document.schema_version, 1);
        assert_eq!(document.settings["density"], "compact");
    }

    #[test]
    fn settings_file_keeps_runtime_paths_separate_from_portable_display_paths() {
        let home = dirs::home_dir().expect("home directory");
        let paths = AppSettingsPaths {
            config_dir: home.join(".assetiweave"),
            config_path: home.join(".assetiweave").join("config.json"),
            conversation_adapter_dir: home.join(".assetiweave").join("conversation-adapters"),
        };

        let file = paths.into_file(json!({}));

        assert!(Path::new(&file.config_path).is_absolute());
        assert_eq!(file.display_config_dir, "~/.assetiweave");
        assert_eq!(file.display_config_path, "~/.assetiweave/config.json");
        assert_eq!(
            file.display_conversation_adapter_dir,
            "~/.assetiweave/conversation-adapters"
        );
    }

    #[test]
    #[cfg(unix)]
    fn settings_path_values_are_normalized_before_persistence() {
        let home = dirs::home_dir().expect("home directory");
        let settings = normalize_settings_paths(json!({
            "dataBackup": {
                "customDirectory": home.join("Backups").to_string_lossy()
            },
            "conversationRuntimeOverrides": {
                "node": home.join(".local/bin/node").to_string_lossy(),
                "python": "",
                "bash": "/opt/homebrew/bin/bash"
            }
        }))
        .expect("normalize settings paths");

        assert_eq!(settings["dataBackup"]["customDirectory"], "~/Backups");
        assert_eq!(
            settings["conversationRuntimeOverrides"]["node"],
            "~/.local/bin/node"
        );
        assert_eq!(
            settings["conversationRuntimeOverrides"]["bash"],
            "/opt/homebrew/bin/bash"
        );
    }

    #[test]
    fn startup_full_conversation_sync_is_enabled_by_default() {
        assert!(conversation_full_sync_on_startup_enabled_from_value(
            &json!({})
        ));
        assert!(conversation_full_sync_on_startup_enabled_from_value(
            &json!({
                "conversations": {}
            })
        ));
    }

    #[test]
    fn startup_full_conversation_sync_respects_an_explicit_disabled_setting() {
        assert!(!conversation_full_sync_on_startup_enabled_from_value(
            &json!({
                "conversations": { "autoFullSyncOnStartup": false }
            })
        ));
        assert!(conversation_full_sync_on_startup_enabled_from_value(
            &json!({
                "conversations": { "autoFullSyncOnStartup": "false" }
            })
        ));
    }

    #[test]
    fn legacy_translation_runtime_moves_to_shared_ai_settings() {
        let settings = normalize_settings_paths(json!({
            "conversationTranslation": {
                "cli": "gemini",
                "model": "gemini-2.5-pro",
                "provider": "cli"
            }
        }))
        .expect("normalize AI settings");

        assert_eq!(settings["aiRuntime"]["cli"], "gemini");
        assert_eq!(settings["aiRuntime"]["model"], "gemini-2.5-pro");
        assert!(settings["conversationTranslation"].get("cli").is_none());
        assert!(settings["conversationTranslation"].get("model").is_none());
        assert_eq!(settings["memory"]["generationEnabled"], true);
        assert_eq!(settings["memory"]["usageEnabled"], true);
    }

    #[test]
    fn service_agent_assignments_migrate_from_legacy_runtime_and_preserve_explicit_values() {
        let settings = normalize_settings_paths(json!({
            "aiRuntime": { "cli": "gemini", "model": "gemini-2.5-pro" },
            "agentModels": { "codex": "openai/gpt-5-codex" },
            "agentCapabilityAssignments": { "memory": "codex" }
        }))
        .expect("normalize service Agent settings");

        assert_eq!(
            settings["agentCapabilityAssignments"]["cardTranslation"],
            "gemini"
        );
        assert_eq!(settings["agentCapabilityAssignments"]["memory"], "codex");
        assert_eq!(
            settings["agentCapabilityAssignments"]["promptOptimization"],
            "gemini"
        );
        assert_eq!(
            settings["agentCapabilityAssignments"]["memory.extraction"],
            "codex"
        );
        assert!(settings["agentCapabilityAssignments"]
            .get("memory.dream")
            .is_none());
        assert_eq!(settings["settingsSchemaVersion"], 3);
        assert_eq!(
            settings["agentAssignments"]["translation.card"]["agentId"],
            "gemini"
        );
        assert_eq!(
            settings["agentAssignments"]["memory.extraction"]["agentId"],
            "codex"
        );
        assert_eq!(
            settings["agentAssignments"]["memory.extraction"]["modelId"],
            "openai/gpt-5-codex"
        );
    }

    const TEST_HOME_VAR: &str = "ASSETIWEAVE_HOME";

    #[tokio::test]
    async fn sqlite_settings_import_is_idempotent_and_legacy_keys_are_removed() {
        let _guard = settings_test_lock().lock().expect("settings test lock");
        let root = std::env::temp_dir().join(format!(
            "assetiweave-settings-migration-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create settings test root");
        let previous_home = std::env::var_os(TEST_HOME_VAR);
        std::env::set_var(TEST_HOME_VAR, &root);
        let config = root.join(CONFIG_FILE_NAME);
        std::fs::write(
            &config,
            serde_json::to_string_pretty(&json!({
                "schemaVersion": 1,
                "settings": {
                    "aiRuntime": { "cli": "gemini", "model": "gemini-2.5-pro" },
                    "agentModels": { "gemini": "gemini-2.5-pro" },
                    "agentCapabilityAssignments": { "memory": "gemini" }
                }
            }))
            .expect("encode legacy settings"),
        )
        .expect("write legacy settings");

        let db_path = root.join("settings.db");
        let database = crate::backend::store::Database::open_initialized_async(&db_path)
            .await
            .expect("open settings database");
        let imported = read_app_settings_value_sqlx(database.pool())
            .await
            .expect("import settings");
        assert_eq!(
            imported["agentAssignments"]["memory.extraction"]["agentId"],
            "gemini"
        );
        assert!(imported.get("agentModels").is_none());
        assert!(imported.get("agentCapabilityAssignments").is_none());

        std::fs::write(
            &config,
            serde_json::to_string_pretty(&json!({
                "settings": {
                    "aiRuntime": { "cli": "opencode", "model": "changed-after-import" }
                }
            }))
            .expect("encode changed legacy settings"),
        )
        .expect("rewrite legacy settings");
        let reopened = read_app_settings_value_sqlx(database.pool())
            .await
            .expect("read settings");
        assert_eq!(reopened, imported);

        match previous_home {
            Some(value) => std::env::set_var(TEST_HOME_VAR, value),
            None => std::env::remove_var(TEST_HOME_VAR),
        }
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn sqlite_settings_remain_available_when_legacy_file_is_corrupt() {
        let _guard = settings_test_lock().lock().expect("settings test lock");
        let root = std::env::temp_dir().join(format!(
            "assetiweave-settings-corrupt-file-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create settings test root");
        let previous_home = std::env::var_os(TEST_HOME_VAR);
        std::env::set_var(TEST_HOME_VAR, &root);
        let database =
            crate::backend::store::Database::open_initialized_async(&root.join("settings.db"))
                .await
                .expect("open settings database");
        let expected = canonicalize_settings(json!({
            "theme": "dark",
            "agentAssignments": {}
        }))
        .expect("canonical settings");
        save_app_settings_sqlx(database.pool(), expected.clone())
            .await
            .expect("save sqlite settings");
        std::fs::write(root.join(CONFIG_FILE_NAME), "{ invalid json")
            .expect("write corrupt legacy file");

        let actual = read_app_settings_value_sqlx(database.pool())
            .await
            .expect("read settings from sqlite despite corrupt legacy file");
        assert_eq!(actual, expected);

        match previous_home {
            Some(value) => std::env::set_var(TEST_HOME_VAR, value),
            None => std::env::remove_var(TEST_HOME_VAR),
        }
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn canonical_settings_do_not_refill_explicitly_unassigned_actions() {
        let settings = canonicalize_settings(json!({
            "aiRuntime": { "cli": "opencode", "model": "model/a" },
            "agentAssignments": {
                "translation.card": { "agentId": "opencode", "modelId": "model/a" }
            }
        }))
        .expect("canonicalize settings");

        assert!(settings["agentAssignments"]
            .get("translation.card")
            .is_some());
        assert!(settings["agentAssignments"]
            .get("memory.extraction")
            .is_none());
        assert!(settings["agentAssignments"].get("memory.dream").is_none());
        assert!(settings["agentAssignments"]
            .get("prompt.optimization")
            .is_none());
    }

    #[test]
    fn canonical_settings_handles_locale_validation_and_normalization() {
        // Missing locale normalizes to null
        let s = canonicalize_settings(json!({})).unwrap();
        assert_eq!(s["locale"], serde_json::Value::Null);

        // Explicit null stays null
        let s = canonicalize_settings(json!({ "locale": null })).unwrap();
        assert_eq!(s["locale"], serde_json::Value::Null);

        // Valid locales are preserved
        let s_zh = canonicalize_settings(json!({ "locale": "zh" })).unwrap();
        assert_eq!(s_zh["locale"], "zh");
        let s_en = canonicalize_settings(json!({ "locale": "en" })).unwrap();
        assert_eq!(s_en["locale"], "en");

        // Invalid locales return validation error
        assert!(canonicalize_settings(json!({ "locale": "fr" })).is_err());
        assert!(canonicalize_settings(json!({ "locale": 123 })).is_err());
        assert!(canonicalize_settings(json!({ "locale": true })).is_err());
        assert!(canonicalize_settings(json!({ "locale": {} })).is_err());
    }

    #[test]
    fn canonical_settings_handles_column_layouts_validation_and_normalization() {
        // Missing columnLayouts normalizes to empty map
        let s = canonicalize_settings(json!({})).unwrap();
        assert_eq!(s["columnLayouts"], json!({}));

        // Valid map is preserved
        let s = canonicalize_settings(json!({
            "columnLayouts": { "explorer": [1.0, 2.0, 1.0] }
        }))
        .unwrap();
        assert_eq!(s["columnLayouts"]["explorer"], json!([1.0, 2.0, 1.0]));

        // Invalid: not an object
        assert!(canonicalize_settings(json!({ "columnLayouts": [1, 2] })).is_err());

        // Invalid: < 2 items
        assert!(canonicalize_settings(json!({
            "columnLayouts": { "explorer": [1.0] }
        }))
        .is_err());

        // Invalid: > 16 items
        let seventeen = vec![1.0; 17];
        assert!(canonicalize_settings(json!({
            "columnLayouts": { "explorer": seventeen }
        }))
        .is_err());

        // Invalid: zero or negative
        assert!(canonicalize_settings(json!({
            "columnLayouts": { "explorer": [0.0, 1.0] }
        }))
        .is_err());
        assert!(canonicalize_settings(json!({
            "columnLayouts": { "explorer": [-1.0, 2.0] }
        }))
        .is_err());
        // Invalid: non-number
        assert!(canonicalize_settings(json!({
            "columnLayouts": { "explorer": ["1", "2"] }
        }))
        .is_err());
    }

    #[test]
    fn canonical_settings_preserves_unknown_fields() {
        let s = canonicalize_settings(json!({
            "customUserKey": "customValue",
            "nestedObject": { "a": 1 }
        }))
        .unwrap();
        assert_eq!(s["customUserKey"], "customValue");
        assert_eq!(s["nestedObject"]["a"], 1);
        assert_eq!(s["locale"], serde_json::Value::Null);
        assert_eq!(s["columnLayouts"], json!({}));
    }

    #[tokio::test]
    async fn sqlite_v3_to_v4_migration_upgrades_schema_version_and_populates_defaults() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE app_settings (settings_id TEXT PRIMARY KEY NOT NULL, schema_version INTEGER NOT NULL, settings_json TEXT NOT NULL, updated_at TEXT NOT NULL)")
            .execute(&pool)
            .await
            .unwrap();

        // Seed a v3 row without locale or columnLayouts
        store::save_app_settings_sqlx(&pool, 3, &json!({ "theme": "promptStudio" }))
            .await
            .unwrap();

        // Load via load_or_import_app_settings_sqlx
        let loaded = load_or_import_app_settings_sqlx(&pool).await.unwrap();
        assert_eq!(loaded["theme"], "promptStudio");
        assert_eq!(loaded["locale"], serde_json::Value::Null);
        assert_eq!(loaded["columnLayouts"], json!({}));

        // Verify stored row was upgraded to version 4
        let (version, stored) = store::load_app_settings_sqlx(&pool).await.unwrap().unwrap();
        assert_eq!(version, 4);
        assert_eq!(stored["theme"], "promptStudio");
        assert_eq!(stored["locale"], serde_json::Value::Null);
        assert_eq!(stored["columnLayouts"], json!({}));
    }

    #[tokio::test]
    async fn save_app_settings_preserves_and_returns_persisted_locale() {
        let temp_dir = std::env::temp_dir().join(format!(
            "assetiweave-settings-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("test.db");
        let db = crate::backend::store::Database::open_initialized_async(&db_path)
            .await
            .unwrap();

        // 1. 先保存一个带有 locale: "en" 的设置
        let res1 = save_app_settings_sqlx(db.pool(), json!({ "theme": "dark", "locale": "en" }))
            .await
            .unwrap();
        assert_eq!(res1.settings["locale"], "en");

        // 2. 模拟客户端提交不含 locale 或 locale 为 null 的更新（如只更新 theme）
        let res2 =
            save_app_settings_sqlx(db.pool(), json!({ "theme": "sunlight", "locale": null }))
                .await
                .unwrap();

        // 3. 验证返回的响应中，locale 依然保留为 "en"，与实际数据库内容一致，而不是返回 null！
        assert_eq!(res2.settings["theme"], "sunlight");
        assert_eq!(
            res2.settings["locale"], "en",
            "Response must reflect persisted locale from database"
        );

        // 4. 再次读取数据库，验证数据库本身也是 "en"
        let loaded = read_app_settings_value_sqlx(db.pool()).await.unwrap();
        assert_eq!(loaded["theme"], "sunlight");
        assert_eq!(loaded["locale"], "en");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn missing_known_fields_get_current_defaults() {
        let settings = BackendSettings::from_value(&json!({})).expect("default backend settings");
        assert!(settings.is_memory_generation_enabled());
        assert!(settings.is_memory_usage_enabled());
        assert!(settings.auto_full_sync_on_startup());
        assert_eq!(settings.ai_runtime.cli, "opencode");
        assert_eq!(settings.ai_runtime.model, None);
        assert_eq!(settings.locale, None);
        assert!(settings.column_layouts.is_empty());
        assert!(settings.agent_assignments.is_empty());
    }

    #[test]
    fn wrong_known_field_types_return_validation() {
        assert!(matches!(
            BackendSettings::from_value(&json!({ "memory": "not_an_object" })),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            BackendSettings::from_value(&json!({ "conversations": "not_an_object" })),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            BackendSettings::from_value(&json!({ "aiRuntime": 123 })),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            BackendSettings::from_value(&json!({ "columnLayouts": "not_an_object" })),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn canonicalize_twice_is_identical() {
        let raw = json!({
            "theme": "dark",
            "locale": "zh",
            "customUnknown": { "nested": [1, 2, 3] },
            "columnLayouts": { "nav": [1.0, 2.0] },
            "conversations": { "autoFullSyncOnStartup": false }
        });
        let first = canonicalize_settings(raw).expect("first canonicalization");
        let second = canonicalize_settings(first.clone()).expect("second canonicalization");
        assert_eq!(first, second);
    }

    #[test]
    fn v3_v4_migration_is_idempotent() {
        let v3_payload = json!({
            "agentCapabilityAssignments": {
                "memory": "opencode",
                "cardTranslation": "opencode"
            },
            "agentModels": {
                "opencode": "default-model"
            }
        });
        let first = canonicalize_settings(v3_payload).expect("v3 migration");
        assert!(first.get("agentCapabilityAssignments").is_none());
        assert!(first.get("agentModels").is_none());
        assert!(first.get("agentAssignments").is_some());

        let second = canonicalize_settings(first.clone()).expect("second pass");
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn unknown_top_level_and_nested_fields_survive_load_save_load() {
        let temp_dir = std::env::temp_dir().join(format!(
            "assetiweave-settings-roundtrip-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("roundtrip.db");
        let db = crate::backend::store::Database::open_initialized_async(&db_path)
            .await
            .unwrap();

        let initial = json!({
            "theme": "synthwave",
            "unknownPlugin": { "enabled": true, "threshold": 42 },
            "nested": { "deep": { "value": "preserved" } },
            "locale": "en",
            "conversations": { "autoFullSyncOnStartup": false }
        });

        let saved = save_app_settings_sqlx(db.pool(), initial.clone())
            .await
            .unwrap();
        assert_eq!(saved.settings["theme"], "synthwave");
        assert_eq!(saved.settings["unknownPlugin"]["threshold"], 42);
        assert_eq!(saved.settings["nested"]["deep"]["value"], "preserved");

        let reloaded = read_app_settings_value_sqlx(db.pool()).await.unwrap();
        assert_eq!(reloaded["theme"], "synthwave");
        assert_eq!(reloaded["unknownPlugin"]["threshold"], 42);
        assert_eq!(reloaded["nested"]["deep"]["value"], "preserved");
        assert_eq!(reloaded["locale"], "en");
        assert_eq!(reloaded["conversations"]["autoFullSyncOnStartup"], false);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
