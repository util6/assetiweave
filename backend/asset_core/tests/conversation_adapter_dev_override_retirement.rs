//! #199：废除 `DevOverride` 特权分支并迁移遗留记录。
//!
//! 用例走 `AppService` 公共接缝，并且必须在独立测试进程里运行：`ASSETIWEAVE_HOME`
//! 是进程级环境变量，和单元测试并行会互相踩踏，也会污染真实 `~/.assetiweave`。

use asset_core::application::AppService;
use asset_core::domain::conversations::{
    ConversationAdapterPackage, ConversationAdapterPackageOrigin,
    ConversationAdapterPackageRecordKind, ConversationAdapterRuntimeGateStatus,
    ConversationPackageUpdatePolicy,
};
use asset_core::infrastructure::path_utils::expand_path;
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
        let root = std::env::temp_dir().join(format!("aiw-199-{label}-{}", Uuid::new_v4()));
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

const PROBE: &str =
    "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":1}}'\n";

fn write_package_dir(dir: &Path, package_id: &str, adapter_id: &str, version: &str, script: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(dir).expect("create package dir");
    fs::write(
        dir.join("conversation-adapter-package.json"),
        format!(
            r#"{{
  "schema_version": 1,
  "package_id": "{package_id}",
  "name": "Legacy Dev Fixture",
  "version": "{version}",
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
  "name": "Legacy Dev Fixture",
  "version": "{version}",
  "protocol_version": 1,
  "command": ["adapter.sh"],
  "capabilities": ["probe", "read_session"],
  "input_kinds": ["directory"]
}}"#
        ),
    )
    .expect("write adapter manifest");
    let executable = dir.join("adapter.sh");
    fs::write(&executable, script).expect("write adapter script");
    let mut permissions = fs::metadata(&executable)
        .expect("adapter metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("make adapter executable");
}

/// A row exactly as the retired `register_conversation_adapter_dev` wrote it:
/// `install_dir` points straight at the source tree and no trusted hash is set.
fn legacy_dev_override_package(
    package_id: &str,
    adapter_id: &str,
    version: &str,
    install_dir: &Path,
) -> ConversationAdapterPackage {
    let now = chrono::Utc::now().to_rfc3339();
    ConversationAdapterPackage {
        package_id: package_id.to_string(),
        adapter_id: adapter_id.to_string(),
        name: "Legacy Dev Fixture".to_string(),
        version: version.to_string(),
        record_kind: ConversationAdapterPackageRecordKind::Session,
        install_dir: install_dir.to_string_lossy().to_string(),
        manifest_path: install_dir
            .join("conversation-adapter-package.json")
            .to_string_lossy()
            .to_string(),
        adapter_manifest_path: install_dir
            .join("conversation-adapter.json")
            .to_string_lossy()
            .to_string(),
        runtime_protocol: "stdio-ndjson-v1".to_string(),
        runtime_ready: true,
        origin: ConversationAdapterPackageOrigin::DevOverride,
        source_url: None,
        git_ref: None,
        git_commit: None,
        catalog_url: None,
        update_policy: ConversationPackageUpdatePolicy::PinExact,
        latest_version: None,
        last_checked_at: None,
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        runtime_validated_at: Some(now.clone()),
        installed_content_hash: None,
        trusted_package_hash: None,
        error_message: None,
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
async fn startup_migrates_legacy_dev_override_into_managed_snapshot() {
    let temp = TempHome::new("migrate");
    let source_dir = temp.root().join("workspace").join("legacy-dev-source");
    write_package_dir(
        &source_dir,
        "com.util6.legacy-dev",
        "legacy-dev",
        "1.2.3",
        PROBE,
    );

    let db_path = temp.home().join("app.db");
    let service = open_service(db_path.clone()).await;
    service
        .save_conversation_adapter_package(&legacy_dev_override_package(
            "com.util6.legacy-dev",
            "legacy-dev",
            "1.2.3",
            &source_dir,
        ))
        .await
        .expect("seed legacy dev_override row");
    drop(service);

    // #195 / ADR-0017：遗留 `dev_override` 迁移只在启动对账执行，
    // 不再挂在租户切换 / reconcile 的同步请求路径上，所以这里必须重新启动一次。
    let service = open_service(db_path).await;

    let package = service
        .load_conversation_adapter_package("com.util6.legacy-dev")
        .await
        .expect("load migrated package")
        .expect("legacy row must be migrated, not deleted");

    assert_eq!(
        package.origin,
        ConversationAdapterPackageOrigin::LocalDirectory
    );
    let trusted = package
        .trusted_package_hash
        .clone()
        .expect("migration must trust the snapshot hash");
    assert_eq!(trusted.len(), 64, "trusted hash must be a sha256 digest");
    assert_eq!(
        package.installed_content_hash.as_deref(),
        Some(trusted.as_str())
    );

    let install_dir = expand_path(&package.install_dir).expect("expand install dir");
    assert_ne!(
        install_dir, source_dir,
        "install_dir must stop pointing at the source tree"
    );
    assert!(
        install_dir.is_dir(),
        "migrated snapshot must exist on disk: {}",
        install_dir.display()
    );
    let expected_revision = format!("1.2.3-{}", &trusted[..12]);
    assert_eq!(
        install_dir.file_name().and_then(|value| value.to_str()),
        Some(expected_revision.as_str())
    );
    assert_eq!(
        install_dir
            .parent()
            .and_then(|value| value.file_name())
            .and_then(|value| value.to_str()),
        Some("versions")
    );
    assert_eq!(
        install_dir
            .parent()
            .and_then(|value| value.parent())
            .and_then(|value| value.file_name())
            .and_then(|value| value.to_str()),
        Some("com.util6.legacy-dev")
    );
    assert!(install_dir
        .join("conversation-adapter-package.json")
        .is_file());

    assert!(
        source_dir.join("adapter.sh").is_file(),
        "migration must never delete the user's source directory"
    );

    let adapters =
        asset_core::store::list_conversation_adapters_sqlx(service.pool(), service.tenant_id())
            .await
            .expect("list adapters");
    let adapter = adapters
        .iter()
        .find(|adapter| adapter.id == "legacy-dev")
        .expect("migrated adapter projection");
    assert_eq!(
        adapter.trusted_hash.as_deref(),
        adapter.content_hash.as_deref(),
        "migrated adapter projection must trust the snapshot content"
    );
    assert!(adapter.content_hash.is_some());
    let manifest_path = expand_path(
        adapter
            .manifest_path
            .as_deref()
            .expect("adapter manifest path"),
    )
    .expect("expand adapter manifest path");
    assert!(
        manifest_path.starts_with(&install_dir),
        "runtime projection must point into the snapshot: {}",
        manifest_path.display()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn startup_keeps_unreachable_dev_override_rows_but_marks_them_unready() {
    let temp = TempHome::new("missing-source");
    let missing_source = temp.root().join("workspace").join("deleted-source");

    let db_path = temp.home().join("app.db");
    let service = open_service(db_path.clone()).await;
    service
        .save_conversation_adapter_package(&legacy_dev_override_package(
            "com.util6.gone-dev",
            "gone-dev",
            "0.9.0",
            &missing_source,
        ))
        .await
        .expect("seed legacy dev_override row");
    drop(service);

    // #195 / ADR-0017：启动期迁移遇到不可达源只标记不可运行，不能阻断启动。
    let service = open_service(db_path).await;

    let package = service
        .load_conversation_adapter_package("com.util6.gone-dev")
        .await
        .expect("load package")
        .expect("unreachable legacy row must not be deleted");
    assert_eq!(
        package.origin,
        ConversationAdapterPackageOrigin::DevOverride
    );
    assert!(
        !package.runtime_ready,
        "unreachable row must not be runnable"
    );
    assert_eq!(
        package.runtime_gate_status,
        ConversationAdapterRuntimeGateStatus::RuntimeMissing
    );
    assert!(
        package
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("not a directory")),
        "row must carry a repair hint: {:?}",
        package.error_message
    );

    let versions_dir = temp
        .adapter_root()
        .join("packages")
        .join("com.util6.gone-dev")
        .join("versions");
    assert!(
        !versions_dir.exists(),
        "no snapshot may be created for an unreachable source"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn repeated_startup_migration_is_idempotent_for_migrated_rows() {
    let temp = TempHome::new("idempotent");
    let source_dir = temp.root().join("workspace").join("legacy-dev-source");
    write_package_dir(
        &source_dir,
        "com.util6.legacy-dev",
        "legacy-dev",
        "1.2.3",
        PROBE,
    );

    let db_path = temp.home().join("app.db");
    let service = open_service(db_path.clone()).await;
    service
        .save_conversation_adapter_package(&legacy_dev_override_package(
            "com.util6.legacy-dev",
            "legacy-dev",
            "1.2.3",
            &source_dir,
        ))
        .await
        .expect("seed legacy dev_override row");
    drop(service);

    // #195 / ADR-0017：第一次启动执行迁移，第二次启动必须幂等跳过。
    let service = open_service(db_path.clone()).await;
    let first = service
        .load_conversation_adapter_package("com.util6.legacy-dev")
        .await
        .expect("load package")
        .expect("package exists");
    drop(service);

    let service = open_service(db_path).await;
    let second = service
        .load_conversation_adapter_package("com.util6.legacy-dev")
        .await
        .expect("load package")
        .expect("package exists");
    assert_eq!(first.install_dir, second.install_dir);
    assert_eq!(first.trusted_package_hash, second.trusted_package_hash);
    assert_eq!(
        first.updated_at, second.updated_at,
        "an already migrated row must not be rewritten"
    );

    let versions_dir = temp
        .adapter_root()
        .join("packages")
        .join("com.util6.legacy-dev")
        .join("versions");
    let revisions = fs::read_dir(&versions_dir)
        .expect("read versions dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .count();
    assert_eq!(revisions, 1, "repeat reconcile must not add snapshots");
}

#[tokio::test(flavor = "multi_thread")]
async fn dev_registration_promotes_into_a_managed_snapshot() {
    let temp = TempHome::new("register");
    let source_dir = temp.root().join("workspace").join("legacy-dev-source");
    write_package_dir(
        &source_dir,
        "com.util6.legacy-dev",
        "legacy-dev",
        "1.2.3",
        PROBE,
    );

    let service = open_service(temp.home().join("app.db")).await;
    let registered = service
        .register_conversation_adapter_dev(
            Some(source_dir.to_string_lossy().to_string()),
            false,
            false,
            true,
        )
        .await
        .expect("register dev adapter");

    assert_eq!(
        registered["origin"], "local_directory",
        "dev registration must no longer create a dev_override package"
    );

    let packages = service
        .load_conversation_adapter_packages()
        .await
        .expect("list packages");
    assert!(
        packages
            .iter()
            .all(|package| package.origin != ConversationAdapterPackageOrigin::DevOverride),
        "no row may be created with the retired origin"
    );

    let package = service
        .load_conversation_adapter_package("com.util6.legacy-dev")
        .await
        .expect("load package")
        .expect("registered package");
    assert_eq!(
        package.origin,
        ConversationAdapterPackageOrigin::LocalDirectory
    );
    assert!(package.trusted_package_hash.is_some());
    let install_dir = expand_path(&package.install_dir).expect("expand install dir");
    assert_ne!(install_dir, source_dir);
    assert!(install_dir.is_dir());
    assert_eq!(
        install_dir
            .parent()
            .and_then(|value| value.file_name())
            .and_then(|value| value.to_str()),
        Some("versions")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tampering_with_a_promoted_snapshot_fails_the_trust_gate() {
    let temp = TempHome::new("tamper");
    let source_dir = temp.root().join("workspace").join("legacy-dev-source");
    write_package_dir(
        &source_dir,
        "com.util6.legacy-dev",
        "legacy-dev",
        "1.2.3",
        PROBE,
    );

    let service = open_service(temp.home().join("app.db")).await;
    service
        .register_conversation_adapter_dev(
            Some(source_dir.to_string_lossy().to_string()),
            false,
            false,
            true,
        )
        .await
        .expect("register dev adapter");

    let package = service
        .load_conversation_adapter_package("com.util6.legacy-dev")
        .await
        .expect("load package")
        .expect("registered package");
    let install_dir = expand_path(&package.install_dir).expect("expand install dir");
    fs::write(
        install_dir.join("adapter.sh"),
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":2}}'\n",
    )
    .expect("tamper snapshot script");

    let adapter =
        asset_core::store::list_conversation_adapters_sqlx(service.pool(), service.tenant_id())
            .await
            .expect("list adapters")
            .into_iter()
            .find(|adapter| adapter.id == "legacy-dev")
            .expect("adapter projection");

    service
        .ensure_conversation_adapter_package_runtime_ready(&adapter)
        .await
        .expect_err("an edited snapshot must fail the hash gate");

    let reloaded = service
        .load_conversation_adapter_package("com.util6.legacy-dev")
        .await
        .expect("load package")
        .expect("package exists");
    assert!(!reloaded.runtime_ready);
    assert_eq!(
        reloaded.runtime_gate_status,
        ConversationAdapterRuntimeGateStatus::HashMismatch
    );
}
