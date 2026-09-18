use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::Context;

use crate::backend::runtime::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeConfig {
    pub(crate) home_dir: PathBuf,
    pub(crate) db_path: PathBuf,
    pub(crate) log_dir: PathBuf,
    pub(crate) policy_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeConfigDefaults {
    pub(crate) home_dir: PathBuf,
    pub(crate) data_dir: PathBuf,
}

impl RuntimeConfigDefaults {
    #[allow(dead_code)]
    pub(crate) fn from_dirs() -> AppResult<Self> {
        let base_dirs = directories::BaseDirs::new();
        let home_dir = base_dirs
            .as_ref()
            .map(|b| b.home_dir().to_path_buf())
            .or_else(dirs::home_dir)
            .context("discovering user home directory")
            .map_err(|e| AppError::NotFound(format!("{e:#}")))?;
        let data_dir = directories::ProjectDirs::from("", "", "AssetIWeave")
            .and_then(|proj| proj.data_dir().parent().map(Path::to_path_buf))
            .or_else(|| base_dirs.as_ref().map(|b| b.data_dir().to_path_buf()))
            .unwrap_or_else(|| home_dir.join(".local").join("share"));
        Ok(Self { home_dir, data_dir })
    }
}

#[derive(serde::Deserialize, Default)]
struct Utf8Overrides {
    home_dir: Option<String>,
    db_path: Option<String>,
    log_dir: Option<String>,
    policy_path: Option<String>,
}

impl RuntimeConfig {
    pub(crate) fn from_env_map(
        env: &BTreeMap<String, OsString>,
        defaults: &RuntimeConfigDefaults,
    ) -> AppResult<Self> {
        let mut builder = config::Config::builder();

        for (env_key, field) in [
            ("ASSETIWEAVE_HOME", "home_dir"),
            ("ASSETIWEAVE_DB_PATH", "db_path"),
            ("ASSETIWEAVE_LOG_DIR", "log_dir"),
            ("ASSETIWEAVE_POLICY_PATH", "policy_path"),
        ] {
            if let Some(raw) = env.get(env_key).and_then(|value| value.to_str()) {
                let value = if env_key == "ASSETIWEAVE_HOME" {
                    raw.trim()
                } else {
                    raw
                };
                if (env_key == "ASSETIWEAVE_HOME" || env_key == "ASSETIWEAVE_DB_PATH")
                    && value.is_empty()
                {
                    continue;
                }
                builder = builder
                    .set_override(field, value)
                    .map_err(AppError::external)?;
            }
        }

        let parsed: Utf8Overrides = builder
            .build()
            .map_err(AppError::external)?
            .try_deserialize()
            .map_err(AppError::external)?;

        let mut home_dir = parsed
            .home_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| defaults.home_dir.join(".assetiweave"));

        let app_data_dir = if let Some(proj) = directories::ProjectDirs::from("", "", "AssetIWeave")
        {
            if proj.data_dir().parent() == Some(&defaults.data_dir) {
                proj.data_dir().to_path_buf()
            } else {
                defaults.data_dir.join("AssetIWeave")
            }
        } else {
            defaults.data_dir.join("AssetIWeave")
        };

        let mut db_path = parsed
            .db_path
            .map(PathBuf::from)
            .unwrap_or_else(|| app_data_dir.join("app.db"));

        let mut log_dir = parsed
            .log_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| app_data_dir.join("logs"));

        let mut policy_path = parsed.policy_path.map(PathBuf::from);

        // Preserve non-UTF-8 OsString values without lossy conversion
        if let Some(raw) = env.get("ASSETIWEAVE_HOME") {
            if raw.to_str().is_none() && !raw.is_empty() {
                home_dir = PathBuf::from(raw.clone());
            }
        }
        if let Some(raw) = env.get("ASSETIWEAVE_DB_PATH") {
            if raw.to_str().is_none() && !raw.is_empty() {
                db_path = PathBuf::from(raw.clone());
            }
        }
        if let Some(raw) = env.get("ASSETIWEAVE_LOG_DIR") {
            if raw.to_str().is_none() {
                log_dir = PathBuf::from(raw.clone());
            }
        }
        if let Some(raw) = env.get("ASSETIWEAVE_POLICY_PATH") {
            if raw.to_str().is_none() {
                policy_path = Some(PathBuf::from(raw.clone()));
            }
        }

        Ok(Self {
            home_dir,
            db_path,
            log_dir,
            policy_path,
        })
    }

    #[allow(dead_code)]
    pub(crate) fn from_environment() -> AppResult<Self> {
        let defaults = RuntimeConfigDefaults::from_dirs()?;
        let mut env = BTreeMap::new();
        for key in [
            "ASSETIWEAVE_HOME",
            "ASSETIWEAVE_DB_PATH",
            "ASSETIWEAVE_LOG_DIR",
            "ASSETIWEAVE_POLICY_PATH",
        ] {
            if let Some(value) = std::env::var_os(key) {
                env.insert(key.to_string(), value);
            }
        }
        Self::from_env_map(&env, &defaults)
    }
}

pub(crate) fn runtime_config() -> AppResult<std::sync::Arc<RuntimeConfig>> {
    if let Some(runtime) = super::current_process_runtime() {
        return Ok(runtime.config());
    }
    RuntimeConfig::from_environment().map(std::sync::Arc::new)
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
