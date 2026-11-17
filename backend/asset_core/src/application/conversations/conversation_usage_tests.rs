use super::*;
use crate::backend::application::system::TaskListParams;
use crate::backend::infrastructure::tasks::stages;

fn temp_db_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "assetiweave-usage-pipeline-{label}-{}.sqlite",
        uuid::Uuid::new_v4()
    ))
}

fn list_visible(service: &AppService) -> Vec<crate::backend::application::system::TaskView> {
    service
        .list_public_tasks(TaskListParams {
            tenant_id: None,
            all_tenants: None,
            active_only: None,
        })
        .expect("list public tasks")
}

#[tokio::test]
async fn usage_scan_pipeline_is_managed_visible_and_projects_result() {
    let service = AppService::open_with_db_path(temp_db_path("managed"))
        .await
        .expect("open app service");

    let handle = service
        .start_conversation_usage_scan(UsageScanOptions {
            // 隔离外部适配器子进程：只匹配不存在的来源，流水线全阶段仍真实走完。
            source_id: Some("test-unmatched-source".to_string()),
            ..Default::default()
        })
        .expect("start usage scan pipeline");

    // 运行期：任务中心可见，元数据完整且可取消
    let visible = list_visible(&service);
    let running = visible
        .iter()
        .find(|task| task.id == handle.task_id())
        .expect("usage scan task must be visible in task center");
    assert_eq!(running.title, "用量统计扫描");
    assert_eq!(running.category, "conversation/usage_scan");
    assert!(
        running.capabilities.cancellable,
        "运行中的用量扫描任务必须声明可取消"
    );

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("usage scan snapshot");
    assert_eq!(
        snapshot.state,
        crate::backend::infrastructure::tasks::TaskState::Succeeded
    );

    // 终态：阶段骨架来自编译期常量
    let stage_ids: Vec<&str> = snapshot.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        stage_ids,
        vec![
            stages::PREPARE_SOURCES,
            stages::COLLECT_USAGE,
            stages::PERSIST_EVENTS,
            stages::RESOLVE_PRICING,
            stages::CALCULATE_COSTS,
        ]
    );

    // 终态：结果由 TaskRuntime 投影到公开 TaskView
    let finished = list_visible(&service);
    let view = finished
        .iter()
        .find(|task| task.id == handle.task_id())
        .expect("finished usage scan task must stay visible");
    assert_eq!(view.state, "succeeded");
    let result = view
        .result
        .as_ref()
        .expect("TaskView.result 必须包含扫描统计");
    assert_eq!(result["total_sources"].as_u64(), Some(0));
    assert_eq!(result["total_events_ingested"].as_u64(), Some(0));
    assert_eq!(result["mode"].as_str(), Some("incremental"));
}

#[cfg(unix)]
mod failure_fixture {
    use super::*;
    use crate::backend::domain::conversations::{
        ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
        ConversationSource, ConversationSourceKind,
    };
    use crate::backend::store::{
        upsert_conversation_adapter_sqlx, upsert_conversation_source_sqlx,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

    const ADAPTER_SCRIPT: &str = r#"#!/bin/sh
request=$(cat)
case "$request" in
  *mode-bad*)
    printf '%s\n' '{"type":"error","error":"adapter exploded"}'
    ;;
  *)
    case "$request" in
      *slow*) sleep 1 ;;
    esac
    printf '%s\n' '{"type":"item","item":{"kind":"usage_event","usage_event":{"external_event_id":"evt-1","session_id":null,"turn_id":null,"logical_request_id":null,"attempt_index":null,"timestamp":"2026-01-01T00:00:00Z","provider":"test-provider","model":"test-model","status":"ok","input_tokens":10,"cache_read_tokens":0,"cache_write_tokens":0,"reasoning_tokens":0,"output_tokens":5,"host_reported_cost":null,"currency":null}}}'
    printf '%s\n' '{"type":"complete","item":{"snapshot_complete":true,"next_cursor":"cursor-1"}}'
    ;;
esac
"#;

    pub(super) fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "assetiweave-usage-failures-{label}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    fn write_script(root: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = root.join("adapter.sh");
        fs::write(&path, ADAPTER_SCRIPT).expect("write adapter script");
        let mut permissions = fs::metadata(&path).expect("stat script").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("chmod script");
        path
    }

    /// 注册一个真实外部适配器与若干数据源（location 含 `mode-bad` 的来源会读取失败）。
    pub(super) async fn seed_usage_fixture(
        service: &AppService,
        root: &Path,
        slugs: &[&str],
    ) -> Vec<ConversationSource> {
        let script = write_script(root);
        let script_entry = script
            .strip_prefix(root)
            .unwrap_or(&script)
            .to_string_lossy()
            .to_string();
        let adapter_id = format!("usage-fixture-{}", uuid::Uuid::new_v4());
        let manifest_path = root.join(format!("{adapter_id}.json"));
        fs::write(
            &manifest_path,
            serde_json::json!({
                "schema_version": 1,
                "id": &adapter_id,
                "name": "Usage failure fixture adapter",
                "version": "0.1.0",
                "protocol_version": 1,
                "command": [script_entry],
                "capabilities": ["read_usage"],
                "input_kinds": ["directory"]
            })
            .to_string(),
        )
        .expect("write adapter manifest");

        let now = "2026-01-01T00:00:00Z".to_string();
        let adapter = ConversationAdapter {
            id: adapter_id.clone(),
            name: "Usage failure fixture adapter".to_string(),
            kind: ConversationAdapterKind::External,
            version: "0.1.0".to_string(),
            enabled: true,
            manifest_path: Some(manifest_path.to_string_lossy().to_string()),
            executable_path: Some(script.to_string_lossy().to_string()),
            content_hash: None,
            trusted_hash: None,
            trust_state: ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["read_usage".to_string()],
            input_kinds: vec![ConversationSourceKind::Directory],
            card_contract_version: None,
            card_kinds: Vec::new(),
            projection_version: Some(1),
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        let pool = service.db.pool();
        let tenant_id = service.tenant_id();
        upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter)
            .await
            .expect("upsert adapter");

        let mut sources = Vec::new();
        for slug in slugs {
            let location = root.join(slug);
            fs::create_dir_all(&location).expect("create source dir");
            let source = ConversationSource {
                id: format!("{adapter_id}-{slug}"),
                adapter_id: adapter_id.clone(),
                name: format!("source {slug}"),
                kind: ConversationSourceKind::Directory,
                location: location.to_string_lossy().to_string(),
                config_json: None,
                enabled: true,
                last_synced_at: None,
                last_sync_status: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            };
            upsert_conversation_source_sqlx(pool, tenant_id, &source)
                .await
                .expect("upsert source");
            sources.push(source);
        }
        sources
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn usage_scan_partial_failure_projects_partial_success_and_source_failure() {
    use crate::backend::infrastructure::tasks::{StageStatus, TaskOutcome, TaskState};
    use failure_fixture::{seed_usage_fixture, temp_root};

    let root = temp_root("partial");
    std::fs::create_dir_all(&root).unwrap();
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open app service");
    let sources = seed_usage_fixture(&service, &root, &["ok-a", "ok-b", "mode-bad-c"]).await;
    let adapter_id = sources[0].adapter_id.clone();
    let bad_source_id = sources
        .iter()
        .find(|source| source.name == "source mode-bad-c")
        .expect("bad source")
        .id
        .clone();

    let handle = service
        .start_conversation_usage_scan(UsageScanOptions {
            adapter_id: Some(adapter_id),
            ..Default::default()
        })
        .expect("start usage scan");
    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("usage scan snapshot");

    assert_eq!(snapshot.state, TaskState::Succeeded);
    assert_eq!(snapshot.outcome, Some(TaskOutcome::PartialSuccess));
    assert_eq!(
        snapshot.failures.len(),
        1,
        "失败来源必须写入任务级 failures: {:?}",
        snapshot.failures
    );
    let failure = &snapshot.failures[0];
    assert_eq!(failure.identity.as_deref(), Some(bad_source_id.as_str()));
    assert_eq!(failure.stage, stages::COLLECT_USAGE);
    assert!(
        failure.message.contains("source mode-bad-c"),
        "failure message 必须携带来源名: {}",
        failure.message
    );
    assert!(
        failure.message.contains("adapter exploded"),
        "failure message 必须携带适配器错误文本: {}",
        failure.message
    );

    let collect_stage = snapshot
        .stages
        .iter()
        .find(|stage| stage.id == stages::COLLECT_USAGE)
        .expect("collect stage");
    assert_eq!(collect_stage.status, StageStatus::PartialSuccess);
    assert_eq!(collect_stage.failures.len(), 1);

    let view = list_visible(&service)
        .into_iter()
        .find(|task| task.id == handle.task_id())
        .expect("finished task visible");
    assert_eq!(view.outcome.as_deref(), Some("partial_success"));
    let result = view.result.as_ref().expect("result projected");
    assert_eq!(result["total_sources"].as_u64(), Some(3));
    assert_eq!(result["successful_sources"].as_u64(), Some(2));
    assert_eq!(result["failed_sources"].as_u64(), Some(1));
    assert_eq!(result["total_events_ingested"].as_u64(), Some(2));

    drop(service);
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn usage_scan_all_sources_failing_ends_as_failure() {
    use crate::backend::infrastructure::tasks::{StageStatus, TaskOutcome, TaskState};
    use failure_fixture::{seed_usage_fixture, temp_root};

    let root = temp_root("all-fail");
    std::fs::create_dir_all(&root).unwrap();
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open app service");
    let sources = seed_usage_fixture(&service, &root, &["mode-bad-a", "mode-bad-b"]).await;
    let adapter_id = sources[0].adapter_id.clone();

    let handle = service
        .start_conversation_usage_scan(UsageScanOptions {
            adapter_id: Some(adapter_id),
            ..Default::default()
        })
        .expect("start usage scan");
    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("usage scan snapshot");

    assert_eq!(snapshot.state, TaskState::Failed);
    assert_eq!(snapshot.outcome, Some(TaskOutcome::Failure));
    assert_eq!(snapshot.failures.len(), 2);
    let collect_stage = snapshot
        .stages
        .iter()
        .find(|stage| stage.id == stages::COLLECT_USAGE)
        .expect("collect stage");
    assert_eq!(collect_stage.status, StageStatus::Failed);
    let persist_stage = snapshot
        .stages
        .iter()
        .find(|stage| stage.id == stages::PERSIST_EVENTS)
        .expect("persist stage");
    assert_eq!(persist_stage.status, StageStatus::Skipped);

    let view = list_visible(&service)
        .into_iter()
        .find(|task| task.id == handle.task_id())
        .expect("finished task visible");
    assert_eq!(view.state, "failed");
    assert_eq!(view.outcome.as_deref(), Some("failure"));

    drop(service);
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn usage_scan_status_exposes_active_task_while_running() {
    use crate::backend::infrastructure::tasks::{TaskOutcome, TaskState};
    use failure_fixture::{seed_usage_fixture, temp_root};

    let root = temp_root("active");
    std::fs::create_dir_all(&root).unwrap();
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open app service");
    let sources = seed_usage_fixture(&service, &root, &["slow-ok"]).await;
    let adapter_id = sources[0].adapter_id.clone();

    let handle = service
        .start_conversation_usage_scan(UsageScanOptions {
            adapter_id: Some(adapter_id),
            ..Default::default()
        })
        .expect("start usage scan");

    let mut active_task_id = None;
    for _ in 0..100 {
        let status = service
            .get_conversation_usage_scan_status()
            .await
            .expect("scan status");
        if status.active_scan_task_id.is_some() {
            active_task_id = status.active_scan_task_id;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        active_task_id.as_deref(),
        Some(handle.task_id()),
        "扫描运行期间 active_scan_task_id 必须是真实任务 id"
    );

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("usage scan snapshot");
    assert_eq!(snapshot.state, TaskState::Succeeded);
    assert_eq!(snapshot.outcome, Some(TaskOutcome::Success));

    let view = list_visible(&service)
        .into_iter()
        .find(|task| task.id == handle.task_id())
        .expect("finished task visible");
    let result = view.result.as_ref().expect("result projected");
    assert_eq!(result["successful_sources"].as_u64(), Some(1));
    assert_eq!(result["total_events_ingested"].as_u64(), Some(1));

    let status = service
        .get_conversation_usage_scan_status()
        .await
        .expect("scan status");
    assert_eq!(
        status.active_scan_task_id, None,
        "扫描结束后 active_scan_task_id 必须清空"
    );

    drop(service);
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
mod full_mode_fixture {
    use super::*;
    use crate::backend::domain::conversations::{
        ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
        ConversationSource, ConversationSourceKind,
    };
    use crate::backend::store::{
        upsert_conversation_adapter_sqlx, upsert_conversation_source_sqlx,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

    pub(super) struct PayloadFixture {
        pub source: ConversationSource,
        pub payload_path: PathBuf,
        pub cursor_path: PathBuf,
    }

    pub(super) fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "assetiweave-usage-full-mode-{label}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    /// 适配器读取 `payload.jsonl` 并回传 `cursor.txt` 中的游标，
    /// 便于测试在两次扫描之间改写源侧数据与游标。
    pub(super) async fn seed_payload_fixture(service: &AppService, root: &Path) -> PayloadFixture {
        use std::os::unix::fs::PermissionsExt;

        let payload_path = root.join("payload.jsonl");
        let cursor_path = root.join("cursor.txt");
        fs::write(&cursor_path, "cursor-1").expect("seed cursor");
        fs::write(&payload_path, "").expect("seed payload");

        let script = root.join("payload-adapter.sh");
        fs::write(
            &script,
            format!(
                r#"#!/bin/sh
cat >/dev/null
if [ -f "{payload}" ]; then cat "{payload}"; fi
cursor="cursor-1"
if [ -f "{cursor}" ]; then cursor=$(cat "{cursor}"); fi
printf '{{"type":"complete","item":{{"snapshot_complete":true,"next_cursor":"%s"}}}}\n' "$cursor"
"#,
                payload = payload_path.display(),
                cursor = cursor_path.display()
            ),
        )
        .expect("write adapter script");
        let mut permissions = fs::metadata(&script).expect("stat script").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).expect("chmod script");

        let script_entry = script
            .strip_prefix(root)
            .unwrap_or(&script)
            .to_string_lossy()
            .to_string();
        let adapter_id = format!("usage-payload-fixture-{}", uuid::Uuid::new_v4());
        let manifest_path = root.join(format!("{adapter_id}.json"));
        fs::write(
            &manifest_path,
            serde_json::json!({
                "schema_version": 1,
                "id": &adapter_id,
                "name": "Usage payload fixture adapter",
                "version": "0.1.0",
                "protocol_version": 1,
                "command": [script_entry],
                "capabilities": ["read_usage"],
                "input_kinds": ["directory"]
            })
            .to_string(),
        )
        .expect("write adapter manifest");

        let now = "2026-01-01T00:00:00Z".to_string();
        let adapter = ConversationAdapter {
            id: adapter_id.clone(),
            name: "Usage payload fixture adapter".to_string(),
            kind: ConversationAdapterKind::External,
            version: "0.1.0".to_string(),
            enabled: true,
            manifest_path: Some(manifest_path.to_string_lossy().to_string()),
            executable_path: Some(script.to_string_lossy().to_string()),
            content_hash: None,
            trusted_hash: None,
            trust_state: ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["read_usage".to_string()],
            input_kinds: vec![ConversationSourceKind::Directory],
            card_contract_version: None,
            card_kinds: Vec::new(),
            projection_version: Some(1),
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        let location = root.join("instance-a");
        fs::create_dir_all(&location).expect("create source dir");
        let source = ConversationSource {
            id: format!("{adapter_id}-instance-a"),
            adapter_id: adapter_id.clone(),
            name: "source full-mode".to_string(),
            kind: ConversationSourceKind::Directory,
            location: location.to_string_lossy().to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        let pool = service.db.pool();
        let tenant_id = service.tenant_id();
        upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter)
            .await
            .expect("upsert adapter");
        upsert_conversation_source_sqlx(pool, tenant_id, &source)
            .await
            .expect("upsert source");

        PayloadFixture {
            source,
            payload_path,
            cursor_path,
        }
    }

    pub(super) fn usage_event_line(external_event_id: &str) -> String {
        format!(
            r#"{{"type":"item","item":{{"kind":"usage_event","usage_event":{{"external_event_id":"{external_event_id}","session_id":null,"turn_id":null,"logical_request_id":null,"attempt_index":null,"timestamp":"2026-01-01T00:00:00Z","provider":"test-provider","model":"test-model","status":"ok","input_tokens":10,"cache_read_tokens":0,"cache_write_tokens":0,"reasoning_tokens":0,"output_tokens":5,"host_reported_cost":null,"currency":null}}}}}}"#
        )
    }
}

#[cfg(unix)]
async fn source_event_ids(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT external_event_id FROM conversation_usage_events \
         WHERE tenant_id = ? AND source_id = ? ORDER BY external_event_id",
    )
    .bind(tenant_id)
    .bind(source_id)
    .fetch_all(pool)
    .await
    .expect("load usage events")
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn usage_scan_full_mode_rereads_sources_and_prunes_stale_events() {
    use crate::backend::infrastructure::tasks::TaskState;
    use crate::backend::store::load_usage_source_state_sqlx;
    use full_mode_fixture::{seed_payload_fixture, temp_root, usage_event_line};

    let root = temp_root("reread");
    std::fs::create_dir_all(&root).unwrap();
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open app service");
    let fixture = seed_payload_fixture(&service, &root).await;
    std::fs::write(
        &fixture.payload_path,
        format!("{}\n", usage_event_line("evt-1")),
    )
    .unwrap();

    // 第一次增量扫描：写入 evt-1 与游标 cursor-1
    let incremental = service
        .start_conversation_usage_scan(UsageScanOptions {
            source_id: Some(fixture.source.id.clone()),
            ..Default::default()
        })
        .expect("start incremental scan");
    let snapshot = incremental
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("incremental snapshot");
    assert_eq!(snapshot.state, TaskState::Succeeded);

    let pool = service.db.pool();
    let tenant_id = service.tenant_id();
    let state = load_usage_source_state_sqlx(pool, tenant_id, &fixture.source.id)
        .await
        .expect("load state")
        .expect("state row");
    assert_eq!(state.opaque_cursor.as_deref(), Some("cursor-1"));
    assert_eq!(
        source_event_ids(pool, tenant_id, &fixture.source.id).await,
        vec!["evt-1".to_string()]
    );

    // 改写源侧用量数据并推进游标，然后执行全量重扫
    std::fs::write(
        &fixture.payload_path,
        format!("{}\n", usage_event_line("evt-2")),
    )
    .unwrap();
    std::fs::write(&fixture.cursor_path, "cursor-full").unwrap();

    let full = service
        .start_conversation_usage_scan(UsageScanOptions {
            mode: Some("full".to_string()),
            source_id: Some(fixture.source.id.clone()),
            ..Default::default()
        })
        .expect("start full scan");
    let full_snapshot = full
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("full snapshot");
    assert_eq!(full_snapshot.state, TaskState::Succeeded);

    let view = list_visible(&service)
        .into_iter()
        .find(|task| task.id == full.task_id())
        .expect("full scan task visible");
    let result = view.result.as_ref().expect("result projected");
    assert_eq!(result["mode"].as_str(), Some("full"));
    assert_eq!(result["total_events_ingested"].as_u64(), Some(1));

    let state = load_usage_source_state_sqlx(pool, tenant_id, &fixture.source.id)
        .await
        .expect("load state after full scan")
        .expect("state row after full scan");
    assert_eq!(
        state.opaque_cursor.as_deref(),
        Some("cursor-full"),
        "全量重扫必须把游标重置为本次扫描结果"
    );
    assert!(
        state.last_full_scan_at.is_some(),
        "全量重扫必须记录 last_full_scan_at"
    );
    assert_eq!(
        source_event_ids(pool, tenant_id, &fixture.source.id).await,
        vec!["evt-2".to_string()],
        "全量重扫必须修剪本次未再出现的失效历史事件"
    );

    drop(service);
    std::fs::remove_dir_all(&root).ok();
}
