use super::*;
use crate::backend::{
    domain::{DeploymentStrategy, ProfileSafety, RuleSet},
    store::Database,
};
use uuid::Uuid;

#[test]
fn target_profile_input_normalizes_absolute_home_paths_before_returning() {
    let home_target = dirs::home_dir()
        .expect("home directory")
        .join(".codex")
        .join("skills")
        .to_string_lossy()
        .to_string();

    let profile = target_profile_from_input(TargetProfileInput {
        id: Some("custom-home".to_string()),
        name: "Custom Home".to_string(),
        app_kind: Some(AppKind::Custom),
        target_provider_id: None,
        target_paths: Some(vec![home_target]),
        supported_kinds: None,
        deployment_strategy: None,
        enabled: None,
        include: None,
        exclude: None,
        safety: None,
    })
    .expect("build target profile");

    assert_eq!(profile.target_paths, vec!["~/.codex/skills"]);
}

#[tokio::test]
async fn application_profile_persistence_normalizes_paths_around_store_access() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-app-profile-normalization-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = Database::open_async(&db_path).await.expect("open database");
    let home_target = dirs::home_dir()
        .expect("home directory")
        .join(".codex")
        .join("skills")
        .to_string_lossy()
        .to_string();
    let mut profile = TargetProfile {
        id: "profile-home".to_string(),
        name: "Home profile".to_string(),
        app_kind: Some(AppKind::Codex),
        target_provider_id: "codex".to_string(),
        target_paths: vec![home_target.clone()],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        enabled: true,
        include: RuleSet {
            kinds: Vec::new(),
            tags: Vec::new(),
            groups: Vec::new(),
            sources: Vec::new(),
            path_patterns: Vec::new(),
        },
        exclude: RuleSet {
            kinds: Vec::new(),
            tags: Vec::new(),
            groups: Vec::new(),
            sources: Vec::new(),
            path_patterns: Vec::new(),
        },
        safety: ProfileSafety {
            allow_remove: false,
            allow_overwrite: false,
        },
    };

    crate::backend::store::upsert_profile_sqlx(database.pool(), "default", &profile)
        .await
        .expect("seed an existing profile with an absolute path");
    let loaded = load_target_profile_sqlx(database.pool(), "default", &profile.id)
        .await
        .expect("load profile through application")
        .expect("profile exists");
    assert_eq!(loaded.target_paths, vec!["~/.codex/skills"]);

    profile.target_paths = vec![home_target];
    upsert_target_profile_sqlx(database.pool(), "default", &profile)
        .await
        .expect("upsert profile through application");
    let persisted =
        crate::backend::store::load_profile_sqlx(database.pool(), "default", &profile.id)
            .await
            .expect("load raw persisted profile")
            .expect("profile exists");
    assert_eq!(persisted.target_paths, vec!["~/.codex/skills"]);

    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}
