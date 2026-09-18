use super::*;

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
