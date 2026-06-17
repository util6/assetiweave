use crate::backend::dto::AppResult;
use crate::backend::models::{AssetMount, DeploymentStrategy};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use super::{
    codec::{db_error, decode_enum, encode_enum, to_sql_error},
    sql,
};

pub(crate) fn load_asset_mounts(
    conn: &Connection,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMount>> {
    let mut stmt = conn.prepare(sql::LIST_ASSET_MOUNTS).map_err(db_error)?;
    let rows = stmt
        .query_map(params![asset_id], decode_mount)
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) fn load_enabled_asset_mounts(
    conn: &Connection,
    profile_id: Option<&str>,
) -> AppResult<Vec<AssetMount>> {
    let mut stmt = conn
        .prepare(sql::LIST_ENABLED_ASSET_MOUNTS)
        .map_err(db_error)?;
    let rows = stmt
        .query_map(params![profile_id], decode_mount)
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) fn delete_orphan_asset_mounts(conn: &Connection) -> AppResult<()> {
    conn.execute(sql::DELETE_ORPHAN_ASSET_MOUNTS, [])
        .map_err(db_error)?;
    Ok(())
}

pub(crate) fn set_asset_mount(
    conn: &Connection,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let now = Utc::now().to_rfc3339();
    let created_at = load_asset_mount(conn, asset_id, profile_id)?
        .map(|mount| mount.created_at)
        .unwrap_or_else(|| now.clone());
    let mount = AssetMount {
        asset_id: asset_id.to_string(),
        profile_id: profile_id.to_string(),
        enabled,
        strategy,
        created_at,
        updated_at: now,
    };
    upsert_asset_mount(conn, &mount)?;
    Ok(mount)
}

fn load_asset_mount(
    conn: &Connection,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<Option<AssetMount>> {
    conn.query_row(
        sql::GET_ASSET_MOUNT,
        params![asset_id, profile_id],
        decode_mount,
    )
    .optional()
    .map_err(db_error)
}

fn upsert_asset_mount(conn: &Connection, mount: &AssetMount) -> AppResult<()> {
    conn.execute(
        sql::UPSERT_ASSET_MOUNT,
        params![
            mount.asset_id,
            mount.profile_id,
            if mount.enabled { 1 } else { 0 },
            encode_enum(mount.strategy)?,
            mount.created_at,
            mount.updated_at,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

fn decode_mount(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetMount> {
    Ok(AssetMount {
        asset_id: row.get(0)?,
        profile_id: row.get(1)?,
        enabled: row.get::<_, i64>(2)? == 1,
        strategy: decode_enum(row.get::<_, String>(3)?).map_err(to_sql_error)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}
