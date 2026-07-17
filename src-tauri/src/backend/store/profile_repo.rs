use crate::backend::dto::AppResult;
use crate::backend::models::TargetProfile;
use crate::backend::path_utils::normalize_path_for_storage;
use crate::backend::{app_paths::AppPathCatalog, models::AppKind};
use sqlx::SqlitePool;

use super::{
    codec::{decode_json, encode_json},
    sql,
};

pub(crate) async fn load_profiles_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<TargetProfile>> {
    let payloads = sqlx::query_scalar::<_, String>(sql::LIST_PROFILES)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    payloads
        .into_iter()
        .map(|payload| decode_json(payload).and_then(normalize_profile_paths))
        .collect()
}

pub(crate) async fn load_profile_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
) -> AppResult<Option<TargetProfile>> {
    sqlx::query_scalar::<_, String>(sql::LOAD_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .map(|payload| decode_json(payload).and_then(normalize_profile_paths))
        .transpose()
}

pub(crate) async fn upsert_profile_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile: &TargetProfile,
) -> AppResult<()> {
    let profile = normalize_profile_paths(profile.clone())?;
    sqlx::query(sql::UPSERT_PROFILE)
        .bind(tenant_id)
        .bind(&profile.id)
        .bind(encode_json(&profile)?)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn normalize_profile_paths(mut profile: TargetProfile) -> AppResult<TargetProfile> {
    profile.target_paths = profile
        .target_paths
        .iter()
        .map(|path| normalize_path_for_storage(path))
        .collect::<AppResult<Vec<_>>>()?;
    if profile.id == "cursor"
        && profile.app_kind == AppKind::Cursor
        && profile.target_paths == ["~/Library/Application Support/Cursor/skills"]
    {
        profile.target_paths =
            vec![AppPathCatalog::default_skill_target(AppKind::Cursor).to_string()];
    }
    Ok(profile)
}

pub(crate) async fn delete_profile_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    profile_id: &str,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_APP_SHORTCUT_BY_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_ASSET_MOUNT_OBSERVATIONS_BY_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_ASSET_MOUNTS_BY_PROFILE)
        .bind(tenant_id)
        .bind(profile_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_PROFILE)
        .bind(tenant_id)
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
                    upsert_profile_sqlx(database.pool(), "default", &profile).await?;
                    sqlx::query(
                        "INSERT INTO app_shortcut_items (
                        profile_id, display_icon, accent_color, enabled, sort_order
                    ) VALUES (?1, 'C', '#000000', 1, 0)",
                    )
                    .bind(&profile.id)
                    .execute(database.pool())
                    .await
                    .map_err(|error| error.to_string())?;
                    let profiles = load_profiles_sqlx(database.pool(), "default").await?;
                    let loaded_profile =
                        load_profile_sqlx(database.pool(), "default", &profile.id).await?;
                    let missing_profile =
                        load_profile_sqlx(database.pool(), "default", "missing").await?;
                    delete_profile_sqlx(database.pool(), "default", &profile.id).await?;
                    let remaining_profiles = load_profiles_sqlx(database.pool(), "default").await?;
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

    #[test]
    fn sqlx_profile_repo_isolates_same_id_by_tenant() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-profile-tenant-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let mut default_profile = test_profile("profile-a");
        default_profile.name = "Default profile".to_string();
        let mut tenant_profile = test_profile("profile-a");
        tenant_profile.name = "Tenant profile".to_string();

        let (default_loaded, tenant_loaded) = database
            .block_on(async {
                upsert_profile_sqlx(database.pool(), "default", &default_profile).await?;
                upsert_profile_sqlx(database.pool(), "tenant-a", &tenant_profile).await?;
                let default_loaded =
                    load_profile_sqlx(database.pool(), "default", "profile-a").await?;
                let tenant_loaded =
                    load_profile_sqlx(database.pool(), "tenant-a", "profile-a").await?;
                AppResult::Ok((default_loaded, tenant_loaded))
            })
            .expect("query tenant-scoped profiles");

        assert_eq!(
            default_loaded.expect("load default profile").name,
            "Default profile"
        );
        assert_eq!(
            tenant_loaded.expect("load tenant profile").name,
            "Tenant profile"
        );
        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_profile_repo_normalizes_home_target_paths_for_storage_and_loading() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-profile-home-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let mut profile = test_profile("profile-home");
        profile.target_paths = vec![dirs::home_dir()
            .expect("home directory")
            .join(".codex")
            .join("skills")
            .to_string_lossy()
            .to_string()];

        let loaded = database
            .block_on(async {
                upsert_profile_sqlx(database.pool(), "default", &profile).await?;
                load_profile_sqlx(database.pool(), "default", &profile.id).await
            })
            .expect("round trip profile")
            .expect("stored profile");

        assert_eq!(loaded.target_paths, vec!["~/.codex/skills"]);
        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_profile_repo_migrates_legacy_cursor_default_path_to_config_anchor() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-profile-cursor-path-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let mut profile = test_profile("cursor");
        profile.app_kind = AppKind::Cursor;
        profile.target_paths = vec!["~/Library/Application Support/Cursor/skills".to_string()];

        let loaded = database
            .block_on(async {
                sqlx::query(sql::UPSERT_PROFILE)
                    .bind("default")
                    .bind(&profile.id)
                    .bind(encode_json(&profile)?)
                    .execute(database.pool())
                    .await
                    .map_err(|error| error.to_string())?;
                load_profile_sqlx(database.pool(), "default", &profile.id).await
            })
            .expect("load legacy cursor profile")
            .expect("stored profile");

        assert_eq!(loaded.target_paths, vec!["@config/Cursor/skills"]);
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
