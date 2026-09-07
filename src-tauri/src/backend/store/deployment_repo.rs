use crate::backend::models::DeploymentState;
use crate::backend::runtime::AppResult;
use sqlx::{Row, SqlitePool};

use super::{codec::encode_enum_app, sql};

pub(crate) async fn upsert_deployment_state_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    state: &DeploymentState,
) -> AppResult<()> {
    sqlx::query(sql::UPSERT_DEPLOYMENT_STATE)
        .bind(tenant_id)
        .bind(&state.profile_id)
        .bind(&state.asset_id)
        .bind(&state.target_path)
        .bind(encode_enum_app(state.strategy)?)
        .bind(&state.source_hash)
        .bind(&state.deployed_at)
        .bind(&state.managed_by)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn is_managed_deployment_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
    asset_id: &str,
    target_path: &str,
) -> AppResult<bool> {
    let managed_by: Option<String> = sqlx::query_scalar(sql::GET_MANAGED_DEPLOYMENT)
        .bind(tenant_id)
        .bind(profile_id)
        .bind(asset_id)
        .bind(target_path)
        .fetch_optional(pool)
        .await?;
    Ok(managed_by.as_deref() == Some("assetiweave"))
}

pub(crate) async fn count_deployment_state_by_profile_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
) -> AppResult<usize> {
    let count: i64 = sqlx::query_scalar(sql::COUNT_DEPLOYMENT_STATE_BY_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .fetch_one(pool)
        .await?;
    Ok(count as usize)
}

pub(crate) async fn load_managed_deployment_targets_by_profile_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
) -> AppResult<Vec<(String, String)>> {
    let rows = sqlx::query(sql::LIST_MANAGED_DEPLOYMENT_TARGETS_BY_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|row| Ok((row.try_get(0)?, row.try_get(1)?)))
        .collect()
}

#[cfg(test)]
pub(crate) async fn delete_deployment_state_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
    asset_id: &str,
    target_path: &str,
) -> AppResult<()> {
    sqlx::query(sql::DELETE_DEPLOYMENT_STATE)
        .bind(tenant_id)
        .bind(profile_id)
        .bind(asset_id)
        .bind(target_path)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn delete_orphan_deployment_state_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    sqlx::query(sql::DELETE_ORPHAN_DEPLOYMENT_STATE)
        .bind(tenant_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::DeploymentStrategy;
    use crate::backend::runtime::AppError;
    use uuid::Uuid;

    #[tokio::test]
    async fn sqlx_deployment_state_round_trips_deletes_and_cleans_orphans() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-deployment-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open_async(&db_path)
            .await
            .expect("open database");

        insert_asset(database.pool(), "asset-a")
            .await
            .expect("insert asset");
        upsert_deployment_state_sqlx(
            database.pool(),
            "default",
            &test_state("profile-a", "asset-a", "/target/a", "assetiweave"),
        )
        .await
        .expect("upsert managed state");
        upsert_deployment_state_sqlx(
            database.pool(),
            "default",
            &test_state("profile-a", "asset-b", "/target/b", "other-tool"),
        )
        .await
        .expect("upsert other state");
        upsert_deployment_state_sqlx(
            database.pool(),
            "default",
            &test_state("profile-b", "asset-a", "/target/c", "assetiweave"),
        )
        .await
        .expect("upsert profile b state");

        assert!(is_managed_deployment_sqlx(
            database.pool(),
            "default",
            "profile-a",
            "asset-a",
            "/target/a"
        )
        .await
        .expect("check managed a"));
        assert!(!is_managed_deployment_sqlx(
            database.pool(),
            "default",
            "profile-a",
            "asset-b",
            "/target/b"
        )
        .await
        .expect("check unmanaged b"));
        assert_eq!(
            count_deployment_state_by_profile_sqlx(database.pool(), "default", "profile-a")
                .await
                .expect("count profile a"),
            2
        );
        assert_eq!(
            count_deployment_state_by_profile_sqlx(database.pool(), "default", "profile-b")
                .await
                .expect("count profile b"),
            1
        );
        assert_eq!(
            load_managed_deployment_targets_by_profile_sqlx(
                database.pool(),
                "default",
                "profile-a",
            )
            .await
            .expect("load managed targets"),
            vec![("asset-a".to_string(), "/target/a".to_string())]
        );

        delete_deployment_state_sqlx(
            database.pool(),
            "default",
            "profile-a",
            "asset-b",
            "/target/b",
        )
        .await
        .expect("delete deployment state");
        upsert_deployment_state_sqlx(
            database.pool(),
            "default",
            &test_state("profile-a", "asset-b", "/target/b", "assetiweave"),
        )
        .await
        .expect("upsert re-managed state");
        delete_orphan_deployment_state_sqlx(database.pool(), "default")
            .await
            .expect("delete orphan deployment state");

        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM deployment_state")
            .fetch_one(database.pool())
            .await
            .map_err(AppError::external)
            .expect("count rows");
        assert_eq!(rows, 2);

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
        .await?;
        Ok(())
    }

    fn cleanup_database(db_path: &std::path::Path) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
