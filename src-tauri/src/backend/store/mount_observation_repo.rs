use crate::backend::dto::{AppResult, AssetMountObservation};
use rusqlite::{params, Connection};

#[cfg(test)]
use super::codec::{decode_enum, to_sql_error};
use super::{
    codec::{db_error, encode_enum},
    sql,
};

pub(crate) fn upsert_asset_mount_observations(
    conn: &Connection,
    observations: &[AssetMountObservation],
) -> AppResult<()> {
    for observation in observations {
        conn.execute(
            sql::UPSERT_ASSET_MOUNT_OBSERVATION,
            params![
                &observation.asset_id,
                &observation.profile_id,
                &observation.target_dir,
                &observation.target_path,
                encode_enum(observation.state)?,
                observation.linked_source.as_deref(),
                &observation.observed_at,
            ],
        )
        .map_err(db_error)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn load_asset_mount_observations(
    conn: &Connection,
) -> AppResult<Vec<AssetMountObservation>> {
    let mut stmt = conn
        .prepare(sql::LIST_ASSET_MOUNT_OBSERVATIONS)
        .map_err(db_error)?;
    let rows = stmt.query_map([], decode_observation).map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) fn delete_orphan_asset_mount_observations(conn: &Connection) -> AppResult<()> {
    conn.execute(sql::DELETE_ORPHAN_ASSET_MOUNT_OBSERVATIONS, [])
        .map_err(db_error)?;
    Ok(())
}

#[cfg(test)]
fn decode_observation(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetMountObservation> {
    Ok(AssetMountObservation {
        asset_id: row.get(0)?,
        profile_id: row.get(1)?,
        target_dir: row.get(2)?,
        target_path: row.get(3)?,
        state: decode_enum(row.get::<_, String>(4)?).map_err(to_sql_error)?,
        linked_source: row.get(5)?,
        observed_at: row.get(6)?,
    })
}
