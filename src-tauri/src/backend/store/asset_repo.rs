use crate::backend::dto::AppResult;
use crate::backend::models::{Asset, AssetFormat, AssetKind};
use rusqlite::{params, Connection, Row};
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};

use super::{
    codec::{db_error, decode_enum, encode_enum, to_sql_error},
    sql,
};

pub(crate) fn load_assets(conn: &Connection) -> AppResult<Vec<Asset>> {
    load_assets_by_kind(conn, None)
}

pub(crate) async fn load_assets_sqlx(
    pool: &SqlitePool,
    kind: Option<AssetKind>,
) -> AppResult<Vec<Asset>> {
    let rows = if let Some(kind) = kind {
        sqlx::query(sql::LIST_ASSETS_BY_KIND)
            .bind(encode_enum(kind)?)
            .fetch_all(pool)
            .await
            .map_err(|error| error.to_string())?
    } else {
        sqlx::query(sql::LIST_ASSETS)
            .fetch_all(pool)
            .await
            .map_err(|error| error.to_string())?
    };
    rows.iter().map(map_sqlx_asset_row).collect()
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

fn map_sqlx_asset_row(row: &SqliteRow) -> AppResult<Asset> {
    Ok(Asset {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        source_id: row.try_get(1).map_err(|error| error.to_string())?,
        name: row.try_get(2).map_err(|error| error.to_string())?,
        kind: decode_enum::<AssetKind>(
            row.try_get::<String, _>(3)
                .map_err(|error| error.to_string())?,
        )?,
        format: decode_enum::<AssetFormat>(
            row.try_get::<String, _>(4)
                .map_err(|error| error.to_string())?,
        )?,
        relative_path: row.try_get(5).map_err(|error| error.to_string())?,
        absolute_path: row.try_get(6).map_err(|error| error.to_string())?,
        entry_file: row.try_get(7).map_err(|error| error.to_string())?,
        description: row.try_get(8).map_err(|error| error.to_string())?,
        content_hash: row.try_get(9).map_err(|error| error.to_string())?,
        discovered_at: row.try_get(10).map_err(|error| error.to_string())?,
        updated_at: row.try_get(11).map_err(|error| error.to_string())?,
    })
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

pub(crate) async fn replace_source_assets_sqlx(
    pool: &SqlitePool,
    source_id: &str,
    assets: &[Asset],
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_ASSETS_BY_SOURCE)
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    for asset in assets {
        sqlx::query(sql::INSERT_ASSET)
            .bind(&asset.id)
            .bind(&asset.source_id)
            .bind(&asset.name)
            .bind(encode_enum(asset.kind)?)
            .bind(encode_enum(asset.format)?)
            .bind(&asset.relative_path)
            .bind(&asset.absolute_path)
            .bind(&asset.entry_file)
            .bind(&asset.description)
            .bind(&asset.content_hash)
            .bind(&asset.discovered_at)
            .bind(&asset.updated_at)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn update_asset_description_sqlx(
    pool: &SqlitePool,
    asset: &Asset,
) -> AppResult<()> {
    let result = sqlx::query(sql::UPDATE_ASSET_DESCRIPTION)
        .bind(&asset.description)
        .bind(&asset.updated_at)
        .bind(&asset.id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    if result.rows_affected() == 0 {
        return Err(format!("asset not found: {}", asset.id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::store::sql;

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

    #[test]
    fn sqlx_asset_repo_replaces_filters_and_updates_descriptions() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-asset-sqlx-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");
        let mut skill = test_asset("skill-a", AssetKind::Skill);
        let rule = test_asset("design", AssetKind::Rule);

        database
            .block_on(async {
                replace_source_assets_sqlx(database.pool(), "source-a", &[skill.clone(), rule])
                    .await?;
                let scoped_assets =
                    load_assets_sqlx(database.pool(), Some(AssetKind::Skill)).await?;
                skill.description = Some("Updated".to_string());
                update_asset_description_sqlx(database.pool(), &skill).await?;
                let all_assets = load_assets_sqlx(database.pool(), None).await?;
                AppResult::Ok((scoped_assets, all_assets))
            })
            .map(|(scoped_assets, all_assets)| {
                assert_eq!(scoped_assets.len(), 1);
                assert_eq!(scoped_assets[0].name, "skill-a");
                let updated = all_assets
                    .iter()
                    .find(|asset| asset.id == "asset-skill-a")
                    .expect("updated asset");
                assert_eq!(updated.description.as_deref(), Some("Updated"));
            })
            .expect("query SQLx asset repo");
        drop(database);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
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
