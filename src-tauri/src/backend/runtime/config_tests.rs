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

#[test]
fn runtime_config_defaults_from_dirs_succeeds_or_returns_app_error() {
    let res = RuntimeConfigDefaults::from_dirs();
    match res {
        Ok(defaults) => {
            assert!(!defaults.home_dir.as_os_str().is_empty());
            assert!(!defaults.data_dir.as_os_str().is_empty());
        }
        Err(err) => {
            assert_eq!(err.code(), "not_found");
        }
    }
}
