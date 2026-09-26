use crate::backend::domain::{AssetKind, Source};
use crate::backend::store::{StoreError, StoreResult};
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
    type Error = StoreError;

    fn try_from(row: SourceRow) -> Result<Self, Self::Error> {
        Ok(Source {
            id: row.id,
            name: row.name,
            kind: decode_enum_app(row.kind)?,
            root_path: row.root_path,
            scanner_kind: decode_enum_app(row.scanner_kind)?,
            source_origin: decode_enum_app(row.source_origin)?,
            repo_root: row.repo_root,
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
) -> StoreResult<Vec<Source>> {
    let rows = sqlx::query_as::<_, SourceRow>(sql::LIST_SOURCES)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    rows.into_iter().map(Source::try_from).collect()
}

pub(crate) async fn load_skill_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Vec<Source>> {
    let rows = sqlx::query_as::<_, SourceRow>(sql::LIST_SKILL_SOURCES)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    rows.into_iter().map(Source::try_from).collect()
}

pub(crate) async fn load_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> StoreResult<Option<Source>> {
    sqlx::query_as::<_, SourceRow>(sql::LOAD_SOURCE)
        .bind(tenant_id)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(StoreError::external)?
        .map(Source::try_from)
        .transpose()
}

pub(crate) async fn upsert_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &Source,
) -> StoreResult<()> {
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
        .map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn delete_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    id: &str,
) -> StoreResult<()> {
    sqlx::query(sql::DELETE_ASSETS_BY_SOURCE)
        .bind(tenant_id)
        .bind(id)
        .execute(pool)
        .await
        .map_err(StoreError::external)?;
    sqlx::query(sql::DELETE_SOURCE)
        .bind(tenant_id)
        .bind(id)
        .execute(pool)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

#[cfg(test)]
#[path = "source_repo_tests.rs"]
mod tests;
