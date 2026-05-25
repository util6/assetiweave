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
