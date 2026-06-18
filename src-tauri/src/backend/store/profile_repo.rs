use crate::backend::dto::AppResult;
use crate::backend::models::TargetProfile;
use rusqlite::{params, Connection};
use sqlx::SqlitePool;

use super::{
    codec::{db_error, decode_json, encode_json, to_sql_error},
    sql,
};

pub(crate) fn load_profiles(conn: &Connection) -> AppResult<Vec<TargetProfile>> {
    let mut stmt = conn.prepare(sql::LIST_PROFILES).map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            let payload: String = row.get(0)?;
            decode_json(payload).map_err(to_sql_error)
        })
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) async fn load_profiles_sqlx(pool: &SqlitePool) -> AppResult<Vec<TargetProfile>> {
    let payloads = sqlx::query_scalar::<_, String>(sql::LIST_PROFILES)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    payloads.into_iter().map(decode_json).collect()
}

pub(crate) fn upsert_profile(conn: &Connection, profile: &TargetProfile) -> AppResult<()> {
    conn.execute(
        sql::UPSERT_PROFILE,
        params![profile.id, encode_json(profile)?],
    )
    .map_err(db_error)?;
    Ok(())
}

pub(crate) async fn upsert_profile_sqlx(
    pool: &SqlitePool,
    profile: &TargetProfile,
) -> AppResult<()> {
    sqlx::query(sql::UPSERT_PROFILE)
        .bind(&profile.id)
        .bind(encode_json(profile)?)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn delete_profile(conn: &Connection, profile_id: &str) -> AppResult<()> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    tx.execute(sql::DELETE_APP_SHORTCUT_BY_PROFILE, params![profile_id])
        .map_err(db_error)?;
    tx.execute(
        sql::DELETE_ASSET_MOUNT_OBSERVATIONS_BY_PROFILE,
        params![profile_id],
    )
    .map_err(db_error)?;
    tx.execute(sql::DELETE_ASSET_MOUNTS_BY_PROFILE, params![profile_id])
        .map_err(db_error)?;
    tx.execute(sql::DELETE_PROFILE, params![profile_id])
        .map_err(db_error)?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn delete_profile_sqlx(pool: &SqlitePool, profile_id: &str) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_APP_SHORTCUT_BY_PROFILE)
        .bind(profile_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_ASSET_MOUNT_OBSERVATIONS_BY_PROFILE)
        .bind(profile_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_ASSET_MOUNTS_BY_PROFILE)
        .bind(profile_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_PROFILE)
        .bind(profile_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn count_deployment_state_by_profile(
    conn: &Connection,
    profile_id: &str,
) -> AppResult<usize> {
    conn.query_row(
        sql::COUNT_DEPLOYMENT_STATE_BY_PROFILE,
        params![profile_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count as usize)
    .map_err(db_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{AppKind, AssetKind, DeploymentStrategy, ProfileSafety, RuleSet};
    use crate::backend::store::Database;
    use uuid::Uuid;

    #[test]
    fn sqlx_profile_repo_round_trips_and_deletes_related_rows() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-profile-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let profile = test_profile("profile-a");

        database
            .block_on(async {
                upsert_profile_sqlx(database.pool(), &profile).await?;
                sqlx::query(
                    "INSERT INTO app_shortcut_items (
                        profile_id, display_icon, accent_color, enabled, sort_order
                    ) VALUES (?1, 'C', '#000000', 1, 0)",
                )
                .bind(&profile.id)
                .execute(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                let profiles = load_profiles_sqlx(database.pool()).await?;
                delete_profile_sqlx(database.pool(), &profile.id).await?;
                let remaining_profiles = load_profiles_sqlx(database.pool()).await?;
                let shortcut_count: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM app_shortcut_items")
                        .fetch_one(database.pool())
                        .await
                        .map_err(|error| error.to_string())?;
                AppResult::Ok((profiles, remaining_profiles, shortcut_count))
            })
            .map(|(profiles, remaining_profiles, shortcut_count)| {
                assert_eq!(profiles, vec![profile]);
                assert!(remaining_profiles.is_empty());
                assert_eq!(shortcut_count, 0);
            })
            .expect("query SQLx profile repo");
        drop(database);
        cleanup_database(&db_path);
    }

    fn test_profile(id: &str) -> TargetProfile {
        TargetProfile {
            id: id.to_string(),
            name: id.to_string(),
            app_kind: AppKind::Codex,
            target_paths: vec![format!("/tmp/{id}")],
            supported_kinds: vec![AssetKind::Skill],
            deployment_strategy: DeploymentStrategy::SymlinkToSource,
            enabled: true,
            include: empty_rules(),
            exclude: empty_rules(),
            safety: ProfileSafety {
                allow_remove: false,
                allow_overwrite: false,
            },
        }
    }

    fn empty_rules() -> RuleSet {
        RuleSet {
            kinds: Vec::new(),
            tags: Vec::new(),
            groups: Vec::new(),
            sources: Vec::new(),
            path_patterns: Vec::new(),
        }
    }

    fn cleanup_database(db_path: &std::path::Path) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
