use super::*;
use crate::backend::domain::{
    ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
    ConversationSource, ConversationSourceKind, ConversationSyncMode,
};
use crate::backend::infrastructure::tasks::{
    ExternalRegistrationOutcome, StageStatus, TaskKind, TaskOutcome, TaskSpec,
};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn write_test_script(dir: &Path, name: &str, body: &str) -> PathBuf {
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

/// #195：测试夹具自己写下清单与脚本，就必须像显式注册一样同时给出
/// `content_hash` 与 `trusted_hash`——信任来自夹具的显式声明，而不是运行期
/// 对受管目录的磁盘观测。指纹复用生产校验器，保证与运行时校验结果一致。
fn trust_fixture_adapter(adapter: &mut ConversationAdapter, manifest_path: &Path) {
    let hash =
        crate::backend::infrastructure::conversations::external::validate_external_adapter_manifest(
            &manifest_path.to_string_lossy(),
        )
        .map(|validation| validation.content_hash)
        .expect("validate fixture adapter manifest");
    adapter.content_hash = Some(hash.clone());
    adapter.trusted_hash = Some(hash);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn conversation_sync_records_failures_once_across_sync_and_persist_stages() {
    let root =
        std::env::temp_dir().join(format!("assetiweave-sync-failure-dedup-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .unwrap();

    let script_content = r#"#!/bin/sh
request=$(cat)
case "$request" in
  *list_sessions*)
    printf '%s\n' '{"type":"item","item":{"kind":"session_descriptor","external_id":"s-bad","version_token":"v1"}}'
    printf '%s\n' '{"type":"item","item":{"kind":"session_descriptor","external_id":"s-ok","version_token":"v1"}}'
    printf '%s\n' '{"type":"complete","item":{"session_count":2,"snapshot_complete":true}}'
    ;;
  *s-bad*)
    exit 1
    ;;
  *s-ok*)
    printf '%s\n' '{"type":"item","item":{"kind":"session","session":{"external_id":"s-ok","source_fingerprint":"v1","turns":[{"external_id":"turn-1","turn_index":0,"user_text":"Hello","parts":[]}]}}}'
    printf '%s\n' '{"type":"complete","item":{"session_count":1}}'
    ;;
  *)
    exit 1
    ;;
esac
"#;
    let script = write_test_script(&root, "adapter.sh", script_content);
    let script_entry = script
        .strip_prefix(&root)
        .unwrap_or(&script)
        .to_string_lossy()
        .to_string();

    let adapter_id = format!("sync-dedup-{}", Uuid::new_v4());
    let source_id = format!("{adapter_id}-source");
    let manifest_path = root.join(format!("{adapter_id}.json"));
    fs::write(
        &manifest_path,
        serde_json::json!({
            "schema_version": 1,
            "id": &adapter_id,
            "name": "Sync failure dedup adapter",
            "version": "0.1.0",
            "protocol_version": 1,
            "command": [script_entry],
            "capabilities": ["list_sessions", "read_session"],
            "input_kinds": ["directory"]
        })
        .to_string(),
    )
    .expect("write adapter manifest");

    let now = "2026-01-01T00:00:00Z".to_string();
    let mut adapter = ConversationAdapter {
        id: adapter_id.clone(),
        name: "Sync failure dedup adapter".to_string(),
        kind: ConversationAdapterKind::External,
        version: "0.1.0".to_string(),
        enabled: true,
        manifest_path: Some(manifest_path.to_string_lossy().to_string()),
        executable_path: Some(script.to_string_lossy().to_string()),
        content_hash: None,
        trusted_hash: None,
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(1),
        capabilities: vec!["list_sessions".to_string(), "read_session".to_string()],
        input_kinds: vec![ConversationSourceKind::Directory],
        card_contract_version: None,
        card_kinds: Vec::new(),
        projection_version: Some(1),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    trust_fixture_adapter(&mut adapter, &manifest_path);
    let source = ConversationSource {
        id: source_id.clone(),
        adapter_id: adapter_id.clone(),
        name: "Sync failure dedup source".to_string(),
        kind: ConversationSourceKind::Directory,
        location: root.to_string_lossy().to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: now.clone(),
        updated_at: now,
    };

    let pool = service.db.pool();
    let tenant_id = service.tenant_id();
    crate::backend::store::upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter)
        .await
        .unwrap();
    crate::backend::store::upsert_conversation_source_sqlx(pool, tenant_id, &source)
        .await
        .unwrap();

    let task_runtime = service.runtime.task_runtime();
    let spec = TaskSpec::new(TaskKind::ConversationSync, None).with_tenant_id(tenant_id);
    let task_id = match task_runtime.register_external(spec).unwrap() {
        ExternalRegistrationOutcome::Started(s)
        | ExternalRegistrationOutcome::Existing(s)
        | ExternalRegistrationOutcome::Conflict(s) => s.task_id,
    };
    let cancellation = task_runtime.cancellation_token(&task_id).unwrap();

    service
        .sync_conversations_with_control(
            ConversationSyncParams {
                source_id: Some(source_id),
                adapter_id: None,
                record_kind: Some("session".to_string()),
                mode: ConversationSyncMode::Full,
                dry_run: false,
            },
            Some(&cancellation),
            Some(&task_id),
            &mut |_, _, _, _| {},
        )
        .await
        .unwrap();

    let snapshot = task_runtime.get(&task_id).unwrap();
    assert_eq!(snapshot.outcome, Some(TaskOutcome::PartialSuccess));
    assert_eq!(
        snapshot.failures.len(),
        1,
        "snapshot.failures should not duplicate sync failures into persist stage: {:?}",
        snapshot.failures
    );
    assert_eq!(snapshot.failures[0].identity.as_deref(), Some("s-bad"));

    let sync_stage = snapshot
        .stages
        .iter()
        .find(|s| s.id.ends_with(":sync"))
        .expect("sync stage exists");
    assert_eq!(sync_stage.status, StageStatus::PartialSuccess);
    assert_eq!(sync_stage.failures.len(), 1);

    let persist_stage = snapshot
        .stages
        .iter()
        .find(|s| s.id.ends_with(":persist"))
        .expect("persist stage exists");
    assert_eq!(persist_stage.status, StageStatus::PartialSuccess);
    assert!(
        persist_stage.failures.is_empty(),
        "persist stage must not clone sync stage failures: {:?}",
        persist_stage.failures
    );

    drop(service);
    fs::remove_dir_all(root).ok();
}
