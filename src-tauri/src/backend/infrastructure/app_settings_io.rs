use crate::backend::infrastructure::{
    app_settings::{
        canonicalize_settings, AppSettingsDocument, AppSettingsFile, CONFIG_FILE_NAME,
        CONVERSATION_ADAPTER_DIR_NAME, SETTINGS_SCHEMA_VERSION,
    },
    InfraError, InfraResult,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn conversation_adapter_dir() -> InfraResult<PathBuf> {
    Ok(app_settings_paths()?.conversation_adapter_dir)
}

pub(crate) struct AppSettingsPaths {
    pub(crate) config_dir: PathBuf,
    pub(crate) config_path: PathBuf,
    pub(crate) conversation_adapter_dir: PathBuf,
}

impl AppSettingsPaths {
    pub(crate) fn into_file(self, settings: Value) -> AppSettingsFile {
        let config_dir = self.config_dir.to_string_lossy().to_string();
        let config_path = self.config_path.to_string_lossy().to_string();
        let conversation_adapter_dir = self.conversation_adapter_dir.to_string_lossy().to_string();
        AppSettingsFile {
            display_config_dir:
                crate::backend::infrastructure::path_utils::display_path_or_original(&config_dir),
            display_config_path:
                crate::backend::infrastructure::path_utils::display_path_or_original(&config_path),
            display_conversation_adapter_dir:
                crate::backend::infrastructure::path_utils::display_path_or_original(
                    &conversation_adapter_dir,
                ),
            config_dir,
            config_path,
            conversation_adapter_dir,
            settings,
        }
    }
}

pub(crate) fn app_settings_paths() -> InfraResult<AppSettingsPaths> {
    let config_dir = app_config_dir()?;
    Ok(AppSettingsPaths {
        config_path: config_dir.join(CONFIG_FILE_NAME),
        conversation_adapter_dir: config_dir.join(CONVERSATION_ADAPTER_DIR_NAME),
        config_dir,
    })
}

pub(crate) fn app_config_dir() -> InfraResult<PathBuf> {
    Ok(
        crate::backend::infrastructure::runtime::config::runtime_config()?
            .home_dir
            .clone(),
    )
}

pub(crate) fn ensure_settings_dirs(paths: &AppSettingsPaths) -> InfraResult<()> {
    fs::create_dir_all(&paths.config_dir).map_err(InfraError::external)?;
    Ok(fs::create_dir_all(&paths.conversation_adapter_dir).map_err(InfraError::external)?)
}

pub(crate) fn read_settings_document(path: &Path) -> InfraResult<AppSettingsDocument> {
    if !path.exists() {
        let document = default_document();
        write_settings_document(path, &document)?;
        return Ok(document);
    }

    let content = fs::read_to_string(path).map_err(InfraError::external)?;
    let parsed: Value = serde_json::from_str(&content)
        .map_err(|error| format!("解析设置文件失败: {} ({error})", path.to_string_lossy()))
        .map_err(InfraError::external)?;
    Ok(normalize_document(parsed))
}

pub(crate) fn write_settings_document(
    path: &Path,
    document: &AppSettingsDocument,
) -> InfraResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| "设置文件缺少父目录".to_string())
        .map_err(InfraError::external)?;
    fs::create_dir_all(parent).map_err(InfraError::external)?;
    let content = serde_json::to_string_pretty(document).map_err(InfraError::external)?;
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, format!("{content}\n")).map_err(InfraError::external)?;
    Ok(fs::rename(&temp_path, path).map_err(InfraError::external)?)
}

pub(crate) fn default_document() -> AppSettingsDocument {
    AppSettingsDocument {
        schema_version: SETTINGS_SCHEMA_VERSION,
        settings: json!({}),
    }
}

pub(crate) fn normalize_document(value: Value) -> AppSettingsDocument {
    if value.get("settings").is_some() {
        return serde_json::from_value(value).unwrap_or_else(|_| default_document());
    }

    AppSettingsDocument {
        schema_version: SETTINGS_SCHEMA_VERSION,
        settings: value,
    }
}
