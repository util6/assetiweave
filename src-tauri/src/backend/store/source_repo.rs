use crate::backend::models::{AssetKind, Source, SourceOrigin, SourceScannerKind};
use crate::backend::{
    compat::LegacyResult,
    path_utils::{
        detect_target_provider, expand_path, find_git_root, is_app_library_path,
        normalize_path_for_storage, normalize_relative_path,
    },
    target_catalog::TargetCatalog,
};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use super::{
    codec::{
        decode_enum, decode_json, decode_optional_enum, encode_enum, encode_json,
        encode_optional_enum,
    },
    sql,
};

pub(crate) async fn load_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> LegacyResult<Vec<Source>> {
    let rows = sqlx::query(sql::LIST_SOURCES)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_source_row).collect()
}

pub(crate) async fn load_skill_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> LegacyResult<Vec<Source>> {
    let rows = sqlx::query(sql::LIST_SKILL_SOURCES)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_source_row).collect()
}

pub(crate) async fn load_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> LegacyResult<Option<Source>> {
    sqlx::query(sql::LOAD_SOURCE)
        .bind(tenant_id)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(map_sqlx_source_row)
        .transpose()
}

fn map_sqlx_source_row(row: &SqliteRow) -> LegacyResult<Source> {
    let root_path: String = row.try_get(3).map_err(|error| error.to_string())?;
    let repo_root: Option<String> = row.try_get(6).map_err(|error| error.to_string())?;
    Ok(Source {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        name: row.try_get(1).map_err(|error| error.to_string())?,
        kind: decode_enum(
            row.try_get::<String, _>(2)
                .map_err(|error| error.to_string())?,
        )?,
        root_path: normalize_path_for_storage(&root_path)?,
        scanner_kind: decode_enum(
            row.try_get::<String, _>(4)
                .map_err(|error| error.to_string())?,
        )?,
        source_origin: decode_enum(
            row.try_get::<String, _>(5)
                .map_err(|error| error.to_string())?,
        )?,
        repo_root: repo_root
            .as_deref()
            .map(normalize_path_for_storage)
            .transpose()?,
        scan_root: row.try_get(7).map_err(|error| error.to_string())?,
        origin_app_kind: decode_optional_enum(row.try_get(8).map_err(|error| error.to_string())?)?,
        origin_provider_id: row.try_get(9).map_err(|error| error.to_string())?,
        include_globs: decode_json(
            row.try_get::<String, _>(10)
                .map_err(|error| error.to_string())?,
        )?,
        exclude_globs: decode_json(
            row.try_get::<String, _>(11)
                .map_err(|error| error.to_string())?,
        )?,
        default_kind: decode_optional_enum::<AssetKind>(
            row.try_get(12).map_err(|error| error.to_string())?,
        )?,
        enabled: row
            .try_get::<i64, _>(13)
            .map_err(|error| error.to_string())?
            == 1,
        priority: row.try_get(14).map_err(|error| error.to_string())?,
        last_scanned_at: row.try_get(15).map_err(|error| error.to_string())?,
        last_scan_status: row.try_get(16).map_err(|error| error.to_string())?,
    })
}

pub(crate) async fn upsert_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &Source,
) -> LegacyResult<()> {
    upsert_source_sqlx_normalized(pool, tenant_id, normalize_source(source)).await
}

pub(crate) async fn upsert_source_sqlx_with_catalog(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &Source,
    catalog: &TargetCatalog,
) -> LegacyResult<()> {
    upsert_source_sqlx_normalized(
        pool,
        tenant_id,
        normalize_source_with_catalog(source, catalog),
    )
    .await
}

async fn upsert_source_sqlx_normalized(
    pool: &SqlitePool,
    tenant_id: &str,
    source: Source,
) -> LegacyResult<()> {
    sqlx::query(sql::UPSERT_SOURCE)
        .bind(tenant_id)
        .bind(&source.id)
        .bind(&source.name)
        .bind(encode_enum(source.kind)?)
        .bind(&source.root_path)
        .bind(encode_enum(source.scanner_kind)?)
        .bind(encode_enum(source.source_origin)?)
        .bind(&source.repo_root)
        .bind(&source.scan_root)
        .bind(encode_optional_enum(source.origin_app_kind)?)
        .bind(&source.origin_provider_id)
        .bind(encode_json(&source.include_globs)?)
        .bind(encode_json(&source.exclude_globs)?)
        .bind(encode_optional_enum(source.default_kind)?)
        .bind(if source.enabled { 1 } else { 0 })
        .bind(source.priority)
        .bind(&source.last_scanned_at)
        .bind(&source.last_scan_status)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn normalize_source(source: &Source) -> Source {
    normalize_source_inner(source, None)
}

pub(crate) fn normalize_source_with_catalog(source: &Source, catalog: &TargetCatalog) -> Source {
    normalize_source_inner(source, Some(catalog))
}

fn normalize_source_inner(source: &Source, catalog: Option<&TargetCatalog>) -> Source {
    let mut source = source.clone();
    normalize_source_paths(&mut source);

    if matches!(source.scanner_kind, SourceScannerKind::Mixed) && is_skill_like_source(&source) {
        source.scanner_kind = SourceScannerKind::Skill;
    }

    if source.id == "assetiweave-library-skills" {
        source.source_origin = SourceOrigin::AssetiweaveLibrary;
        source.scanner_kind = SourceScannerKind::Skill;
        source.repo_root = None;
        source.scan_root = String::new();
        source.origin_app_kind = None;
        source.origin_provider_id = None;
        return source;
    }

    if source.id == crate::backend::builtin_skills::SYSTEM_SKILL_SOURCE_ID {
        return crate::backend::builtin_skills::system_skill_source().unwrap_or(source);
    }

    let Ok(root_path) = expand_path(&source.root_path) else {
        return source;
    };

    if is_app_library_path(&root_path) {
        source.source_origin = SourceOrigin::AssetiweaveLibrary;
        source.scanner_kind = SourceScannerKind::Skill;
        source.repo_root = None;
        source.scan_root = String::new();
        source.origin_app_kind = None;
        source.origin_provider_id = None;
        return source;
    }

    if let Some(catalog) = catalog {
        if let Some((provider_id, app_kind)) = detect_target_provider(&root_path, catalog) {
            source.source_origin = SourceOrigin::AppTarget;
            source.scanner_kind = SourceScannerKind::Skill;
            source.repo_root = None;
            source.scan_root = String::new();
            source.origin_app_kind = app_kind;
            source.origin_provider_id = Some(provider_id);
            return source;
        }
    }

    if let Some(git_root) = find_git_root(&root_path) {
        source.source_origin = SourceOrigin::GitRepo;
        source.repo_root = normalize_path_for_storage(&git_root.to_string_lossy()).ok();
        source.scan_root = root_path
            .strip_prefix(&git_root)
            .ok()
            .map(normalize_relative_path)
            .unwrap_or_default();
    }
    source
}

fn normalize_source_paths(source: &mut Source) {
    if let Ok(root_path) = normalize_path_for_storage(&source.root_path) {
        source.root_path = root_path;
    }
    source.repo_root = source
        .repo_root
        .as_deref()
        .map(|path| normalize_path_for_storage(path).unwrap_or_else(|_| path.to_string()));
}

fn is_skill_like_source(source: &Source) -> bool {
    source.default_kind == Some(AssetKind::Skill)
        || source
            .include_globs
            .iter()
            .any(|glob| glob.to_ascii_lowercase().contains("skill.md"))
}

pub(crate) async fn delete_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    id: &str,
) -> LegacyResult<()> {
    sqlx::query(sql::DELETE_ASSETS_BY_SOURCE)
        .bind(tenant_id)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_SOURCE)
        .bind(tenant_id)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::SourceKind;
    use crate::backend::store::Database;
    use uuid::Uuid;

    #[test]
    fn sqlx_source_repo_upserts_lists_and_filters_skill_sources() {
        let db_path =
            std::env::temp_dir().join(format!("assetiweave-source-sqlx-{}.sqlite", Uuid::new_v4()));
        let database = Database::open(&db_path).expect("open database");
        let regular_source = test_source("regular", SourceScannerKind::Mixed);
        let skill_source = test_source("skill", SourceScannerKind::Skill);

        let (all_sources, skill_sources, loaded_skill_source, missing_source) = database
            .block_on(async {
                upsert_source_sqlx(database.pool(), "default", &regular_source).await?;
                upsert_source_sqlx(database.pool(), "default", &skill_source).await?;
                let all_sources = load_sources_sqlx(database.pool(), "default").await?;
                let skill_sources = load_skill_sources_sqlx(database.pool(), "default").await?;
                let loaded_skill_source =
                    load_source_sqlx(database.pool(), "default", &skill_source.id).await?;
                let missing_source =
                    load_source_sqlx(database.pool(), "default", "missing").await?;
                LegacyResult::Ok((
                    all_sources,
                    skill_sources,
                    loaded_skill_source,
                    missing_source,
                ))
            })
            .expect("query SQLx source repo");

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

    #[test]
    fn sqlx_source_repo_isolates_same_id_by_tenant() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-source-tenant-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let mut default_source = test_source("shared", SourceScannerKind::Mixed);
        default_source.name = "Default source".to_string();
        let mut tenant_source = test_source("shared", SourceScannerKind::Skill);
        tenant_source.name = "Tenant source".to_string();

        let (default_sources, tenant_sources, default_loaded, tenant_loaded) = database
            .block_on(async {
                upsert_source_sqlx(database.pool(), "default", &default_source).await?;
                upsert_source_sqlx(database.pool(), "tenant-a", &tenant_source).await?;
                let default_sources = load_sources_sqlx(database.pool(), "default").await?;
                let tenant_sources = load_sources_sqlx(database.pool(), "tenant-a").await?;
                let default_loaded = load_source_sqlx(database.pool(), "default", "shared").await?;
                let tenant_loaded = load_source_sqlx(database.pool(), "tenant-a", "shared").await?;
                LegacyResult::Ok((
                    default_sources,
                    tenant_sources,
                    default_loaded,
                    tenant_loaded,
                ))
            })
            .expect("query tenant-scoped sources");

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

    #[test]
    fn sqlx_source_repo_normalizes_home_paths_for_storage_and_loading() {
        let db_path =
            std::env::temp_dir().join(format!("assetiweave-source-home-{}.sqlite", Uuid::new_v4()));
        let database = Database::open(&db_path).expect("open database");
        let mut source = test_source("home-source", SourceScannerKind::Skill);
        source.root_path = dirs::home_dir()
            .expect("home directory")
            .join("portable-source-test")
            .to_string_lossy()
            .to_string();
        source.repo_root = Some(
            dirs::home_dir()
                .expect("home directory")
                .join("code-space")
                .to_string_lossy()
                .to_string(),
        );

        let loaded = database
            .block_on(async {
                upsert_source_sqlx(database.pool(), "default", &source).await?;
                load_source_sqlx(database.pool(), "default", &source.id).await
            })
            .expect("round trip source")
            .expect("stored source");

        assert_eq!(loaded.root_path, "~/portable-source-test");
        assert_eq!(loaded.repo_root.as_deref(), Some("~/code-space"));
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
}
