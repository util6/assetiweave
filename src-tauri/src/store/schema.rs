use crate::{defaults, types::AppResult};
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

use super::{
    codec::db_error, menu_repo::seed_navigation_model, profile_repo::upsert_profile,
    source_repo::upsert_source, sql,
};

pub(crate) fn open_initialized(db_path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(db_path).map_err(db_error)?;
    init_schema(&conn)?;
    seed_defaults(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(sql::INIT_SCHEMA).map_err(db_error)
}

fn seed_defaults(conn: &Connection) -> AppResult<()> {
    if count_rows(conn, "sources")? == 0 {
        for source in defaults::default_sources() {
            upsert_source(conn, &source)?;
        }
    }

    if count_rows(conn, "profiles")? == 0 {
        for profile in defaults::default_profiles() {
            upsert_profile(conn, &profile)?;
        }
    }

    if count_rows(conn, "navigation_state")? == 0 {
        seed_navigation_model(conn, &defaults::default_navigation_model())?;
    }

    Ok(())
}

pub(crate) fn latest_scan_status(conn: &Connection) -> AppResult<String> {
    let status: Option<String> = conn
        .query_row(sql::LATEST_SCAN_STATUS, [], |row| row.get(0))
        .optional()
        .map_err(db_error)?
        .flatten();
    Ok(status.unwrap_or_else(|| "等待首次扫描".to_string()))
}

pub(crate) fn count_rows(conn: &Connection, table: &str) -> AppResult<usize> {
    let statement = match table {
        "sources" => sql::COUNT_SOURCES,
        "assets" => sql::COUNT_ASSETS,
        "profiles" => sql::COUNT_PROFILES,
        "navigation_state" => sql::COUNT_NAVIGATION_STATE,
        other => return Err(format!("unsupported count table: {other}")),
    };
    let count: i64 = conn
        .query_row(statement, [], |row| row.get(0))
        .map_err(db_error)?;
    Ok(count as usize)
}
