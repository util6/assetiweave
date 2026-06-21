use crate::backend::dto::AppResult;
use crate::backend::models::TargetProfile;
use sqlx::SqlitePool;

use super::{
    codec::{decode_json, encode_json},
    sql,
};

pub(crate) async fn load_profiles_sqlx(pool: &SqlitePool) -> AppResult<Vec<TargetProfile>> {
    let payloads = sqlx::query_scalar::<_, String>(sql::LIST_PROFILES)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    payloads.into_iter().map(decode_json).collect()
}

pub(crate) async fn load_profile_sqlx(
    pool: &SqlitePool,
    profile_id: &str,
) -> AppResult<Option<TargetProfile>> {
    sqlx::query_scalar::<_, String>(sql::LOAD_PROFILE)
        .bind(profile_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .map(decode_json)
        .transpose()
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

        let (profiles, loaded_profile, missing_profile, remaining_profiles, shortcut_count) =
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
                    let loaded_profile = load_profile_sqlx(database.pool(), &profile.id).await?;
                    let missing_profile = load_profile_sqlx(database.pool(), "missing").await?;
                    delete_profile_sqlx(database.pool(), &profile.id).await?;
                    let remaining_profiles = load_profiles_sqlx(database.pool()).await?;
                    let shortcut_count: i64 =
                        sqlx::query_scalar("SELECT COUNT(*) FROM app_shortcut_items")
                            .fetch_one(database.pool())
                            .await
                            .map_err(|error| error.to_string())?;
                    AppResult::Ok((
                        profiles,
                        loaded_profile,
                        missing_profile,
                        remaining_profiles,
                        shortcut_count,
                    ))
                })
                .expect("query SQLx profile repo");

        assert_eq!(profiles, vec![profile.clone()]);
        assert_eq!(loaded_profile.expect("load profile by id").id, profile.id);
        assert!(missing_profile.is_none());
        assert!(remaining_profiles.is_empty());
        assert_eq!(shortcut_count, 0);
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
