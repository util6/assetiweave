use crate::{
    service::{
        self, AppService, ApplySkillGroupMountParams, AssetIdParams, AssetProfileParams,
        AssetRefParams, CreateProfileParams, CreateSkillGroupParams, CreateSourceParams,
        ExecutePlanParams, GroupIdParams, IdParams, ImportSkillParams, ListAssetsParams,
        LogsGetSnapshotParams, LogsWriteOperationParams, ProfileIdParams, RequiredAssetIdParams,
        RevealPathParams, SetAssetMountParams, SetSkillGroupManualMembersParams,
        SkillGroupExclusiveMountParams, SkillGroupMountParams, SourceAddParams, SourceRemoveParams,
        SourceScanParams, UpdateAppShortcutsParams, UpdateNavigationModelParams,
        UpdateProfileParams, UpdateSkillBackupSettingsParams, UpdateSkillGroupParams,
        UpdateSourceParams,
    },
    types::AppResult,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read, Write};

type EngineResult<T> = Result<T, EngineError>;

#[derive(Debug, Deserialize)]
struct EngineRequest {
    id: Option<String>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct EngineResponse {
    id: Option<String>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<EngineError>,
}

#[derive(Debug, Serialize)]
pub(crate) struct EngineError {
    #[serde(rename = "type")]
    kind: String,
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

pub(crate) fn run_stdio() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let request: EngineRequest = match serde_json::from_str(&input) {
        Ok(request) => request,
        Err(error) => {
            return write_response(EngineResponse {
                id: None,
                ok: false,
                data: None,
                meta: None,
                error: Some(EngineError::validation(
                    "invalid_json",
                    format!("request body is not valid JSON: {error}"),
                    Some("send one JSON-RPC request object on stdin".to_string()),
                )),
            });
        }
    };

    let id = request.id.clone();
    let response = match dispatch(request) {
        Ok(data) => EngineResponse {
            id,
            ok: true,
            data: Some(data),
            meta: None,
            error: None,
        },
        Err(error) => EngineResponse {
            id,
            ok: false,
            data: None,
            meta: None,
            error: Some(error),
        },
    };

    write_response(response)
}

fn response_string(response: EngineResponse) -> String {
    serde_json::to_string_pretty(&response).unwrap_or_else(|_| {
        r#"{"ok":false,"error":{"type":"internal","code":"serialization","message":"failed to serialize response"}}"#.to_string()
    })
}

fn write_response(response: EngineResponse) -> Result<(), String> {
    let mut stdout = io::stdout();
    stdout
        .write_all(response_string(response).as_bytes())
        .map_err(|error| error.to_string())?;
    stdout.write_all(b"\n").map_err(|error| error.to_string())
}

fn dispatch(request: EngineRequest) -> EngineResult<Value> {
    let method = request.method.clone();
    match method.as_str() {
        "schema.list" => Ok(service::schema_index()),
        "schema.get" => Ok(request
            .params
            .get("method")
            .and_then(Value::as_str)
            .map_or_else(service::schema_index, service::schema_get)),
        service_method if service::is_service_method(service_method) => dispatch_service(request),
        other => Err(EngineError::unknown_method(other)),
    }
}

fn dispatch_service(request: EngineRequest) -> EngineResult<Value> {
    let service = AppService::open_for_engine().map_err(EngineError::internal)?;

    match request.method.as_str() {
        "overview.get" | "get_app_overview" => json_result(service.overview()),
        "source.list" | "list_sources" => json_result(service.list_sources()),
        "list_skill_sources" => json_result(service.list_skill_sources()),
        "create_source" => {
            let params = parse_params::<CreateSourceParams>(request.params)?;
            json_result(service.add_source(params.source))
        }
        "update_source" => {
            let params = parse_params::<UpdateSourceParams>(request.params)?;
            json_result(service.update_source(params.source))
        }
        "delete_source" => {
            let params = parse_params::<IdParams>(request.params)?;
            json_result(service.delete_source(params.id))
        }
        "source.add" => {
            let params = parse_params::<SourceAddParams>(request.params)?;
            json_result(service.add_source_with_options(params))
        }
        "source.remove" => {
            let params = parse_params::<SourceRemoveParams>(request.params)?;
            json_result(service.remove_source(params))
        }
        "source.scan" | "scan_sources" => {
            let params = parse_params::<SourceScanParams>(request.params)?;
            json_result(service.scan_sources(params))
        }
        "scan_skill_sources" => json_result(service.scan_skill_sources()),
        "profile.list" | "list_profiles" => json_result(service.list_profiles()),
        "create_profile" => {
            let params = parse_params::<CreateProfileParams>(request.params)?;
            json_result(service.create_profile(params.input))
        }
        "update_profile" => {
            let params = parse_params::<UpdateProfileParams>(request.params)?;
            json_result(service.update_profile(params.profile))
        }
        "delete_profile" => {
            let params = parse_params::<IdParams>(request.params)?;
            json_result(service.delete_profile(params.id))
        }
        "get_navigation_model" => json_result(service.navigation_model()),
        "update_navigation_model" => {
            let params = parse_params::<UpdateNavigationModelParams>(request.params)?;
            json_result(service.update_navigation_model(params.model))
        }
        "list_app_shortcuts" => json_result(service.list_app_shortcuts()),
        "list_app_shortcut_settings" => json_result(service.list_app_shortcut_settings()),
        "update_app_shortcuts" => {
            let params = parse_params::<UpdateAppShortcutsParams>(request.params)?;
            json_result(service.update_app_shortcuts(params.shortcuts))
        }
        "asset.list" | "list_assets" => {
            let params = parse_params::<ListAssetsParams>(request.params)?;
            json_result(service.list_assets(params))
        }
        "get_skill_backup_settings" => json_result(service.get_skill_backup_settings()),
        "update_skill_backup_settings" => {
            let params = parse_params::<UpdateSkillBackupSettingsParams>(request.params)?;
            json_result(service.update_skill_backup_settings(params))
        }
        "backup_skill" => {
            let params = parse_params::<RequiredAssetIdParams>(request.params)?;
            json_result(service.backup_skill(params.asset_id))
        }
        "update_asset_description" => {
            let params_value = request.params;
            let params = parse_params::<RequiredAssetIdParams>(params_value.clone())?;
            let description = params_value
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string);
            json_result(service.update_asset_description(params.asset_id, description))
        }
        "delete_asset" => {
            let params_value = request.params;
            let params = parse_params::<RequiredAssetIdParams>(params_value.clone())?;
            let unmount = params_value
                .get("unmount")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            json_result(service.delete_asset(params.asset_id, unmount))
        }
        "list_asset_mounts" => {
            let params = parse_params::<AssetIdParams>(request.params)?;
            json_result(service.list_asset_mounts(params.asset_id.as_deref()))
        }
        "list_asset_mount_statuses" => {
            let params = parse_params::<AssetIdParams>(request.params)?;
            json_result(service.list_asset_mount_statuses(params.asset_id.as_deref()))
        }
        "refresh_asset_mount_statuses" => {
            let params = parse_params::<AssetIdParams>(request.params)?;
            json_result(service.refresh_asset_mount_statuses(params.asset_id.as_deref()))
        }
        "skill.list" => json_result(service.list_skills()),
        "skill.import" => {
            let params = parse_params::<ImportSkillParams>(request.params)?;
            json_result(service.import_skill(params))
        }
        "skill.backup" => {
            let params = parse_params::<RequiredAssetIdParams>(request.params)?;
            json_result(service.backup_skill(params.asset_id))
        }
        "skill.delete" => {
            let params = parse_params::<AssetRefParams>(request.params)?;
            json_result(service.delete_skill(params))
        }
        "skill.mount" => {
            let params = parse_params::<AssetRefParams>(request.params)?;
            json_result(service.mount_skill(params, true))
        }
        "skill.unmount" => {
            let params = parse_params::<AssetRefParams>(request.params)?;
            json_result(service.mount_skill(params, false))
        }
        "mount_asset_mount" => {
            let params = parse_params::<AssetProfileParams>(request.params)?;
            json_result(service.mount_asset_by_id(&params.asset_id, &params.profile_id))
        }
        "unmount_asset_mount" => {
            let params = parse_params::<AssetProfileParams>(request.params)?;
            json_result(service.unmount_asset_by_id(&params.asset_id, &params.profile_id))
        }
        "skill.group.list" | "list_skill_groups" => json_result(service.list_skill_groups()),
        "skill.group.mount" => {
            let params = parse_params::<SkillGroupMountParams>(request.params)?;
            json_result(service.mount_skill_group(params, true))
        }
        "skill.group.unmount" => {
            let params = parse_params::<SkillGroupMountParams>(request.params)?;
            json_result(service.mount_skill_group(params, false))
        }
        "create_skill_group" => {
            let params = parse_params::<CreateSkillGroupParams>(request.params)?;
            json_result(service.create_skill_group(params.input))
        }
        "update_skill_group" => {
            let params = parse_params::<UpdateSkillGroupParams>(request.params)?;
            json_result(service.update_skill_group(params.group))
        }
        "delete_skill_group" => {
            let params = parse_params::<GroupIdParams>(request.params)?;
            json_result(service.delete_skill_group(params.group_id))
        }
        "set_skill_group_manual_members" => {
            let params = parse_params::<SetSkillGroupManualMembersParams>(request.params)?;
            json_result(service.set_skill_group_manual_members(params.group_id, params.asset_ids))
        }
        "apply_skill_group_mount" => {
            let params = parse_params::<ApplySkillGroupMountParams>(request.params)?;
            json_result(service.apply_skill_group_mount(
                &params.group_id,
                &params.profile_id,
                params.enabled,
            ))
        }
        "preview_skill_group_exclusive_mount" => {
            let params = parse_params::<SkillGroupExclusiveMountParams>(request.params)?;
            json_result(service.preview_skill_group_exclusive_mount(params.input))
        }
        "apply_skill_group_exclusive_mount" => {
            let params = parse_params::<SkillGroupExclusiveMountParams>(request.params)?;
            json_result(service.apply_skill_group_exclusive_mount(params.input))
        }
        "toggle_asset_mount" => {
            let params = parse_params::<AssetProfileParams>(request.params)?;
            json_result(service.toggle_asset_mount(&params.asset_id, &params.profile_id))
        }
        "set_asset_mount" => {
            let params = parse_params::<SetAssetMountParams>(request.params)?;
            json_result(service.set_asset_mount(
                &params.asset_id,
                &params.profile_id,
                params.enabled,
                params.strategy,
            ))
        }
        "create_plan" => {
            let params = parse_params::<ProfileIdParams>(request.params)?;
            json_result(service.create_plan(params.profile_id.as_deref()))
        }
        "execute_plan" => {
            let params = parse_params::<ExecutePlanParams>(request.params)?;
            json_result(service.execute_plan(params.plan, params.action_ids))
        }
        "logs_get_snapshot" => {
            let params = parse_params::<LogsGetSnapshotParams>(request.params)?;
            json_result(service.logs_get_snapshot(params.file_name, params.line_limit))
        }
        "logs_open_log_directory" => json_result(service.logs_open_log_directory()),
        "logs_write_operation" => {
            let params = parse_params::<LogsWriteOperationParams>(request.params)?;
            json_result(service.logs_write_operation(
                params.level,
                params.operation,
                params.message,
                params.fields,
            ))
        }
        "reveal_path" => {
            let params = parse_params::<RevealPathParams>(request.params)?;
            json_result(service.reveal_path(params.path))
        }
        "doctor.run" => json_result(service.run_doctor()),
        other => Err(EngineError::unknown_method(other)),
    }
}

fn parse_params<T: for<'de> Deserialize<'de>>(value: Value) -> EngineResult<T> {
    serde_json::from_value(value).map_err(|error| {
        EngineError::validation(
            "invalid_params",
            format!("invalid method params: {error}"),
            Some(
                "run `assetiweave-cli schema get <method>` to inspect required params".to_string(),
            ),
        )
    })
}

fn json_result<T: Serialize>(result: AppResult<T>) -> EngineResult<Value> {
    let value = result.map_err(EngineError::from_app)?;
    serde_json::to_value(value).map_err(|error| EngineError::internal(error.to_string()))
}

impl EngineError {
    fn unknown_method(method: &str) -> Self {
        Self {
            kind: "unknown_method".to_string(),
            code: "unknown_method".to_string(),
            message: format!("unknown engine method: {method}"),
            hint: Some("run `assetiweave-cli schema` to list supported methods".to_string()),
            details: Some(json!({ "method": method })),
        }
    }

    fn validation(code: &str, message: String, hint: Option<String>) -> Self {
        Self {
            kind: "validation".to_string(),
            code: code.to_string(),
            message,
            hint,
            details: None,
        }
    }

    fn internal(message: String) -> Self {
        Self {
            kind: "internal".to_string(),
            code: "internal".to_string(),
            message,
            hint: None,
            details: None,
        }
    }

    fn from_app(message: String) -> Self {
        let kind = if message.contains("not found") {
            "not_found"
        } else if message.contains("already exists")
            || message.contains("ambiguous")
            || message.contains("requires --yes")
            || message.contains("enabled mounts")
        {
            "conflict"
        } else {
            "operation_error"
        };
        Self {
            kind: kind.to_string(),
            code: kind.to_string(),
            message,
            hint: None,
            details: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeSet,
        env, fs,
        path::PathBuf,
        sync::{Mutex, OnceLock},
    };
    use uuid::Uuid;

    #[test]
    fn unknown_method_returns_structured_error() {
        let _guard = env_lock().lock().expect("env lock");
        let error = dispatch(EngineRequest {
            id: Some("1".to_string()),
            method: "missing.method".to_string(),
            params: json!({}),
        })
        .expect_err("unknown method should fail");

        assert_eq!(error.kind, "unknown_method");
        assert_eq!(error.code, "unknown_method");
        assert!(error.hint.as_deref().unwrap_or_default().contains("schema"));
    }

    #[test]
    fn import_skill_dry_run_does_not_copy_to_library() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-home");
        let db_path = home.join("app.db");
        let source = unique_temp_dir("assetiweave-engine-skill");
        fs::create_dir_all(&home).expect("create temp home");
        fs::create_dir_all(&source).expect("create skill source");
        fs::write(source.join("SKILL.md"), "description: dry run").expect("write skill");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);
        let value = dispatch(EngineRequest {
            id: Some("1".to_string()),
            method: "skill.import".to_string(),
            params: json!({
                "from": source.to_string_lossy(),
                "name": "dry-run-skill",
                "dry_run": true
            }),
        })
        .expect("dry run import");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        let target = home
            .join(".assetiweave")
            .join("library")
            .join("skills")
            .join("downloaded")
            .join("dry-run-skill");
        assert_eq!(value["dry_run"], json!(true));
        assert!(!target.exists());
        fs::remove_dir_all(home).ok();
        fs::remove_dir_all(source).ok();
    }

    #[test]
    fn import_skill_uses_configured_backup_downloaded_directory() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-import-home");
        let db_path = home.join("app.db");
        let backup_root = unique_temp_dir("assetiweave-engine-backup-root");
        let source = unique_temp_dir("assetiweave-engine-import-skill");
        fs::create_dir_all(&home).expect("create temp home");
        fs::create_dir_all(&source).expect("create skill source");
        fs::write(source.join("SKILL.md"), "description: downloaded").expect("write skill");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        dispatch(EngineRequest {
            id: Some("settings".to_string()),
            method: "update_skill_backup_settings".to_string(),
            params: json!({
                "root_path": backup_root.to_string_lossy(),
                "migrate": true
            }),
        })
        .expect("update backup settings");
        let value = dispatch(EngineRequest {
            id: Some("import".to_string()),
            method: "skill.import".to_string(),
            params: json!({
                "from": source.to_string_lossy(),
                "name": "downloaded-skill",
                "dry_run": false
            }),
        })
        .expect("import skill");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        let target = backup_root.join("downloaded").join("downloaded-skill");
        assert!(target.join("SKILL.md").exists());
        assert_eq!(
            value["asset"]["relative_path"],
            json!("downloaded/downloaded-skill")
        );
        fs::remove_dir_all(home).ok();
        fs::remove_dir_all(backup_root).ok();
        fs::remove_dir_all(source).ok();
    }

    #[test]
    fn backup_settings_migrate_custom_root_and_delete_old_custom_root() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-migration-home");
        let db_path = home.join("app.db");
        let old_root = unique_temp_dir("assetiweave-engine-old-backup");
        let new_root = unique_temp_dir("assetiweave-engine-new-backup");
        fs::create_dir_all(&home).expect("create temp home");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        dispatch(EngineRequest {
            id: Some("old".to_string()),
            method: "update_skill_backup_settings".to_string(),
            params: json!({
                "root_path": old_root.to_string_lossy(),
                "migrate": true
            }),
        })
        .expect("move to old custom backup root");
        let old_skill = old_root.join("downloaded").join("old-skill");
        fs::create_dir_all(&old_skill).expect("create old downloaded skill");
        fs::write(old_skill.join("SKILL.md"), "description: old").expect("write old skill");

        let settings = dispatch(EngineRequest {
            id: Some("new".to_string()),
            method: "update_skill_backup_settings".to_string(),
            params: json!({
                "root_path": new_root.to_string_lossy(),
                "migrate": true
            }),
        })
        .expect("move to new custom backup root");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert_eq!(settings["root_path"], json!(new_root.to_string_lossy()));
        assert!(new_root
            .join("downloaded")
            .join("old-skill")
            .join("SKILL.md")
            .exists());
        assert!(!old_root.exists());
        fs::remove_dir_all(home).ok();
        fs::remove_dir_all(new_root).ok();
    }

    #[test]
    fn backup_skill_copies_app_target_skill_and_catalog_shows_backup_copy() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-backup-home");
        let db_path = home.join("app.db");
        let backup_root = unique_temp_dir("assetiweave-engine-backup-target");
        let app_source_root = unique_temp_dir("assetiweave-engine-app-source");
        let skill = app_source_root.join("app-skill");
        fs::create_dir_all(&home).expect("create temp home");
        fs::create_dir_all(&skill).expect("create app skill");
        fs::write(skill.join("SKILL.md"), "description: app target").expect("write app skill");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        dispatch(EngineRequest {
            id: Some("settings".to_string()),
            method: "update_skill_backup_settings".to_string(),
            params: json!({
                "root_path": backup_root.to_string_lossy(),
                "migrate": true
            }),
        })
        .expect("update backup settings");
        dispatch(EngineRequest {
            id: Some("source".to_string()),
            method: "source.add".to_string(),
            params: json!({
                "name": "App Target",
                "kind": "local",
                "root_path": app_source_root.to_string_lossy(),
                "scanner_kind": "skill",
                "source_origin": "app_target",
                "include_globs": ["**/SKILL.md"],
                "exclude_globs": [],
                "default_kind": "skill",
                "enabled": true,
                "priority": 100,
                "repo_root": null,
                "scan_root": "",
                "origin_app_kind": "codex"
            }),
        })
        .expect("add app target source");
        let scanned = dispatch(EngineRequest {
            id: Some("scan".to_string()),
            method: "source.scan".to_string(),
            params: json!({ "kind": "skill" }),
        })
        .expect("scan source");
        let app_asset_id = scanned
            .as_array()
            .expect("asset array")
            .iter()
            .find(|asset| asset["source_id"] != json!("assetiweave-library-skills"))
            .and_then(|asset| asset["id"].as_str())
            .expect("app asset id")
            .to_string();

        dispatch(EngineRequest {
            id: Some("backup".to_string()),
            method: "backup_skill".to_string(),
            params: json!({ "asset_id": app_asset_id }),
        })
        .expect("backup skill");
        let catalog = dispatch(EngineRequest {
            id: Some("list".to_string()),
            method: "asset.list".to_string(),
            params: json!({ "kind": "skill" }),
        })
        .expect("list catalog");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        let assets = catalog.as_array().expect("catalog array");
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0]["source_id"], json!("assetiweave-library-skills"));
        assert_eq!(assets[0]["backup_status"]["state"], json!("backed_up"));
        assert!(backup_root
            .join("backed-up")
            .join("codex")
            .join("app-skill")
            .join("SKILL.md")
            .exists());
        fs::remove_dir_all(home).ok();
        fs::remove_dir_all(backup_root).ok();
        fs::remove_dir_all(app_source_root).ok();
    }

    #[test]
    fn source_add_dry_run_does_not_persist() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-source-home");
        let db_path = home.join("app.db");
        fs::create_dir_all(&home).expect("create temp home");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        let value = dispatch(EngineRequest {
            id: Some("1".to_string()),
            method: "source.add".to_string(),
            params: json!({
                "name": "DryRun Source",
                "kind": "local",
                "root_path": home.to_string_lossy(),
                "scanner_kind": "skill",
                "source_origin": "local_folder",
                "include_globs": ["**/SKILL.md"],
                "exclude_globs": [],
                "default_kind": "skill",
                "enabled": true,
                "priority": 100,
                "repo_root": null,
                "scan_root": "",
                "origin_app_kind": null,
                "dry_run": true
            }),
        })
        .expect("dry run source add");

        let sources = dispatch(EngineRequest {
            id: Some("2".to_string()),
            method: "source.list".to_string(),
            params: json!({}),
        })
        .expect("source list");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert_eq!(value["dry_run"], json!(true));
        assert!(!sources
            .as_array()
            .expect("sources array")
            .iter()
            .any(|source| source["name"] == json!("DryRun Source")));
        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn invalid_params_return_validation_error() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-invalid-params-home");
        let db_path = home.join("app.db");
        fs::create_dir_all(&home).expect("create temp home");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        let error = dispatch(EngineRequest {
            id: Some("1".to_string()),
            method: "skill.import".to_string(),
            params: json!({}),
        })
        .expect_err("missing required params should fail");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert_eq!(error.kind, "validation");
        assert_eq!(error.code, "invalid_params");
        assert!(error.hint.as_deref().unwrap_or_default().contains("schema"));
        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn tauri_command_aliases_are_callable_and_listed() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-alias-home");
        let db_path = home.join("app.db");
        fs::create_dir_all(&home).expect("create temp home");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        let profiles = dispatch(EngineRequest {
            id: Some("1".to_string()),
            method: "list_profiles".to_string(),
            params: json!({}),
        })
        .expect("list profiles alias");
        let schema = dispatch(EngineRequest {
            id: Some("2".to_string()),
            method: "schema.list".to_string(),
            params: json!({}),
        })
        .expect("schema list");
        let mounts = dispatch(EngineRequest {
            id: Some("3".to_string()),
            method: "list_asset_mounts".to_string(),
            params: json!({ "assetId": null }),
        })
        .expect("list asset mounts with Tauri camelCase params");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert!(profiles.as_array().expect("profiles array").len() >= 1);
        assert!(mounts.as_array().expect("mounts array").is_empty());
        assert!(schema["methods"]
            .as_array()
            .expect("methods array")
            .iter()
            .any(|method| method == "list_profiles"));
        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn schema_lists_every_cli_and_tauri_command_method() {
        let schema = service::schema_index();
        let methods = schema["methods"].as_array().expect("methods array");
        let method_set = methods
            .iter()
            .map(|method| method.as_str().expect("method string"))
            .collect::<BTreeSet<_>>();

        for method in service::CLI_SERVICE_METHODS
            .iter()
            .chain(service::TAURI_COMMAND_METHODS.iter())
            .chain(["schema.list", "schema.get"].iter())
        {
            assert!(
                method_set.contains(method),
                "schema.list is missing method {method}"
            );
        }
        assert_eq!(
            method_set.len(),
            methods.len(),
            "schema.list should not contain duplicate methods"
        );
    }

    #[test]
    fn frontend_invoke_methods_are_registered_for_cli_api() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf();
        let mut frontend_methods = BTreeSet::new();
        for file in ["src/services/catalog.ts", "src/services/logService.ts"] {
            let content =
                fs::read_to_string(workspace_root.join(file)).expect("read frontend service");
            frontend_methods.extend(extract_frontend_invoke_methods(&content));
        }

        let registered = service::TAURI_COMMAND_METHODS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let missing = frontend_methods
            .iter()
            .filter(|method| !registered.contains(method.as_str()))
            .cloned()
            .collect::<Vec<_>>();

        assert!(
            missing.is_empty(),
            "frontend invoke methods missing from CLI API coverage: {missing:?}"
        );
    }

    #[test]
    fn newly_supported_tauri_write_command_is_not_unknown() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-create-profile-home");
        let db_path = home.join("app.db");
        fs::create_dir_all(&home).expect("create temp home");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        let error = dispatch(EngineRequest {
            id: Some("1".to_string()),
            method: "create_profile".to_string(),
            params: json!({}),
        })
        .expect_err("missing profile input should fail validation");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert_eq!(error.kind, "validation");
        assert_eq!(error.code, "invalid_params");
        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn external_source_skill_delete_is_rejected() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-delete-home");
        let db_path = home.join("app.db");
        let source = unique_temp_dir("assetiweave-engine-external-source");
        let skill = source.join("external-skill");
        fs::create_dir_all(&home).expect("create temp home");
        fs::create_dir_all(&skill).expect("create external skill");
        fs::write(skill.join("SKILL.md"), "description: external").expect("write skill");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        dispatch(EngineRequest {
            id: Some("1".to_string()),
            method: "source.add".to_string(),
            params: json!({
                "name": "External Source",
                "kind": "local",
                "root_path": source.to_string_lossy(),
                "scanner_kind": "skill",
                "source_origin": "local_folder",
                "include_globs": ["**/SKILL.md"],
                "exclude_globs": [],
                "default_kind": "skill",
                "enabled": true,
                "priority": 100,
                "repo_root": null,
                "scan_root": "",
                "origin_app_kind": null
            }),
        })
        .expect("add external source");
        dispatch(EngineRequest {
            id: Some("2".to_string()),
            method: "source.scan".to_string(),
            params: json!({ "kind": "skill" }),
        })
        .expect("scan external source");
        let error = dispatch(EngineRequest {
            id: Some("3".to_string()),
            method: "skill.delete".to_string(),
            params: json!({ "asset_ref": "external-skill", "yes": true }),
        })
        .expect_err("external skill delete should fail");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert_eq!(error.kind, "operation_error");
        assert!(error
            .message
            .contains("only AssetIWeave backup library skills"));
        assert!(skill.exists());
        fs::remove_dir_all(home).ok();
        fs::remove_dir_all(source).ok();
    }

    #[test]
    fn default_library_source_remove_is_rejected() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-protected-source-home");
        let db_path = home.join("app.db");
        fs::create_dir_all(&home).expect("create temp home");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        let error = dispatch(EngineRequest {
            id: Some("1".to_string()),
            method: "source.remove".to_string(),
            params: json!({ "id": "assetiweave-library-skills", "yes": true }),
        })
        .expect_err("default library source remove should fail");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert_eq!(error.kind, "operation_error");
        assert!(error.message.contains("cannot be deleted"));
        fs::remove_dir_all(home).ok();
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
    }

    fn extract_frontend_invoke_methods(content: &str) -> BTreeSet<String> {
        let mut methods = BTreeSet::new();
        for line in content.lines().filter(|line| line.contains("invoke")) {
            let Some(invoke_start) = line.find("invoke") else {
                continue;
            };
            let after_invoke = &line[invoke_start..];
            if !(after_invoke.starts_with("invoke<") || after_invoke.starts_with("invoke(")) {
                continue;
            }
            let Some(first_quote) = after_invoke.find('"') else {
                continue;
            };
            let after_first_quote = &after_invoke[first_quote + 1..];
            let Some(second_quote) = after_first_quote.find('"') else {
                continue;
            };
            methods.insert(after_first_quote[..second_quote].to_string());
        }
        methods
    }
}
