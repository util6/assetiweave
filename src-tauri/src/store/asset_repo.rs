use crate::types::AppResult;
use assetiweave_core::{Asset, AssetFormat, AssetKind};
use rusqlite::{params, Connection};

use super::{
    codec::{db_error, decode_enum, encode_enum, to_sql_error},
    sql,
};

pub(crate) fn load_assets(conn: &Connection) -> AppResult<Vec<Asset>> {
    let mut stmt = conn.prepare(sql::LIST_ASSETS).map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Asset {
                id: row.get(0)?,
                source_id: row.get(1)?,
                name: row.get(2)?,
                kind: decode_enum::<AssetKind>(row.get::<_, String>(3)?).map_err(to_sql_error)?,
                format: decode_enum::<AssetFormat>(row.get::<_, String>(4)?)
                    .map_err(to_sql_error)?,
                relative_path: row.get(5)?,
                absolute_path: row.get(6)?,
                entry_file: row.get(7)?,
                description: row.get(8)?,
                content_hash: row.get(9)?,
                discovered_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) fn replace_source_assets(
    conn: &Connection,
    source_id: &str,
    assets: &[Asset],
) -> AppResult<()> {
    conn.execute(sql::DELETE_ASSETS_BY_SOURCE, params![source_id])
        .map_err(db_error)?;
    for asset in assets {
        conn.execute(
            sql::INSERT_ASSET,
            params![
                asset.id,
                asset.source_id,
                asset.name,
                encode_enum(asset.kind)?,
                encode_enum(asset.format)?,
                asset.relative_path,
                asset.absolute_path,
                asset.entry_file,
                asset.description,
                asset.content_hash,
                asset.discovered_at,
                asset.updated_at,
            ],
        )
        .map_err(db_error)?;
    }
    Ok(())
}
