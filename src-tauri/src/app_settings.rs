use crate::types::AppResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
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
    let settings = read_settings_document(&paths.config_path)?.settings;
    Ok(paths.into_file(settings))
}

pub(crate) fn save_app_settings(settings: Value) -> AppResult<AppSettingsFile> {
    let paths = app_settings_paths()?;
    ensure_settings_dirs(&paths)?;
    let document = AppSettingsDocument {
        schema_version: SETTINGS_SCHEMA_VERSION,
        settings,
    };
    write_settings_document(&paths.config_path, &document)?;
    Ok(paths.into_file(document.settings))
}

struct AppSettingsPaths {
    config_dir: PathBuf,
    config_path: PathBuf,
    conversation_adapter_dir: PathBuf,
}

impl AppSettingsPaths {
    fn into_file(self, settings: Value) -> AppSettingsFile {
        AppSettingsFile {
            config_dir: self.config_dir.to_string_lossy().to_string(),
            config_path: self.config_path.to_string_lossy().to_string(),
            conversation_adapter_dir: self.conversation_adapter_dir.to_string_lossy().to_string(),
            settings,
        }
    }
}

fn app_settings_paths() -> AppResult<AppSettingsPaths> {
    let home = dirs::home_dir().ok_or("无法确定用户主目录")?;
    let config_dir = home.join(CONFIG_DIR_NAME);
    Ok(AppSettingsPaths {
        config_path: config_dir.join(CONFIG_FILE_NAME),
        conversation_adapter_dir: config_dir.join(CONVERSATION_ADAPTER_DIR_NAME),
        config_dir,
    })
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
}
