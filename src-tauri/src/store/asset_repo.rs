use crate::types::AppResult;
use assetiweave_core::{Asset, AssetFormat, AssetKind};
use rusqlite::{params, Connection, Row};

use super::{
    codec::{db_error, decode_enum, encode_enum, to_sql_error},
    sql,
};

pub(crate) fn load_assets(conn: &Connection) -> AppResult<Vec<Asset>> {
    load_assets_by_kind(conn, None)
}

pub(crate) fn load_assets_by_kind(
    conn: &Connection,
    kind: Option<AssetKind>,
) -> AppResult<Vec<Asset>> {
    let Some(kind) = kind else {
        return load_all_assets(conn);
    };
    let kind = encode_enum(kind)?;
    let mut stmt = conn.prepare(sql::LIST_ASSETS_BY_KIND).map_err(db_error)?;
    let rows = stmt
        .query_map(params![kind], map_asset_row)
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn load_all_assets(conn: &Connection) -> AppResult<Vec<Asset>> {
    let mut stmt = conn.prepare(sql::LIST_ASSETS).map_err(db_error)?;
    let rows = stmt.query_map([], map_asset_row).map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn map_asset_row(row: &Row<'_>) -> rusqlite::Result<Asset> {
    Ok(Asset {
        id: row.get(0)?,
        source_id: row.get(1)?,
        name: row.get(2)?,
        kind: decode_enum::<AssetKind>(row.get::<_, String>(3)?).map_err(to_sql_error)?,
        format: decode_enum::<AssetFormat>(row.get::<_, String>(4)?).map_err(to_sql_error)?,
        relative_path: row.get(5)?,
        absolute_path: row.get(6)?,
        entry_file: row.get(7)?,
        description: row.get(8)?,
        content_hash: row.get(9)?,
        discovered_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
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

pub(crate) fn update_asset_description(conn: &Connection, asset: &Asset) -> AppResult<()> {
    let updated = conn
        .execute(
            sql::UPDATE_ASSET_DESCRIPTION,
            params![asset.description, asset.updated_at, asset.id],
        )
        .map_err(db_error)?;
    if updated == 0 {
        return Err(format!("asset not found: {}", asset.id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::sql;

    #[test]
    fn load_assets_by_kind_filters_out_other_catalog_kinds() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(sql::INIT_SCHEMA).expect("init schema");
        let assets = [
            test_asset("skill-a", AssetKind::Skill),
            test_asset("design", AssetKind::Rule),
        ];
        replace_source_assets(&conn, "source-a", &assets).expect("insert assets");

        let scoped_assets =
            load_assets_by_kind(&conn, Some(AssetKind::Skill)).expect("load scoped assets");

        assert_eq!(scoped_assets.len(), 1);
        assert_eq!(scoped_assets[0].name, "skill-a");
        assert_eq!(scoped_assets[0].kind, AssetKind::Skill);
    }

    fn test_asset(name: &str, kind: AssetKind) -> Asset {
        Asset {
            id: format!("asset-{name}"),
            source_id: "source-a".to_string(),
            name: name.to_string(),
            kind,
            format: AssetFormat::Markdown,
            relative_path: format!("{name}.md"),
            absolute_path: format!("/tmp/{name}.md"),
            entry_file: None,
            description: None,
            content_hash: None,
            discovered_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }
}
