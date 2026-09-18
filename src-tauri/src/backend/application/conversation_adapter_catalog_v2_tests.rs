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
