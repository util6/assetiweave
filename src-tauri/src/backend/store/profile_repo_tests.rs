use super::*;
use crate::backend::models::{AppKind, AssetKind, DeploymentStrategy, ProfileSafety, RuleSet};
use crate::backend::runtime::AppError;
use crate::backend::store::Database;
use uuid::Uuid;

#[test]
fn legacy_profile_payload_derives_a_provider_id_and_round_trips() {
    let legacy = serde_json::json!({
        "id": "codex",
        "name": "Codex",
        "app_kind": "codex",
        "target_paths": ["~/.codex/skills"],
        "supported_kinds": ["skill"],
        "deployment_strategy": "symlink_to_source",
        "enabled": true,
        "include": { "kinds": ["skill"], "tags": [], "groups": [], "sources": [], "path_patterns": [] },
        "exclude": { "kinds": ["unclassified"], "tags": [], "groups": [], "sources": [], "path_patterns": [] },
        "safety": { "allow_remove": false, "allow_overwrite": false }
    });
    let profile: TargetProfile = serde_json::from_value(legacy).expect("decode legacy profile");

    let migrated = normalize_profile_paths(profile).expect("migrate legacy profile");
    assert_eq!(migrated.target_provider_id, "codex");
    let encoded = serde_json::to_string(&migrated).expect("encode migrated profile");
    let decoded: TargetProfile = serde_json::from_str(&encoded).expect("decode migrated profile");
    assert_eq!(decoded.target_provider_id, "codex");
}

#[tokio::test]
async fn sqlx_profile_repo_round_trips_and_deletes_related_rows() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-profile-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let profile = test_profile("profile-a");

    upsert_profile_sqlx(database.pool(), "default", &profile)
        .await
        .expect("upsert profile");
    sqlx::query(
        "INSERT INTO app_shortcut_items (
        profile_id, display_icon, accent_color, enabled, sort_order
    ) VALUES (?1, 'C', '#000000', 1, 0)",
    )
    .bind(&profile.id)
    .execute(database.pool())
    .await
    .expect("insert shortcut");
    let profiles = load_profiles_sqlx(database.pool(), "default")
        .await
        .expect("load profiles");
    let loaded_profile = load_profile_sqlx(database.pool(), "default", &profile.id)
        .await
        .expect("load profile");
    let missing_profile = load_profile_sqlx(database.pool(), "default", "missing")
        .await
        .expect("load missing profile");
    delete_profile_sqlx(database.pool(), "default", &profile.id)
        .await
        .expect("delete profile");
    let remaining_profiles = load_profiles_sqlx(database.pool(), "default")
        .await
        .expect("load remaining profiles");
    let shortcut_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_shortcut_items")
        .fetch_one(database.pool())
        .await
        .map_err(AppError::external)
        .expect("count shortcuts");

    assert_eq!(profiles, vec![profile.clone()]);
    assert_eq!(loaded_profile.expect("load profile by id").id, profile.id);
    assert!(missing_profile.is_none());
    assert!(remaining_profiles.is_empty());
    assert_eq!(shortcut_count, 0);
    drop(database);
    cleanup_database(&db_path);
}

#[tokio::test]
async fn sqlx_profile_repo_isolates_same_id_by_tenant() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-profile-tenant-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let mut default_profile = test_profile("profile-a");
    default_profile.name = "Default profile".to_string();
    let mut tenant_profile = test_profile("profile-a");
    tenant_profile.name = "Tenant profile".to_string();

    upsert_profile_sqlx(database.pool(), "default", &default_profile)
        .await
        .expect("upsert default profile");
    upsert_profile_sqlx(database.pool(), "tenant-a", &tenant_profile)
        .await
        .expect("upsert tenant profile");
    let default_loaded = load_profile_sqlx(database.pool(), "default", "profile-a")
        .await
        .expect("load default profile");
    let tenant_loaded = load_profile_sqlx(database.pool(), "tenant-a", "profile-a")
        .await
        .expect("load tenant profile");

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

#[tokio::test]
async fn sqlx_profile_repo_normalizes_home_target_paths_for_storage_and_loading() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-profile-home-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let mut profile = test_profile("profile-home");
    profile.target_paths = vec![dirs::home_dir()
        .expect("home directory")
        .join(".codex")
        .join("skills")
        .to_string_lossy()
        .to_string()];

    upsert_profile_sqlx(database.pool(), "default", &profile)
        .await
        .expect("upsert profile");
    let loaded = load_profile_sqlx(database.pool(), "default", &profile.id)
        .await
        .expect("round trip profile")
        .expect("stored profile");

    assert_eq!(loaded.target_paths, vec!["~/.codex/skills"]);
    drop(database);
    cleanup_database(&db_path);
}

fn test_profile(id: &str) -> TargetProfile {
    TargetProfile {
        id: id.to_string(),
        name: id.to_string(),
        app_kind: Some(AppKind::Codex),
        target_provider_id: "codex".to_string(),
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
