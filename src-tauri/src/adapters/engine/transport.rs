use super::{policy, protocol, registry as command_registry, runtime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read, Write};

type EngineResult<T> = Result<T, EngineError>;

#[derive(Debug)]
struct EngineRequest {
    method: String,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct WireEngineRequest {
    id: Option<String>,
    method: String,
    #[serde(default)]
    params: Value,
    protocol_version: Option<u32>,
    contract_version: Option<u32>,
}

impl From<WireEngineRequest> for EngineRequest {
    fn from(request: WireEngineRequest) -> Self {
        Self {
            method: request.method,
            params: if request.params.is_null() {
                json!({})
            } else {
                request.params
            },
        }
    }
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
    let request: WireEngineRequest = match serde_json::from_str(&input) {
        Ok(request) => request,
        Err(error) => {
            return write_response(EngineResponse {
                id: None,
                ok: false,
                data: None,
                meta: Some(response_meta()),
                error: Some(EngineError::validation(
                    "invalid_json",
                    format!("request body is not valid JSON: {error}"),
                    Some("send one JSON-RPC request object on stdin".to_string()),
                )),
            });
        }
    };

    let id = request.id.clone();
    let (hooks, mut invocation) = runtime::before(&request.method);
    let result = handle_wire_request(request);
    runtime::after(
        &hooks,
        &mut invocation,
        result.as_ref().err().map(|error| error.kind.as_str()),
    );
    let meta = response_meta_with_invocation(&invocation);
    let response = match result {
        Ok(data) => EngineResponse {
            id,
            ok: true,
            data: Some(data),
            meta: Some(meta),
            error: None,
        },
        Err(error) => EngineResponse {
            id,
            ok: false,
            data: None,
            meta: Some(meta),
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

fn response_meta() -> Value {
    protocol::response_meta()
}

fn response_meta_with_invocation(invocation: &runtime::Invocation) -> Value {
    let mut meta = response_meta();
    if let Some(object) = meta.as_object_mut() {
        object.insert("invocation".to_string(), runtime::response_meta(invocation));
    }
    meta
}

fn handle_wire_request(request: WireEngineRequest) -> EngineResult<Value> {
    validate_wire_compatibility(&request)?;
    dispatch(request.into())
}

fn validate_wire_compatibility(request: &WireEngineRequest) -> EngineResult<()> {
    if request.method == "system.version" {
        return Ok(());
    }
    if request.protocol_version != Some(protocol::PROTOCOL_VERSION) {
        return Err(EngineError::engine_incompatible(
            "protocol_version_mismatch",
            "Engine protocol version is incompatible with this request",
            json!({
                "expected": protocol::PROTOCOL_VERSION,
                "received": request.protocol_version
            }),
        ));
    }
    if request.contract_version != Some(protocol::CONTRACT_VERSION) {
        return Err(EngineError::engine_incompatible(
            "contract_version_mismatch",
            "Engine command contract version is incompatible with this request",
            json!({
                "expected": protocol::CONTRACT_VERSION,
                "received": request.contract_version
            }),
        ));
    }
    Ok(())
}

fn dispatch(mut request: EngineRequest) -> EngineResult<Value> {
    let method = request.method.clone();
    let spec =
        command_registry::find(&method).ok_or_else(|| EngineError::unknown_method(&method))?;
    policy::authorize(spec).map_err(EngineError::from_policy)?;
    if command_registry::requires_confirmation(spec, &request.params) {
        return Err(EngineError::confirmation_required(
            &method,
            spec.risk.as_str(),
        ));
    }
    request.params = command_registry::validate_params(spec, &request.params)
        .map_err(|violations| EngineError::invalid_params(&method, violations))?;
    spec.dispatch(request.params)
        .map_err(EngineError::from_dispatch)
}

impl EngineError {
    fn from_dispatch(failure: command_registry::DispatchFailure) -> Self {
        match failure {
            command_registry::DispatchFailure::InvalidParams(message) => Self::internal(message),
            command_registry::DispatchFailure::OpenService(message) => Self::internal(message),
            command_registry::DispatchFailure::App(message) => Self::from_app(message),
            command_registry::DispatchFailure::Serialize(message) => Self::internal(message),
        }
    }

    fn engine_incompatible(code: &str, message: &str, details: Value) -> Self {
        Self {
            kind: "engine_incompatible".to_string(),
            code: code.to_string(),
            message: message.to_string(),
            hint: Some("install the CLI and Engine from the same AssetIWeave release".to_string()),
            details: Some(details),
        }
    }

    fn unknown_method(method: &str) -> Self {
        Self {
            kind: "unknown_method".to_string(),
            code: "unknown_method".to_string(),
            message: format!("unknown engine method: {method}"),
            hint: Some("run `assetiweave-cli schema` to list supported methods".to_string()),
            details: Some(json!({ "method": method })),
        }
    }

    fn confirmation_required(method: &str, risk: &str) -> Self {
        Self {
            kind: "confirmation_required".to_string(),
            code: "confirmation_required".to_string(),
            message: format!("{method} requires explicit confirmation"),
            hint: Some("rerun with yes=true after reviewing the operation".to_string()),
            details: Some(json!({
                "method": method,
                "risk": risk
            })),
        }
    }

    fn invalid_params(method: &str, violations: Vec<command_registry::ParamViolation>) -> Self {
        Self {
            kind: "validation".to_string(),
            code: "invalid_params".to_string(),
            message: format!("invalid method params for {method}"),
            hint: Some(
                "run `assetiweave-cli schema get <method>` to inspect required params".to_string(),
            ),
            details: Some(json!({
                "method": method,
                "violations": violations
            })),
        }
    }

    fn from_policy(failure: policy::PolicyFailure) -> Self {
        Self {
            kind: failure.kind.to_string(),
            code: failure.kind.to_string(),
            message: failure.message,
            hint: Some(
                "review ASSETIWEAVE_POLICY_PATH or run a diagnostic command for details"
                    .to_string(),
            ),
            details: Some(failure.details),
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
        process::Command,
        sync::{Mutex, OnceLock},
    };
    use uuid::Uuid;

    #[test]
    fn unknown_method_returns_structured_error() {
        let _guard = env_lock().lock().expect("env lock");
        let error = dispatch(EngineRequest {
            method: "missing.method".to_string(),
            params: json!({}),
        })
        .expect_err("unknown method should fail");

        assert_eq!(error.kind, "unknown_method");
        assert_eq!(error.code, "unknown_method");
        assert!(error.hint.as_deref().unwrap_or_default().contains("schema"));
    }

    #[test]
    fn registered_handler_uses_the_bound_request_type() {
        let spec = command_registry::find("source.add").expect("source.add spec");
        let error = spec
            .dispatch(json!({ "id": "source-id" }))
            .expect_err("registered handler must parse the bound request type");

        assert!(matches!(
            error,
            command_registry::DispatchFailure::InvalidParams(message)
                if message.contains("registered handler params failed")
        ));
    }

    #[test]
    fn mismatched_wire_protocol_is_rejected_before_dispatch() {
        let error = handle_wire_request(WireEngineRequest {
            id: None,
            method: "profile.list".to_string(),
            params: json!({}),
            protocol_version: Some(99),
            contract_version: Some(protocol::CONTRACT_VERSION),
        })
        .expect_err("mismatched protocol should fail");

        assert_eq!(error.kind, "engine_incompatible");
        assert_eq!(error.code, "protocol_version_mismatch");
    }

    #[test]
    fn version_probe_does_not_require_compatibility_fields() {
        let value = handle_wire_request(WireEngineRequest {
            id: Some("version".to_string()),
            method: "system.version".to_string(),
            params: json!({}),
            protocol_version: None,
            contract_version: None,
        })
        .expect("version probe");

        assert_eq!(value["protocol_version"], json!(protocol::PROTOCOL_VERSION));
        assert_eq!(value["contract_version"], json!(protocol::CONTRACT_VERSION));
    }

    #[test]
    fn system_version_exposes_compatibility_contract() {
        let value = dispatch(EngineRequest {
            method: "system.version".to_string(),
            params: json!({}),
        })
        .expect("system.version");

        assert_eq!(value["product"], json!("AssetIWeave"));
        assert_eq!(value["protocol_version"], json!(protocol::PROTOCOL_VERSION));
        assert_eq!(value["contract_version"], json!(protocol::CONTRACT_VERSION));
        assert_eq!(value["engine_version"], json!(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn response_meta_exposes_compatibility_contract() {
        let meta = response_meta();
        assert_eq!(meta["protocol_version"], json!(protocol::PROTOCOL_VERSION));
        assert_eq!(meta["contract_version"], json!(protocol::CONTRACT_VERSION));
        assert_eq!(meta["engine_version"], json!(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn response_meta_exposes_postflight_invocation_details() {
        let (hooks, mut invocation) = runtime::before("delete_source");
        runtime::after(&hooks, &mut invocation, Some("command_denied"));
        let meta = response_meta_with_invocation(&invocation);

        assert_eq!(meta["invocation"]["method"], json!("delete_source"));
        assert_eq!(
            meta["invocation"]["canonical_method"],
            json!("source.remove")
        );
        assert_eq!(meta["invocation"]["outcome"], json!("error"));
        assert_eq!(meta["invocation"]["error_type"], json!("command_denied"));
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
            method: "update_skill_backup_settings".to_string(),
            params: json!({
                "root_path": backup_root.to_string_lossy(),
                "migrate": true,
                "yes": true
            }),
        })
        .expect("update backup settings");
        let value = dispatch(EngineRequest {
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
    fn acquire_skill_dry_run_plans_github_tree_without_cloning() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-acquire-home");
        let db_path = home.join("app.db");
        fs::create_dir_all(&home).expect("create temp home");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        let value = dispatch(EngineRequest {
            method: "skill.acquire".to_string(),
            params: json!({
                "url": "https://github.com/util6/util6-agents/tree/main/skills/browser",
                "dry_run": true
            }),
        })
        .expect("dry run acquire");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert_eq!(value["dry_run"], json!(true));
        assert_eq!(
            value["repo_url"],
            json!("https://github.com/util6/util6-agents.git")
        );
        assert_eq!(value["branch"], json!("main"));
        assert_eq!(value["path"], json!("skills/browser"));
        assert_eq!(value["name"], json!("browser"));
        assert!(value["security_notice"]
            .as_str()
            .expect("security notice")
            .contains("does not execute or trust remote code"));
        let staging_path = value["staging_path"].as_str().expect("staging path");
        assert!(!std::path::Path::new(staging_path).exists());
        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn acquire_skill_imports_from_isolated_git_repo_and_records_remote_source() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-acquire-import-home");
        let db_path = home.join("app.db");
        let backup_root = unique_temp_dir("assetiweave-engine-acquire-import-backup");
        let repo = unique_temp_dir("assetiweave-engine-acquire-import-repo");
        fs::create_dir_all(&home).expect("create temp home");
        fs::create_dir_all(repo.join("skills/browser")).expect("create local skill repo");
        fs::write(
            repo.join("skills/browser/SKILL.md"),
            "# Browser Skill\n\nUse browser automation safely.",
        )
        .expect("write skill file");
        run_git(&repo, &["init", "-b", "main"]);
        run_git(&repo, &["add", "skills/browser/SKILL.md"]);
        run_git(
            &repo,
            &[
                "-c",
                "user.name=AssetIWeave Test",
                "-c",
                "user.email=test@example.local",
                "commit",
                "-m",
                "add browser skill",
            ],
        );
        let repo_url = format!("file://{}", repo.display());
        fs::write(
            home.join(".gitconfig"),
            format!(
                "[url \"{repo_url}\"]\n\tinsteadOf = https://github.com/util6/test-skill.git\n"
            ),
        )
        .expect("write git url rewrite");

        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);
        dispatch(EngineRequest {
            method: "update_skill_backup_settings".to_string(),
            params: json!({
                "root_path": backup_root.to_string_lossy(),
                "migrate": true,
                "yes": true
            }),
        })
        .expect("configure backup root");
        let value = dispatch(EngineRequest {
            method: "skill.acquire".to_string(),
            params: json!({
                "url": "https://github.com/util6/test-skill/tree/main/skills/browser",
                "yes": true
            }),
        })
        .expect("acquire skill from rewritten local repo");
        let remotes = dispatch(EngineRequest {
            method: "skill.remote.list".to_string(),
            params: json!({}),
        })
        .expect("list remote sources");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        let target = backup_root.join("downloaded/browser");
        assert!(target.join("SKILL.md").exists());
        assert_eq!(value["dry_run"], json!(false));
        assert_eq!(value["name"], json!("browser"));
        assert_eq!(
            value["import"]["asset"]["relative_path"],
            json!("downloaded/browser")
        );
        assert_eq!(
            value["remote_source"]["repo_url"],
            json!("https://github.com/util6/test-skill.git")
        );
        assert_eq!(value["remote_source"]["branch"], json!("main"));
        assert_eq!(value["remote_source"]["path"], json!("skills/browser"));
        assert!(value["remote_source"]["acquired_tree_sha"]
            .as_str()
            .is_some_and(|sha| !sha.is_empty()));
        assert!(value["remote_source"]["local_content_hash"]
            .as_str()
            .is_some_and(|hash| !hash.is_empty()));
        assert!(value["security_notice"]
            .as_str()
            .expect("security notice")
            .contains("does not execute or trust remote code"));
        let remote_list = remotes.as_array().expect("remote list array");
        assert_eq!(remote_list.len(), 1);
        assert_eq!(remote_list[0]["asset_id"], value["import"]["asset"]["id"]);
        fs::remove_dir_all(home).ok();
        fs::remove_dir_all(backup_root).ok();
        fs::remove_dir_all(repo).ok();
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
            method: "update_skill_backup_settings".to_string(),
            params: json!({
                "root_path": old_root.to_string_lossy(),
                "migrate": true,
                "yes": true
            }),
        })
        .expect("move to old custom backup root");
        let old_skill = old_root.join("downloaded").join("old-skill");
        fs::create_dir_all(&old_skill).expect("create old downloaded skill");
        fs::write(old_skill.join("SKILL.md"), "description: old").expect("write old skill");

        let settings = dispatch(EngineRequest {
            method: "update_skill_backup_settings".to_string(),
            params: json!({
                "root_path": new_root.to_string_lossy(),
                "migrate": true,
                "yes": true
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
            method: "update_skill_backup_settings".to_string(),
            params: json!({
                "root_path": backup_root.to_string_lossy(),
                "migrate": true,
                "yes": true
            }),
        })
        .expect("update backup settings");
        dispatch(EngineRequest {
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
            method: "backup_skill".to_string(),
            params: json!({ "asset_id": app_asset_id }),
        })
        .expect("backup skill");
        let catalog = dispatch(EngineRequest {
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
    fn source_add_aliases_are_normalized_before_typed_dispatch() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-source-alias-home");
        let db_path = home.join("app.db");
        fs::create_dir_all(&home).expect("create temp home");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        let value = dispatch(EngineRequest {
            method: "source.add".to_string(),
            params: json!({
                "name": "Alias Source",
                "kind": "local",
                "rootPath": home.to_string_lossy(),
                "scannerKind": "skill",
                "sourceOrigin": "local_folder",
                "includeGlobs": ["**/SKILL.md"],
                "excludeGlobs": [],
                "defaultKind": "skill",
                "enabled": true,
                "priority": 100,
                "repoRoot": null,
                "scanRoot": "",
                "originAppKind": null,
                "dryRun": true
            }),
        })
        .expect("aliases should reach typed source.add dispatch");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert_eq!(value["dry_run"], json!(true));
        assert_eq!(value["source"]["root_path"], json!(home.to_string_lossy()));
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
            method: "list_profiles".to_string(),
            params: json!({}),
        })
        .expect("list profiles alias");
        let schema = dispatch(EngineRequest {
            method: "schema.list".to_string(),
            params: json!({}),
        })
        .expect("schema list");
        let mounts = dispatch(EngineRequest {
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
        let schema = command_registry::schema_index();
        let methods = schema["methods"].as_array().expect("methods array");
        let method_set = methods
            .iter()
            .map(|method| method.as_str().expect("method string"))
            .collect::<BTreeSet<_>>();

        for method in command_registry::command_specs()
            .iter()
            .map(|spec| spec.method)
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
    fn command_registry_owns_engine_dispatch_handlers() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf();
        let engine_source =
            fs::read_to_string(workspace_root.join("src-tauri/src/adapters/engine/transport.rs"))
                .expect("read engine");
        let version = command_registry::find("system.version")
            .expect("system.version spec")
            .dispatch(json!({}))
            .expect("registered handler should execute");

        assert_eq!(version["product"], json!("AssetIWeave"));
        let legacy_dispatch_function = ["fn dispatch_", "service("].concat();
        assert!(!engine_source.contains(&legacy_dispatch_function));
    }

    #[test]
    fn registry_matches_tauri_handler_methods() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf();
        let tauri_command_source =
            fs::read_to_string(workspace_root.join("src-tauri/src/adapters/tauri/commands.rs"))
                .expect("read Tauri commands");
        let tauri_handlers = extract_tauri_handler_methods(&tauri_command_source);
        let registered = command_registry::command_specs()
            .iter()
            .filter(|spec| command_registry::is_app_method(spec.method))
            .map(|spec| spec.method.to_string())
            .collect::<BTreeSet<_>>();

        assert_eq!(
            tauri_handlers, registered,
            "Tauri invoke handlers and App command registry drifted"
        );
    }

    #[test]
    fn schema_exposes_contract_metadata_for_every_method() {
        let schema = command_registry::schema_index();
        let methods = schema["methods"].as_array().expect("methods array");
        let commands = schema["commands"].as_array().expect("commands array");

        assert_eq!(commands.len(), methods.len());
        for command in commands {
            assert!(
                command["method"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty()),
                "command contract is missing method: {command}"
            );
            assert!(
                matches!(
                    command["risk"].as_str(),
                    Some("read" | "write" | "high-risk-write")
                ),
                "command contract has invalid risk: {command}"
            );
            assert!(
                command["params_schema"].is_object(),
                "command contract is missing params_schema: {command}"
            );
        }
    }

    #[test]
    fn high_risk_raw_method_requires_explicit_confirmation() {
        let error = dispatch(EngineRequest {
            method: "delete_source".to_string(),
            params: json!({ "id": "source-id" }),
        })
        .expect_err("high-risk raw method should require confirmation");

        assert_eq!(error.kind, "confirmation_required");
        assert_eq!(error.code, "confirmation_required");
        assert_eq!(
            error.details,
            Some(json!({
                "method": "delete_source",
                "risk": "high-risk-write"
            }))
        );
    }

    #[test]
    fn unsupported_dry_run_does_not_bypass_high_risk_confirmation() {
        let error = dispatch(EngineRequest {
            method: "delete_source".to_string(),
            params: json!({ "id": "source-id", "dry_run": true }),
        })
        .expect_err("unsupported dry-run must not bypass confirmation");

        assert_eq!(error.kind, "confirmation_required");
        assert_eq!(error.code, "confirmation_required");
    }

    #[test]
    fn unknown_method_params_are_rejected_before_service_dispatch() {
        let error = dispatch(EngineRequest {
            method: "profile.list".to_string(),
            params: json!({ "typo": true }),
        })
        .expect_err("unknown params should fail");

        assert_eq!(error.kind, "validation");
        assert_eq!(error.code, "invalid_params");
        assert!(error
            .details
            .as_ref()
            .is_some_and(|details| details["violations"].is_array()));
    }

    #[test]
    fn nested_type_mismatch_is_rejected_before_service_dispatch() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-nested-params-home");
        let db_path = home.join("app.db");
        fs::create_dir_all(&home).expect("create temp home");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", &db_path);

        let error = dispatch(EngineRequest {
            method: "create_profile".to_string(),
            params: json!({
                "input": {
                    "name": "Invalid profile",
                    "target_paths": "not-an-array"
                }
            }),
        })
        .expect_err("nested type mismatch should fail");

        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert_eq!(error.kind, "validation");
        assert_eq!(error.code, "invalid_params");
        assert!(
            !db_path.exists(),
            "validation must run before database open"
        );
        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn command_policy_denies_confirmed_high_risk_method_before_service_dispatch() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-policy-home");
        let policy_path = home.join("policy.json");
        fs::create_dir_all(&home).expect("create policy home");
        fs::write(&policy_path, r#"{"version":1,"deny":["delete_source"]}"#).expect("write policy");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", home.join("app.db"));
        env::set_var("ASSETIWEAVE_POLICY_PATH", &policy_path);

        let error = dispatch(EngineRequest {
            method: "delete_source".to_string(),
            params: json!({ "id": "missing", "yes": true }),
        })
        .expect_err("policy should deny command");

        env::remove_var("ASSETIWEAVE_POLICY_PATH");
        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert_eq!(error.kind, "command_denied");
        assert_eq!(error.code, "command_denied");
        assert!(!home.join("app.db").exists());
        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn invalid_command_policy_fails_closed_for_non_diagnostic_methods() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-invalid-policy-home");
        let policy_path = home.join("policy.json");
        fs::create_dir_all(&home).expect("create policy home");
        fs::write(&policy_path, "{").expect("write invalid policy");
        env::set_var("HOME", &home);
        env::set_var("ASSETIWEAVE_DB_PATH", home.join("app.db"));
        env::set_var("ASSETIWEAVE_POLICY_PATH", &policy_path);

        let error = dispatch(EngineRequest {
            method: "profile.list".to_string(),
            params: json!({}),
        })
        .expect_err("invalid policy should fail closed");

        env::remove_var("ASSETIWEAVE_POLICY_PATH");
        env::remove_var("ASSETIWEAVE_DB_PATH");
        env::remove_var("HOME");
        assert_eq!(error.kind, "policy_invalid");
        assert_eq!(error.code, "policy_invalid");
        assert!(!home.join("app.db").exists());
        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn diagnostic_method_remains_available_when_command_policy_is_invalid() {
        let _guard = env_lock().lock().expect("env lock");
        let home = unique_temp_dir("assetiweave-engine-diagnostic-policy-home");
        let policy_path = home.join("policy.json");
        fs::create_dir_all(&home).expect("create policy home");
        fs::write(&policy_path, "{").expect("write invalid policy");
        env::set_var("ASSETIWEAVE_POLICY_PATH", &policy_path);

        let value = dispatch(EngineRequest {
            method: "system.version".to_string(),
            params: json!({}),
        })
        .expect("diagnostic method should bypass invalid policy");

        env::remove_var("ASSETIWEAVE_POLICY_PATH");
        assert_eq!(value["product"], json!("AssetIWeave"));
        fs::remove_dir_all(home).ok();
    }

    #[test]
    fn frontend_invoke_methods_are_registered_for_cli_api() {
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .to_path_buf();
        let mut frontend_methods = BTreeSet::new();
        for file in frontend_source_files(&workspace_root.join("frontend/src")) {
            let content = fs::read_to_string(file).expect("read frontend source");
            frontend_methods.extend(extract_frontend_invoke_methods(&content));
        }

        let registered = command_registry::command_specs()
            .iter()
            .filter(|spec| command_registry::is_app_method(spec.method))
            .map(|spec| spec.method)
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
            method: "source.scan".to_string(),
            params: json!({ "kind": "skill" }),
        })
        .expect("scan external source");
        let error = dispatch(EngineRequest {
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

    fn run_git(repo: &std::path::Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn extract_frontend_invoke_methods(content: &str) -> BTreeSet<String> {
        let mut methods = BTreeSet::new();
        let mut remaining = content;
        while let Some(invoke_start) = remaining.find("invoke") {
            let after_invoke = &remaining[invoke_start..];
            if !(after_invoke.starts_with("invoke<") || after_invoke.starts_with("invoke(")) {
                remaining = &after_invoke["invoke".len()..];
                continue;
            }
            let Some(first_quote) = after_invoke.find('"') else {
                break;
            };
            let after_first_quote = &after_invoke[first_quote + 1..];
            let Some(second_quote) = after_first_quote.find('"') else {
                break;
            };
            methods.insert(after_first_quote[..second_quote].to_string());
            remaining = &after_first_quote[second_quote + 1..];
        }
        methods
    }

    fn extract_tauri_handler_methods(content: &str) -> BTreeSet<String> {
        let marker = "::tauri::generate_handler![";
        let start = content.find(marker).expect("generate_handler marker") + marker.len();
        let body = &content[start..];
        let end = body.find(']').expect("generate_handler end");
        body[..end]
            .split(',')
            .map(str::trim)
            .filter(|method| !method.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn frontend_source_files(root: &std::path::Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let entries = fs::read_dir(root).expect("read frontend source directory");
        for entry in entries {
            let path = entry.expect("frontend source entry").path();
            if path.is_dir() {
                files.extend(frontend_source_files(&path));
            } else if matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("ts" | "tsx")
            ) && !path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| name.contains(".test."))
            {
                files.push(path);
            }
        }
        files
    }
}
