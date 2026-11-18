//! #204：综合内容 Hash 驱动的存量会话重解析。
//!
//! 这些用例走 `AppService` 公共接缝，必须在**独立测试进程**里运行：`ASSETIWEAVE_HOME`
//! 是进程级环境变量，放在单元测试模块里会与其他并行用例互相踩踏，也会污染真实
//! `~/.assetiweave`。
//!
//! 覆盖点：
//! 1. 修改 `common/payload-policy.mjs` 后，下一轮增量同步自动重解析并刷新水位；
//! 2. 同一综合 Hash 重复同步不再读取会话（不触发第二次解析）；
//! 3. 源文件已缺失的会话在重解析 / 全量对账后轮次与问答零丢失；
//! 4. 全量同步指令仍强制重解析；
//! 5. 全过程不修改任何编译期常量。

use asset_core::application::{
    AppService, ConversationAdapterLocalRegisterParams, ConversationSourceUpsertParams,
    ConversationSyncMode, ConversationSyncParams,
};
use asset_core::domain::conversations::{
    ConversationAdapterPackageOrigin, ConversationSource, ConversationSourceKind,
};
use asset_core::infrastructure::conversations::adapter_runtime_content_hash;
use asset_core::infrastructure::runtime::RuntimeRole;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;

static HOME_LOCK: Mutex<()> = Mutex::new(());

struct TempHome {
    root: PathBuf,
    previous: Option<OsString>,
    _guard: MutexGuard<'static, ()>,
}

impl TempHome {
    fn new(label: &str) -> Self {
        let guard = HOME_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let root = std::env::temp_dir().join(format!("aiw-204-{label}-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("home")).expect("create temp home");
        let previous = std::env::var_os("ASSETIWEAVE_HOME");
        std::env::set_var("ASSETIWEAVE_HOME", root.join("home"));
        Self {
            root,
            previous,
            _guard: guard,
        }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn adapter_root(&self) -> PathBuf {
        self.home().join("conversation-adapters")
    }

    fn common_dir(&self) -> PathBuf {
        self.adapter_root().join("common")
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var("ASSETIWEAVE_HOME", value),
            None => std::env::remove_var("ASSETIWEAVE_HOME"),
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

const ADAPTER_ID: &str = "hash-reparse-fixture";
const PACKAGE_ID: &str = "com.util6.hash-reparse";

/// 夹具适配器：读取 `common/payload-policy.mjs` 里的标记作为轮次文本，
/// 并把每次 `read_session` 追加到日志文件，从而可以精确断言"是否发生了重解析"。
const ADAPTER_SCRIPT: &str = r#"#!/bin/sh
request=$(cat)
log="$AIW_204_READ_LOG"
state="$AIW_204_STATE"
policy="$ASSETIWEAVE_HOME/conversation-adapters/common/payload-policy.mjs"
case "$request" in
  *'"method":"probe"'*)
    printf '%s\n' '{"type":"complete","item":{"revision":1}}'
    ;;
  *list_sessions*)
    count=0
    while IFS='|' read -r id token question answer; do
      [ -z "$id" ] && continue
      printf '%s\n' "{\"type\":\"item\",\"item\":{\"kind\":\"session_descriptor\",\"external_id\":\"$id\",\"version_token\":\"$token\"}}"
      count=$((count+1))
    done < "$state"
    printf '%s\n' "{\"type\":\"complete\",\"item\":{\"session_count\":$count,\"snapshot_complete\":true}}"
    ;;
  *read_session*)
    marker=$(grep -o 'HASH_REPARSE_MARKER=[a-z0-9]*' "$policy" | head -n 1)
    [ -z "$marker" ] && marker="HASH_REPARSE_MARKER=none"
    while IFS='|' read -r id token question answer; do
      [ -z "$id" ] && continue
      case "$request" in
        *"\"${id}\""*)
          printf '%s %s\n' "$id" "$marker" >> "$log"
          printf '%s\n' "{\"type\":\"item\",\"item\":{\"kind\":\"session\",\"session\":{\"external_id\":\"$id\",\"title\":\"Session $id\",\"source_fingerprint\":\"$token\",\"turns\":[{\"external_id\":\"t1\",\"turn_index\":0,\"user_text\":\"$question\",\"parts\":[{\"role\":\"assistant\",\"kind\":\"text\",\"text\":\"$answer\"}]}]}}}"
          printf '%s\n' '{"type":"complete","item":{"session_count":1}}'
          exit 0
          ;;
      esac
    done < "$state"
    printf '%s\n' '{"type":"complete","item":{"session_count":0}}'
    ;;
  *)
    printf '%s\n' '{"type":"complete","item":{}}'
    ;;
esac
"#;

fn write_adapter_package(package_dir: &Path) {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(package_dir).expect("create package dir");
    fs::write(
        package_dir.join("conversation-adapter-package.json"),
        format!(
            r#"{{
  "schema_version": 1,
  "package_id": "{PACKAGE_ID}",
  "name": "Hash Reparse Fixture",
  "version": "1.0.0",
  "min_core_version": "0.1.0",
  "record_kind": "session",
  "adapter_manifest": "conversation-adapter.json",
  "capabilities": ["probe", "list_sessions", "read_session"],
  "runtime": {{ "protocol": "stdio-ndjson-v1" }},
  "changelog": []
}}"#
        ),
    )
    .expect("write package manifest");
    fs::write(
        package_dir.join("conversation-adapter.json"),
        format!(
            r#"{{
  "schema_version": 1,
  "id": "{ADAPTER_ID}",
  "name": "Hash Reparse Fixture",
  "version": "1.0.0",
  "protocol_version": 1,
  "command": ["adapter.sh"],
  "capabilities": ["probe", "list_sessions", "read_session"],
  "input_kinds": ["directory"]
}}"#
        ),
    )
    .expect("write adapter manifest");
    let executable = package_dir.join("adapter.sh");
    fs::write(&executable, ADAPTER_SCRIPT).expect("write adapter script");
    let mut permissions = fs::metadata(&executable)
        .expect("adapter metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("make adapter executable");
}

fn write_state(path: &Path, sessions: &[(&str, &str, &str)]) {
    let body = sessions
        .iter()
        .map(|(id, question, answer)| format!("{id}|v1|{question}|{answer}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path, format!("{body}\n")).expect("write session state file");
}

fn write_policy(common_dir: &Path, marker: &str) {
    fs::create_dir_all(common_dir).expect("create common dir");
    fs::write(
        common_dir.join("payload-policy.mjs"),
        format!(
            "// HASH_REPARSE_MARKER={marker}\nexport function reprojectStoredSession(session) {{ return session; }}\n"
        ),
    )
    .expect("write payload policy");
}

fn read_log(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .filter(|line| !line.trim().is_empty())
        .collect()
}

async fn open_service(db_path: PathBuf) -> AppService {
    let runtime = AppService::bootstrap_runtime(db_path, RuntimeRole::OneShot)
        .await
        .expect("bootstrap runtime");
    AppService::from_runtime(&runtime)
}

async fn register_fixture(service: &AppService, package_dir: &Path) {
    service
        .register_conversation_adapter_local(ConversationAdapterLocalRegisterParams {
            package_dir: package_dir.to_string_lossy().to_string(),
            origin: ConversationAdapterPackageOrigin::LocalDirectory,
            source_url: None,
            git_ref: None,
            git_commit: None,
            dry_run: false,
            yes: true,
        })
        .await
        .expect("register local adapter package");
}

async fn upsert_source(service: &AppService, source_id: &str, location: &Path) {
    let now = "2026-01-01T00:00:00Z".to_string();
    service
        .upsert_conversation_source(ConversationSourceUpsertParams {
            source: ConversationSource {
                id: source_id.to_string(),
                adapter_id: ADAPTER_ID.to_string(),
                name: format!("source {source_id}"),
                kind: ConversationSourceKind::Directory,
                location: location.to_string_lossy().to_string(),
                config_json: None,
                enabled: true,
                last_synced_at: None,
                last_sync_status: None,
                created_at: now.clone(),
                updated_at: now,
            },
            dry_run: false,
        })
        .await
        .expect("upsert source");
}

async fn sync(service: &AppService, source_id: &str, mode: ConversationSyncMode) {
    service
        .sync_conversations(ConversationSyncParams {
            source_id: Some(source_id.to_string()),
            adapter_id: None,
            record_kind: Some("session".to_string()),
            mode,
            dry_run: false,
        })
        .await
        .expect("sync conversation source");
}

async fn synced_adapter_hash(service: &AppService, source_id: &str) -> Option<String> {
    sqlx::query_scalar(
        "SELECT synced_adapter_hash FROM conversation_sources WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(service.tenant_id())
    .bind(source_id)
    .fetch_one(service.pool())
    .await
    .expect("load synced adapter hash")
}

async fn current_runtime_hash(service: &AppService) -> String {
    let adapter = service
        .list_conversation_adapters()
        .expect("list adapters")
        .into_iter()
        .find(|adapter| adapter.id == ADAPTER_ID)
        .expect("fixture adapter registered");
    adapter_runtime_content_hash(&adapter)
        .expect("compute runtime hash")
        .expect("fixture adapter has a content hash")
}

async fn session_turns(
    service: &AppService,
    source_id: &str,
    external_id: &str,
) -> Vec<(String, String)> {
    // 快照模型下提问落在 `conversation_turns.prompt`，回答落在 `response`。
    // 逐轮取出 (提问, 回答)，用于断言重解析后问答零丢失。
    sqlx::query_as(
        r#"
        SELECT turns.prompt, COALESCE(turns.response, '')
        FROM conversation_turns turns
        JOIN conversation_sessions sessions
          ON sessions.tenant_id = turns.tenant_id AND sessions.id = turns.session_id
        WHERE turns.tenant_id = ?1 AND sessions.source_id = ?2 AND sessions.external_id = ?3
        ORDER BY turns.turn_index
        "#,
    )
    .bind(service.tenant_id())
    .bind(source_id)
    .bind(external_id)
    .fetch_all(service.pool())
    .await
    .expect("load session turns")
}

async fn turn_count(service: &AppService, source_id: &str, external_id: &str) -> i64 {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM conversation_turns turns
        JOIN conversation_sessions sessions
          ON sessions.tenant_id = turns.tenant_id AND sessions.id = turns.session_id
        WHERE turns.tenant_id = ?1 AND sessions.source_id = ?2 AND sessions.external_id = ?3
        "#,
    )
    .bind(service.tenant_id())
    .bind(source_id)
    .bind(external_id)
    .fetch_one(service.pool())
    .await
    .expect("count session turns")
}

#[tokio::test(flavor = "multi_thread")]
async fn common_policy_hash_drift_reparses_incrementally_and_refreshes_watermark() {
    let temp = TempHome::new("drift");
    let read_log_path = temp.root().join("read.log");
    let state = temp.root().join("state.txt");
    std::env::set_var("AIW_204_READ_LOG", &read_log_path);
    std::env::set_var("AIW_204_STATE", &state);
    write_state(&state, &[("s-1", "Q1", "A1")]);

    let source_id = "hash-reparse-source";
    let source_dir = temp.root().join("source-dir");
    fs::create_dir_all(&source_dir).expect("create source dir");
    let package_dir = temp.root().join("workspace").join("hash-reparse-fixture");
    write_adapter_package(&package_dir);

    let service = open_service(temp.root().join("app.db")).await;
    write_policy(&temp.common_dir(), "v1");
    register_fixture(&service, &package_dir).await;
    upsert_source(&service, source_id, &source_dir).await;

    // 首轮增量同步：整源读取一次，并落库综合内容 Hash 水位。
    sync(&service, source_id, ConversationSyncMode::Incremental).await;
    let first_hash = synced_adapter_hash(&service, source_id)
        .await
        .expect("first sync stamps the adapter hash");
    assert_eq!(
        first_hash,
        current_runtime_hash(&service).await,
        "the sync watermark must record the composite content hash"
    );
    assert_eq!(
        read_log(&read_log_path).len(),
        1,
        "first sync must read the session once"
    );
    assert_eq!(
        session_turns(&service, source_id, "s-1").await,
        vec![("Q1".to_string(), "A1".to_string())]
    );
    let first_log = read_log(&read_log_path);

    // 同一 Hash 再同步：不触发第二次解析。
    sync(&service, source_id, ConversationSyncMode::Incremental).await;
    assert_eq!(
        read_log(&read_log_path),
        first_log,
        "an unchanged composite hash must not re-read the session"
    );
    assert_eq!(
        synced_adapter_hash(&service, source_id).await.as_deref(),
        Some(first_hash.as_str())
    );

    // 晋升改变公共策略脚本：综合 Hash 必须变化，下一轮同步自动重解析。
    write_policy(&temp.common_dir(), "v2");
    let drifted_hash = current_runtime_hash(&service).await;
    assert_ne!(
        drifted_hash, first_hash,
        "editing common/payload-policy.mjs must change the composite content hash"
    );

    sync(&service, source_id, ConversationSyncMode::Incremental).await;
    let after_drift_log = read_log(&read_log_path);
    assert_eq!(
        after_drift_log.len(),
        first_log.len() + 1,
        "hash drift must trigger a re-parse on the next incremental sync"
    );
    assert!(
        after_drift_log
            .last()
            .is_some_and(|line| line.contains("HASH_REPARSE_MARKER=v2")),
        "the re-parse must run under the promoted policy, got {after_drift_log:?}"
    );
    assert_eq!(
        synced_adapter_hash(&service, source_id).await.as_deref(),
        Some(drifted_hash.as_str()),
        "the adapter hash watermark must advance after the re-parse"
    );
    assert_eq!(
        session_turns(&service, source_id, "s-1").await,
        vec![("Q1".to_string(), "A1".to_string())],
        "re-parsing under a new policy must keep the session content intact"
    );

    // 水位已刷新：同一 Hash 重复同步不再重复解析。
    sync(&service, source_id, ConversationSyncMode::Incremental).await;
    assert_eq!(
        read_log(&read_log_path),
        after_drift_log,
        "once the watermark is refreshed, repeated syncs must not re-parse again"
    );

    std::env::remove_var("AIW_204_READ_LOG");
    std::env::remove_var("AIW_204_STATE");
    drop(service);
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_source_session_keeps_all_turns_and_qa_through_reparse_and_full_sync() {
    let temp = TempHome::new("missing");
    let read_log_path = temp.root().join("read.log");
    let state = temp.root().join("state.txt");
    std::env::set_var("AIW_204_READ_LOG", &read_log_path);
    std::env::set_var("AIW_204_STATE", &state);
    write_state(
        &state,
        &[("s-1", "Q-one", "A-one"), ("s-2", "Q-two", "A-two")],
    );

    let source_id = "hash-reparse-missing-source";
    let source_dir = temp.root().join("source-dir");
    fs::create_dir_all(&source_dir).expect("create source dir");
    let package_dir = temp.root().join("workspace").join("hash-reparse-fixture");
    write_adapter_package(&package_dir);

    let service = open_service(temp.root().join("app.db")).await;
    write_policy(&temp.common_dir(), "v1");
    register_fixture(&service, &package_dir).await;
    upsert_source(&service, source_id, &source_dir).await;

    sync(&service, source_id, ConversationSyncMode::Full).await;
    let baseline_two = session_turns(&service, source_id, "s-2").await;
    assert_eq!(
        baseline_two,
        vec![("Q-two".to_string(), "A-two".to_string())]
    );
    assert_eq!(turn_count(&service, source_id, "s-2").await, 1);

    // 源文件被删除：发现阶段不再返回 s-2；同时策略脚本晋升改变 Hash。
    write_state(&state, &[("s-1", "Q-one", "A-one")]);
    write_policy(&temp.common_dir(), "v2");
    let log_before_drift = read_log(&read_log_path);
    sync(&service, source_id, ConversationSyncMode::Incremental).await;
    assert_eq!(
        read_log(&read_log_path).len(),
        log_before_drift.len() + 1,
        "the surviving session must be re-parsed after the hash drift"
    );

    // 源文件缺失会话的全部轮次与问答零丢失。
    assert_eq!(
        session_turns(&service, source_id, "s-2").await,
        baseline_two,
        "a session whose source file vanished must keep its questions and answers"
    );
    assert_eq!(
        turn_count(&service, source_id, "s-2").await,
        1,
        "a session whose source file vanished must keep its turns"
    );

    // 全量同步仍然强制重解析，并做缺失对账；历史内容依旧零丢失。
    let log_before_full = read_log(&read_log_path);
    sync(&service, source_id, ConversationSyncMode::Full).await;
    assert!(
        read_log(&read_log_path).len() > log_before_full.len(),
        "a full sync must force a re-parse even without hash drift"
    );
    assert_eq!(
        session_turns(&service, source_id, "s-2").await,
        baseline_two,
        "full sync reconciliation must never drop a missing session's questions and answers"
    );
    assert_eq!(
        turn_count(&service, source_id, "s-2").await,
        1,
        "full sync reconciliation must never drop a missing session's turns"
    );

    let missing: i64 = sqlx::query_scalar(
        r#"
        SELECT missing FROM conversation_sessions
        WHERE tenant_id = ?1 AND source_id = ?2 AND external_id = 's-2'
        "#,
    )
    .bind(service.tenant_id())
    .bind(source_id)
    .fetch_one(service.pool())
    .await
    .expect("load missing flag");
    assert_eq!(
        missing, 1,
        "the vanished session must be flagged missing, not deleted"
    );

    std::env::remove_var("AIW_204_READ_LOG");
    std::env::remove_var("AIW_204_STATE");
    drop(service);
}
