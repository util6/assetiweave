use crate::backend::dto::AppResult;
use crate::backend::models::DeploymentState;
use rusqlite::{params, Connection, OptionalExtension};

use super::{
    codec::{db_error, encode_enum},
    sql,
};

pub(crate) fn upsert_deployment_state(conn: &Connection, state: &DeploymentState) -> AppResult<()> {
    conn.execute(
        sql::UPSERT_DEPLOYMENT_STATE,
        params![
            state.profile_id,
            state.asset_id,
            state.target_path,
            encode_enum(state.strategy)?,
            state.source_hash,
            state.deployed_at,
            state.managed_by,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub(crate) fn is_managed_deployment(
    conn: &Connection,
    profile_id: &str,
    asset_id: &str,
    target_path: &str,
) -> AppResult<bool> {
    conn.query_row(
        sql::GET_MANAGED_DEPLOYMENT,
        params![profile_id, asset_id, target_path],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map(|managed_by| managed_by.as_deref() == Some("assetiweave"))
    .map_err(db_error)
}

pub(crate) fn delete_deployment_state(
    conn: &Connection,
    profile_id: &str,
    asset_id: &str,
    target_path: &str,
) -> AppResult<()> {
    conn.execute(
        sql::DELETE_DEPLOYMENT_STATE,
        params![profile_id, asset_id, target_path],
    )
    .map_err(db_error)?;
    Ok(())
}

pub(crate) fn delete_orphan_deployment_state(conn: &Connection) -> AppResult<()> {
    conn.execute(sql::DELETE_ORPHAN_DEPLOYMENT_STATE, [])
        .map_err(db_error)?;
    Ok(())
}
