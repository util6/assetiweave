use crate::types::AppResult;
use assetiweave_core::{AssetKind, Source};
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
    let rows = stmt
        .query_map([], |row| {
            Ok(Source {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: decode_enum(row.get::<_, String>(2)?).map_err(to_sql_error)?,
                root_path: row.get(3)?,
                include_globs: decode_json(row.get::<_, String>(4)?).map_err(to_sql_error)?,
                exclude_globs: decode_json(row.get::<_, String>(5)?).map_err(to_sql_error)?,
                default_kind: decode_optional_enum::<AssetKind>(row.get(6)?)
                    .map_err(to_sql_error)?,
                enabled: row.get::<_, i64>(7)? == 1,
                priority: row.get(8)?,
                last_scanned_at: row.get(9)?,
                last_scan_status: row.get(10)?,
            })
        })
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) fn upsert_source(conn: &Connection, source: &Source) -> AppResult<()> {
    conn.execute(
        sql::UPSERT_SOURCE,
        params![
            source.id,
            source.name,
            encode_enum(source.kind)?,
            source.root_path,
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

pub(crate) fn delete_source(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute(sql::DELETE_SOURCE, params![id])
        .map_err(db_error)?;
    Ok(())
}
