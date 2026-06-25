use super::prelude::*;
use crate::backend::models::{
    AssetFormat, AssetGroupRules, ConversationAdapterKind, ConversationAdapterTrustState,
    ConversationPartKind, ConversationPartRole, ConversationSourceKind, DeploymentState,
    NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
    SourceKind,
};
use sqlx::AssertSqlSafe;
use std::fs;

fn execute_test_sql(service: &AppService, sql: &str) -> AppResult<()> {
    let pool = service.db.pool().clone();
    service.db.block_on(async move {
        for statement in sql.split(';').map(str::trim).filter(|sql| !sql.is_empty()) {
            sqlx::query(AssertSqlSafe(statement.to_string()))
                .execute(&pool)
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    })
}

fn clear_test_tables(service: &AppService, tables: &[&str]) {
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            for table in tables {
                let statement = format!("DELETE FROM {table}");
                sqlx::query(AssertSqlSafe(statement))
                    .execute(&pool)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            AppResult::Ok(())
        })
        .expect("clear test tables");
}

fn upsert_test_source(service: &AppService, source: &Source) {
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move { crate::backend::store::upsert_source_sqlx(&pool, source).await })
        .expect("save source");
}

fn replace_test_source_assets(service: &AppService, source_id: &str, assets: &[Asset]) {
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            crate::backend::store::replace_source_assets_sqlx(&pool, source_id, assets).await
        })
        .expect("save source assets");
}

fn load_test_assets(service: &AppService) -> Vec<Asset> {
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move { crate::backend::store::load_assets_sqlx(&pool, None).await })
        .expect("load assets")
}

#[test]
fn doctor_reports_conversation_adapter_runtime_statuses() {
    let root = std::env::temp_dir().join(format!("assetiweave-doctor-runtime-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create temp dir");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");

    let report = service.run_doctor().expect("run doctor");
    let checks = report["checks"].as_array().expect("doctor checks");
    let runtime_check = checks
        .iter()
        .find(|check| check["name"] == "conversation_adapter_runtimes")
        .expect("runtime check");
    let details = runtime_check["details"]
        .as_array()
        .expect("runtime details");
    let kinds = details
        .iter()
        .filter_map(|detail| detail["kind"].as_str())
        .collect::<Vec<_>>();
    let node_available = details
        .iter()
        .find(|detail| detail["kind"].as_str() == Some("node"))
        .and_then(|detail| detail["available"].as_bool())
        .unwrap_or(false);
    let node_required_version = details
        .iter()
        .find(|detail| detail["kind"].as_str() == Some("node"))
        .and_then(|detail| detail["required_version"].as_str());

    assert_eq!(
        runtime_check["status"].as_str(),
        Some(if node_available { "pass" } else { "warn" })
    );
    assert_eq!(kinds, vec!["node", "python", "bash"]);
    assert_eq!(node_required_version, Some(">=20"));
    assert!(runtime_check["message"]
        .as_str()
        .expect("runtime message")
        .contains("runtimes available"));

    fs::remove_dir_all(root).ok();
}

fn set_test_asset_mount(
    service: &AppService,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: DeploymentStrategy,
) {
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            crate::backend::store::set_asset_mount_sqlx(
                &pool, asset_id, profile_id, enabled, strategy,
            )
            .await
        })
        .expect("persist mount preference");
}

fn count_asset_rows(service: &AppService, table: &str, asset_id: &str) -> i64 {
    let pool = service.db.pool().clone();
    let statement = format!("SELECT COUNT(*) FROM {table} WHERE asset_id = ?");
    let asset_id = asset_id.to_string();
    service
        .db
        .block_on(async move {
            sqlx::query_scalar::<_, i64>(AssertSqlSafe(statement))
                .bind(asset_id)
                .fetch_one(&pool)
                .await
                .map_err(|error| error.to_string())
        })
        .expect("count asset rows")
}

#[cfg(unix)]
fn write_executable_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    fs::write(&path, body).expect("write executable script");
    let mut permissions = fs::metadata(&path)
        .expect("read script metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("set script permissions");
    path
}

#[cfg(unix)]
fn adapter_manifest_entry(root: &Path, script: &Path) -> String {
    script
        .strip_prefix(root)
        .unwrap_or(script)
        .to_string_lossy()
        .to_string()
}

#[cfg(unix)]
fn upsert_conversation_export_fixture(
    service: &AppService,
    root: &Path,
    adapter_capabilities: Vec<String>,
    adapter_script: Option<&Path>,
    web_record: bool,
) -> String {
    let adapter_id = format!("fixture-export-{}", Uuid::new_v4());
    let source_id = format!("{adapter_id}-source");
    let manifest_path = root.join(format!("{adapter_id}.json"));
    if let Some(script) = adapter_script {
        fs::write(
            &manifest_path,
            serde_json::json!({
                "schema_version": 1,
                "id": &adapter_id,
                "name": "Fixture export adapter",
                "version": "0.1.0",
                "protocol_version": 1,
                "command": [adapter_manifest_entry(root, script)],
                "capabilities": &adapter_capabilities,
                "input_kinds": ["directory"]
            })
            .to_string(),
        )
        .expect("write export adapter manifest");
    } else {
        fs::write(&manifest_path, "{}").expect("write placeholder manifest");
    }
    let now = "2026-01-01T00:00:00Z".to_string();
    let adapter = ConversationAdapter {
        id: adapter_id.clone(),
        name: "Fixture export adapter".to_string(),
        kind: ConversationAdapterKind::External,
        version: "0.1.0".to_string(),
        enabled: true,
        manifest_path: Some(manifest_path.to_string_lossy().to_string()),
        executable_path: adapter_script.map(|path| path.to_string_lossy().to_string()),
        content_hash: None,
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: adapter_capabilities,
        input_kinds: vec![ConversationSourceKind::Directory],
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let source = ConversationSource {
        id: source_id,
        adapter_id,
        name: "Fixture export source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: root.to_string_lossy().to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: now.clone(),
        updated_at: now,
    };
    let session = NormalizedConversationSession {
        external_id: "export-session".to_string(),
        title: Some("Export Fixture".to_string()),
        project_path: Some(root.join("project").to_string_lossy().to_string()),
        started_at: None,
        updated_at: None,
        source_locator: None,
        source_fingerprint: None,
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-1".to_string(),
            turn_index: 0,
            user_text: "Export this".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("Rust fallback should not appear".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                metadata_json: Some(
                    r#"{"content_card":{"type":"answer","format":"markdown"}}"#.to_string(),
                ),
            }],
        }],
    };
    let pool = service.db.pool().clone();
    let session_id = service
        .db
        .block_on(async move {
            crate::backend::store::upsert_conversation_adapter_sqlx(&pool, &adapter).await?;
            crate::backend::store::upsert_conversation_source_sqlx(&pool, &source).await?;
            let sessions = if web_record {
                crate::backend::store::import_web_record_sessions_sqlx(
                    &pool,
                    &source,
                    &[session],
                    false,
                )
                .await?;
                crate::backend::store::list_web_record_sessions_sqlx(
                    &pool,
                    Some(&source.adapter_id),
                    Some(&source.id),
                    None,
                    1,
                    0,
                )
                .await?
            } else {
                crate::backend::store::import_conversation_sessions_sqlx(
                    &pool,
                    &source,
                    &[session],
                    false,
                )
                .await?;
                crate::backend::store::list_conversation_sessions_sqlx(
                    &pool,
                    Some(&source.adapter_id),
                    Some(&source.id),
                    None,
                    1,
                    0,
                )
                .await?
            };
            AppResult::Ok(sessions[0].session.id.clone())
        })
        .expect("upsert conversation export fixture");
    session_id
}

#[cfg(unix)]
fn load_export_fixture_adapter(service: &AppService, session_id: &str) -> ConversationAdapter {
    let pool = service.db.pool().clone();
    let session_id = session_id.to_string();
    service
        .db
        .block_on(async move {
            let detail =
                crate::backend::store::load_conversation_session_detail_sqlx(&pool, &session_id)
                    .await?;
            crate::backend::store::load_conversation_adapter_sqlx(&pool, &detail.session.adapter_id)
                .await?
                .ok_or_else(|| "fixture adapter not found".to_string())
        })
        .expect("load export fixture adapter")
}

#[cfg(unix)]
#[test]
fn conversation_session_export_uses_adapter_markdown_formatter() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-plugin-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"adapter markdown export","relative_path":"plugin/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let output_root = root.join("exports");

    let result = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: output_root.to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            dry_run: false,
        })
        .expect("export through adapter");

    let path = PathBuf::from(result["path"].as_str().expect("export path"));
    assert_eq!(path, output_root.join("plugin/export.md"));
    assert_eq!(
        fs::read_to_string(path).expect("read exported file"),
        "adapter markdown export"
    );
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_dry_run_calls_adapter_without_writing_file() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-dry-run-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf 'ran' > "$0.ran"
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"dry run markdown","relative_path":"plugin/dry-run.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let ran_marker = script.with_file_name("adapter.sh.ran");
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let output_root = root.join("exports");

    let result = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: output_root.to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            dry_run: true,
        })
        .expect("dry-run export through adapter");

    let path = PathBuf::from(result["path"].as_str().expect("export path"));
    assert_eq!(path, output_root.join("plugin/dry-run.md"));
    assert_eq!(result["bytes"], "dry run markdown".len());
    assert!(ran_marker.exists());
    assert!(!path.exists());
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn web_record_export_uses_adapter_markdown_formatter() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-web-record-export-plugin-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"web adapter markdown export","relative_path":"web/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        true,
    );
    let output_root = root.join("exports");

    let result = service
        .export_web_record_session(ConversationSessionExportParams {
            session_id,
            output_root: output_root.to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            dry_run: false,
        })
        .expect("export web record through adapter");

    let path = PathBuf::from(result["path"].as_str().expect("export path"));
    assert_eq!(path, output_root.join("web/export.md"));
    assert_eq!(
        fs::read_to_string(path).expect("read exported file"),
        "web adapter markdown export"
    );
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_requires_adapter_markdown_capability() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-no-cap-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["read_session".to_string()],
        None,
        false,
    );

    let error = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: root.join("exports").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            dry_run: true,
        })
        .expect_err("missing export_markdown should fail");

    assert!(error.contains("export_markdown"));
    assert!(error.contains("fixture-export"));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_rejects_unsafe_adapter_relative_path() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-unsafe-path-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"unsafe","relative_path":"../escape.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );

    let error = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: root.join("exports").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            dry_run: true,
        })
        .expect_err("unsafe adapter relative path should fail");

    assert!(error.contains("relative_path"));
    assert!(!root.join("escape.md").exists());
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_requires_manifest_markdown_capability() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-manifest-no-cap-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"adapter markdown export","relative_path":"plugin/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let adapter = load_export_fixture_adapter(&service, &session_id);
    let manifest_path = adapter.manifest_path.expect("manifest path");
    fs::write(
        &manifest_path,
        serde_json::json!({
            "schema_version": 1,
            "id": adapter.id,
            "name": "Fixture export adapter",
            "version": "0.1.0",
            "protocol_version": 1,
            "command": [adapter_manifest_entry(&root, &script)],
            "capabilities": ["read_session"],
            "input_kinds": ["directory"]
        })
        .to_string(),
    )
    .expect("rewrite manifest without export capability");

    let error = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: root.join("exports").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            dry_run: true,
        })
        .expect_err("manifest missing export_markdown should fail");

    assert!(error.contains("export_markdown"));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_rejects_trusted_hash_mismatch() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-hash-mismatch-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"adapter markdown export","relative_path":"plugin/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let adapter = load_export_fixture_adapter(&service, &session_id);
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            sqlx::query("UPDATE conversation_adapters SET trusted_hash = ? WHERE id = ?")
                .bind("definitely-not-the-current-hash")
                .bind(&adapter.id)
                .execute(&pool)
                .await
                .map_err(|error| error.to_string())?;
            AppResult::Ok(())
        })
        .expect("force hash mismatch");

    let error = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: root.join("exports").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            dry_run: true,
        })
        .expect_err("trusted hash mismatch should fail");

    assert!(error.contains("trusted hash mismatch"));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_rejects_manifest_tampering_after_trust() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-manifest-tamper-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"adapter markdown export","relative_path":"plugin/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let adapter = load_export_fixture_adapter(&service, &session_id);
    let manifest_path = adapter.manifest_path.clone().expect("manifest path");
    let validation = crate::backend::conversations::validate_external_adapter(
        crate::backend::conversations::ExternalAdapterValidateParams {
            manifest_path: manifest_path.clone(),
        },
    )
    .expect("validate adapter");
    let pool = service.db.pool().clone();
    let trusted_hash = validation.content_hash.clone();
    let adapter_id = adapter.id.clone();
    service
        .db
        .block_on(async move {
            sqlx::query(
                "UPDATE conversation_adapters SET content_hash = ?, trusted_hash = ? WHERE id = ?",
            )
            .bind(&trusted_hash)
            .bind(&trusted_hash)
            .bind(&adapter_id)
            .execute(&pool)
            .await
            .map_err(|error| error.to_string())?;
            AppResult::Ok(())
        })
        .expect("store trusted hash");

    fs::write(
        &manifest_path,
        serde_json::json!({
            "schema_version": 1,
            "id": adapter.id,
            "name": "Fixture export adapter",
            "version": "0.1.0",
            "protocol_version": 1,
            "command": [adapter_manifest_entry(&root, &script), "--changed"],
            "capabilities": ["export_markdown"],
            "input_kinds": ["directory"]
        })
        .to_string(),
    )
    .expect("rewrite manifest with changed args");

    let error = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: root.join("exports").to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            dry_run: true,
        })
        .expect_err("manifest tampering should fail trusted hash check");

    assert!(error.contains("trusted hash mismatch"));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[cfg(unix)]
#[test]
fn conversation_session_export_rejects_symlink_escape_under_output_root() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-export-symlink-escape-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let script = write_executable_script(
        &root,
        "adapter.sh",
        r#"#!/bin/sh
cat >/dev/null
printf '%s\n' '{"type":"item","item":{"kind":"markdown_export","content":"escape","relative_path":"link/export.md"}}'
printf '%s\n' '{"type":"complete","item":{"export_count":1}}'
"#,
    );
    let session_id = upsert_conversation_export_fixture(
        &service,
        &root,
        vec!["export_markdown".to_string()],
        Some(&script),
        false,
    );
    let output_root = root.join("exports");
    let outside_root = root.join("outside");
    fs::create_dir_all(&output_root).expect("create output root");
    fs::create_dir_all(&outside_root).expect("create outside root");
    symlink(&outside_root, output_root.join("link")).expect("create export symlink");

    let error = service
        .export_conversation_session(ConversationSessionExportParams {
            session_id,
            output_root: output_root.to_string_lossy().to_string(),
            question_ids: Vec::new(),
            content_filter: crate::backend::dto::ConversationExportContentFilter::default(),
            dry_run: false,
        })
        .expect_err("symlink escape under output root should fail");

    assert!(error.contains("symlink") || error.contains("output_root"));
    assert!(!outside_root.join("export.md").exists());
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn navigation_model_updates_through_sqlx_path() {
    let root = std::env::temp_dir().join(format!("assetiweave-sqlx-navigation-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let mut model = service.navigation_model().expect("load navigation model");
    model.active_sub_nav_id = "sqlx-updated-sub-nav".to_string();
    model.rail_items[0].label = "SQLx Rail".to_string();

    let updated = service
        .update_navigation_model(model)
        .expect("update navigation model");

    assert_eq!(updated.active_sub_nav_id, "sqlx-updated-sub-nav");
    assert_eq!(updated.rail_items[0].label, "SQLx Rail");
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn app_shortcuts_update_through_sqlx_path() {
    let root = std::env::temp_dir().join(format!("assetiweave-sqlx-shortcuts-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let mut settings = service
        .list_app_shortcut_settings()
        .expect("load shortcut settings");
    settings[0].display_icon = "Q".to_string();
    settings[0].enabled = false;
    let disabled_profile_id = settings[0].profile_id.clone();

    let updated = service
        .update_app_shortcuts(settings)
        .expect("update shortcuts");
    let enabled = service
        .list_app_shortcuts()
        .expect("load enabled shortcuts");

    assert_eq!(updated[0].display_icon, "Q");
    assert!(!updated[0].enabled);
    assert!(enabled
        .iter()
        .all(|shortcut| shortcut.profile_id != disabled_profile_id));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn profile_delete_guard_blocks_sqlx_deployment_state() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-sqlx-profile-delete-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let profile = service
        .create_profile(TargetProfileInput {
            id: Some("team-app".to_string()),
            name: "Team App".to_string(),
            app_kind: None,
            target_paths: Some(vec![root.join("target").to_string_lossy().to_string()]),
            supported_kinds: None,
            deployment_strategy: None,
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("create profile");

    service
        .db
        .block_on(async {
            crate::backend::store::upsert_deployment_state_sqlx(
                service.db.pool(),
                &DeploymentState {
                    profile_id: profile.id.clone(),
                    asset_id: "asset-a".to_string(),
                    target_path: "/target/a".to_string(),
                    strategy: DeploymentStrategy::SymlinkToSource,
                    source_hash: "hash".to_string(),
                    deployed_at: "2026-06-18T00:00:00Z".to_string(),
                    managed_by: "assetiweave".to_string(),
                },
            )
            .await
        })
        .expect("insert deployment state");

    let error = service
        .delete_profile(profile.id)
        .expect_err("delete blocked by deployment state");

    assert!(error.contains("managed deployments"));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn scan_skill_sources_reads_sqlx_sources() {
    let root = std::env::temp_dir().join(format!("assetiweave-sqlx-scan-skill-{}", Uuid::new_v4()));
    let source_root = root.join("skills");
    let skill_dir = source_root.join("skill-a");
    fs::create_dir_all(&skill_dir).expect("create skill directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: skill-a\n---\n\n# Skill A\n",
    )
    .expect("write skill file");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(&service, &["assets", "sources"]);
    service
        .add_source(SourceInput {
            id: Some("sqlx-skill-source".to_string()),
            name: "SQLx Skill Source".to_string(),
            kind: SourceKind::Local,
            root_path: source_root.to_string_lossy().to_string(),
            scanner_kind: Some(SourceScannerKind::Skill),
            source_origin: Some(SourceOrigin::LocalFolder),
            repo_root: None,
            scan_root: None,
            origin_app_kind: None,
            include_globs: vec!["**/SKILL.md".to_string()],
            exclude_globs: Vec::new(),
            default_kind: Some(AssetKind::Skill),
            enabled: true,
            priority: 0,
        })
        .expect("add source through service");

    let assets = service
        .scan_skill_sources()
        .expect("scan skill sources through service");

    assert!(assets
        .iter()
        .any(|candidate| candidate.asset.name == "skill-a"));
    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn skill_group_crud_and_members_use_sqlx_path() {
    let root = std::env::temp_dir().join(format!("assetiweave-sqlx-groups-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(
        &service,
        &["asset_group_members", "asset_groups", "assets", "sources"],
    );

    let source = Source {
        id: "source-a".to_string(),
        name: "Source A".to_string(),
        kind: SourceKind::Local,
        root_path: root.join("source-a").to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    let now = Utc::now().to_rfc3339();
    let assets = vec![
        Asset {
            id: "skill-a".to_string(),
            source_id: source.id.clone(),
            name: "Frontend UI".to_string(),
            kind: AssetKind::Skill,
            format: AssetFormat::Directory,
            relative_path: "frontend/ui".to_string(),
            absolute_path: root
                .join("source-a/frontend/ui")
                .to_string_lossy()
                .to_string(),
            entry_file: Some("SKILL.md".to_string()),
            description: None,
            content_hash: Some("hash-a".to_string()),
            discovered_at: now.clone(),
            updated_at: now.clone(),
        },
        Asset {
            id: "skill-b".to_string(),
            source_id: source.id.clone(),
            name: "Backend API".to_string(),
            kind: AssetKind::Skill,
            format: AssetFormat::Directory,
            relative_path: "backend/api".to_string(),
            absolute_path: root
                .join("source-a/backend/api")
                .to_string_lossy()
                .to_string(),
            entry_file: Some("SKILL.md".to_string()),
            description: None,
            content_hash: Some("hash-b".to_string()),
            discovered_at: now.clone(),
            updated_at: now,
        },
    ];
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            crate::backend::store::upsert_source_sqlx(&pool, &source).await?;
            crate::backend::store::replace_source_assets_sqlx(&pool, "source-a", &assets).await
        })
        .expect("seed SQLx catalog");

    let created = service
        .create_skill_group(AssetGroupInput {
            id: Some("frontend".to_string()),
            name: "Frontend".to_string(),
            description: Some(" UI work ".to_string()),
            color: Some("#10b981".to_string()),
            display_icon: Some("F".to_string()),
            icon_svg: None,
            enabled: Some(true),
            sort_order: Some(1),
            rules: Some(AssetGroupRules {
                source_ids: vec!["source-a".to_string()],
                relative_path_globs: vec!["frontend/**".to_string()],
                name_contains: Some("ui".to_string()),
            }),
        })
        .expect("create SQLx group");
    assert_eq!(created.group.id, "frontend");
    assert_eq!(created.members.len(), 1);
    assert_eq!(created.members[0].asset_id, "skill-a");

    let with_manual = service
        .set_skill_group_manual_members(
            "frontend".to_string(),
            vec!["skill-b".to_string(), "skill-b".to_string()],
        )
        .expect("save SQLx manual members");
    assert_eq!(with_manual.manual_asset_ids, vec!["skill-b".to_string()]);
    assert_eq!(with_manual.members.len(), 2);

    let mut updated_group = with_manual.group.clone();
    updated_group.name = "Frontend Updated".to_string();
    let updated = service
        .update_skill_group(updated_group)
        .expect("update SQLx group");
    assert_eq!(updated.group.name, "Frontend Updated");
    assert_eq!(
        service
            .get_skill_group("frontend".to_string())
            .expect("get SQLx group")
            .group
            .name,
        "Frontend Updated"
    );
    assert_eq!(
        service.list_skill_groups().expect("list SQLx groups").len(),
        1
    );

    service
        .delete_skill_group("frontend".to_string())
        .expect("delete SQLx group");
    assert!(service
        .list_skill_groups()
        .expect("list after delete")
        .is_empty());

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn cleanup_orphan_asset_records_uses_sqlx_for_migrated_tables() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-sqlx-orphan-cleanup-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    execute_test_sql(
        &service,
        r#"
                INSERT INTO asset_mounts (
                    asset_id, profile_id, enabled, strategy, created_at, updated_at
                ) VALUES (
                    'orphan-asset', 'codex', 1, 'symlink_to_source',
                    '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
                );
                INSERT INTO deployment_state (
                    profile_id, asset_id, target_path, strategy,
                    source_hash, deployed_at, managed_by
                ) VALUES (
                    'codex', 'orphan-asset', '/tmp/orphan-asset', 'symlink_to_source',
                    'hash', '2026-01-01T00:00:00Z', 'assetiweave'
                );
                INSERT INTO skill_remote_sources (
                    asset_id, provider, source_url, repo_url, branch,
                    acquired_at, status
                ) VALUES (
                    'orphan-asset', 'github',
                    'https://github.com/example/repo/tree/main/skill',
                    'https://github.com/example/repo.git',
                    'main', '2026-01-01T00:00:00Z', 'unknown'
                );
                INSERT INTO asset_groups (
                    id, name, color, asset_kind, enabled, sort_order,
                    rules_payload, created_at, updated_at
                ) VALUES (
                    'orphan-group', 'Orphan Group', '#10b981', 'skill', 1, 0,
                    '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
                );
                INSERT INTO asset_group_members (group_id, asset_id, created_at)
                VALUES ('orphan-group', 'orphan-asset', '2026-01-01T00:00:00Z');
                "#,
    )
    .expect("seed orphan records");

    capabilities::cleanup_orphan_asset_records(&service.db).expect("cleanup orphan records");

    for table in [
        "asset_mounts",
        "deployment_state",
        "skill_remote_sources",
        "asset_group_members",
    ] {
        let count = count_asset_rows(&service, table, "orphan-asset");
        assert_eq!(count, 0, "orphan row remained in {table}");
    }

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn list_skill_remote_sources_prunes_orphans_through_sqlx_path() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-sqlx-skill-remote-cleanup-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    let orphan = SkillRemoteSource {
        asset_id: "missing-asset".to_string(),
        provider: "github".to_string(),
        source_url: "https://github.com/example/repo/tree/main/skill".to_string(),
        repo_url: "https://github.com/example/repo.git".to_string(),
        branch: "main".to_string(),
        path: Some("skill".to_string()),
        acquired_at: "2026-01-01T00:00:00Z".to_string(),
        acquired_tree_sha: None,
        local_content_hash: None,
        last_checked_at: None,
        latest_tree_sha: None,
        status: "unknown".to_string(),
        message: None,
    };
    let pool = service.db.pool().clone();
    service
        .db
        .block_on(async move {
            crate::backend::store::upsert_skill_remote_source_sqlx(&pool, &orphan).await
        })
        .expect("save orphan remote source");

    assert!(service
        .list_skill_remote_sources()
        .expect("list remote sources")
        .is_empty());

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn disabled_mount_preference_persists_through_sqlx_path() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-sqlx-disabled-mount-preference-{}",
        Uuid::new_v4()
    ));
    let source_root = root.join("source");
    let target_root = root.join("target");
    let skill_root = source_root.join("skill-a");
    fs::create_dir_all(&skill_root).expect("create skill source");
    fs::create_dir_all(&target_root).expect("create target root");
    fs::write(
        skill_root.join("SKILL.md"),
        "---\ndescription: Skill A\n---\n",
    )
    .expect("write skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(
        &service,
        &["asset_mounts", "deployment_state", "assets", "sources"],
    );
    let source = Source {
        id: "source-a".to_string(),
        name: "Source A".to_string(),
        kind: SourceKind::Local,
        root_path: source_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);

    let now = Utc::now().to_rfc3339();
    let asset = Asset {
        id: "asset-a".to_string(),
        source_id: source.id.clone(),
        name: "skill-a".to_string(),
        kind: AssetKind::Skill,
        format: AssetFormat::Directory,
        relative_path: "skill-a".to_string(),
        absolute_path: skill_root.to_string_lossy().to_string(),
        entry_file: Some(skill_root.join("SKILL.md").to_string_lossy().to_string()),
        description: None,
        content_hash: Some("hash-a".to_string()),
        discovered_at: now.clone(),
        updated_at: now,
    };
    replace_test_source_assets(&service, &source.id, &[asset.clone()]);

    let profile = service
        .create_profile(TargetProfileInput {
            id: Some("target-a".to_string()),
            name: "Target A".to_string(),
            app_kind: Some(crate::backend::models::AppKind::Custom),
            target_paths: Some(vec![target_root.to_string_lossy().to_string()]),
            supported_kinds: None,
            deployment_strategy: Some(DeploymentStrategy::SymlinkToSource),
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("create target profile");

    let mount = service
        .set_asset_mount(
            &asset.id,
            &profile.id,
            false,
            Some(DeploymentStrategy::CopyToTarget),
        )
        .expect("persist disabled preference");
    assert!(!mount.enabled);
    assert_eq!(mount.strategy, DeploymentStrategy::CopyToTarget);

    let saved_mounts = service
        .list_asset_mounts(Some(&asset.id))
        .expect("read SQLx mount preference");
    assert_eq!(saved_mounts, vec![mount]);

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn mount_skill_dry_run_reads_profile_through_sqlx_path() {
    let root =
        std::env::temp_dir().join(format!("assetiweave-sqlx-mount-dry-run-{}", Uuid::new_v4()));
    let source_root = root.join("source");
    let target_root = root.join("target");
    let skill_root = source_root.join("skill-a");
    fs::create_dir_all(&skill_root).expect("create skill source");
    fs::create_dir_all(&target_root).expect("create target root");
    fs::write(
        skill_root.join("SKILL.md"),
        "---\ndescription: Skill A\n---\n",
    )
    .expect("write skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(
        &service,
        &["asset_mounts", "deployment_state", "assets", "sources"],
    );
    let source = Source {
        id: "source-a".to_string(),
        name: "Source A".to_string(),
        kind: SourceKind::Local,
        root_path: source_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);

    let now = Utc::now().to_rfc3339();
    let asset = Asset {
        id: "asset-a".to_string(),
        source_id: source.id.clone(),
        name: "skill-a".to_string(),
        kind: AssetKind::Skill,
        format: AssetFormat::Directory,
        relative_path: "skill-a".to_string(),
        absolute_path: skill_root.to_string_lossy().to_string(),
        entry_file: Some(skill_root.join("SKILL.md").to_string_lossy().to_string()),
        description: None,
        content_hash: Some("hash-a".to_string()),
        discovered_at: now.clone(),
        updated_at: now,
    };
    replace_test_source_assets(&service, &source.id, &[asset.clone()]);

    let profile = service
        .create_profile(TargetProfileInput {
            id: Some("target-a".to_string()),
            name: "Target A".to_string(),
            app_kind: Some(crate::backend::models::AppKind::Custom),
            target_paths: Some(vec![target_root.to_string_lossy().to_string()]),
            supported_kinds: None,
            deployment_strategy: Some(DeploymentStrategy::SymlinkToSource),
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("create target profile");

    let preview = service
        .mount_skill(
            AssetRefParams {
                asset_ref: asset.id.clone(),
                profile_id: Some(profile.id.clone()),
                dry_run: true,
                yes: false,
                unmount: false,
            },
            true,
        )
        .expect("dry-run mount skill");

    assert_eq!(preview["dry_run"], json!(true));
    assert_eq!(preview["profile_id"], json!(profile.id));
    assert_eq!(preview["status"]["state"], json!("not_mounted"));
    assert!(!target_root.join("skill-a").exists());
    assert!(service
        .list_asset_mounts(Some(&asset.id))
        .expect("load mounts after dry-run")
        .is_empty());

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn batch_skill_backup_deduplicates_assets_and_reports_copy_progress() {
    let root =
        std::env::temp_dir().join(format!("assetiweave-skill-backup-batch-{}", Uuid::new_v4()));
    let source_root = root.join("source");
    let backup_root = root.join("backup");
    fs::create_dir_all(source_root.join("skill-a")).expect("create first skill");
    fs::create_dir_all(source_root.join("skill-b")).expect("create second skill");
    fs::write(
        source_root.join("skill-a").join("SKILL.md"),
        "---\ndescription: Skill A\n---\n",
    )
    .expect("write first skill");
    fs::write(
        source_root.join("skill-b").join("SKILL.md"),
        "---\ndescription: Skill B\n---\n",
    )
    .expect("write second skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(&service, &["assets", "sources"]);
    let source = Source {
        id: "source-a".to_string(),
        name: "Source A".to_string(),
        kind: SourceKind::Local,
        root_path: source_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);
    service
        .update_skill_backup_settings(UpdateSkillBackupSettingsParams {
            root_path: backup_root.to_string_lossy().to_string(),
            migrate: false,
        })
        .expect("configure backup root");

    let mut source_assets = load_test_assets(&service)
        .into_iter()
        .filter(|asset| asset.source_id == "source-a")
        .collect::<Vec<_>>();
    source_assets.sort_by(|left, right| left.name.cmp(&right.name));
    assert_eq!(source_assets.len(), 2);

    let first_id = source_assets[0].id.clone();
    let second_id = source_assets[1].id.clone();
    let mut progress = Vec::new();
    let backed_up = service
        .backup_skills_with_progress(
            vec![first_id.clone(), first_id, second_id.clone()],
            |completed, next_asset_id| {
                progress.push((completed, next_asset_id.map(str::to_string)));
            },
        )
        .expect("back up skills");

    assert_eq!(backed_up.len(), 2);
    assert_eq!(progress, vec![(1, Some(second_id)), (2, None)]);
    assert!(backup_root
        .join("backed-up")
        .join("source-a")
        .join("skill-a")
        .join("SKILL.md")
        .is_file());
    assert!(backup_root
        .join("backed-up")
        .join("source-a")
        .join("skill-b")
        .join("SKILL.md")
        .is_file());
    assert!(backed_up.iter().all(|asset| asset.backup_status.is_some()));

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn backed_up_duplicate_skill_is_hidden_from_plan_and_mount_statuses() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-skill-backup-duplicate-plan-{}",
        Uuid::new_v4()
    ));
    let source_root = root.join("source");
    let backup_root = root.join("backup");
    let target_root = root.join("target");
    fs::create_dir_all(source_root.join("skill-a")).expect("create skill");
    fs::create_dir_all(&target_root).expect("create target root");
    fs::write(
        source_root.join("skill-a").join("SKILL.md"),
        "---\ndescription: Skill A\n---\n",
    )
    .expect("write skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(&service, &["assets", "sources"]);
    let source = Source {
        id: "source-a".to_string(),
        name: "Source A".to_string(),
        kind: SourceKind::Local,
        root_path: source_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);
    let profile = service
        .create_profile(TargetProfileInput {
            id: Some("test-target".to_string()),
            name: "Test Target".to_string(),
            app_kind: Some(crate::backend::models::AppKind::Custom),
            target_paths: Some(vec![target_root.to_string_lossy().to_string()]),
            supported_kinds: None,
            deployment_strategy: Some(DeploymentStrategy::SymlinkToSource),
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("create target profile");
    service
        .update_skill_backup_settings(UpdateSkillBackupSettingsParams {
            root_path: backup_root.to_string_lossy().to_string(),
            migrate: false,
        })
        .expect("configure backup root");

    let source_asset = load_test_assets(&service)
        .into_iter()
        .find(|asset| asset.source_id == "source-a")
        .expect("source asset");
    service
        .backup_skill(source_asset.id.clone())
        .expect("backup skill");

    let raw_skill_assets = load_test_assets(&service)
        .into_iter()
        .filter(|asset| asset.kind == AssetKind::Skill)
        .collect::<Vec<_>>();
    assert_eq!(raw_skill_assets.len(), 2);
    for asset in &raw_skill_assets {
        set_test_asset_mount(
            &service,
            &asset.id,
            &profile.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        );
    }

    let catalog = service.list_skills().expect("list catalog");
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].asset.source_id, "source-a");
    assert_eq!(
        catalog[0]
            .backup_status
            .as_ref()
            .map(|status| status.hidden_asset_ids.len()),
        Some(1)
    );

    let plan = service
        .create_plan(Some(&profile.id))
        .expect("create deployment plan");
    assert_eq!(plan.actions.len(), 1);
    assert_eq!(
        plan.actions[0].asset_id.as_deref(),
        Some(source_asset.id.as_str())
    );
    assert_eq!(plan.summary.create_count, 1);
    assert_eq!(plan.summary.conflict_count, 0);

    let target_statuses = service
        .list_asset_mount_statuses(None)
        .expect("list mount statuses")
        .into_iter()
        .filter(|status| status.profile_id == profile.id)
        .collect::<Vec<_>>();
    assert_eq!(target_statuses.len(), 1);
    assert_eq!(target_statuses[0].asset_id, source_asset.id);

    drop(service);
    fs::remove_dir_all(root).ok();
}

#[test]
fn app_target_backup_copy_does_not_report_identical_target_as_conflict() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-app-target-backup-status-{}",
        Uuid::new_v4()
    ));
    let app_target_root = root.join("codex-skills");
    let backup_root = root.join("backup");
    let skill_path = app_target_root.join("browser-testing-with-devtools");
    fs::create_dir_all(&skill_path).expect("create app target skill");
    fs::write(
        skill_path.join("SKILL.md"),
        "---\ndescription: Browser testing\n---\n",
    )
    .expect("write skill");

    let service =
        AppService::open_with_db_path(root.join("app.db")).expect("open application service");
    clear_test_tables(&service, &["assets", "sources"]);
    let source = Source {
        id: "codex-source".to_string(),
        name: "Codex Source".to_string(),
        kind: SourceKind::Local,
        root_path: app_target_root.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::AppTarget,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: Some(crate::backend::models::AppKind::Codex),
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: Vec::new(),
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    upsert_test_source(&service, &source);
    let profile = service
        .create_profile(TargetProfileInput {
            id: Some("codex-test".to_string()),
            name: "Codex Test".to_string(),
            app_kind: Some(crate::backend::models::AppKind::Codex),
            target_paths: Some(vec![app_target_root.to_string_lossy().to_string()]),
            supported_kinds: None,
            deployment_strategy: Some(DeploymentStrategy::SymlinkToSource),
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("create codex target profile");
    service
        .update_skill_backup_settings(UpdateSkillBackupSettingsParams {
            root_path: backup_root.to_string_lossy().to_string(),
            migrate: false,
        })
        .expect("configure backup root");

    let app_asset = load_test_assets(&service)
        .into_iter()
        .find(|asset| asset.source_id == "codex-source")
        .expect("app target asset");
    service
        .backup_skill(app_asset.id)
        .expect("backup app target skill");

    let catalog = service.list_skills().expect("list catalog");
    assert_eq!(catalog.len(), 1);
    assert_eq!(
        catalog[0].asset.source_id,
        capabilities::SKILL_BACKUP_SOURCE_ID
    );

    let statuses = service
        .list_asset_mount_statuses(None)
        .expect("list mount statuses");
    let status = statuses
        .iter()
        .find(|status| status.profile_id == profile.id)
        .expect("status for profile");
    assert_eq!(status.asset_id, catalog[0].asset.id);
    assert_eq!(status.state, PhysicalMountStateDto::NotMounted);

    let plan = service
        .create_plan(Some(&profile.id))
        .expect("create deployment plan");
    assert_eq!(plan.summary.conflict_count, 0);

    let mounted = service
        .mount_asset_by_id(&catalog[0].asset.id, &profile.id)
        .expect("mount backup copy over identical app target");
    assert_eq!(mounted.status.state, PhysicalMountStateDto::Mounted);
    let target_metadata = fs::symlink_metadata(&skill_path).expect("target metadata");
    assert!(target_metadata.file_type().is_symlink());
    assert_eq!(
        fs::read_link(&skill_path)
            .expect("read target symlink")
            .canonicalize()
            .expect("canonical target symlink"),
        PathBuf::from(&catalog[0].asset.absolute_path)
            .canonicalize()
            .expect("canonical backup path")
    );

    drop(service);
    fs::remove_dir_all(root).ok();
}

fn github_repo_item() -> Value {
    json!({
        "full_name": "util6/util6-agents",
        "html_url": "https://github.com/util6/util6-agents",
        "clone_url": "https://github.com/util6/util6-agents.git",
        "default_branch": "main",
        "description": "Codex skills and agent workflows",
        "stargazers_count": 7
    })
}

fn github_code_item() -> Value {
    json!({
        "path": "skills/browser/SKILL.md",
        "repository": {
            "full_name": "util6/util6-agents",
            "html_url": "https://github.com/util6/util6-agents",
            "clone_url": "https://github.com/util6/util6-agents.git",
            "default_branch": "main",
            "description": "Codex skills and agent workflows",
            "stargazers_count": 7
        }
    })
}

#[test]
fn skill_search_provider_supports_github_code_aliases() {
    assert_eq!(
        normalize_skill_search_provider(None).as_deref(),
        Ok("github")
    );
    assert_eq!(
        normalize_skill_search_provider(Some("github_code")).as_deref(),
        Ok("github-code")
    );
    assert_eq!(
        normalize_skill_search_provider(Some("code")).as_deref(),
        Ok("github-code")
    );
    assert!(normalize_skill_search_provider(Some("unknown")).is_err());
}

#[test]
fn github_code_search_url_targets_skill_markdown_files() {
    let url = github_code_search_url("browser automation", 25);

    assert_eq!(
        url,
        "https://api.github.com/search/code?q=browser+automation+filename%3ASKILL.md&per_page=20"
    );
}

#[test]
fn github_tree_paths_extract_concrete_skill_dirs() {
    let value = json!({
        "tree": [
            { "path": "SKILL.md", "type": "blob" },
            { "path": "skills/browser/SKILL.md", "type": "blob" },
            { "path": "skills/browser/README.md", "type": "blob" },
            { "path": "skills/../escape/SKILL.md", "type": "blob" },
            { "path": "plugins/browser/SKILL.md", "type": "tree" }
        ]
    });

    let paths = github_skill_paths_from_tree_value(&value);

    assert_eq!(paths, vec!["".to_string(), "skills/browser".to_string()]);
}

#[test]
fn github_tree_sha_for_skill_path_reads_root_and_nested_tree() {
    let value = json!({
        "sha": "root-tree",
        "tree": [
            { "path": "skills/browser", "type": "tree", "sha": "browser-tree" },
            { "path": "skills/browser/SKILL.md", "type": "blob", "sha": "skill-file" }
        ]
    });

    assert_eq!(
        github_tree_sha_for_skill_path(&value, None).as_deref(),
        Ok("root-tree")
    );
    assert_eq!(
        github_tree_sha_for_skill_path(&value, Some("skills/browser")).as_deref(),
        Ok("browser-tree")
    );
    assert!(github_tree_sha_for_skill_path(&value, Some("missing")).is_err());
}

#[test]
fn github_skill_path_candidate_points_acquire_at_tree_url() {
    let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();

    let candidate = skill_search_candidate_from_github_skill_path(
        &repo_candidate,
        "util6/util6-agents",
        "main",
        "skills/browser",
    );

    assert_eq!(candidate.name, "util6/util6-agents/skills/browser");
    assert_eq!(candidate.path.as_deref(), Some("skills/browser"));
    assert_eq!(
        candidate.match_reason.as_deref(),
        Some("Resolved concrete Skill directory from skills/browser/SKILL.md")
    );
    assert_eq!(
        candidate.url,
        "https://github.com/util6/util6-agents/tree/main/skills/browser"
    );
    assert_eq!(
            candidate.acquire_command,
            "assetiweave-cli skill acquire --url https://github.com/util6/util6-agents/tree/main/skills/browser --yes"
        );
}

#[test]
fn github_code_candidate_points_acquire_at_skill_directory() {
    let candidate = skill_search_candidate_from_github_code(&github_code_item())
        .expect("github code item should become candidate");

    assert_eq!(candidate.name, "util6/util6-agents/skills/browser");
    assert_eq!(candidate.path.as_deref(), Some("skills/browser"));
    assert_eq!(
        candidate.match_reason.as_deref(),
        Some("GitHub code search matched skills/browser/SKILL.md")
    );
    assert_eq!(
        candidate.url,
        "https://github.com/util6/util6-agents/tree/main/skills/browser"
    );
    assert_eq!(
            candidate.acquire_command,
            "assetiweave-cli skill acquire --url https://github.com/util6/util6-agents/tree/main/skills/browser --yes"
        );
}

#[test]
fn root_skill_path_candidate_keeps_repo_name() {
    let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();

    let candidate = skill_search_candidate_from_github_skill_path(
        &repo_candidate,
        "util6/util6-agents",
        "main",
        "",
    );

    assert_eq!(candidate.name, "util6/util6-agents");
    assert_eq!(candidate.path, None);
    assert_eq!(
        candidate.match_reason.as_deref(),
        Some("Resolved concrete Skill directory from SKILL.md")
    );
    assert_eq!(
        candidate.url,
        "https://github.com/util6/util6-agents/tree/main"
    );
}

#[test]
fn repository_fallback_candidate_explains_missing_skill_path() {
    let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();

    let candidate = skill_search_repository_fallback_candidate(repo_candidate, "main");

    assert_eq!(
        candidate.match_reason.as_deref(),
        Some("Repository fallback: no concrete SKILL.md directory was resolved on branch main")
    );
    assert_eq!(
        candidate.acquire_command,
        "assetiweave-cli skill acquire --url https://github.com/util6/util6-agents --yes"
    );
}

#[test]
fn concrete_skill_candidate_scores_above_repo_fallback() {
    let repo_candidate = skill_search_candidate_from_github(&github_repo_item()).unwrap();
    let skill_candidate = skill_search_candidate_from_github_skill_path(
        &repo_candidate,
        "util6/util6-agents",
        "main",
        "skills/browser",
    );
    let terms = search_query_terms("browser skill");

    assert!(
        skill_candidate_score(&skill_candidate, &terms)
            > skill_candidate_score(&repo_candidate, &terms)
    );
}
