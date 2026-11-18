use super::super::conversation_script_catalog::{
    load_conversation_script_catalog, resolve_conversation_adapter_package_catalog_entries,
};
use super::*;
use uuid::Uuid;

#[test]
fn catalog_v2_rejects_mutable_or_invalid_release_identity() {
    assert!(validate_catalog_package_id("com.util6.codex-session").is_ok());
    assert!(validate_catalog_package_id("../codex").is_err());
    assert!(semver_is_newer("1.1.0", "1.0.9"));
    assert!(!semver_is_newer("1.0.0", "1.0.0"));
}

#[tokio::test(flavor = "multi_thread")]
async fn local_catalog_v2_refresh_caches_history_and_changelog() {
    let root = std::env::temp_dir().join(format!("assetiweave-catalog-v2-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create Catalog v2 test root");
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open service");
    let index_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend root")
        .parent()
        .expect("workspace root")
        .join("builtin-assets/index.json");

    let releases = service
        .refresh_conversation_adapter_catalogs(ConversationAdapterCatalogRefreshParams {
            catalog_url: Some(index_path.to_string_lossy().to_string()),
            force: true,
        })
        .await
        .expect("refresh local Catalog v2");

    let index: Value = serde_json::from_str(&fs::read_to_string(&index_path).unwrap()).unwrap();
    let expected_release_count = index["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|package| {
            let history_path = index_path
                .parent()
                .unwrap()
                .join(package["history_url"].as_str().unwrap());
            let history: Value =
                serde_json::from_str(&fs::read_to_string(history_path).unwrap()).unwrap();
            history["releases"].as_array().unwrap().len()
        })
        .sum::<usize>();
    assert_eq!(releases.len(), expected_release_count);
    assert!(releases
        .iter()
        .all(|release| release.catalog_url == index_path.to_string_lossy()));
    assert!(releases
        .iter()
        .all(|release| !release.changelog_markdown.is_empty()));
    assert!(releases
        .iter()
        .all(|release| release_is_core_compatible(release)));

    drop(service);
    let _ = fs::remove_dir_all(root);
}

#[tokio::test(flavor = "multi_thread")]
async fn exact_compatible_release_can_be_selected_for_install_preview() {
    let root = std::env::temp_dir().join(format!("assetiweave-release-select-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create release selection root");
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open service");
    let index_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend root")
        .parent()
        .expect("workspace root")
        .join("builtin-assets/index.json");

    let preview = service
        .install_conversation_adapter_package(ConversationAdapterPackageInstallParams {
            catalog_url: Some(index_path.to_string_lossy().to_string()),
            package_id: "io.github.util6.codex-session".to_string(),
            version: Some("1.0.1".to_string()),
            dry_run: true,
            yes: false,
        })
        .await
        .expect("preview exact release install");

    assert_eq!(preview["package_id"], "io.github.util6.codex-session");
    assert!(preview["install_path"].as_str().is_some_and(|path| {
        let normalized = path.replace('\\', "/");
        normalized.ends_with("/versions/1.0.1")
    }));

    drop(service);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn catalog_fetch_keeps_not_modified() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let n = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..n])
            .to_lowercase()
            .contains("if-none-match:"));
        stream
            .write_all(b"HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n")
            .unwrap();
        stream.flush().unwrap();
    });
    assert!(matches!(
        fetch_catalog_document(&format!("http://{address}/index.json"), Some("etag-1")).unwrap(),
        CatalogFetchResult::NotModified
    ));
    server.join().unwrap();
}

#[test]
fn catalog_document_uses_reqwest_not_ureq() {
    let source = include_str!("conversation_adapter_catalog_v2.rs");
    assert!(!source.contains(concat!("ur", "eq::")));
}

fn builtin_assets_path(relative: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend root")
        .parent()
        .expect("workspace root")
        .join("builtin-assets")
        .join(relative)
}

/// Every directory under `builtin-assets/adapters` that ships a package manifest
/// is an installable adapter and must therefore be discovered from this listing.
fn builtin_adapter_directories() -> Vec<(String, std::path::PathBuf)> {
    let adapters_root = builtin_assets_path("adapters");
    let mut directories = fs::read_dir(&adapters_root)
        .expect("read builtin-assets/adapters")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join("conversation-adapter-package.json").is_file())
        .collect::<Vec<_>>();
    directories.sort();
    assert!(
        !directories.is_empty(),
        "no conversation adapter package directories found under {}",
        adapters_root.display()
    );
    directories
        .into_iter()
        .map(|path| {
            let adapter_id = path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("adapter directory name is valid UTF-8")
                .to_string();
            (adapter_id, path)
        })
        .collect()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(
        &fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read {} failed: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("parse {} failed: {error}", path.display()))
}

#[test]
fn bundled_catalogs_cover_every_adapter_directory() {
    let mut problems = Vec::new();

    let legacy = load_conversation_script_catalog(None).expect("load bundled Catalog v1");
    let index_text = bundled_catalog_document("index.json").expect("bundled Catalog v2 index");
    let index: CatalogV2Index = serde_json::from_str(index_text).expect("parse bundled Catalog v2");

    for (adapter_id, directory) in builtin_adapter_directories() {
        let manifest = read_json(&directory.join("conversation-adapter-package.json"));
        let package_id = manifest["package_id"]
            .as_str()
            .expect("package_id is a string")
            .to_string();

        if !legacy.items.iter().any(|item| item.id == package_id) {
            problems.push(format!(
                "{package_id}: missing from builtin-assets/catalog.json (v1)"
            ));
        }
        if !index
            .packages
            .iter()
            .any(|package| package.package_id == package_id)
        {
            problems.push(format!(
                "{package_id}: missing from builtin-assets/index.json (v2)"
            ));
        }
        let history_path = format!("history/{package_id}.json");
        match bundled_catalog_document(&history_path) {
            None => problems.push(format!(
                "{package_id}: missing {history_path} from bundled Catalog v2 history"
            )),
            Some(text) => {
                let history: CatalogV2History =
                    serde_json::from_str(text).expect("parse bundled Catalog v2 history");
                if history.adapter_id != adapter_id {
                    problems.push(format!(
                        "{package_id}: history adapter_id {} does not match directory {adapter_id}",
                        history.adapter_id
                    ));
                }
            }
        }
    }

    for item in &legacy.items {
        let adapter_id = item.adapter_id.as_deref().unwrap_or(item.id.as_str());
        if !builtin_assets_path("adapters").join(adapter_id).is_dir() {
            problems.push(format!(
                "{}: Catalog v1 item has no builtin-assets/adapters/{adapter_id} directory",
                item.id
            ));
        }
    }
    for package in &index.packages {
        let history_text =
            bundled_catalog_document(&format!("history/{}.json", package.package_id))
                .unwrap_or_else(|| {
                    panic!(
                        "{}: Catalog v2 package has no bundled history",
                        package.package_id
                    )
                });
        let history: CatalogV2History =
            serde_json::from_str(history_text).expect("parse bundled Catalog v2 history");
        if !builtin_assets_path("adapters")
            .join(&history.adapter_id)
            .is_dir()
        {
            problems.push(format!(
                "{}: Catalog v2 package has no builtin-assets/adapters/{} directory",
                package.package_id, history.adapter_id
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "bundled catalogs do not cover every builtin adapter:\n- {}",
        problems.join("\n- ")
    );
}

#[test]
fn offline_catalog_lists_every_adapter_directory_package() {
    let entries = resolve_conversation_adapter_package_catalog_entries(
        load_conversation_script_catalog(None)
            .expect("load bundled Catalog v1")
            .items,
        &[],
        &[],
    );
    let mut listed = entries
        .iter()
        .map(|entry| entry.item.id.clone())
        .collect::<Vec<_>>();
    listed.sort();
    let mut expected = builtin_adapter_directories()
        .into_iter()
        .map(|(_, directory)| {
            read_json(&directory.join("conversation-adapter-package.json"))["package_id"]
                .as_str()
                .expect("package_id is a string")
                .to_string()
        })
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(
        listed, expected,
        "offline catalog listing must mirror every builtin adapter directory"
    );

    for (package_id, adapter_id) in [
        ("io.github.util6.antigravity-session", "antigravity"),
        ("io.github.util6.codebuddy-session", "codebuddy"),
    ] {
        let entry = entries
            .iter()
            .find(|entry| entry.item.id == package_id)
            .unwrap_or_else(|| panic!("{package_id} missing from the offline catalog listing"));
        assert_eq!(entry.item.adapter_key(), adapter_id);
        assert_eq!(entry.status, "not_installed");
    }
}

#[test]
fn bundled_catalog_entries_match_adapter_manifests() {
    let mut problems = Vec::new();
    let legacy = load_conversation_script_catalog(None).expect("load bundled Catalog v1");
    let index: CatalogV2Index =
        serde_json::from_str(bundled_catalog_document("index.json").expect("bundled index"))
            .expect("parse bundled Catalog v2");

    for (adapter_id, directory) in builtin_adapter_directories() {
        let manifest = read_json(&directory.join("conversation-adapter-package.json"));
        let package_id = manifest["package_id"].as_str().unwrap().to_string();
        let version = manifest["version"].as_str().unwrap().to_string();
        let record_kind = manifest["record_kind"].as_str().unwrap().to_string();
        let adapter_manifest_file = manifest["adapter_manifest"].as_str().unwrap().to_string();
        let adapter_manifest = read_json(&directory.join(&adapter_manifest_file));
        if adapter_manifest["id"].as_str() != Some(adapter_id.as_str()) {
            problems.push(format!(
                "{package_id}: adapter manifest id {} does not match directory {adapter_id}",
                adapter_manifest["id"]
            ));
        }
        if adapter_manifest["version"].as_str() != Some(version.as_str()) {
            problems.push(format!(
                "{package_id}: adapter manifest version {} does not match package version {version}",
                adapter_manifest["version"]
            ));
        }

        let Some(item) = legacy.items.iter().find(|item| item.id == package_id) else {
            continue;
        };
        if item.version != version {
            problems.push(format!(
                "{package_id}: Catalog v1 version {} != package manifest {version}",
                item.version
            ));
        }
        if item.adapter_id.as_deref() != Some(adapter_id.as_str()) {
            problems.push(format!(
                "{package_id}: Catalog v1 adapter_id {:?} != directory {adapter_id}",
                item.adapter_id
            ));
        }
        let v1_record_kind = item.record_kind.as_package_record_kind();
        let package_record_kind = serde_json::from_value::<ConversationAdapterPackageRecordKind>(
            Value::String(record_kind.clone()),
        )
        .expect("record_kind is a valid enum value");
        if v1_record_kind != package_record_kind {
            problems.push(format!(
                "{package_id}: Catalog v1 record_kind {v1_record_kind:?} != package manifest {package_record_kind:?}"
            ));
        }

        let Some(package) = index
            .packages
            .iter()
            .find(|package| package.package_id == package_id)
        else {
            continue;
        };
        if package.stable_version.as_deref() != Some(version.as_str()) {
            problems.push(format!(
                "{package_id}: Catalog v2 stable_version {:?} != package manifest {version}",
                package.stable_version
            ));
        }
        let history_text = bundled_catalog_document(&format!("history/{package_id}.json"))
            .expect("bundled Catalog v2 history");
        let history: CatalogV2History =
            serde_json::from_str(history_text).expect("parse bundled Catalog v2 history");
        if history.record_kind != package_record_kind {
            problems.push(format!(
                "{package_id}: Catalog v2 history record_kind {:?} != package manifest {package_record_kind:?}",
                history.record_kind
            ));
        }
        if history.name != manifest["name"].as_str().unwrap() {
            problems.push(format!(
                "{package_id}: Catalog v2 history name {:?} != package manifest {:?}",
                history.name, manifest["name"]
            ));
        }
        let Some(release) = history
            .releases
            .iter()
            .find(|release| release.version == version)
        else {
            problems.push(format!(
                "{package_id}: Catalog v2 history has no {version} release"
            ));
            continue;
        };
        if release.runtime_protocol != "stdio-ndjson-v1" {
            problems.push(format!(
                "{package_id}: release {version} runtime_protocol {} is not stdio-ndjson-v1",
                release.runtime_protocol
            ));
        }
        if release.artifact_sha256.len() != 64
            || !release
                .artifact_sha256
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            problems.push(format!(
                "{package_id}: release {version} artifact_sha256 {:?} is not a 64 character hex digest",
                release.artifact_sha256
            ));
        }
        let expected_artifact = format!("{package_id}-{version}-universal.zip");
        if !release.artifact_url.ends_with(&expected_artifact) {
            problems.push(format!(
                "{package_id}: release {version} artifact_url {} does not end with {expected_artifact}",
                release.artifact_url
            ));
        }
        let source_url = serde_json::from_str::<Value>(history_text)
            .expect("parse bundled Catalog v2 history json")
            .get("releases")
            .and_then(Value::as_array)
            .and_then(|releases| {
                releases
                    .iter()
                    .find(|release| release["version"].as_str() == Some(version.as_str()))
            })
            .and_then(|release| release["source"]["url"].as_str())
            .map(str::to_string)
            .unwrap_or_default();
        if !source_url.ends_with(&format!("/builtin-assets/adapters/{adapter_id}")) {
            problems.push(format!(
                "{package_id}: release {version} source url {source_url} does not point at builtin-assets/adapters/{adapter_id}"
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "bundled catalog entries disagree with adapter manifests:\n- {}",
        problems.join("\n- ")
    );
}

#[test]
fn bundled_catalog_hashes_match_adapter_directory_contents() {
    use crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir;

    let mut problems = Vec::new();
    let legacy = load_conversation_script_catalog(None).expect("load bundled Catalog v1");

    for (_, directory) in builtin_adapter_directories() {
        let validation = validate_conversation_adapter_package_dir(&directory)
            .unwrap_or_else(|error| panic!("validate {} failed: {error}", directory.display()));
        let package_id = validation.manifest.package_id.clone();
        let Some(item) = legacy.items.iter().find(|item| item.id == package_id) else {
            continue;
        };
        if item.expected_package_hash.as_deref() != Some(validation.content_hash.as_str()) {
            problems.push(format!(
                "{package_id}: expected_package_hash {:?} != actual package directory hash {}",
                item.expected_package_hash, validation.content_hash
            ));
        }
        let adapter_hash = validation.adapter_validation.content_hash.as_str();
        if item.expected_content_hash.as_deref() != Some(adapter_hash) {
            problems.push(format!(
                "{package_id}: expected_content_hash {:?} != actual adapter content hash {adapter_hash}",
                item.expected_content_hash
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "bundled Catalog v1 checksums are stale (recompute them from the adapter directories):\n- {}",
        problems.join("\n- ")
    );
}
