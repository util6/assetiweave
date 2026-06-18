use crate::backend::dto::AppResult;
use crate::backend::models::DeploymentState;
use rusqlite::{params, Connection, OptionalExtension};
use sqlx::SqlitePool;

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

pub(crate) async fn upsert_deployment_state_sqlx(
    pool: &SqlitePool,
    state: &DeploymentState,
) -> AppResult<()> {
    sqlx::query(sql::UPSERT_DEPLOYMENT_STATE)
        .bind(&state.profile_id)
        .bind(&state.asset_id)
        .bind(&state.target_path)
        .bind(encode_enum(state.strategy)?)
        .bind(&state.source_hash)
        .bind(&state.deployed_at)
        .bind(&state.managed_by)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
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

pub(crate) async fn is_managed_deployment_sqlx(
    pool: &SqlitePool,
    profile_id: &str,
    asset_id: &str,
    target_path: &str,
) -> AppResult<bool> {
    let managed_by: Option<String> = sqlx::query_scalar(sql::GET_MANAGED_DEPLOYMENT)
        .bind(profile_id)
        .bind(asset_id)
        .bind(target_path)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(managed_by.as_deref() == Some("assetiweave"))
}

pub(crate) async fn count_deployment_state_by_profile_sqlx(
    pool: &SqlitePool,
    profile_id: &str,
) -> AppResult<usize> {
    let count: i64 = sqlx::query_scalar(sql::COUNT_DEPLOYMENT_STATE_BY_PROFILE)
        .bind(profile_id)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(count as usize)
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

#[cfg(test)]
pub(crate) async fn delete_deployment_state_sqlx(
    pool: &SqlitePool,
    profile_id: &str,
    asset_id: &str,
    target_path: &str,
) -> AppResult<()> {
    sqlx::query(sql::DELETE_DEPLOYMENT_STATE)
        .bind(profile_id)
        .bind(asset_id)
        .bind(target_path)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn delete_orphan_deployment_state_sqlx(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query(sql::DELETE_ORPHAN_DEPLOYMENT_STATE)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::DeploymentStrategy;
    use uuid::Uuid;

    #[test]
    fn sqlx_deployment_state_round_trips_deletes_and_cleans_orphans() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-deployment-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");

        database
            .block_on(async {
                insert_asset(database.pool(), "asset-a").await?;
                upsert_deployment_state_sqlx(
                    database.pool(),
                    &test_state("profile-a", "asset-a", "/target/a", "assetiweave"),
                )
                .await?;
                upsert_deployment_state_sqlx(
                    database.pool(),
                    &test_state("profile-a", "asset-b", "/target/b", "other-tool"),
                )
                .await?;
                upsert_deployment_state_sqlx(
                    database.pool(),
                    &test_state("profile-b", "asset-a", "/target/c", "assetiweave"),
                )
                .await?;

                assert!(
                    is_managed_deployment_sqlx(
                        database.pool(),
                        "profile-a",
                        "asset-a",
                        "/target/a"
                    )
                    .await?
                );
                assert!(
                    !is_managed_deployment_sqlx(
                        database.pool(),
                        "profile-a",
                        "asset-b",
                        "/target/b"
                    )
                    .await?
                );
                assert_eq!(
                    count_deployment_state_by_profile_sqlx(database.pool(), "profile-a").await?,
                    2
                );
                assert_eq!(
                    count_deployment_state_by_profile_sqlx(database.pool(), "profile-b").await?,
                    1
                );

                delete_deployment_state_sqlx(database.pool(), "profile-a", "asset-b", "/target/b")
                    .await?;
                upsert_deployment_state_sqlx(
                    database.pool(),
                    &test_state("profile-a", "asset-b", "/target/b", "assetiweave"),
                )
                .await?;
                delete_orphan_deployment_state_sqlx(database.pool()).await?;

                let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM deployment_state")
                    .fetch_one(database.pool())
                    .await
                    .map_err(|error| error.to_string())?;
                AppResult::Ok(rows)
            })
            .map(|rows| assert_eq!(rows, 2))
            .expect("query SQLx deployment repo");
        drop(database);
        cleanup_database(&db_path);
    }

    fn test_state(
        profile_id: &str,
        asset_id: &str,
        target_path: &str,
        managed_by: &str,
    ) -> DeploymentState {
        DeploymentState {
            profile_id: profile_id.to_string(),
            asset_id: asset_id.to_string(),
            target_path: target_path.to_string(),
            strategy: DeploymentStrategy::SymlinkToSource,
            source_hash: "hash".to_string(),
            deployed_at: "2026-06-18T00:00:00Z".to_string(),
            managed_by: managed_by.to_string(),
        }
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
