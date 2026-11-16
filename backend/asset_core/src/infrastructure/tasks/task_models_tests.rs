use super::*;

#[test]
fn conversation_long_running_kinds_default_to_managed_admission() {
    assert_eq!(
        TaskKind::ConversationUsageScan.default_admission(),
        TaskAdmission::Managed
    );
    assert_eq!(
        TaskKind::ModelPricingSync.default_admission(),
        TaskAdmission::Managed
    );
}

#[test]
fn managed_default_kind_spec_is_user_visible_without_override() {
    let usage_spec = TaskSpec::new(TaskKind::ConversationUsageScan, None);
    assert_eq!(usage_spec.admission, TaskAdmission::Managed);
    assert!(usage_spec.resolved_user_visible());

    let pricing_spec = TaskSpec::new(TaskKind::ModelPricingSync, None);
    assert_eq!(pricing_spec.admission, TaskAdmission::Managed);
    assert!(pricing_spec.resolved_user_visible());
}

#[test]
fn explicit_shadow_admission_still_flattens_user_visibility() {
    for kind in [TaskKind::ConversationUsageScan, TaskKind::ModelPricingSync] {
        let spec = TaskSpec::new(kind, None)
            .with_admission(TaskAdmission::Shadow)
            .with_user_visible(true);
        assert!(
            !spec.resolved_user_visible(),
            "{kind:?} 显式 Shadow 时必须对 UI 隐形"
        );
    }
}
