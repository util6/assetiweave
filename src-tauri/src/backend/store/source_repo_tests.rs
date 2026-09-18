use super::*;
use crate::backend::models::SourceKind;
use crate::backend::store::Database;
use uuid::Uuid;

#[tokio::test]
async fn sqlx_source_repo_upserts_lists_and_filters_skill_sources() {
    let db_path =
        std::env::temp_dir().join(format!("assetiweave-source-sqlx-{}.sqlite", Uuid::new_v4()));
    let database = Database::open_async(&db_path).await.expect("open database");
    let regular_source = test_source("regular", SourceScannerKind::Mixed);
    let skill_source = test_source("skill", SourceScannerKind::Skill);

    upsert_source_sqlx(database.pool(), "default", &regular_source)
        .await
        .expect("upsert regular source");
    upsert_source_sqlx(database.pool(), "default", &skill_source)
        .await
        .expect("upsert skill source");
    let all_sources = load_sources_sqlx(database.pool(), "default")
        .await
        .expect("load all sources");
    let skill_sources = load_skill_sources_sqlx(database.pool(), "default")
        .await
        .expect("load skill sources");
    let loaded_skill_source = load_source_sqlx(database.pool(), "default", &skill_source.id)
        .await
        .expect("load source");
    let missing_source = load_source_sqlx(database.pool(), "default", "missing")
        .await
        .expect("load missing source");

    assert_eq!(all_sources.len(), 2);
    assert_eq!(skill_sources.len(), 1);
    assert_eq!(skill_sources[0].id, "skill");
    assert_eq!(
        loaded_skill_source.expect("load source by id").id,
        skill_source.id
    );
    assert!(missing_source.is_none());
    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_source_repo_isolates_same_id_by_tenant() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-source-tenant-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let mut default_source = test_source("shared", SourceScannerKind::Mixed);
    default_source.name = "Default source".to_string();
    let mut tenant_source = test_source("shared", SourceScannerKind::Skill);
    tenant_source.name = "Tenant source".to_string();

    upsert_source_sqlx(database.pool(), "default", &default_source)
        .await
        .expect("upsert default source");
    upsert_source_sqlx(database.pool(), "tenant-a", &tenant_source)
        .await
        .expect("upsert tenant source");
    let default_sources = load_sources_sqlx(database.pool(), "default")
        .await
        .expect("load default sources");
    let tenant_sources = load_sources_sqlx(database.pool(), "tenant-a")
        .await
        .expect("load tenant sources");
    let default_loaded = load_source_sqlx(database.pool(), "default", "shared")
        .await
        .expect("load default source");
    let tenant_loaded = load_source_sqlx(database.pool(), "tenant-a", "shared")
        .await
        .expect("load tenant source");

    assert_eq!(default_sources.len(), 1);
    assert_eq!(tenant_sources.len(), 1);
    assert_eq!(
        default_loaded.expect("load default source").name,
        "Default source"
    );
    assert_eq!(
        tenant_loaded.expect("load tenant source").name,
        "Tenant source"
    );
    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_source_repo_normalizes_home_paths_for_storage_and_loading() {
    let db_path =
        std::env::temp_dir().join(format!("assetiweave-source-home-{}.sqlite", Uuid::new_v4()));
    let database = Database::open_async(&db_path).await.expect("open database");
    let mut source = test_source("home-source", SourceScannerKind::Skill);
    source.root_path = dirs::home_dir()
        .expect("home directory")
        .join("portable-source-test")
        .to_str()
        .expect("valid utf-8 path")
        .to_string();
    source.repo_root = Some(
        dirs::home_dir()
            .expect("home directory")
            .join("code-space")
            .to_str()
            .expect("valid utf-8 path")
            .to_string(),
    );

    upsert_source_sqlx(database.pool(), "default", &source)
        .await
        .expect("upsert source");
    let loaded = load_source_sqlx(database.pool(), "default", &source.id)
        .await
        .expect("round trip source")
        .expect("stored source");

    assert_eq!(loaded.root_path, "~/portable-source-test");
    assert_eq!(loaded.repo_root.as_deref(), Some("~/code-space"));
    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_source_repo_decodes_source_row_and_detects_invalid_json() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-source-decode-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let mut source = test_source("json-test", SourceScannerKind::Mixed);
    source.include_globs = vec!["*.md".to_string(), "*.txt".to_string()];
    source.exclude_globs = vec!["node_modules/**".to_string()];

    upsert_source_sqlx(database.pool(), "default", &source)
        .await
        .expect("upsert source");
    let loaded = load_source_sqlx(database.pool(), "default", &source.id)
        .await
        .expect("load source")
        .expect("source exists");
    assert_eq!(loaded.include_globs, vec!["*.md", "*.txt"]);
    assert_eq!(loaded.exclude_globs, vec!["node_modules/**"]);
    assert_eq!(loaded.repo_root, None);

    // Now corrupt include_globs with invalid JSON
    sqlx::query(
        "UPDATE sources SET include_globs = '{not_valid_json' WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind("default")
    .bind(&source.id)
    .execute(database.pool())
    .await
    .expect("corrupt row");

    // Loading should return an error and NOT swallow it into an empty vec
    let err = load_source_sqlx(database.pool(), "default", &source.id)
        .await
        .expect_err("should fail on invalid JSON");
    assert_eq!(err.code(), "storage_error");

    drop(database);
    cleanup_database(&db_path);
}

fn test_source(id: &str, scanner_kind: SourceScannerKind) -> Source {
    Source {
        id: id.to_string(),
        name: id.to_string(),
        kind: SourceKind::Local,
        root_path: format!("/tmp/{id}"),
        scanner_kind,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/*".to_string()],
        exclude_globs: Vec::new(),
        default_kind: if matches!(scanner_kind, SourceScannerKind::Skill) {
            Some(AssetKind::Skill)
        } else {
            None
        },
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    }
}

fn cleanup_database(db_path: &std::path::Path) {
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}
