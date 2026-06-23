use super::prelude::*;

pub(crate) fn target_profile_from_input(input: TargetProfileInput) -> AppResult<TargetProfile> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("profile name is required".to_string());
    }

    let id = input
        .id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| slug_profile_id(&name));
    if id.is_empty() {
        return Err("profile id is required".to_string());
    }

    let target_paths = input
        .target_paths
        .unwrap_or_default()
        .into_iter()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    if target_paths.is_empty() {
        return Err("profile target path is required".to_string());
    }

    let profile = TargetProfile {
        id,
        name,
        app_kind: input.app_kind.unwrap_or(AppKind::Custom),
        target_paths,
        supported_kinds: input
            .supported_kinds
            .unwrap_or_else(|| vec![AssetKind::Skill]),
        deployment_strategy: input
            .deployment_strategy
            .unwrap_or(DeploymentStrategy::SymlinkToSource),
        enabled: input.enabled.unwrap_or(true),
        include: input.include.unwrap_or_else(default_profile_include),
        exclude: input.exclude.unwrap_or_else(default_profile_exclude),
        safety: input.safety.unwrap_or(ProfileSafety {
            allow_remove: false,
            allow_overwrite: false,
        }),
    };
    validate_target_profile(&profile)?;
    Ok(profile)
}

pub(crate) fn ensure_profile_can_be_deleted_sqlx(
    db: &crate::backend::store::Database,
    profile_id: &str,
) -> AppResult<()> {
    if crate::backend::defaults::is_default_app_profile_id(profile_id) {
        return Err(format!("default app cannot be deleted: {profile_id}"));
    }

    let deployment_count = db.block_on(async {
        crate::backend::store::count_deployment_state_by_profile_sqlx(db.pool(), profile_id).await
    })?;
    if deployment_count > 0 {
        return Err(format!("profile has managed deployments: {profile_id}"));
    }

    if scan_asset_mount_statuses_sqlx(db, None)?
        .iter()
        .any(|status| {
            status.profile_id == profile_id && status.state == PhysicalMountStateDto::Mounted
        })
    {
        return Err(format!("profile has mounted assets: {profile_id}"));
    }

    Ok(())
}

pub(crate) fn ensure_default_profile_update_is_allowed(
    existing: &TargetProfile,
    next: &TargetProfile,
) -> AppResult<()> {
    if crate::backend::defaults::is_default_app_profile_id(&existing.id) && existing.id != next.id {
        return Err(format!(
            "default app profile id cannot be changed: {}",
            existing.id
        ));
    }
    Ok(())
}

pub(crate) fn validate_target_profile(profile: &TargetProfile) -> AppResult<()> {
    if profile.id.trim().is_empty() {
        return Err("profile id is required".to_string());
    }
    if profile.name.trim().is_empty() {
        return Err("profile name is required".to_string());
    }
    if profile
        .target_paths
        .iter()
        .map(|path| path.trim())
        .all(str::is_empty)
    {
        return Err("profile target path is required".to_string());
    }
    Ok(())
}

fn default_profile_include() -> RuleSet {
    RuleSet {
        kinds: vec![AssetKind::Skill],
        tags: vec![],
        groups: vec![],
        sources: vec![],
        path_patterns: vec![],
    }
}

fn default_profile_exclude() -> RuleSet {
    RuleSet {
        kinds: vec![AssetKind::Unclassified],
        tags: vec![],
        groups: vec![],
        sources: vec![],
        path_patterns: vec![],
    }
}

fn slug_profile_id(name: &str) -> String {
    let mut id = String::new();
    let mut last_was_separator = false;
    for character in name.trim().chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            id.push(character);
            last_was_separator = false;
        } else if !last_was_separator && !id.is_empty() {
            id.push('-');
            last_was_separator = true;
        }
    }
    id.trim_matches('-').to_string()
}
