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
        source.repo_root =
            crate::backend::path_utils::normalize_std_path_for_storage(&git_root).ok();
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
#[path = "source_repo_tests.rs"]
mod tests;
