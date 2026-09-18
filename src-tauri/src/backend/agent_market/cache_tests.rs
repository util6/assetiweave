use super::*;

fn cache_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "assetiweave-agent-market-cache-{label}-{}",
        uuid::Uuid::new_v4()
    ))
}

#[test]
fn atomic_cache_round_trip_preserves_valid_catalog_and_etag() {
    let root = cache_root("round-trip");
    let cache = CatalogCache::new(root.clone());
    let bytes = include_bytes!("../../../../builtin-assets/agent-market/catalog-v1.json");
    cache
        .write_atomic(bytes, Some("fixture-etag"))
        .expect("write catalog cache");
    let (catalog, etag) = cache.read().expect("read catalog cache").expect("cache");
    assert_eq!(catalog.schema, "assetiweave.agent-market/v1");
    assert_eq!(etag.as_deref(), Some("fixture-etag"));
    let metadata = std::fs::read_to_string(&cache.meta_path).expect("cache metadata");
    assert!(metadata.contains("fixture-etag"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn invalid_cache_is_not_used_as_a_catalog() {
    let root = cache_root("invalid");
    let cache = CatalogCache::new(root.clone());
    std::fs::create_dir_all(&root).expect("cache directory");
    std::fs::write(&cache.catalog_path, br#"{"schema":"invalid"}"#).expect("invalid cache");
    assert!(cache.read().is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn older_cache_does_not_mask_newer_bundled_catalog() {
    let bundled = super::super::catalog::bundled_catalog().expect("bundled catalog");
    let mut cached = bundled.clone();
    cached.catalog_version = "2026.08.16.1".to_string();
    for item in &mut cached.items {
        item.core_compatibility.min = "0.5.0".to_string();
        item.core_compatibility.max_exclusive = "0.6.0".to_string();
    }

    let selected = select_active_catalog(bundled.clone(), Some(cached));

    assert_eq!(selected.catalog_version, bundled.catalog_version);
}

#[test]
fn newer_cache_is_selected_even_when_core_range_is_only_observational() {
    let bundled = super::super::catalog::bundled_catalog().expect("bundled catalog");
    let mut cached = bundled.clone();
    cached.catalog_version = "2099.01.02.1".to_string();
    for item in &mut cached.items {
        item.core_compatibility.min = "0.1.0".to_string();
        item.core_compatibility.max_exclusive = "0.2.0".to_string();
    }

    let selected = select_active_catalog(bundled, Some(cached.clone()));

    assert_eq!(selected.catalog_version, cached.catalog_version);
}

#[test]
fn same_revision_different_hash_fails_closed_to_bundled_catalog() {
    let bundled = super::super::catalog::bundled_catalog().expect("bundled catalog");
    let mut cached = bundled.clone();
    cached.items[0].description.push_str(" tampered");

    let selected = select_active_catalog(bundled.clone(), Some(cached));

    assert_eq!(
        catalog_fingerprint(&selected),
        catalog_fingerprint(&bundled)
    );
}

#[test]
fn newer_compatible_cache_beats_bundled_catalog_by_parsed_revision() {
    let bundled = super::super::catalog::bundled_catalog().expect("bundled catalog");
    let mut cached = bundled.clone();
    cached.catalog_version = "2099.01.02.3".to_string();

    let selected = select_active_catalog(bundled, Some(cached.clone()));

    assert_eq!(selected.catalog_version, cached.catalog_version);
}

#[test]
fn catalog_revision_requires_a_real_calendar_date_and_sequence() {
    assert!(CatalogRevision::parse("latest").is_err());
    assert!(CatalogRevision::parse("2026.02.30.1").is_err());
    assert!(CatalogRevision::parse("2026.08.20.1").is_ok());
    assert!(CatalogRevision::parse("2026.08.20").is_err());
}

#[test]
fn catalog_refresh_loopback_rejects_untrusted_host() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let handle = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = r#"{"catalog_version":"2026.08.20.1","items":[]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let result =
        CatalogCache::refresh_from_url_with_client(&client, &format!("http://{addr}/catalog.json"));
    let _ = handle.join();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not allowlisted"));
}

#[test]
fn catalog_cache_uses_reqwest_not_ureq() {
    let source = include_str!("cache.rs");
    assert!(!source.contains(concat!("ur", "eq::")));
}
