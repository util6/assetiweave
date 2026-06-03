use crate::types::AppResult;
use assetiweave_core::TargetProfile;
use rusqlite::{params, Connection};

use super::{
    codec::{db_error, decode_json, encode_json, to_sql_error},
    sql,
};

pub(crate) fn load_profiles(conn: &Connection) -> AppResult<Vec<TargetProfile>> {
    let mut stmt = conn.prepare(sql::LIST_PROFILES).map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            let payload: String = row.get(0)?;
            decode_json(payload).map_err(to_sql_error)
        })
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) fn upsert_profile(conn: &Connection, profile: &TargetProfile) -> AppResult<()> {
    conn.execute(
        sql::UPSERT_PROFILE,
        params![profile.id, encode_json(profile)?],
    )
    .map_err(db_error)?;
    Ok(())
}

pub(crate) fn delete_profile(conn: &Connection, profile_id: &str) -> AppResult<()> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    tx.execute(sql::DELETE_APP_SHORTCUT_BY_PROFILE, params![profile_id])
        .map_err(db_error)?;
    tx.execute(
        sql::DELETE_ASSET_MOUNT_OBSERVATIONS_BY_PROFILE,
        params![profile_id],
    )
    .map_err(db_error)?;
    tx.execute(sql::DELETE_ASSET_MOUNTS_BY_PROFILE, params![profile_id])
        .map_err(db_error)?;
    tx.execute(sql::DELETE_PROFILE, params![profile_id])
        .map_err(db_error)?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn count_deployment_state_by_profile(
    conn: &Connection,
    profile_id: &str,
) -> AppResult<usize> {
    conn.query_row(
        sql::COUNT_DEPLOYMENT_STATE_BY_PROFILE,
        params![profile_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count as usize)
    .map_err(db_error)
}
