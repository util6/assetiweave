use crate::backend::models::{Asset, AssetFormat, AssetKind};
use crate::backend::runtime::{AppError, AppResult};
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};

use super::{
    codec::{decode_enum_app, encode_enum_app},
    sql,
};

pub(crate) async fn load_assets_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    kind: Option<AssetKind>,
) -> AppResult<Vec<Asset>> {
    let rows = if let Some(kind) = kind {
        sqlx::query(sql::LIST_ASSETS_BY_KIND)
            .bind(tenant_id)
            .bind(encode_enum_app(kind)?)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query(sql::LIST_ASSETS)
            .bind(tenant_id)
            .fetch_all(pool)
            .await?
    };
    rows.iter().map(map_sqlx_asset_row).collect()
}

pub(crate) async fn load_asset_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: &str,
) -> AppResult<Option<Asset>> {
    sqlx::query(sql::LOAD_ASSET)
        .bind(tenant_id)
        .bind(asset_id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(map_sqlx_asset_row)
        .transpose()
}

fn map_sqlx_asset_row(row: &SqliteRow) -> AppResult<Asset> {
    Ok(Asset {
        id: row.try_get(0)?,
        source_id: row.try_get(1)?,
        name: row.try_get(2)?,
        kind: decode_enum_app::<AssetKind>(row.try_get::<String, _>(3)?)?,
        format: decode_enum_app::<AssetFormat>(row.try_get::<String, _>(4)?)?,
        relative_path: row.try_get(5)?,
        absolute_path: row.try_get(6)?,
        entry_file: row.try_get(7)?,
        description: row.try_get(8)?,
        content_hash: row.try_get(9)?,
        discovered_at: row.try_get(10)?,
        updated_at: row.try_get(11)?,
        detector_id: row.try_get(12)?,
        detector_version: row.try_get::<i64, _>(13)? as u32,
    })
}

pub(crate) async fn replace_source_assets_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
    assets: &[Asset],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(sql::DELETE_ASSETS_BY_SOURCE)
        .bind(tenant_id)
        .bind(source_id)
        .execute(&mut *tx)
        .await?;
    for asset in assets {
        sqlx::query(sql::INSERT_ASSET)
            .bind(tenant_id)
            .bind(&asset.id)
            .bind(&asset.source_id)
            .bind(&asset.name)
            .bind(encode_enum_app(asset.kind)?)
            .bind(encode_enum_app(asset.format)?)
            .bind(&asset.relative_path)
            .bind(&asset.absolute_path)
            .bind(&asset.entry_file)
            .bind(&asset.description)
            .bind(&asset.content_hash)
            .bind(&asset.discovered_at)
            .bind(&asset.updated_at)
            .bind(&asset.detector_id)
            .bind(asset.detector_version)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn update_asset_description_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    asset: &Asset,
) -> AppResult<()> {
    let result = sqlx::query(sql::UPDATE_ASSET_DESCRIPTION)
        .bind(&asset.description)
        .bind(&asset.updated_at)
        .bind(tenant_id)
        .bind(&asset.id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("asset not found: {}", asset.id)));
    }
    Ok(())
}

#[cfg(test)]
#[path = "asset_repo_tests.rs"]
mod tests;
