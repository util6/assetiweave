use crate::backend::dto::AppResult;
use crate::backend::models::{Asset, AssetFormat, AssetKind};
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};

use super::{
    codec::{decode_enum, encode_enum},
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
            .bind(encode_enum(kind)?)
            .fetch_all(pool)
            .await
            .map_err(|error| error.to_string())?
    } else {
        sqlx::query(sql::LIST_ASSETS)
            .bind(tenant_id)
            .fetch_all(pool)
            .await
            .map_err(|error| error.to_string())?
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
        .await
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(map_sqlx_asset_row)
        .transpose()
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

pub(crate) async fn replace_source_assets_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
    assets: &[Asset],
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_ASSETS_BY_SOURCE)
        .bind(tenant_id)
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    for asset in assets {
        sqlx::query(sql::INSERT_ASSET)
            .bind(tenant_id)
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
    tenant_id: &str,
    asset: &Asset,
) -> AppResult<()> {
    let result = sqlx::query(sql::UPDATE_ASSET_DESCRIPTION)
        .bind(&asset.description)
        .bind(&asset.updated_at)
        .bind(tenant_id)
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
                replace_source_assets_sqlx(
                    database.pool(),
                    "default",
                    "source-a",
                    &[skill.clone(), rule],
                )
                .await?;
                let scoped_assets =
                    load_assets_sqlx(database.pool(), "default", Some(AssetKind::Skill)).await?;
                let loaded_skill = load_asset_sqlx(database.pool(), "default", &skill.id).await?;
                let missing_asset = load_asset_sqlx(database.pool(), "default", "missing").await?;
                skill.description = Some("Updated".to_string());
                update_asset_description_sqlx(database.pool(), "default", &skill).await?;
                let all_assets = load_assets_sqlx(database.pool(), "default", None).await?;
                AppResult::Ok((scoped_assets, loaded_skill, missing_asset, all_assets))
            })
            .map(|(scoped_assets, loaded_skill, missing_asset, all_assets)| {
                assert_eq!(scoped_assets.len(), 1);
                assert_eq!(scoped_assets[0].name, "skill-a");
                assert_eq!(loaded_skill.expect("load asset by id").id, skill.id);
                assert!(missing_asset.is_none());
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

    #[test]
    fn sqlx_asset_repo_isolates_source_replacement_by_tenant() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-asset-tenant-sqlx-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");
        let mut default_asset = test_asset("skill-a", AssetKind::Skill);
        default_asset.absolute_path = "/tmp/default-skill-a.md".to_string();
        let mut tenant_asset = test_asset("skill-a", AssetKind::Skill);
        tenant_asset.absolute_path = "/tmp/tenant-skill-a.md".to_string();

        let (default_assets, tenant_assets) = database
            .block_on(async {
                replace_source_assets_sqlx(
                    database.pool(),
                    "default",
                    "source-a",
                    &[default_asset],
                )
                .await?;
                replace_source_assets_sqlx(
                    database.pool(),
                    "tenant-a",
                    "source-a",
                    &[tenant_asset],
                )
                .await?;
                replace_source_assets_sqlx(database.pool(), "default", "source-a", &[]).await?;
                let default_assets = load_assets_sqlx(database.pool(), "default", None).await?;
                let tenant_assets = load_assets_sqlx(database.pool(), "tenant-a", None).await?;
                AppResult::Ok((default_assets, tenant_assets))
            })
            .expect("query tenant-scoped assets");

        assert!(default_assets.is_empty());
        assert_eq!(tenant_assets.len(), 1);
        assert_eq!(tenant_assets[0].absolute_path, "/tmp/tenant-skill-a.md");
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
