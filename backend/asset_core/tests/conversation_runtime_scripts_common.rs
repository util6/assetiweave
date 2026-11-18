//! #201：公共脚本外置到受管用户目录 `common/`。
//!
//! 这些用例走 `AppService` 公共接缝，必须在**独立测试进程**里运行：`ASSETIWEAVE_HOME`
//! 是进程级环境变量，放在单元测试模块里会与其他并行用例互相踩踏，也会污染真实
//! `~/.assetiweave`。

use asset_core::application::{
    AppService, ConversationAdapterLocalRegisterParams, ConversationSessionGetParams,
    ConversationSourceDisableParams, ConversationSourceUpsertParams, ConversationSyncMode,
    ConversationSyncParams,
};
use asset_core::domain::conversations::{
    ConversationAdapterPackageOrigin, ConversationSource, ConversationSourceKind,
};
use asset_core::infrastructure::conversations::{
    ensure_reproject_runner_scripts, ensure_shell_command_projector,
    project_external_adapter_command_parts_with_settings, ConversationCommandProjectionPart,
};
use asset_core::infrastructure::runtime::RuntimeRole;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;
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
        let root = std::env::temp_dir().join(format!("aiw-201-{label}-{}", Uuid::new_v4()));
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

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn snapshot(path: &Path) -> (Vec<u8>, Option<SystemTime>) {
    (
        fs::read(path).expect("read snapshot file"),
        fs::metadata(path)
            .expect("read snapshot metadata")
            .modified()
            .ok(),
    )
}

fn write_session_adapter_package(package_dir: &Path, empty_marker: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(package_dir).expect("create package dir");
    fs::write(
        package_dir.join("conversation-adapter-package.json"),
        r#"{
  "schema_version": 1,
  "package_id": "com.util6.common-policy",
  "name": "Common Policy Fixture",
  "version": "1.0.0",
  "min_core_version": "0.1.0",
  "record_kind": "session",
  "adapter_manifest": "conversation-adapter.json",
  "capabilities": ["probe", "list_sessions", "read_session"],
  "runtime": { "protocol": "stdio-ndjson-v1" },
  "changelog": []
}"#,
    )
    .expect("write package manifest");
    fs::write(
        package_dir.join("conversation-adapter.json"),
        r#"{
  "schema_version": 1,
  "id": "common-policy-fixture",
  "name": "Common Policy Fixture",
  "version": "1.0.0",
  "protocol_version": 1,
  "command": ["adapter.sh"],
  "capabilities": ["probe", "list_sessions", "read_session"],
  "input_kinds": ["directory"]
}"#,
    )
    .expect("write adapter manifest");

    let script = format!(
        r#"#!/bin/sh
request=$(cat)
case "$request" in
  *'"method":"probe"'*)
    printf '%s\n' '{{"type":"complete","item":{{"revision":1}}}}'
    ;;
  *list_sessions*)
    if [ -f "{empty_marker}" ]; then
      printf '%s\n' '{{"type":"complete","item":{{"session_count":0,"snapshot_complete":true}}}}'
    else
      printf '%s\n' '{{"type":"item","item":{{"kind":"session_descriptor","external_id":"s-1","version_token":"v1"}}}}'
      printf '%s\n' '{{"type":"complete","item":{{"session_count":1,"snapshot_complete":true}}}}'
    fi
    ;;
  *read_session*)
    printf '%s\n' '{{"type":"item","item":{{"kind":"session","session":{{"external_id":"s-1","title":"Common Policy Fixture","source_fingerprint":"v1","turns":[{{"external_id":"turn-1","turn_index":0,"user_text":"Hello","parts":[{{"role":"assistant","kind":"command","command":"git status --short","cwd":"/tmp"}}]}}]}}}}}}'
    printf '%s\n' '{{"type":"complete","item":{{"session_count":1}}}}'
    ;;
  *)
    printf '%s\n' '{{"type":"complete","item":{{}}}}'
    ;;
esac
"#
    );
    let executable = package_dir.join("adapter.sh");
    fs::write(&executable, script).expect("write adapter script");
    let mut permissions = fs::metadata(&executable)
        .expect("adapter metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("make adapter executable");
}

fn source(id: &str, adapter_id: &str, location: &Path) -> ConversationSource {
    let now = "2026-01-01T00:00:00Z".to_string();
    ConversationSource {
        id: id.to_string(),
        adapter_id: adapter_id.to_string(),
        name: format!("source {id}"),
        kind: ConversationSourceKind::Directory,
        location: location.to_string_lossy().to_string(),
        config_json: None,
        enabled: true,
        last_synced_at: None,
        last_sync_status: None,
        created_at: now.clone(),
        updated_at: now,
    }
}

async fn open_service(db_path: PathBuf) -> AppService {
    let runtime = AppService::bootstrap_runtime(db_path, RuntimeRole::OneShot)
        .await
        .expect("bootstrap runtime");
    AppService::from_runtime(&runtime)
}

#[tokio::test(flavor = "multi_thread")]
async fn user_edited_common_payload_policy_survives_bootstrap_and_full_sync() {
    if !node_available() {
        eprintln!("skipping: node is not available on PATH");
        return;
    }
    let temp = TempHome::new("user-policy");
    let marker = temp.root().join("empty-source-marker");
    let package_dir = temp.root().join("workspace").join("common-policy-fixture");
    let source_dir = temp.root().join("source-dir");
    fs::create_dir_all(&source_dir).expect("create source dir");
    write_session_adapter_package(&package_dir, &marker.to_string_lossy());

    let service = open_service(temp.root().join("app.db")).await;
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

    let adapter_id = "common-policy-fixture";
    let source_a = "common-policy-source-a";
    let mut source_a_record = source(source_a, adapter_id, &source_dir);
    service
        .upsert_conversation_source(ConversationSourceUpsertParams {
            source: source_a_record.clone(),
            dry_run: false,
        })
        .await
        .expect("upsert source a");
    service
        .sync_conversations(ConversationSyncParams {
            source_id: Some(source_a.to_string()),
            adapter_id: None,
            record_kind: Some("session".to_string()),
            mode: ConversationSyncMode::Full,
            dry_run: false,
        })
        .await
        .expect("initial full sync");

    // The user externalizes a custom payload policy after the first sync.
    let common_dir = temp.common_dir();
    fs::create_dir_all(&common_dir).expect("create common dir");
    let policy_path = common_dir.join("payload-policy.mjs");
    fs::write(
        &policy_path,
        r#"export function reprojectStoredSession(session) {
  if (!session || typeof session !== "object" || !Array.isArray(session.turns)) {
    return session;
  }
  return {
    ...session,
    turns: session.turns.map((turn) => ({
      ...turn,
      parts: (Array.isArray(turn.parts) ? turn.parts : []).map((part, index) =>
        index === 0 ? { ...part, command: "echo COMMON POLICY V2" } : part),
    })),
  };
}
"#,
    )
    .expect("write custom payload policy");
    let policy_before = snapshot(&policy_path);

    // Leave one enabled empty source and disable the source that already owns
    // the persisted session, so the next full sync reprojects it externally.
    source_a_record.enabled = false;
    service
        .upsert_conversation_source(ConversationSourceUpsertParams {
            source: source_a_record,
            dry_run: false,
        })
        .await
        .expect("disable source a");
    let _ = service
        .disable_conversation_source(ConversationSourceDisableParams {
            id: source_a.to_string(),
            dry_run: false,
        })
        .await;
    fs::write(&marker, "empty").expect("write empty marker");
    service
        .upsert_conversation_source(ConversationSourceUpsertParams {
            source: source("common-policy-source-b", adapter_id, &source_dir),
            dry_run: false,
        })
        .await
        .expect("upsert source b");

    service
        .sync_conversations(ConversationSyncParams {
            source_id: None,
            adapter_id: None,
            record_kind: Some("session".to_string()),
            mode: ConversationSyncMode::Full,
            dry_run: false,
        })
        .await
        .expect("second full sync with external policy");

    assert_eq!(
        snapshot(&policy_path),
        policy_before,
        "bootstrap and full sync must never rewrite a user-edited common/payload-policy.mjs"
    );

    let session_id: String = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND source_id = ?2 AND external_id = 's-1'",
    )
    .bind(service.tenant_id())
    .bind(source_a)
    .fetch_one(service.pool())
    .await
    .expect("load persisted session id");

    let detail = service
        .get_conversation_session(ConversationSessionGetParams {
            session_id,
            roles: None,
        })
        .await
        .expect("load session detail");
    let commands: Vec<String> = detail
        .questions
        .iter()
        .flat_map(|question| question.parts.iter())
        .filter_map(|part| part.command.clone())
        .collect();
    assert!(
        commands
            .iter()
            .any(|command| command == "echo COMMON POLICY V2"),
        "the full sync must apply the externally edited common payload policy, got {commands:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_common_script_falls_back_and_stays_usable() {
    if !node_available() {
        eprintln!("skipping: node is not available on PATH");
        return;
    }
    let temp = TempHome::new("fallback");
    let common_dir = temp.common_dir();
    fs::create_dir_all(&common_dir).expect("create common dir");
    let custom_policy = common_dir.join("payload-policy.mjs");
    fs::write(&custom_policy, "export const keep = true;\n").expect("write custom policy");
    let policy_before = snapshot(&custom_policy);

    let adapter = ensure_shell_command_projector().expect("materialize common projector");
    assert_eq!(
        Path::new(adapter.manifest_path.as_deref().expect("manifest path"))
            .file_name()
            .and_then(|name| name.to_str()),
        Some("projector-manifest.json"),
        "the shell projector manifest lives in common/"
    );

    fs::remove_file(common_dir.join("shell-projector.cjs")).expect("delete projector script");
    let restored = ensure_shell_command_projector().expect("restore missing projector script");
    let projections = project_external_adapter_command_parts_with_settings(
        &restored,
        &[ConversationCommandProjectionPart {
            part_id: "part-1".to_string(),
            command: "printf '%s\\n' '--- status ---'; git status --short".to_string(),
            command_label: None,
        }],
        &serde_json::json!({}),
    )
    .await
    .expect("project through the restored core projector");
    assert!(!projections.is_empty(), "restored projector must be usable");
    assert_eq!(
        snapshot(&custom_policy),
        policy_before,
        "fallback must only release the missing file and never touch the user policy"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn repeated_runtime_bootstrap_does_not_rewrite_existing_common_scripts() {
    let temp = TempHome::new("stable");
    let common_dir = temp.common_dir();
    fs::create_dir_all(&common_dir).expect("create common dir");
    let runner = ensure_reproject_runner_scripts().expect("release common runner");
    assert_eq!(
        runner.file_name().and_then(|name| name.to_str()),
        Some("reproject-runner.mjs")
    );
    assert_eq!(runner.parent(), Some(common_dir.as_path()));

    let files = [
        "payload-policy.mjs",
        "reproject-runner.mjs",
        "shell-projector.cjs",
        "projector-adapter.cjs",
        "projector-manifest.json",
    ];
    let before: Vec<(Vec<u8>, Option<SystemTime>)> = files
        .iter()
        .map(|name| snapshot(&common_dir.join(name)))
        .collect();

    ensure_reproject_runner_scripts().expect("second runner bootstrap");
    ensure_shell_command_projector().expect("second projector bootstrap");

    let after: Vec<(Vec<u8>, Option<SystemTime>)> = files
        .iter()
        .map(|name| snapshot(&common_dir.join(name)))
        .collect();
    assert_eq!(
        after, before,
        "existing common scripts must not be rewritten on repeated bootstrap"
    );
}
