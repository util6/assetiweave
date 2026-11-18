//! #200：更新与安装优先本地待晋升工作区。
//!
//! 这些用例走 `AppService` 公共接缝，必须在独立测试进程里运行：`ASSETIWEAVE_HOME`
//! 是进程级环境变量，放在单元测试模块里会与其他并行用例互相踩踏，也会污染真实
//! `~/.assetiweave`。

use asset_core::application::conversations::conversation_adapter_installer::install_conversation_adapter_package_from_spec;
use asset_core::application::{
    AppService, ConversationAdapterLocalRegisterParams, ConversationAdapterPackageCatalogParams,
    ConversationAdapterPackageInstallParams,
};
use asset_core::domain::conversations::{
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
};
use asset_core::infrastructure::conversations::{
    validate_conversation_adapter_package_dir, ConversationAdapterPackageInstallSource,
    ConversationAdapterPackageInstallSourceKind, ConversationAdapterPackageInstallSpec,
};
use asset_core::infrastructure::runtime::RuntimeRole;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;

static HOME_LOCK: Mutex<()> = Mutex::new(());

const PACKAGE_ID: &str = "com.util6.promote-local";
const ADAPTER_ID: &str = "promote-local";

struct TempHome {
    root: PathBuf,
    previous: Option<OsString>,
    _guard: MutexGuard<'static, ()>,
}

impl TempHome {
    fn new(label: &str) -> Self {
        let guard = HOME_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let root = std::env::temp_dir().join(format!("aiw-200-{label}-{}", Uuid::new_v4()));
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

    fn adapter_root(&self) -> PathBuf {
        self.root.join("home").join("conversation-adapters")
    }

    fn package_root(&self) -> PathBuf {
        self.adapter_root().join("packages").join(PACKAGE_ID)
    }

    /// 必然不可达的目录文件路径：任何远端/目录读取尝试都会直接失败。
    fn missing_catalog_url(&self) -> String {
        self.root
            .join("missing-catalog.json")
            .to_string_lossy()
            .to_string()
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
  "name": "Local Promotion Fixture",
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
  "name": "Local Promotion Fixture",
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

fn probe_script(revision: u32) -> String {
    format!(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{{\"type\":\"complete\",\"item\":{{\"revision\":{revision}}}}}'\n"
    )
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

fn update_params(
    temp: &TempHome,
    version: Option<&str>,
) -> ConversationAdapterPackageInstallParams {
    ConversationAdapterPackageInstallParams {
        catalog_url: Some(temp.missing_catalog_url()),
        package_id: PACKAGE_ID.to_string(),
        version: version.map(str::to_string),
        dry_run: false,
        yes: true,
    }
}

fn snapshot_name(install_dir: &str) -> String {
    Path::new(install_dir)
        .file_name()
        .and_then(|value| value.to_str())
        .expect("snapshot directory name")
        .to_string()
}

fn version_snapshot_dirs(package_root: &Path) -> Vec<String> {
    let versions = package_root.join("versions");
    if !versions.is_dir() {
        return Vec::new();
    }
    let mut names = fs::read_dir(&versions)
        .expect("read versions dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect::<Vec<_>>();
    names.sort();
    names
}

async fn active_state(service: &AppService, temp: &TempHome) -> (String, usize) {
    let package = service
        .load_conversation_adapter_package(PACKAGE_ID)
        .await
        .expect("load package")
        .expect("registered package");
    (
        package.install_dir,
        version_snapshot_dirs(&temp.package_root()).len(),
    )
}

async fn trusted_package_hash(service: &AppService) -> String {
    service
        .load_conversation_adapter_package(PACKAGE_ID)
        .await
        .expect("load package")
        .expect("registered package")
        .trusted_package_hash
        .expect("package must carry a trusted hash")
}

fn disk_content_hash(install_dir: &Path) -> String {
    validate_conversation_adapter_package_dir(install_dir)
        .expect("validate snapshot")
        .content_hash
}

#[tokio::test(flavor = "multi_thread")]
async fn update_promotes_pending_local_workspace_without_touching_the_remote_catalog() {
    let temp = TempHome::new("update-promotes");
    let service = open_service(temp.root().join("app.db")).await;
    let workspace = temp.root().join("workspace").join(ADAPTER_ID);
    write_package_dir(
        &workspace,
        PACKAGE_ID,
        ADAPTER_ID,
        "1.0.0",
        &probe_script(1),
    );
    register_local_package(&service, &workspace).await;

    let candidate = temp.adapter_root().join(ADAPTER_ID);
    write_package_dir(
        &candidate,
        PACKAGE_ID,
        ADAPTER_ID,
        "1.0.0",
        &probe_script(2),
    );

    let first = service
        .update_conversation_adapter_package(update_params(&temp, None))
        .await
        .expect("update from the pending local workspace");
    assert_eq!(first["source"], "local_workspace");
    assert_eq!(first["upgraded"], true);
    let first_install = first["package"]["install_dir"]
        .as_str()
        .expect("first install dir")
        .to_string();
    let first_name = snapshot_name(&first_install);
    let first_hash = first_name
        .strip_prefix("1.0.0-")
        .expect("snapshot must be named <version>-<hash12>");
    assert_eq!(first_hash.len(), 12, "snapshot hash suffix is 12 chars");
    assert!(first_hash.chars().all(|value| value.is_ascii_hexdigit()));
    assert_eq!(
        trusted_package_hash(&service).await,
        disk_content_hash(Path::new(&first_install)),
        "the trusted hash must be the promoted snapshot's disk hash"
    );
    assert_eq!(
        version_snapshot_dirs(&temp.package_root()),
        vec![first_name.clone()],
        "only the active snapshot may remain after promotion"
    );

    write_package_dir(
        &candidate,
        PACKAGE_ID,
        ADAPTER_ID,
        "1.0.0",
        &probe_script(3),
    );
    let second = service
        .update_conversation_adapter_package(update_params(&temp, None))
        .await
        .expect("update from the edited local workspace");
    assert_eq!(second["source"], "local_workspace");
    let second_install = second["package"]["install_dir"]
        .as_str()
        .expect("second install dir")
        .to_string();
    assert_ne!(second_install, first_install);
    assert!(
        !Path::new(&first_install).exists(),
        "the previous snapshot must be cleaned up"
    );
    assert_eq!(
        version_snapshot_dirs(&temp.package_root()),
        vec![snapshot_name(&second_install)],
        "exactly one immutable snapshot may remain"
    );
    assert_eq!(
        trusted_package_hash(&service).await,
        disk_content_hash(Path::new(&second_install))
    );

    let entry = service
        .list_conversation_adapter_packages(ConversationAdapterPackageCatalogParams {
            catalog_url: None,
        })
        .await
        .expect("list packages after promotion")
        .into_iter()
        .find(|entry| entry.item.package_id() == PACKAGE_ID)
        .expect("promoted package must be visible in the runtime catalog");
    assert_eq!(entry.status, "local_registered");
    assert!(entry.runtime_ready);
    assert_eq!(
        entry.installed_package.expect("installed package").version,
        "1.0.0"
    );

    drop(service);
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_promotion_keeps_the_active_snapshot_and_leaves_no_prepared_workspace() {
    let temp = TempHome::new("failed-promotion");
    let service = open_service(temp.root().join("app.db")).await;
    let workspace = temp.root().join("workspace").join(ADAPTER_ID);
    write_package_dir(
        &workspace,
        PACKAGE_ID,
        ADAPTER_ID,
        "1.0.0",
        &probe_script(1),
    );
    register_local_package(&service, &workspace).await;
    // #199 routes registration through workspace promotion, so the first install
    // already owns an immutable snapshot. Capture that state instead of assuming
    // the source directory stays active.
    let (install_before, snapshots_before) = active_state(&service, &temp).await;
    let trusted_before = trusted_package_hash(&service).await;

    let candidate = temp.adapter_root().join(ADAPTER_ID);
    write_package_dir(
        &candidate,
        PACKAGE_ID,
        ADAPTER_ID,
        "1.0.0",
        "#!/bin/sh\ncat >/dev/null\nprintf 'invalid\\n'\n",
    );

    let error = service
        .update_conversation_adapter_package(update_params(&temp, None))
        .await
        .expect_err("a broken local workspace must not be promoted");
    assert!(
        error.to_string().contains("probe"),
        "the promotion failure must surface the probe error: {error}"
    );

    let package = service
        .load_conversation_adapter_package(PACKAGE_ID)
        .await
        .expect("load package")
        .expect("package must remain registered");
    let (install_after, snapshots_after) = active_state(&service, &temp).await;
    assert_eq!(
        install_after, install_before,
        "a failed promotion must keep the previously active snapshot"
    );
    assert_eq!(
        snapshots_after, snapshots_before,
        "a failed promotion must not add a new version snapshot"
    );
    assert_eq!(
        package.trusted_package_hash.as_deref(),
        Some(trusted_before.as_str())
    );
    let prepared = temp.package_root().join("prepared");
    assert!(
        !prepared.exists()
            || fs::read_dir(&prepared)
                .expect("read prepared dir")
                .next()
                .is_none(),
        "a failed promotion must clean up prepared/ workspaces"
    );

    drop(service);
}

#[tokio::test(flavor = "multi_thread")]
async fn install_prefers_a_local_workspace_over_the_remote_catalog() {
    let temp = TempHome::new("install-promotes");
    let service = open_service(temp.root().join("app.db")).await;
    let candidate = temp.adapter_root().join(ADAPTER_ID);
    write_package_dir(
        &candidate,
        PACKAGE_ID,
        ADAPTER_ID,
        "1.0.0",
        &probe_script(1),
    );

    let result = service
        .install_conversation_adapter_package(update_params(&temp, None))
        .await
        .expect("install from the local workspace");
    assert_eq!(result["source"], "local_workspace");
    let install_dir = result["package"]["install_dir"]
        .as_str()
        .expect("install dir")
        .to_string();
    assert!(
        snapshot_name(&install_dir).starts_with("1.0.0-"),
        "install must activate an immutable <version>-<hash12> snapshot"
    );

    let package = service
        .load_conversation_adapter_package(PACKAGE_ID)
        .await
        .expect("load package")
        .expect("installed package");
    assert_eq!(
        package.origin,
        ConversationAdapterPackageOrigin::LocalDirectory
    );
    assert_eq!(package.version, "1.0.0");
    assert_eq!(
        package.trusted_package_hash.as_deref(),
        Some(disk_content_hash(Path::new(&install_dir)).as_str())
    );

    drop(service);
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_version_still_goes_through_the_remote_release_catalog() {
    let temp = TempHome::new("explicit-version");
    let service = open_service(temp.root().join("app.db")).await;
    let workspace = temp.root().join("workspace").join(ADAPTER_ID);
    write_package_dir(
        &workspace,
        PACKAGE_ID,
        ADAPTER_ID,
        "1.0.0",
        &probe_script(1),
    );
    register_local_package(&service, &workspace).await;
    let original_install = service
        .load_conversation_adapter_package(PACKAGE_ID)
        .await
        .expect("load package")
        .expect("registered package")
        .install_dir;
    let snapshots_before = version_snapshot_dirs(&temp.package_root()).len();

    let candidate = temp.adapter_root().join(ADAPTER_ID);
    write_package_dir(
        &candidate,
        PACKAGE_ID,
        ADAPTER_ID,
        "1.0.0",
        &probe_script(2),
    );

    let error = service
        .update_conversation_adapter_package(update_params(&temp, Some("1.0.0")))
        .await
        .expect_err("an unreachable release catalog must fail the explicit-version update");
    let message = error.to_string();
    assert!(
        !message.contains("workspace") && !message.contains("promote"),
        "explicit version must not fall back to local promotion: {message}"
    );
    assert_eq!(
        version_snapshot_dirs(&temp.package_root()).len(),
        snapshots_before,
        "a pinned version must not add a local promotion snapshot"
    );
    let package = service
        .load_conversation_adapter_package(PACKAGE_ID)
        .await
        .expect("load package")
        .expect("registered package");
    assert_eq!(package.install_dir, original_install);

    drop(service);
}

#[tokio::test(flavor = "multi_thread")]
async fn managed_release_install_without_expected_checksum_refuses_to_self_sign_trust() {
    let temp = TempHome::new("no-self-sign");
    let service = open_service(temp.root().join("app.db")).await;
    let spec = ConversationAdapterPackageInstallSpec {
        id: PACKAGE_ID.to_string(),
        name: "Local Promotion Fixture".to_string(),
        version: "1.0.0".to_string(),
        record_kind: ConversationAdapterPackageRecordKind::Session,
        provider: None,
        adapter_id: Some(ADAPTER_ID.to_string()),
        description: None,
        homepage_url: None,
        repository_url: None,
        tags: Vec::new(),
        manifest_file: None,
        package_manifest_file: None,
        expected_content_hash: None,
        expected_package_hash: None,
        expected_artifact_hash: None,
        artifact_size: None,
        source: ConversationAdapterPackageInstallSource {
            kind: ConversationAdapterPackageInstallSourceKind::Github,
            url: "https://github.com/util6/assetiweave".to_string(),
            branch: None,
            path: None,
        },
    };

    let error = install_conversation_adapter_package_from_spec(&service, &spec, false, None)
        .await
        .expect_err("a managed release without an expected checksum must be rejected");
    assert!(
        error.to_string().contains("checksum"),
        "the error must explain the missing expected checksum: {error}"
    );
    assert!(
        service
            .load_conversation_adapter_package(PACKAGE_ID)
            .await
            .expect("load package")
            .is_none(),
        "a rejected install must not persist a self-signed trusted hash"
    );

    drop(service);
}
