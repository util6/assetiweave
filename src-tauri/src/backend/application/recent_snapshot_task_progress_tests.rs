use super::*;

#[test]
fn recent_snapshot_task_pipeline_has_six_ordered_stages() {
    let stages = recent_snapshot_task_stages();
    let ids = stages
        .iter()
        .map(|stage| stage.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec![
            "claim",
            "load_facts",
            "agent_execution",
            "validation",
            "publish",
            "cleanup_session",
        ]
    );
    assert!(stages
        .iter()
        .all(|stage| stage.status == crate::backend::runtime::tasks::StageStatus::Pending));
}
