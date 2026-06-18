use crate::backend::dto::AppResult;
use crate::backend::models::{AssetMount, DeploymentStrategy};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};

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

pub(crate) async fn load_asset_mounts_sqlx(
    pool: &SqlitePool,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMount>> {
    let rows = sqlx::query(sql::LIST_ASSET_MOUNTS)
        .bind(asset_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;

    rows.iter().map(map_sqlx_mount).collect()
}

pub(crate) async fn load_enabled_asset_mounts_sqlx(
    pool: &SqlitePool,
    profile_id: Option<&str>,
) -> AppResult<Vec<AssetMount>> {
    let rows = sqlx::query(sql::LIST_ENABLED_ASSET_MOUNTS)
        .bind(profile_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;

    rows.iter().map(map_sqlx_mount).collect()
}

pub(crate) async fn delete_orphan_asset_mounts_sqlx(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query(sql::DELETE_ORPHAN_ASSET_MOUNTS)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
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

pub(crate) async fn set_asset_mount_sqlx(
    pool: &SqlitePool,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let now = Utc::now().to_rfc3339();
    let created_at = load_asset_mount_sqlx(pool, asset_id, profile_id)
        .await?
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
    upsert_asset_mount_sqlx(pool, &mount).await?;
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

async fn load_asset_mount_sqlx(
    pool: &SqlitePool,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<Option<AssetMount>> {
    sqlx::query(sql::GET_ASSET_MOUNT)
        .bind(asset_id)
        .bind(profile_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .map(|row| map_sqlx_mount(&row))
        .transpose()
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

async fn upsert_asset_mount_sqlx(pool: &SqlitePool, mount: &AssetMount) -> AppResult<()> {
    sqlx::query(sql::UPSERT_ASSET_MOUNT)
        .bind(&mount.asset_id)
        .bind(&mount.profile_id)
        .bind(if mount.enabled { 1_i64 } else { 0_i64 })
        .bind(encode_enum(mount.strategy)?)
        .bind(&mount.created_at)
        .bind(&mount.updated_at)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
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

fn map_sqlx_mount(row: &SqliteRow) -> AppResult<AssetMount> {
    Ok(AssetMount {
        asset_id: row.try_get(0).map_err(|error| error.to_string())?,
        profile_id: row.try_get(1).map_err(|error| error.to_string())?,
        enabled: row
            .try_get::<i64, _>(2)
            .map_err(|error| error.to_string())?
            == 1,
        strategy: decode_enum(
            row.try_get::<String, _>(3)
                .map_err(|error| error.to_string())?,
        )?,
        created_at: row.try_get(4).map_err(|error| error.to_string())?,
        updated_at: row.try_get(5).map_err(|error| error.to_string())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn sqlx_mount_repo_sets_lists_filters_and_cleans_orphans() {
        let db_path =
            std::env::temp_dir().join(format!("assetiweave-mount-sqlx-{}.sqlite", Uuid::new_v4()));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");

        database
            .block_on(async {
                insert_asset(database.pool(), "asset-a").await?;
                let initial = set_asset_mount_sqlx(
                    database.pool(),
                    "asset-a",
                    "profile-a",
                    true,
                    DeploymentStrategy::SymlinkToSource,
                )
                .await?;
                let updated = set_asset_mount_sqlx(
                    database.pool(),
                    "asset-a",
                    "profile-a",
                    false,
                    DeploymentStrategy::CopyToTarget,
                )
                .await?;
                set_asset_mount_sqlx(
                    database.pool(),
                    "asset-b",
                    "profile-a",
                    true,
                    DeploymentStrategy::SymlinkToSource,
                )
                .await?;

                let scoped = load_asset_mounts_sqlx(database.pool(), Some("asset-a")).await?;
                let enabled =
                    load_enabled_asset_mounts_sqlx(database.pool(), Some("profile-a")).await?;
                delete_orphan_asset_mounts_sqlx(database.pool()).await?;
                let all_after_cleanup = load_asset_mounts_sqlx(database.pool(), None).await?;

                AppResult::Ok((initial, updated, scoped, enabled, all_after_cleanup))
            })
            .map(|(initial, updated, scoped, enabled, all_after_cleanup)| {
                assert_eq!(initial.created_at, updated.created_at);
                assert!(!updated.enabled);
                assert_eq!(updated.strategy, DeploymentStrategy::CopyToTarget);
                assert_eq!(scoped, vec![updated]);
                assert_eq!(enabled.len(), 1);
                assert_eq!(enabled[0].asset_id, "asset-b");
                assert_eq!(all_after_cleanup.len(), 1);
                assert_eq!(all_after_cleanup[0].asset_id, "asset-a");
            })
            .expect("query SQLx mount repo");
        drop(database);
        cleanup_database(&db_path);
    }

    async fn insert_asset(pool: &SqlitePool, asset_id: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO assets (
                id, source_id, name, kind, format, relative_path, absolute_path,
                entry_file, description, content_hash, discovered_at, updated_at
            ) VALUES (?1, 'source-a', ?1, 'skill', 'markdown', ?1, ?1, NULL, NULL, NULL, ?2, ?2)
            "#,
        )
        .bind(asset_id)
        .bind("2026-06-18T00:00:00Z")
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn cleanup_database(db_path: &std::path::Path) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
