use crate::backend::models::{AssetKind, Source, SourceOrigin, SourceScannerKind};
use crate::backend::{
    dto::AppResult,
    path_utils::{
        detect_app_target, expand_path, find_git_root, is_app_library_path, normalize_relative_path,
    },
};
use rusqlite::{params, Connection};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use super::{
    codec::{
        db_error, decode_enum, decode_json, decode_optional_enum, encode_enum, encode_json,
        encode_optional_enum, to_sql_error,
    },
    sql,
};

pub(crate) fn load_sources(conn: &Connection) -> AppResult<Vec<Source>> {
    let mut stmt = conn.prepare(sql::LIST_SOURCES).map_err(db_error)?;
    load_sources_with_statement(&mut stmt)
}

pub(crate) async fn load_sources_sqlx(pool: &SqlitePool) -> AppResult<Vec<Source>> {
    let rows = sqlx::query(sql::LIST_SOURCES)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_source_row).collect()
}

pub(crate) async fn load_skill_sources_sqlx(pool: &SqlitePool) -> AppResult<Vec<Source>> {
    let rows = sqlx::query(sql::LIST_SKILL_SOURCES)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_source_row).collect()
}

pub(crate) async fn load_source_sqlx(
    pool: &SqlitePool,
    source_id: &str,
) -> AppResult<Option<Source>> {
    sqlx::query(sql::LOAD_SOURCE)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(map_sqlx_source_row)
        .transpose()
}

fn load_sources_with_statement(stmt: &mut rusqlite::Statement<'_>) -> AppResult<Vec<Source>> {
    let rows = stmt
        .query_map([], |row| {
            Ok(Source {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: decode_enum(row.get::<_, String>(2)?).map_err(to_sql_error)?,
                root_path: row.get(3)?,
                scanner_kind: decode_enum(row.get::<_, String>(4)?).map_err(to_sql_error)?,
                source_origin: decode_enum(row.get::<_, String>(5)?).map_err(to_sql_error)?,
                repo_root: row.get(6)?,
                scan_root: row.get(7)?,
                origin_app_kind: decode_optional_enum(row.get(8)?).map_err(to_sql_error)?,
                include_globs: decode_json(row.get::<_, String>(9)?).map_err(to_sql_error)?,
                exclude_globs: decode_json(row.get::<_, String>(10)?).map_err(to_sql_error)?,
                default_kind: decode_optional_enum::<AssetKind>(row.get(11)?)
                    .map_err(to_sql_error)?,
                enabled: row.get::<_, i64>(12)? == 1,
                priority: row.get(13)?,
                last_scanned_at: row.get(14)?,
                last_scan_status: row.get(15)?,
            })
        })
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn map_sqlx_source_row(row: &SqliteRow) -> AppResult<Source> {
    Ok(Source {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        name: row.try_get(1).map_err(|error| error.to_string())?,
        kind: decode_enum(
            row.try_get::<String, _>(2)
                .map_err(|error| error.to_string())?,
        )?,
        root_path: row.try_get(3).map_err(|error| error.to_string())?,
        scanner_kind: decode_enum(
            row.try_get::<String, _>(4)
                .map_err(|error| error.to_string())?,
        )?,
        source_origin: decode_enum(
            row.try_get::<String, _>(5)
                .map_err(|error| error.to_string())?,
        )?,
        repo_root: row.try_get(6).map_err(|error| error.to_string())?,
        scan_root: row.try_get(7).map_err(|error| error.to_string())?,
        origin_app_kind: decode_optional_enum(row.try_get(8).map_err(|error| error.to_string())?)?,
        include_globs: decode_json(
            row.try_get::<String, _>(9)
                .map_err(|error| error.to_string())?,
        )?,
        exclude_globs: decode_json(
            row.try_get::<String, _>(10)
                .map_err(|error| error.to_string())?,
        )?,
        default_kind: decode_optional_enum::<AssetKind>(
            row.try_get(11).map_err(|error| error.to_string())?,
        )?,
        enabled: row
            .try_get::<i64, _>(12)
            .map_err(|error| error.to_string())?
            == 1,
        priority: row.try_get(13).map_err(|error| error.to_string())?,
        last_scanned_at: row.try_get(14).map_err(|error| error.to_string())?,
        last_scan_status: row.try_get(15).map_err(|error| error.to_string())?,
    })
}

pub(crate) fn upsert_source(conn: &Connection, source: &Source) -> AppResult<()> {
    let source = normalize_source(source);
    conn.execute(
        sql::UPSERT_SOURCE,
        params![
            source.id,
            source.name,
            encode_enum(source.kind)?,
            source.root_path,
            encode_enum(source.scanner_kind)?,
            encode_enum(source.source_origin)?,
            source.repo_root,
            source.scan_root,
            encode_optional_enum(source.origin_app_kind)?,
            encode_json(&source.include_globs)?,
            encode_json(&source.exclude_globs)?,
            encode_optional_enum(source.default_kind)?,
            if source.enabled { 1 } else { 0 },
            source.priority,
            source.last_scanned_at,
            source.last_scan_status
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub(crate) async fn upsert_source_sqlx(pool: &SqlitePool, source: &Source) -> AppResult<()> {
    let source = normalize_source(source);
    sqlx::query(sql::UPSERT_SOURCE)
        .bind(&source.id)
        .bind(&source.name)
        .bind(encode_enum(source.kind)?)
        .bind(&source.root_path)
        .bind(encode_enum(source.scanner_kind)?)
        .bind(encode_enum(source.source_origin)?)
        .bind(&source.repo_root)
        .bind(&source.scan_root)
        .bind(encode_optional_enum(source.origin_app_kind)?)
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
    let mut source = source.clone();

    if matches!(source.scanner_kind, SourceScannerKind::Mixed) && is_skill_like_source(&source) {
        source.scanner_kind = SourceScannerKind::Skill;
    }

    if source.id == "assetiweave-library-skills" {
        source.source_origin = SourceOrigin::AssetiweaveLibrary;
        source.scanner_kind = SourceScannerKind::Skill;
        source.repo_root = None;
        source.scan_root = String::new();
        source.origin_app_kind = None;
        return source;
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
        return source;
    }

    if let Some(app_kind) = detect_app_target(&root_path) {
        source.source_origin = SourceOrigin::AppTarget;
        source.scanner_kind = SourceScannerKind::Skill;
        source.repo_root = None;
        source.scan_root = String::new();
        source.origin_app_kind = Some(app_kind);
        return source;
    }

    if let Some(git_root) = find_git_root(&root_path) {
        source.source_origin = SourceOrigin::GitRepo;
        source.repo_root = Some(git_root.to_string_lossy().to_string());
        source.scan_root = root_path
            .strip_prefix(&git_root)
            .ok()
            .map(normalize_relative_path)
            .unwrap_or_default();
        return source;
    }

    if source.scan_root.is_empty() {
        source.scan_root = String::new();
    }
    source
}

fn is_skill_like_source(source: &Source) -> bool {
    source.default_kind == Some(AssetKind::Skill)
        || source
            .include_globs
            .iter()
            .any(|glob| glob.to_ascii_lowercase().contains("skill.md"))
}

pub(crate) async fn delete_source_sqlx(pool: &SqlitePool, id: &str) -> AppResult<()> {
    sqlx::query(sql::DELETE_ASSETS_BY_SOURCE)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_SOURCE)
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
                upsert_source_sqlx(database.pool(), &regular_source).await?;
                upsert_source_sqlx(database.pool(), &skill_source).await?;
                let all_sources = load_sources_sqlx(database.pool()).await?;
                let skill_sources = load_skill_sources_sqlx(database.pool()).await?;
                let loaded_skill_source =
                    load_source_sqlx(database.pool(), &skill_source.id).await?;
                let missing_source = load_source_sqlx(database.pool(), "missing").await?;
                AppResult::Ok((
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
