use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::Context;

use crate::backend::infrastructure::{InfraError, InfraResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub home_dir: PathBuf,
    pub db_path: PathBuf,
    pub log_dir: PathBuf,
    pub policy_path: Option<PathBuf>,
    /// 记忆存储根目录（租户子目录的父目录）。
    ///
    /// 生产环境默认 `~/.assetiweave/memories`，允许 `ASSETIWEAVE_MEMORY_ROOT` 显式覆盖；
    /// 绑定非默认数据库路径（临时库 / 测试库）时由 [`Self::anchor_memory_root_for_isolated_db`]
    /// 强制重锚定到数据库同级的隔离目录——此时显式环境变量也会被忽略，
    /// 优先级是「隔离安全 > 用户覆盖」，避免测试误触真实记忆资产。
    pub memory_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfigDefaults {
    pub home_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl RuntimeConfigDefaults {
    #[allow(dead_code)]
    pub fn from_dirs() -> InfraResult<Self> {
        let base_dirs = directories::BaseDirs::new();
        let home_dir = base_dirs
            .as_ref()
            .map(|b| b.home_dir().to_path_buf())
            .or_else(dirs::home_dir)
            .context("discovering user home directory")
            .map_err(|e| InfraError::NotFound(format!("{e:#}")))?;
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
    memory_root: Option<String>,
}

impl RuntimeConfig {
    pub fn from_env_map(
        env: &BTreeMap<String, OsString>,
        defaults: &RuntimeConfigDefaults,
    ) -> InfraResult<Self> {
        let mut builder = config::Config::builder();

        for (env_key, field) in [
            ("ASSETIWEAVE_HOME", "home_dir"),
            ("ASSETIWEAVE_DB_PATH", "db_path"),
            ("ASSETIWEAVE_LOG_DIR", "log_dir"),
            ("ASSETIWEAVE_POLICY_PATH", "policy_path"),
            ("ASSETIWEAVE_MEMORY_ROOT", "memory_root"),
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
                    .map_err(InfraError::external)?;
            }
        }

        let parsed: Utf8Overrides = builder
            .build()
            .map_err(InfraError::external)?
            .try_deserialize()
            .map_err(InfraError::external)?;

        let mut home_dir = parsed
            .home_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| defaults.home_dir.join(".assetiweave"));

        let mut db_path = parsed
            .db_path
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir.join("app.db"));

        let mut log_dir = parsed
            .log_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir.join("logs"));

        let mut policy_path = parsed.policy_path.map(PathBuf::from);

        let mut memory_root = parsed
            .memory_root
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir.join("memories"));

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
        if let Some(raw) = env.get("ASSETIWEAVE_MEMORY_ROOT") {
            if raw.to_str().is_none() && !raw.is_empty() {
                memory_root = PathBuf::from(raw.clone());
            }
        }

        Ok(Self {
            home_dir,
            db_path,
            log_dir,
            policy_path,
            memory_root,
        })
    }

    #[allow(dead_code)]
    pub fn from_environment() -> InfraResult<Self> {
        let defaults = RuntimeConfigDefaults::from_dirs()?;
        let mut env = BTreeMap::new();
        for key in [
            "ASSETIWEAVE_HOME",
            "ASSETIWEAVE_DB_PATH",
            "ASSETIWEAVE_LOG_DIR",
            "ASSETIWEAVE_POLICY_PATH",
            "ASSETIWEAVE_MEMORY_ROOT",
        ] {
            if let Some(value) = std::env::var_os(key) {
                env.insert(key.to_string(), value);
            }
        }
        Self::from_env_map(&env, &defaults)
    }

    /// 把记忆根目录锚定到数据库同级的隔离目录。
    ///
    /// `environment_db_path` 是本进程解析出的数据库路径。当运行时绑定的数据库
    /// 与之不同（临时库、测试库或显式指定的其它库），说明当前实例不是用户的生产
    /// 实例，记忆读写必须整体隔离到 `<db_parent>/memories`——此时即使显式设置了
    /// `ASSETIWEAVE_MEMORY_ROOT` 也不采纳，因为「绝不触碰用户真实记忆」优先于
    /// 「允许覆盖路径」，否则测试仍可能清空宿主机记忆。
    pub fn anchor_memory_root_for_isolated_db(&mut self, environment_db_path: &Path) {
        if self.db_path == environment_db_path {
            return;
        }
        let Some(parent) = self.db_path.parent().filter(|p| !p.as_os_str().is_empty()) else {
            return;
        };
        self.memory_root = parent.join("memories");
    }

    /// 测试专用：完全不读取进程环境变量，数据库与记忆根目录都锚定在隔离目录下。
    #[cfg(test)]
    pub fn for_isolated_db(db_path: PathBuf) -> Self {
        let parent = db_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            home_dir: parent.join("home"),
            memory_root: parent.join("memories"),
            log_dir: parent.join("logs"),
            db_path,
            policy_path: None,
        }
    }
}

pub fn runtime_config() -> InfraResult<std::sync::Arc<RuntimeConfig>> {
    if let Some(runtime) = super::current_process_runtime() {
        return Ok(runtime.config());
    }
    RuntimeConfig::from_environment().map(std::sync::Arc::new)
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
