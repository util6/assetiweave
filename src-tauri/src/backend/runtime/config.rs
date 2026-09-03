use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

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
        let home_dir =
            dirs::home_dir().ok_or_else(|| AppError::NotFound("无法确定用户主目录".to_string()))?;
        let data_dir = dirs::data_dir()
            .ok_or_else(|| AppError::NotFound("无法确定系统数据目录".to_string()))?;
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

        let mut db_path = parsed
            .db_path
            .map(PathBuf::from)
            .unwrap_or_else(|| defaults.data_dir.join("AssetIWeave").join("app.db"));

        let mut log_dir = parsed
            .log_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| defaults.data_dir.join("AssetIWeave").join("logs"));

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_defaults() -> RuntimeConfigDefaults {
        RuntimeConfigDefaults {
            home_dir: PathBuf::from("/fixture/home"),
            data_dir: PathBuf::from("/fixture/data"),
        }
    }

    #[test]
    fn explicit_db_override_does_not_relocate_log_dir() {
        let defaults = test_defaults();
        let env = BTreeMap::from([
            ("ASSETIWEAVE_HOME".into(), OsString::from("/fixture/custom")),
            (
                "ASSETIWEAVE_DB_PATH".into(),
                OsString::from("/fixture/test.db"),
            ),
            (
                "ASSETIWEAVE_TEAM_TOOL_CREDENTIAL".into(),
                OsString::from("secret"),
            ),
        ]);
        let value = RuntimeConfig::from_env_map(&env, &defaults).unwrap();
        assert_eq!(value.db_path, PathBuf::from("/fixture/test.db"));
        assert_eq!(value.home_dir, PathBuf::from("/fixture/custom"));
        assert_eq!(
            value.log_dir,
            PathBuf::from("/fixture/data/AssetIWeave/logs")
        );
        assert_eq!(value.policy_path, None);
    }

    #[test]
    fn defaults_when_env_is_empty() {
        let defaults = test_defaults();
        let env = BTreeMap::new();
        let value = RuntimeConfig::from_env_map(&env, &defaults).unwrap();
        assert_eq!(value.home_dir, PathBuf::from("/fixture/home/.assetiweave"));
        assert_eq!(
            value.db_path,
            PathBuf::from("/fixture/data/AssetIWeave/app.db")
        );
        assert_eq!(
            value.log_dir,
            PathBuf::from("/fixture/data/AssetIWeave/logs")
        );
        assert_eq!(value.policy_path, None);
    }

    #[test]
    fn whitespace_home_is_treated_as_unset() {
        let defaults = test_defaults();
        let env = BTreeMap::from([("ASSETIWEAVE_HOME".into(), OsString::from("   \t  \n  "))]);
        let value = RuntimeConfig::from_env_map(&env, &defaults).unwrap();
        assert_eq!(value.home_dir, PathBuf::from("/fixture/home/.assetiweave"));
    }

    #[test]
    fn whitespace_db_path_is_preserved_as_raw_path() {
        let defaults = test_defaults();
        let env = BTreeMap::from([(
            "ASSETIWEAVE_DB_PATH".into(),
            OsString::from("  path with spaces.db  "),
        )]);
        let value = RuntimeConfig::from_env_map(&env, &defaults).unwrap();
        assert_eq!(value.db_path, PathBuf::from("  path with spaces.db  "));
    }

    #[test]
    fn empty_policy_path_and_log_dir_are_preserved() {
        let defaults = test_defaults();
        let env = BTreeMap::from([
            ("ASSETIWEAVE_POLICY_PATH".into(), OsString::from("")),
            ("ASSETIWEAVE_LOG_DIR".into(), OsString::from("")),
        ]);
        let value = RuntimeConfig::from_env_map(&env, &defaults).unwrap();
        assert_eq!(value.policy_path, Some(PathBuf::from("")));
        assert_eq!(value.log_dir, PathBuf::from(""));
    }

    #[test]
    fn parsing_does_not_create_any_filesystem_directories() {
        let temp_dir = std::env::temp_dir().join(format!(
            "assetiweave_config_no_create_{}",
            uuid::Uuid::new_v4()
        ));
        let non_existent_home = temp_dir.join("home");
        let non_existent_data = temp_dir.join("data");
        let defaults = RuntimeConfigDefaults {
            home_dir: non_existent_home.clone(),
            data_dir: non_existent_data.clone(),
        };
        let env = BTreeMap::new();
        let _ = RuntimeConfig::from_env_map(&env, &defaults).unwrap();
        assert!(
            !temp_dir.exists(),
            "Parsing config must not create directories"
        );
    }

    #[test]
    #[cfg(unix)]
    fn preserves_non_utf8_os_strings() {
        use std::os::unix::ffi::OsStringExt;
        let defaults = test_defaults();
        let non_utf8_bytes = vec![0x66, 0x6f, 0x6f, 0x80, 0x62, 0x61, 0x72];
        let non_utf8_os = OsString::from_vec(non_utf8_bytes);

        let env = BTreeMap::from([
            ("ASSETIWEAVE_HOME".into(), non_utf8_os.clone()),
            ("ASSETIWEAVE_DB_PATH".into(), non_utf8_os.clone()),
            ("ASSETIWEAVE_LOG_DIR".into(), non_utf8_os.clone()),
            ("ASSETIWEAVE_POLICY_PATH".into(), non_utf8_os.clone()),
        ]);

        let value = RuntimeConfig::from_env_map(&env, &defaults).unwrap();
        assert_eq!(value.home_dir, PathBuf::from(&non_utf8_os));
        assert_eq!(value.db_path, PathBuf::from(&non_utf8_os));
        assert_eq!(value.log_dir, PathBuf::from(&non_utf8_os));
        assert_eq!(value.policy_path, Some(PathBuf::from(&non_utf8_os)));
    }

    #[test]
    fn adoption_guard_verifies_config_builder_is_used() {
        let content = include_str!("config.rs");
        assert!(
            content.contains("config::Config::builder()"),
            "config.rs must use config::Config::builder() from config crate"
        );
    }

    #[test]
    fn startup_consumers_do_not_parse_environment_again() {
        let sources = [
            include_str!("../path_utils.rs"),
            include_str!("../logs.rs"),
            include_str!("../app_settings.rs"),
        ];
        let old_read = concat!("var_os(\"ASSETIWEAVE_", "DB_PATH\")");
        let old_home = concat!("var(\"ASSETIWEAVE_", "HOME\")");
        assert!(sources.iter().all(|source| !source.contains(old_read)));
        assert!(sources.iter().all(|source| !source.contains(old_home)));
    }
}

pub(crate) fn runtime_config() -> AppResult<std::sync::Arc<RuntimeConfig>> {
    if let Some(runtime) = super::current_process_runtime() {
        return Ok(runtime.config());
    }
    RuntimeConfig::from_environment().map(std::sync::Arc::new)
}
