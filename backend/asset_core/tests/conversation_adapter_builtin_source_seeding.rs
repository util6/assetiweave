//! #205：内置对话来源随适配器存在性播种。
//!
//! #196 之后二进制不再内嵌官方适配器包，全新安装的受管目录可以是空目录。
//! 这些用例验证来源行跟着适配器存在性走：适配器缺席时不落来源、适配器注册/
//! 晋升后自动补齐、重复引导与重复注册幂等、注销后不再重新播种且历史保留。
//!
//! 必须在**独立测试进程**里运行：`ASSETIWEAVE_HOME` 是进程级环境变量，会和
//! 其它并行用例互相踩踏，也会污染真实 `~/.assetiweave`。

use asset_core::application::{
    AppService, ConversationAdapterLocalRegisterParams, ConversationAdapterUnregisterParams,
    ConversationSessionListParams, ConversationSourceUpsertParams,
};
use asset_core::domain::conversations::ConversationAdapterPackageOrigin;
use asset_core::domain::{
    ConversationPartKind, ConversationPartRole, NormalizedConversationPart,
    NormalizedConversationSession, NormalizedConversationTurn,
};
use asset_core::infrastructure::runtime::RuntimeRole;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;

const BUILTIN_SOURCE_IDS: [&str; 4] = [
    "codex-live",
    "claude-code-live",
    "opencode-live",
    "codebuddy-live",
];

static HOME_LOCK: Mutex<()> = Mutex::new(());

struct TempHome {
    root: PathBuf,
    previous: Option<OsString>,
    _guard: MutexGuard<'static, ()>,
}

impl TempHome {
    fn new(label: &str) -> Self {
        let guard = HOME_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let root = std::env::temp_dir().join(format!("aiw-205-{label}-{}", Uuid::new_v4()));
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

/// 写一个最小可注册的包目录：探测脚本用 shell，避免依赖 node 运行时。
fn write_adapter_package(dir: &Path, adapter_id: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(dir).expect("create package dir");
    fs::write(
        dir.join("conversation-adapter-package.json"),
        format!(
            r#"{{
  "schema_version": 1,
  "package_id": "com.util6.{adapter_id}-source-fixture",
  "name": "Builtin Source Fixture",
  "version": "1.0.0",
  "min_core_version": "0.1.0",
  "record_kind": "session",
  "adapter_manifest": "conversation-adapter.json",
  "capabilities": ["probe", "read_session"],
  "runtime": {{ "protocol": "stdio-ndjson-v1" }},
  "changelog": []
}}"#
        ),
    )
    .expect("write package manifest");
    fs::write(
        dir.join("conversation-adapter.json"),
        format!(
            r#"{{
  "schema_version": 1,
  "id": "{adapter_id}",
  "name": "Builtin Source Fixture",
  "version": "1.0.0",
  "protocol_version": 1,
  "command": ["adapter.sh"],
  "capabilities": ["probe", "read_session"],
  "input_kinds": ["directory"]
}}"#
        ),
    )
    .expect("write adapter manifest");
    let executable = dir.join("adapter.sh");
    fs::write(
        &executable,
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":1}}'\n",
    )
    .expect("write adapter script");
    let mut permissions = fs::metadata(&executable)
        .expect("adapter metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("make adapter executable");
}

async fn open_service(db_path: PathBuf) -> AppService {
    let runtime = AppService::bootstrap_runtime(db_path, RuntimeRole::OneShot)
        .await
        .expect("bootstrap runtime");
    AppService::from_runtime(&runtime)
}

async fn register_local_package(service: &AppService, package_dir: &Path) {
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
        .expect("register local package");
}

async fn builtin_source_ids(service: &AppService) -> Vec<String> {
    let mut ids = service
        .list_conversation_sources()
        .await
        .expect("list conversation sources")
        .into_iter()
        .map(|source| source.id)
        .filter(|id| BUILTIN_SOURCE_IDS.contains(&id.as_str()))
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn make_session(external_id: &str) -> NormalizedConversationSession {
    let timestamp = "2026-11-18T00:00:00Z".to_string();
    NormalizedConversationSession {
        external_id: external_id.to_string(),
        title: Some(format!("fixture {external_id}")),
        project_path: Some("/code/fixture".to_string()),
        started_at: Some(timestamp.clone()),
        updated_at: Some(timestamp.clone()),
        source_locator: Some(format!("fixture://{external_id}")),
        source_fingerprint: Some(format!("fp-{external_id}")),
        turns: vec![NormalizedConversationTurn {
            external_id: Some(format!("turn-{external_id}")),
            turn_index: 0,
            user_text: "历史问题".to_string(),
            started_at: Some(timestamp.clone()),
            ended_at: Some(timestamp.clone()),
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some("历史回答".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: None,
            }],
            ..Default::default()
        }],
        user_visible: Some(true),
        ..Default::default()
    }
}

async fn session_count_for_source(service: &AppService, source_id: &str) -> usize {
    service
        .list_conversation_sessions(ConversationSessionListParams {
            adapter_id: None,
            source_id: Some(source_id.to_string()),
            query: None,
            limit: None,
            offset: None,
            project_path: None,
            since: None,
            until: None,
            window: None,
            range: None,
            parent_session_id: None,
        })
        .await
        .expect("list conversation sessions")
        .len()
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_managed_directory_seeds_no_builtin_sources() {
    let temp = TempHome::new("empty-managed");
    let service = open_service(temp.root().join("app.db")).await;

    assert_eq!(
        builtin_source_ids(&service).await,
        Vec::<String>::new(),
        "a fresh installation without any adapter must not own dangling built-in sources"
    );
    assert!(
        service
            .list_conversation_adapters()
            .expect("list adapters")
            .is_empty(),
        "the managed directory is empty, so no adapter can be discovered"
    );

    drop(service);
}

#[tokio::test(flavor = "multi_thread")]
async fn registering_codex_backfills_only_its_builtin_source() {
    let temp = TempHome::new("register-codex");
    let service = open_service(temp.root().join("app.db")).await;
    assert!(builtin_source_ids(&service).await.is_empty());

    let package_dir = temp.root().join("workspace").join("codex");
    write_adapter_package(&package_dir, "codex");
    register_local_package(&service, &package_dir).await;

    assert_eq!(
        builtin_source_ids(&service).await,
        vec!["codex-live".to_string()],
        "activation must backfill exactly the source owned by the activated adapter"
    );
    let source = service
        .list_conversation_sources()
        .await
        .expect("list conversation sources")
        .into_iter()
        .find(|source| source.id == "codex-live")
        .expect("codex-live source");
    assert!(source.enabled, "a freshly backfilled source starts enabled");
    assert_eq!(source.adapter_id, "codex");

    drop(service);
}

#[tokio::test(flavor = "multi_thread")]
async fn repeated_bootstrap_and_registration_keep_sources_idempotent() {
    let temp = TempHome::new("idempotent");
    let db_path = temp.root().join("app.db");
    let package_dir = temp.root().join("workspace").join("codex");
    write_adapter_package(&package_dir, "codex");

    let service = open_service(db_path.clone()).await;
    register_local_package(&service, &package_dir).await;
    assert_eq!(
        builtin_source_ids(&service).await,
        vec!["codex-live".to_string()]
    );

    // 用户改动：改名并停用。
    let mut customized = service
        .list_conversation_sources()
        .await
        .expect("list conversation sources")
        .into_iter()
        .find(|source| source.id == "codex-live")
        .expect("codex-live source");
    customized.name = "My Codex".to_string();
    customized.enabled = false;
    service
        .upsert_conversation_source(ConversationSourceUpsertParams {
            source: customized,
            dry_run: false,
        })
        .await
        .expect("customize source");

    // 重复注册 + 重新引导都不能产生重复行，也不能重置用户改动。
    register_local_package(&service, &package_dir).await;
    drop(service);
    let service = open_service(db_path.clone()).await;

    let sources = service
        .list_conversation_sources()
        .await
        .expect("list conversation sources");
    let codex_sources = sources
        .iter()
        .filter(|source| source.id == "codex-live")
        .collect::<Vec<_>>();
    assert_eq!(codex_sources.len(), 1, "seeding must stay idempotent");
    assert_eq!(codex_sources[0].name, "My Codex");
    assert!(
        !codex_sources[0].enabled,
        "seeding must never re-enable a source the user disabled"
    );
    assert_eq!(
        builtin_source_ids(&service).await,
        vec!["codex-live".to_string()],
        "no other built-in source may appear"
    );

    drop(service);
}

#[tokio::test(flavor = "multi_thread")]
async fn unregistered_adapter_source_is_not_reseeded_and_history_survives() {
    let temp = TempHome::new("unregister");
    let db_path = temp.root().join("app.db");
    let package_dir = temp.root().join("workspace").join("codex");
    write_adapter_package(&package_dir, "codex");

    let service = open_service(db_path.clone()).await;
    register_local_package(&service, &package_dir).await;

    let source = service
        .list_conversation_sources()
        .await
        .expect("list conversation sources")
        .into_iter()
        .find(|source| source.id == "codex-live")
        .expect("codex-live source");
    asset_core::store::import_conversation_sessions_sqlx(
        service.pool(),
        service.tenant_id(),
        &source,
        &[make_session("codex-session-1")],
        false,
    )
    .await
    .expect("import historical session");
    assert_eq!(session_count_for_source(&service, "codex-live").await, 1);

    service
        .unregister_conversation_adapter(ConversationAdapterUnregisterParams {
            adapter_id: "codex".to_string(),
            dry_run: false,
            yes: true,
        })
        .await
        .expect("unregister codex adapter");
    drop(service);

    let service = open_service(db_path.clone()).await;
    assert_eq!(
        builtin_source_ids(&service).await,
        vec!["codex-live".to_string()],
        "an unregistered adapter must never be re-seeded as an active source"
    );
    assert!(
        service
            .list_conversation_adapters()
            .expect("list adapters")
            .is_empty(),
        "the unregistered adapter must not come back from the managed directory"
    );
    let disabled = service
        .list_conversation_sources()
        .await
        .expect("list conversation sources")
        .into_iter()
        .find(|source| source.id == "codex-live")
        .expect("the historical source row is preserved, just disabled");
    assert!(!disabled.enabled);
    assert_eq!(
        session_count_for_source(&service, "codex-live").await,
        1,
        "unregistering an adapter must never delete its conversation history"
    );

    drop(service);
}
