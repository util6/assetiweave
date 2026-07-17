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
const SETTINGS_SCHEMA_VERSION: u32 = 1;

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
        return Ok(json!({}));
    }
    Ok(read_normalized_settings_document(&paths.config_path)?.settings)
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
    if normalized != document.settings {
        document.settings = normalized;
        write_settings_document(path, &document)?;
    }
    Ok(document)
}

fn normalize_settings_paths(mut settings: Value) -> AppResult<Value> {
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
}
