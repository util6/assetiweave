use crate::backend::dto::AppResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const CONFIG_DIR_NAME: &str = ".assetiweave";
const CONFIG_FILE_NAME: &str = "config.json";
const CONVERSATION_ADAPTER_DIR_NAME: &str = "conversation-adapters";
const SETTINGS_SCHEMA_VERSION: u32 = 3;
const DEFAULT_AI_RUNTIME_CLI: &str = "opencode";
const DEFAULT_AUTO_DREAM_MIN_HOURS: i64 = 12;
const DEFAULT_AUTO_DREAM_MIN_SESSIONS: i64 = 3;
const DEFAULT_CONVERSATION_FULL_SYNC_ON_STARTUP: bool = true;

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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSettingsDocument {
    schema_version: u32,
    settings: Value,
}

pub(crate) fn get_app_settings() -> AppResult<AppSettingsFile> {
    let paths = app_settings_paths()?;
    ensure_settings_dirs(&paths)?;
    let settings = read_normalized_settings_document(&paths.config_path)?.settings;
    Ok(paths.into_file(settings))
}

pub(crate) fn save_app_settings(settings: Value) -> AppResult<AppSettingsFile> {
    let paths = app_settings_paths()?;
    ensure_settings_dirs(&paths)?;
    let document = AppSettingsDocument {
        schema_version: SETTINGS_SCHEMA_VERSION,
        settings: normalize_settings_paths(settings)?,
    };
    write_settings_document(&paths.config_path, &document)?;
    Ok(paths.into_file(document.settings))
}

pub(crate) fn read_app_settings_value() -> AppResult<Value> {
    let paths = app_settings_paths()?;
    if !paths.config_path.exists() {
        return normalize_settings_paths(json!({}));
    }
    Ok(read_normalized_settings_document(&paths.config_path)?.settings)
}

pub(crate) fn conversation_full_sync_on_startup_enabled() -> AppResult<bool> {
    Ok(conversation_full_sync_on_startup_enabled_from_value(
        &read_app_settings_value()?,
    ))
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
    if let Ok(home) = env::var("ASSETIWEAVE_HOME") {
        let home = home.trim();
        if !home.is_empty() {
            return Ok(PathBuf::from(home));
        }
    }
    let home = dirs::home_dir().ok_or("无法确定用户主目录")?;
    Ok(home.join(CONFIG_DIR_NAME))
}

fn ensure_settings_dirs(paths: &AppSettingsPaths) -> AppResult<()> {
    fs::create_dir_all(&paths.config_dir).map_err(|error| error.to_string())?;
    fs::create_dir_all(&paths.conversation_adapter_dir).map_err(|error| error.to_string())
}

fn read_settings_document(path: &Path) -> AppResult<AppSettingsDocument> {
    if !path.exists() {
        let document = default_document();
        write_settings_document(path, &document)?;
        return Ok(document);
    }

    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let parsed: Value = serde_json::from_str(&content)
        .map_err(|error| format!("解析设置文件失败: {} ({error})", path.to_string_lossy()))?;
    Ok(normalize_document(parsed))
}

fn read_normalized_settings_document(path: &Path) -> AppResult<AppSettingsDocument> {
    let mut document = read_settings_document(path)?;
    let normalized = normalize_settings_paths(document.settings.clone())?;
    let schema_changed = document.schema_version != SETTINGS_SCHEMA_VERSION;
    if normalized != document.settings || schema_changed {
        document.settings = normalized;
        document.schema_version = SETTINGS_SCHEMA_VERSION;
        write_settings_document(path, &document)?;
    }
    Ok(document)
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
        "memory.dream",
        "promptOptimization",
    ];
    let unknown_capabilities = agent_capabilities
        .keys()
        .filter(|key| !supported_capabilities.contains(&key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    for key in unknown_capabilities {
        agent_capabilities.remove(&key);
        crate::backend::operation_log::log_warn(
            "settings.agent_capability",
            "未知的 Agent capability 已禁用",
            &[("capability", key)],
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
    for service_id in ["memory.extraction", "memory.dream"] {
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
    let auto_dream_enabled = memory
        .get("autoDreamEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let min_hours =
        normalize_integer_setting(memory.get("minHours"), 1, 168, DEFAULT_AUTO_DREAM_MIN_HOURS);
    let min_sessions = normalize_integer_setting(
        memory.get("minSessions"),
        1,
        50,
        DEFAULT_AUTO_DREAM_MIN_SESSIONS,
    );
    memory.insert(
        "autoDreamEnabled".to_string(),
        Value::Bool(auto_dream_enabled),
    );
    memory.insert("minHours".to_string(), json!(min_hours));
    memory.insert("minSessions".to_string(), json!(min_sessions));
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
    let action_sources = [
        ("translation.card", "cardTranslation"),
        ("memory.extraction", "memory.extraction"),
        ("memory.dream", "memory.dream"),
        ("prompt.optimization", "promptOptimization"),
    ];
    let mut assignments = serde_json::Map::new();
    for (action_id, legacy_id) in action_sources {
        let existing_assignment = existing
            .and_then(|values| values.get(action_id))
            .and_then(Value::as_object);
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
                crate::backend::operation_log::log_warn(
                    "settings.agent_assignment",
                    "未知的 Agent action assignment 已隔离",
                    &[("action", key.clone())],
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

fn normalize_integer_setting(value: Option<&Value>, min: i64, max: i64, fallback: i64) -> i64 {
    value
        .and_then(Value::as_i64)
        .map(|value| value.clamp(min, max))
        .unwrap_or(fallback)
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
        .ok_or_else(|| "设置文件缺少父目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let content = serde_json::to_string_pretty(document).map_err(|error| error.to_string())?;
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, format!("{content}\n")).map_err(|error| error.to_string())?;
    fs::rename(&temp_path, path).map_err(|error| error.to_string())
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
        assert_eq!(settings["memory"]["autoDreamEnabled"], false);
        assert_eq!(settings["memory"]["minHours"], 12);
        assert_eq!(settings["memory"]["minSessions"], 3);
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
        assert_eq!(
            settings["agentCapabilityAssignments"]["memory.dream"],
            "codex"
        );
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
}
