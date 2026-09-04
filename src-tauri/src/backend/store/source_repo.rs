use crate::backend::models::{AssetKind, Source, SourceOrigin, SourceScannerKind};
use crate::backend::{
    path_utils::{
        detect_target_provider, expand_path, find_git_root, is_app_library_path,
        normalize_path_for_storage, normalize_relative_path,
    },
    runtime::{AppError, AppResult},
    target_catalog::TargetCatalog,
};
use sqlx::SqlitePool;

use super::{
    codec::{
        decode_enum_app, decode_json_app, decode_optional_enum_app, encode_enum_app,
        encode_json_app, encode_optional_enum_app,
    },
    sql,
};

#[derive(sqlx::FromRow)]
struct SourceRow {
    id: String,
    name: String,
    kind: String,
    root_path: String,
    scanner_kind: String,
    source_origin: String,
    repo_root: Option<String>,
    scan_root: String,
    origin_app_kind: Option<String>,
    origin_provider_id: Option<String>,
    include_globs: String,
    exclude_globs: String,
    default_kind: Option<String>,
    enabled: i64,
    priority: i32,
    last_scanned_at: Option<String>,
    last_scan_status: Option<String>,
}

impl TryFrom<SourceRow> for Source {
    type Error = AppError;

    fn try_from(row: SourceRow) -> Result<Self, Self::Error> {
        Ok(Source {
            id: row.id,
            name: row.name,
            kind: decode_enum_app(row.kind)?,
            root_path: normalize_path_for_storage(&row.root_path)?,
            scanner_kind: decode_enum_app(row.scanner_kind)?,
            source_origin: decode_enum_app(row.source_origin)?,
            repo_root: row
                .repo_root
                .as_deref()
                .map(normalize_path_for_storage)
                .transpose()?,
            scan_root: row.scan_root,
            origin_app_kind: decode_optional_enum_app(row.origin_app_kind)?,
            origin_provider_id: row.origin_provider_id,
            include_globs: decode_json_app(row.include_globs)?,
            exclude_globs: decode_json_app(row.exclude_globs)?,
            default_kind: decode_optional_enum_app::<AssetKind>(row.default_kind)?,
            enabled: row.enabled == 1,
            priority: row.priority,
            last_scanned_at: row.last_scanned_at,
            last_scan_status: row.last_scan_status,
        })
    }
}

pub(crate) async fn load_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<Source>> {
    let rows = sqlx::query_as::<_, SourceRow>(sql::LIST_SOURCES)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.into_iter().map(Source::try_from).collect()
}

pub(crate) async fn load_skill_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<Source>> {
    let rows = sqlx::query_as::<_, SourceRow>(sql::LIST_SKILL_SOURCES)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.into_iter().map(Source::try_from).collect()
}

pub(crate) async fn load_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> AppResult<Option<Source>> {
    sqlx::query_as::<_, SourceRow>(sql::LOAD_SOURCE)
        .bind(tenant_id)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::external)?
        .map(Source::try_from)
        .transpose()
}

pub(crate) async fn upsert_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &Source,
) -> AppResult<()> {
    upsert_source_sqlx_normalized(pool, tenant_id, normalize_source(source)).await
}

pub(crate) async fn upsert_source_sqlx_with_catalog(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &Source,
    catalog: &TargetCatalog,
) -> AppResult<()> {
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
) -> AppResult<()> {
    sqlx::query(sql::UPSERT_SOURCE)
        .bind(tenant_id)
        .bind(&source.id)
        .bind(&source.name)
        .bind(encode_enum_app(source.kind)?)
        .bind(&source.root_path)
        .bind(encode_enum_app(source.scanner_kind)?)
        .bind(encode_enum_app(source.source_origin)?)
        .bind(&source.repo_root)
        .bind(&source.scan_root)
        .bind(encode_optional_enum_app(source.origin_app_kind)?)
        .bind(&source.origin_provider_id)
        .bind(encode_json_app(&source.include_globs)?)
        .bind(encode_json_app(&source.exclude_globs)?)
        .bind(encode_optional_enum_app(source.default_kind)?)
        .bind(if source.enabled { 1 } else { 0 })
        .bind(source.priority)
        .bind(&source.last_scanned_at)
        .bind(&source.last_scan_status)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
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
) -> AppResult<()> {
    sqlx::query(sql::DELETE_ASSETS_BY_SOURCE)
        .bind(tenant_id)
        .bind(id)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
    sqlx::query(sql::DELETE_SOURCE)
        .bind(tenant_id)
        .bind(id)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
    Ok(())
}

#[cfg(test)]
mod tests {
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
            .to_string_lossy()
            .to_string();
        source.repo_root = Some(
            dirs::home_dir()
                .expect("home directory")
                .join("code-space")
                .to_string_lossy()
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
        assert_eq!(err.code(), "external_error");

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
