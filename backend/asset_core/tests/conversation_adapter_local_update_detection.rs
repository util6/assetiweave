//! #198：受管目录本地更新检测与状态机优先级。
//!
//! 这些用例走 `AppService` 公共接缝，必须在**独立测试进程**里运行：`ASSETIWEAVE_HOME`
//! 是进程级环境变量，放在单元测试模块里会与其他并行用例互相踩踏，也会污染真实
//! `~/.assetiweave`。

use asset_core::application::conversations::conversation_script_catalog::discover_local_conversation_adapter_packages;
use asset_core::application::{
    AppService, ConversationAdapterLocalRegisterParams, ConversationAdapterPackageCatalogParams,
    ConversationAdapterPackageUpdateCheckParams,
};
use asset_core::domain::conversations::{
    ConversationAdapterPackageOrigin, ConversationAdapterTrustState,
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
        let root = std::env::temp_dir().join(format!("aiw-198-{label}-{}", Uuid::new_v4()));
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

fn write_package_dir(
    dir: &Path,
    package_id: &str,
    adapter_id: &str,
    version: &str,
    script_body: &str,
) {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(dir).expect("create package dir");
    fs::write(
        dir.join("conversation-adapter-package.json"),
        format!(
            r#"{{
  "schema_version": 1,
  "package_id": "{package_id}",
  "name": "Local Update Fixture",
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
  "name": "Local Update Fixture",
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
    fs::write(&executable, script_body).expect("write adapter script");
    let mut permissions = fs::metadata(&executable)
        .expect("adapter metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("make adapter executable");
}

const PROBE_OK: &str =
    "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":1}}'\n";

/// Stage the official adapter packages that #196 stopped embedding in the
/// binary. Production installs them from the market; these catalog assertions
/// only need a realistic untouched managed directory.
fn stage_official_adapter_packages(adapter_root: &Path) {
    for adapter_id in [
        "codex",
        "claude-code",
        "opencode",
        "antigravity",
        "zcode",
        "codebuddy",
    ] {
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../builtin-assets/adapters")
                .join(adapter_id),
            &adapter_root.join(adapter_id),
        );
    }
}

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).expect("create adapter fixture dir");
    for entry in fs::read_dir(source).expect("read adapter fixture dir") {
        let entry = entry.expect("read adapter fixture entry");
        let destination = target.join(entry.file_name());
        if entry.file_type().expect("fixture entry type").is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), &destination).expect("copy adapter fixture file");
        }
    }
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

async fn list_entry(service: &AppService, package_id: &str) -> serde_json::Value {
    let entries = service
        .list_conversation_adapter_packages(ConversationAdapterPackageCatalogParams {
            catalog_url: None,
        })
        .await
        .expect("list conversation adapter packages");
    let entry = entries
        .into_iter()
        .find(|entry| entry.item.package_id() == package_id)
        .unwrap_or_else(|| panic!("catalog entry missing for {package_id}"));
    serde_json::to_value(&entry).expect("serialize catalog entry")
}

fn local_catalog_url() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend root")
        .parent()
        .expect("workspace root")
        .join("builtin-assets/index.json")
        .to_string_lossy()
        .to_string()
}

async fn checked_update_status(service: &AppService, package_id: &str) -> serde_json::Value {
    let statuses = service
        .check_conversation_adapter_package_updates(ConversationAdapterPackageUpdateCheckParams {
            catalog_url: Some(local_catalog_url()),
            force: true,
        })
        .await
        .expect("check conversation adapter package updates");
    let status = statuses
        .into_iter()
        .find(|status| status.package_id == package_id)
        .unwrap_or_else(|| panic!("update status missing for {package_id}"));
    serde_json::to_value(&status).expect("serialize update status")
}

#[tokio::test(flavor = "multi_thread")]
async fn conversation_script_catalog_local_candidate_exposes_package_and_content_hash() {
    let temp = TempHome::new("discover-hashes");
    let adapter_root = temp.adapter_root();
    let candidate = adapter_root.join("com.util6.local-update");
    fs::create_dir_all(&adapter_root).expect("create adapter root");
    write_package_dir(
        &candidate,
        "com.util6.local-update",
        "local-update",
        "1.0.0",
        PROBE_OK,
    );

    let discovered = discover_local_conversation_adapter_packages(&adapter_root)
        .expect("discover local package");
    let item = discovered
        .iter()
        .find(|item| item.package_id() == "com.util6.local-update")
        .expect("discovered local candidate");
    let package_hash = item
        .expected_package_hash
        .as_deref()
        .expect("discovered candidate must expose the package hash");
    let content_hash = item
        .expected_content_hash
        .as_deref()
        .expect("discovered candidate must expose the adapter content hash");
    assert_eq!(package_hash.len(), 64);
    assert_eq!(content_hash.len(), 64);
    assert_ne!(package_hash, content_hash);

    write_package_dir(
        &candidate,
        "com.util6.local-update",
        "local-update",
        "1.0.0",
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":2}}'\n",
    );
    let rediscovered = discover_local_conversation_adapter_packages(&adapter_root)
        .expect("rediscover local package");
    let redisc_item = rediscovered
        .iter()
        .find(|item| item.package_id() == "com.util6.local-update")
        .expect("rediscovered local candidate");
    assert_ne!(
        redisc_item.expected_package_hash.as_deref(),
        Some(package_hash),
        "editing the candidate must change the disk-computed package hash"
    );
    assert_ne!(
        redisc_item.expected_content_hash.as_deref(),
        Some(content_hash),
        "editing the candidate must change the disk-computed content hash"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn conversation_script_catalog_higher_local_version_is_an_available_update() {
    let temp = TempHome::new("higher-version");
    let service = open_service(temp.root().join("app.db")).await;
    let workspace = temp.root().join("workspace");
    let install_dir = workspace.join("local-update");
    write_package_dir(
        &install_dir,
        "com.util6.local-update",
        "local-update",
        "1.0.0",
        PROBE_OK,
    );
    register_local_package(&service, &install_dir).await;

    let before = list_entry(&service, "com.util6.local-update").await;
    assert_eq!(before["status"], "local_registered");
    assert_eq!(before["update_available"], false);
    assert_eq!(before["runtime_ready"], true);

    write_package_dir(
        &temp.adapter_root().join("com.util6.local-update"),
        "com.util6.local-update",
        "local-update",
        "2.0.0",
        PROBE_OK,
    );

    let after = list_entry(&service, "com.util6.local-update").await;
    assert_eq!(
        after["update_available"], true,
        "a newer candidate version in the managed root must surface as an available update"
    );
    assert_eq!(after["status"], "update_available");
    assert_eq!(after["installed_package"]["version"], "1.0.0");

    let checked = checked_update_status(&service, "com.util6.local-update").await;
    assert_eq!(checked["update_available"], true);
    assert_eq!(checked["current_version"], "1.0.0");

    drop(service);
}

#[tokio::test(flavor = "multi_thread")]
async fn conversation_script_catalog_discovered_adapters_stay_untrusted_until_registered() {
    let temp = TempHome::new("builtin-baseline");
    stage_official_adapter_packages(&temp.adapter_root());
    let service = open_service(temp.root().join("app.db")).await;

    let entries = service
        .list_conversation_adapter_packages(ConversationAdapterPackageCatalogParams {
            catalog_url: None,
        })
        .await
        .expect("list conversation adapter packages");
    let discovered = entries
        .iter()
        .filter(|entry| {
            entry.installed_adapter.as_ref().is_some_and(|adapter| {
                adapter.trust_state == ConversationAdapterTrustState::Untrusted
            })
        })
        .collect::<Vec<_>>();
    assert!(
        !discovered.is_empty(),
        "adapters staged in the managed directory must still appear in the catalog"
    );
    for entry in discovered {
        assert_eq!(
            entry.status, "uninstalled",
            "untouched managed-directory adapter {} must require explicit registration",
            entry.item.id
        );
        assert!(
            !entry.update_available,
            "an untrusted projection of the official baseline must not be reported as an update: {}",
            entry.item.id
        );
        assert!(
            !entry.runtime_ready,
            "an untrusted adapter must not be advertised as runnable: {}",
            entry.item.id
        );
        let adapter = entry
            .installed_adapter
            .as_ref()
            .expect("discovered adapter is present");
        assert_eq!(
            adapter.trusted_hash, None,
            "discovery must not self-sign a trusted hash for {}",
            entry.item.id
        );
    }

    drop(service);
}

#[tokio::test(flavor = "multi_thread")]
async fn conversation_script_catalog_same_version_content_change_is_an_available_update() {
    let temp = TempHome::new("same-version");
    let service = open_service(temp.root().join("app.db")).await;
    let workspace = temp.root().join("workspace");
    let install_dir = workspace.join("local-update");
    write_package_dir(
        &install_dir,
        "com.util6.local-update",
        "local-update",
        "1.0.0",
        PROBE_OK,
    );
    register_local_package(&service, &install_dir).await;

    write_package_dir(
        &temp.adapter_root().join("com.util6.local-update"),
        "com.util6.local-update",
        "local-update",
        "1.0.0",
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":9}}'\n",
    );

    let after = list_entry(&service, "com.util6.local-update").await;
    assert_eq!(
        after["update_available"], true,
        "an edited candidate at the same version must surface as an available update"
    );
    assert_eq!(after["status"], "update_available");

    let checked = checked_update_status(&service, "com.util6.local-update").await;
    assert_eq!(
        checked["update_available"], true,
        "the update check must also report the same-version content change"
    );

    drop(service);
}

#[tokio::test(flavor = "multi_thread")]
async fn conversation_script_catalog_tampered_active_snapshot_stays_hash_mismatch() {
    let temp = TempHome::new("tampered");
    let service = open_service(temp.root().join("app.db")).await;
    let active_dir = temp.adapter_root().join("com.util6.tamper-test");
    write_package_dir(
        &active_dir,
        "com.util6.tamper-test",
        "tamper-test",
        "1.0.0",
        PROBE_OK,
    );
    register_local_package(&service, &active_dir).await;
    let registered = service
        .load_conversation_adapter_package("com.util6.tamper-test")
        .await
        .expect("load package")
        .expect("registered package");
    let trusted_before = registered
        .trusted_package_hash
        .clone()
        .expect("registered package has a trusted hash");
    assert_eq!(registered.version, "1.0.0");
    // #199: registration promotes the candidate into an immutable managed snapshot;
    // the active runtime is the snapshot, not the candidate directory.
    let active_snapshot = expand_path(&registered.install_dir).expect("expand install dir");
    assert_ne!(active_snapshot, active_dir);

    fs::write(
        active_snapshot.join("adapter.sh"),
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"type\":\"complete\",\"item\":{\"revision\":99}}'\n",
    )
    .expect("tamper with the active snapshot");

    let entry = list_entry(&service, "com.util6.tamper-test").await;
    assert_eq!(
        entry["status"], "hash_mismatch",
        "a tampered active snapshot must stay a hash mismatch, not an available update"
    );
    assert_eq!(entry["runtime_ready"], false);

    let _ = checked_update_status(&service, "com.util6.tamper-test").await;

    let tampered = service
        .load_conversation_adapter_package("com.util6.tamper-test")
        .await
        .expect("load tampered package")
        .expect("tampered package still registered");
    assert_eq!(
        tampered.trusted_package_hash.as_deref(),
        Some(trusted_before.as_str()),
        "detection must never silently re-trust the tampered snapshot"
    );
    assert_eq!(tampered.version, "1.0.0");

    drop(service);
}
