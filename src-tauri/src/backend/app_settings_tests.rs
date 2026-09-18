use super::*;
use std::sync::{Mutex, OnceLock};

fn settings_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn conversation_full_sync_on_startup_enabled_from_value(settings: &Value) -> bool {
    settings
        .get("conversations")
        .and_then(Value::as_object)
        .and_then(|conversations| conversations.get("autoFullSyncOnStartup"))
        .and_then(Value::as_bool)
        .unwrap_or(DEFAULT_CONVERSATION_FULL_SYNC_ON_STARTUP)
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
    struct EnvReset(Option<std::ffi::OsString>, std::path::PathBuf);
    impl Drop for EnvReset {
        fn drop(&mut self) {
            match &self.0 {
                Some(value) => std::env::set_var(TEST_HOME_VAR, value),
                None => std::env::remove_var(TEST_HOME_VAR),
            }
            std::fs::remove_dir_all(&self.1).ok();
        }
    }
    let _env_guard = EnvReset(std::env::var_os(TEST_HOME_VAR), root.clone());
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
    let res2 = save_app_settings_sqlx(db.pool(), json!({ "theme": "sunlight", "locale": null }))
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

#[tokio::test]
async fn sqlite_settings_ignore_corrupt_legacy_file_after_import() {
    let _guard = settings_test_lock().lock().expect("settings test lock");
    let root = std::env::temp_dir().join(format!(
        "assetiweave-settings-corrupt-import-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create settings test root");
    let previous_home = std::env::var_os(TEST_HOME_VAR);
    std::env::set_var(TEST_HOME_VAR, &root);

    let db_path = root.join("app.db");
    let db = crate::backend::store::Database::open_initialized_async(&db_path)
        .await
        .expect("open db");

    let expected_settings = canonicalize_settings(json!({
        "theme": "synthwave",
        "locale": "zh",
        "memory": {
            "generationEnabled": true,
            "usageEnabled": false
        }
    }))
    .expect("canonicalize");

    save_app_settings_sqlx(db.pool(), expected_settings.clone())
        .await
        .expect("save sqlite settings");

    std::fs::write(root.join(CONFIG_FILE_NAME), "{ this is corrupt json !!!")
        .expect("write corrupt legacy file");

    let service = crate::backend::application::AppService::open_with_db_path(db_path.clone())
        .await
        .expect("open service despite corrupt legacy file");

    let backend_settings = service
        .backend_settings()
        .expect("read backend settings from sqlite");
    assert!(backend_settings.is_memory_generation_enabled());
    assert!(!backend_settings.is_memory_usage_enabled());

    match previous_home {
        Some(value) => std::env::set_var(TEST_HOME_VAR, value),
        None => std::env::remove_var(TEST_HOME_VAR),
    }
    std::fs::remove_dir_all(root).ok();
}
