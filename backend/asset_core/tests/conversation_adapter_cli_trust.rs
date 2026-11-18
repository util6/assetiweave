//! #202：register / upgrade 统一为显式信任晋升。
//!
//! 这些用例走 `AppService` 公共接缝，并必须在独立测试进程里运行：
//! `ASSETIWEAVE_HOME` 与当前工作目录都是进程级状态，放进单元测试会互相踩踏，
//! 也会污染真实 `~/.assetiweave`。

use asset_core::application::{AppService, ConversationAdapterWorkspaceUpgradeParams};
use asset_core::infrastructure::runtime::RuntimeRole;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;

static HOME_LOCK: Mutex<()> = Mutex::new(());

struct TempWorkspace {
    root: PathBuf,
    previous_home: Option<OsString>,
    previous_cwd: PathBuf,
    _guard: MutexGuard<'static, ()>,
}

impl TempWorkspace {
    fn new() -> Self {
        let guard = HOME_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let root = std::env::temp_dir().join(format!("aiw-202-trust-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("home")).expect("create temp home");
        let previous_home = std::env::var_os("ASSETIWEAVE_HOME");
        std::env::set_var("ASSETIWEAVE_HOME", root.join("home"));
        let previous_cwd = std::env::current_dir().expect("read current dir");
        Self {
            root,
            previous_home,
            previous_cwd,
            _guard: guard,
        }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn adapter_root(&self) -> PathBuf {
        self.home().join("conversation-adapters")
    }

    fn repo(&self) -> PathBuf {
        self.root.join("repo")
    }

    fn db_path(&self) -> PathBuf {
        self.home().join("app.db")
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous_cwd);
        match self.previous_home.take() {
            Some(value) => std::env::set_var("ASSETIWEAVE_HOME", value),
            None => std::env::remove_var("ASSETIWEAVE_HOME"),
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn probe_script(revision: u32) -> String {
    format!(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{{\"type\":\"complete\",\"item\":{{\"revision\":{revision}}}}}'\n"
    )
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
  "adapter_id": "{adapter_id}",
  "name": "Trust Fixture {adapter_id}",
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
  "name": "Trust Fixture {adapter_id}",
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

fn write_repo_adapter(workspace: &TempWorkspace, adapter_id: &str, version: &str) -> PathBuf {
    let package_id = format!("com.example.{adapter_id}");
    let dir = workspace
        .repo()
        .join("builtin-assets")
        .join("adapters")
        .join(adapter_id);
    write_package_dir(&dir, &package_id, adapter_id, version, &probe_script(1));
    dir
}

fn write_common_scripts(workspace: &TempWorkspace, revision: &str) {
    let common = workspace
        .repo()
        .join("builtin-assets")
        .join("adapters")
        .join("common");
    fs::create_dir_all(&common).expect("create common dir");
    for name in [
        "payload-policy.mjs",
        "reproject-runner.mjs",
        "shell-projector.cjs",
        "projector-adapter.cjs",
        "projector-manifest.json",
    ] {
        fs::write(common.join(name), format!("// {revision} {name}\n"))
            .expect("write common helper");
    }
}

fn tree_snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut snapshot = BTreeMap::new();
    collect_tree(root, root, &mut snapshot);
    snapshot
}

fn collect_tree(root: &Path, current: &Path, snapshot: &mut BTreeMap<String, Vec<u8>>) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .expect("tree entry under root")
            .to_string_lossy()
            .to_string();
        if path.is_dir() {
            snapshot.insert(format!("{relative}/"), Vec::new());
            collect_tree(root, &path, snapshot);
        } else {
            snapshot.insert(relative, fs::read(&path).expect("read tree file"));
        }
    }
}

fn file_bytes(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn developer_sync_and_user_upgrade_share_one_trusted_promotion_path() {
    let workspace = TempWorkspace::new();
    let adapter_root = workspace.adapter_root();

    write_common_scripts(&workspace, "repo-v1");
    write_repo_adapter(&workspace, "acme-dev", "1.0.0");
    write_repo_adapter(&workspace, "beta-dev", "1.0.0");

    // A managed package that the developer sync must not touch.
    let keep_me = adapter_root.join("keep-me");
    write_package_dir(
        &keep_me,
        "com.example.keep-me",
        "keep-me",
        "1.0.0",
        &probe_script(1),
    );
    fs::write(keep_me.join("USER-MARKER"), b"keep my edits").expect("write marker");
    let keep_me_before = tree_snapshot(&keep_me);
    let canonical_managed = fs::canonicalize(&adapter_root).expect("canonical managed root");

    std::env::set_current_dir(workspace.repo()).expect("enter repository root");
    let runtime = AppService::bootstrap_runtime(workspace.db_path(), RuntimeRole::OneShot)
        .await
        .expect("bootstrap runtime");
    let service = AppService::from_runtime(&runtime);

    // 1. upgrade -d: sync repository scripts into the managed root, then promote.
    let upgraded = service
        .upgrade_conversation_adapter_workspace(ConversationAdapterWorkspaceUpgradeParams {
            package_dir: None,
            developer: true,
            dry_run: false,
        })
        .await
        .expect("developer upgrade");
    assert_eq!(upgraded["developer_synced"], true);
    let upgraded_items = upgraded["upgraded"].as_array().expect("upgraded array");
    assert_eq!(upgraded_items.len(), 2, "both repository adapters promote");
    for item in upgraded_items {
        assert_eq!(item["upgraded"], true);
        assert_eq!(item["activated_version"], "1.0.0");
        assert_eq!(
            item["package"]["origin"].as_str(),
            Some("local_directory"),
            "the snapshot origin must not grant developer-only trust"
        );
        let snapshot = PathBuf::from(item["snapshot_path"].as_str().expect("snapshot path"));
        assert!(snapshot.is_dir(), "snapshot must exist on disk");
        assert!(fs::canonicalize(&snapshot)
            .expect("canonical snapshot")
            .starts_with(canonical_managed.join("packages")));
        let source = PathBuf::from(item["source_dir"].as_str().expect("source dir"));
        assert!(
            fs::canonicalize(&source)
                .expect("canonical source")
                .starts_with(&canonical_managed),
            "promotion must read from the synchronized managed copy, got {source:?}"
        );
    }

    assert!(adapter_root.join("acme-dev").join("adapter.sh").is_file());
    assert!(adapter_root.join("beta-dev").join("adapter.sh").is_file());
    assert_eq!(
        fs::read_to_string(adapter_root.join("common").join("payload-policy.mjs"))
            .expect("read synced common helper"),
        "// repo-v1 payload-policy.mjs\n",
        "developer sync must refresh the authoritative common helpers"
    );
    assert_eq!(
        tree_snapshot(&keep_me),
        keep_me_before,
        "a developer sync must not overwrite managed packages outside the synced set"
    );

    let adapters = service.list_conversation_adapters().expect("list adapters");
    for id in ["acme-dev", "beta-dev"] {
        let adapter = adapters
            .iter()
            .find(|adapter| adapter.id == id)
            .unwrap_or_else(|| panic!("{id} must be visible after promotion"));
        assert_eq!(adapter.version, "1.0.0");
    }

    let first_acme = service
        .load_conversation_adapter_package_by_adapter("acme-dev")
        .await
        .expect("load acme package")
        .expect("acme package is active");
    let first_snapshot = PathBuf::from(first_acme.install_dir.clone());
    let first_hash = first_acme
        .trusted_package_hash
        .clone()
        .expect("trusted hash");

    // 2. Tamper with the managed workspace and promote the same version again.
    write_package_dir(
        &adapter_root.join("acme-dev"),
        "com.example.acme-dev",
        "acme-dev",
        "1.0.0",
        &probe_script(2),
    );
    let tampered = service
        .upgrade_conversation_adapter_workspace(ConversationAdapterWorkspaceUpgradeParams {
            package_dir: Some("acme-dev".to_string()),
            developer: false,
            dry_run: false,
        })
        .await
        .expect("user upgrade after tampering");
    let tampered_item = &tampered["upgraded"][0];
    assert_eq!(tampered_item["upgraded"], true);
    assert_eq!(tampered_item["activated_version"], "1.0.0");
    let second_hash = tampered_item["activated_hash"]
        .as_str()
        .expect("activated hash");
    assert_ne!(second_hash, first_hash, "content hash must change");
    let second_snapshot = PathBuf::from(
        tampered_item["snapshot_path"]
            .as_str()
            .expect("second snapshot"),
    );
    assert_ne!(second_snapshot, first_snapshot);
    assert!(second_snapshot.is_dir());
    assert!(
        !first_snapshot.exists(),
        "workspace promotion keeps only the active snapshot"
    );
    assert!(
        second_snapshot
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(&second_hash[..12])),
        "snapshot directory must embed <version>-<hash12>"
    );

    // 3. Reserved managed directories and hidden transfer dirs never become workspaces.
    for reserved in ["common", "runtime", ".hidden-transfer"] {
        let dir = adapter_root.join(reserved);
        fs::create_dir_all(&dir).expect("create reserved dir");
        fs::write(dir.join("conversation-adapter-package.json"), "{}")
            .expect("write invalid package marker");
    }
    let scanned = service
        .upgrade_conversation_adapter_workspace(ConversationAdapterWorkspaceUpgradeParams {
            package_dir: None,
            developer: false,
            dry_run: false,
        })
        .await
        .expect("user upgrade skips reserved directories instead of failing on them");
    for item in scanned["upgraded"].as_array().expect("scanned array") {
        let source = PathBuf::from(item["source_dir"].as_str().expect("source dir"));
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        assert!(
            !matches!(name, "packages" | "staging" | "common" | "runtime")
                && !name.starts_with('.'),
            "reserved entry {name} must never be promoted"
        );
    }

    // 4. Dry-run must not sync, promote, or write package rows.
    let gamma_source = write_repo_adapter(&workspace, "gamma-dev", "1.0.0");
    let managed_before = tree_snapshot(&adapter_root);
    let db_before = file_bytes(&workspace.db_path());
    let wal_before = file_bytes(&workspace.home().join("app.db-wal"));
    let dry_run = service
        .upgrade_conversation_adapter_workspace(ConversationAdapterWorkspaceUpgradeParams {
            package_dir: None,
            developer: true,
            dry_run: true,
        })
        .await
        .expect("developer dry-run");
    assert_eq!(dry_run["dry_run"], true);
    assert_eq!(dry_run["developer_synced"], false);
    let preview = dry_run["upgraded"]
        .as_array()
        .expect("preview array")
        .iter()
        .find(|item| item["adapter_id"] == "gamma-dev")
        .expect("gamma-dev preview");
    assert!(preview["snapshot_path"].as_str().is_some());
    assert_eq!(
        preview["content_hash"].as_str().map(str::len),
        Some(64),
        "dry-run must report the content hash it would trust"
    );
    assert!(
        preview["preflight"].is_object(),
        "dry-run reports preflight"
    );
    assert!(
        !adapter_root.join("gamma-dev").exists(),
        "dry-run must not synchronize repository scripts"
    );
    assert!(
        !adapter_root
            .join("packages")
            .join("com.example.gamma-dev")
            .exists(),
        "dry-run must not create a snapshot"
    );
    assert_eq!(
        tree_snapshot(&adapter_root),
        managed_before,
        "dry-run must leave the managed directory unchanged"
    );
    assert_eq!(
        file_bytes(&workspace.db_path()),
        db_before,
        "dry-run must not write the package database"
    );
    assert_eq!(
        file_bytes(&workspace.home().join("app.db-wal")),
        wal_before,
        "dry-run must not write the package database WAL"
    );
    assert!(
        service
            .load_conversation_adapter_package_by_adapter("gamma-dev")
            .await
            .expect("load gamma package")
            .is_none(),
        "dry-run must not create a package row"
    );
    assert!(
        service
            .list_conversation_adapters()
            .expect("list adapters")
            .iter()
            .all(|adapter| adapter.id != "gamma-dev"),
        "dry-run must not register the previewed adapter"
    );

    // 5. register still requires explicit confirmation in user mode.
    let error = service
        .register_conversation_adapter_dev(
            Some(gamma_source.to_string_lossy().to_string()),
            false,
            false,
            false,
        )
        .await
        .expect_err("register without --yes must be rejected");
    assert!(
        error.to_string().contains("confirmation"),
        "unexpected error: {error}"
    );

    // 6. register -d shares the same sync-then-promote behavior as upgrade -d.
    let registered = service
        .register_conversation_adapter_dev(None, true, false, true)
        .await
        .expect("developer register with --yes");
    assert_eq!(registered["developer_synced"], true);
    assert!(adapter_root.join("gamma-dev").join("adapter.sh").is_file());
    let adapters = service.list_conversation_adapters().expect("list adapters");
    assert!(
        adapters.iter().any(|adapter| adapter.id == "gamma-dev"),
        "register -d must promote the synchronized adapter"
    );

    // 7. A failing sync leaves no half-written workspaces or transfer leftovers.
    let acme_before_failure = tree_snapshot(&adapter_root.join("acme-dev"));
    let broken = workspace
        .repo()
        .join("builtin-assets")
        .join("adapters")
        .join("broken-dev");
    fs::create_dir_all(&broken).expect("create broken adapter dir");
    fs::write(
        broken.join("conversation-adapter-package.json"),
        "{\"schema_version\":1}",
    )
    .expect("write broken package manifest");
    service
        .upgrade_conversation_adapter_workspace(ConversationAdapterWorkspaceUpgradeParams {
            package_dir: None,
            developer: true,
            dry_run: false,
        })
        .await
        .expect_err("invalid repository adapter must abort the sync");
    assert!(!adapter_root.join("broken-dev").exists());
    assert_eq!(
        tree_snapshot(&adapter_root.join("acme-dev")),
        acme_before_failure,
        "a failed sync must leave already-managed workspaces untouched"
    );
    let leftover_transfer = fs::read_dir(&adapter_root)
        .expect("read managed root")
        .flatten()
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".dev-sync-")
        });
    assert!(
        !leftover_transfer,
        "a failed sync must remove its hidden transfer directory"
    );

    drop(service);
}
