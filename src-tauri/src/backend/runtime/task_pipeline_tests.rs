use super::*;

#[test]
fn task_category_from_str_and_display() {
    let cat = TaskCategory::from("custom/sync");
    assert_eq!(cat.as_str(), "custom/sync");
    assert_eq!(format!("{cat}"), "custom/sync");

    let serialized = serde_json::to_string(&cat).expect("serialize");
    assert_eq!(serialized, "\"custom/sync\"");

    let deserialized: TaskCategory = serde_json::from_str(&serialized).expect("deserialize");
    assert_eq!(deserialized, cat);
}

#[test]
fn task_category_from_task_kind() {
    assert_eq!(
        TaskCategory::from(TaskKind::ConversationSync).as_str(),
        "conversation/sync"
    );
    assert_eq!(
        TaskCategory::from(TaskKind::Backup).as_str(),
        "system/backup"
    );
    assert_eq!(
        TaskCategory::from(TaskKind::BatchMount).as_str(),
        "mount/batch"
    );
}

#[test]
fn pipeline_descriptor_builder_and_to_initial_stages() {
    let pipeline = PipelineDescriptor::builder()
        .stage("stage_prepare", "准备环境")
        .stage_with_visibility("stage_internal", "内部索引", false)
        .stage("stage_execute", "执行操作")
        .build();

    assert_eq!(pipeline.stages.len(), 3);
    assert_eq!(pipeline.stages[0].id, "stage_prepare");
    assert_eq!(pipeline.stages[0].name, "准备环境");
    assert!(pipeline.stages[0].user_visible);

    assert_eq!(pipeline.stages[1].id, "stage_internal");
    assert!(!pipeline.stages[1].user_visible);

    let initial_stages = pipeline.to_initial_stages();
    assert_eq!(initial_stages.len(), 3);

    for stage in &initial_stages {
        assert_eq!(stage.status, StageStatus::Pending);
        assert!(stage.started_at.is_none());
        assert!(stage.finished_at.is_none());
        assert!(stage.duration_ms.is_none());
    }

    assert_eq!(initial_stages[0].id, "stage_prepare");
    assert_eq!(initial_stages[1].id, "stage_internal");
    assert_eq!(initial_stages[2].id, "stage_execute");
}
