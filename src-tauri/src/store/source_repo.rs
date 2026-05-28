use crate::{
    path_utils::{
        detect_app_target, expand_path, find_git_root, is_app_library_path, normalize_relative_path,
    },
    types::AppResult,
};
use assetiweave_core::{AssetKind, Source, SourceOrigin, SourceScannerKind};
use rusqlite::{params, Connection};

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

pub(crate) fn load_skill_sources(conn: &Connection) -> AppResult<Vec<Source>> {
    let mut stmt = conn.prepare(sql::LIST_SKILL_SOURCES).map_err(db_error)?;
    load_sources_with_statement(&mut stmt)
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

pub(crate) fn normalize_source(source: &Source) -> Source {
    let mut source = source.clone();

    if matches!(source.scanner_kind, SourceScannerKind::Mixed) && is_skill_like_source(&source) {
        source.scanner_kind = SourceScannerKind::Skill;
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

pub(crate) fn delete_source(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute(sql::DELETE_ASSETS_BY_SOURCE, params![id])
        .map_err(db_error)?;
    conn.execute(sql::DELETE_SOURCE, params![id])
        .map_err(db_error)?;
    Ok(())
}
