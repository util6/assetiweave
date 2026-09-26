use super::*;

#[test]
fn deployment_planning_classifies_prepared_mount_candidates() {
    let plan = build_deployment_plan(
        vec![
            plan_candidate("create", true, true, PhysicalMountState::NotMounted),
            plan_candidate("mounted", true, true, PhysicalMountState::Mounted),
            plan_candidate("conflict", true, true, PhysicalMountState::Conflict),
            plan_candidate("disabled", false, true, PhysicalMountState::NotMounted),
            plan_candidate("unsupported", true, false, PhysicalMountState::NotMounted),
        ],
        None,
    );

    assert_eq!(plan.actions.len(), 5);
    assert_eq!(plan.summary.create_count, 1);
    assert_eq!(plan.summary.skip_count, 3);
    assert_eq!(plan.summary.conflict_count, 1);
    assert_eq!(plan.actions[0].target_path, "/targets/create");
    assert_eq!(
        plan.actions[0].display_target_path.as_deref(),
        Some("~/targets/create")
    );
}

fn plan_candidate(
    id: &str,
    profile_enabled: bool,
    supported: bool,
    state: PhysicalMountState,
) -> DeploymentPlanCandidate {
    DeploymentPlanCandidate {
        asset_id: format!("asset-{id}"),
        asset_kind: AssetKind::Skill,
        profile_id: format!("profile-{id}"),
        profile_name: format!("Profile {id}"),
        source_path: format!("/sources/{id}"),
        display_source_path: format!("~/sources/{id}"),
        target_path: format!("/targets/{id}"),
        display_target_path: format!("~/targets/{id}"),
        strategy: DeploymentStrategy::SymlinkToSource,
        profile_enabled,
        supported,
        state,
    }
}

#[test]
fn app_kind_uses_frontend_compatible_names() {
    assert_eq!(
        serde_json::to_string(&AppKind::OpenCode).unwrap(),
        "\"opencode\""
    );
    assert_eq!(
        serde_json::to_string(&AppKind::OpenClaw).unwrap(),
        "\"openclaw\""
    );
    assert_eq!(
        serde_json::from_str::<AppKind>("\"open_code\"").unwrap(),
        AppKind::OpenCode
    );
    assert_eq!(
        serde_json::from_str::<AppKind>("\"open_claw\"").unwrap(),
        AppKind::OpenClaw
    );
}
