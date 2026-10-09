use crate::backend::application::AppService;
use crate::backend::infrastructure::tasks::{
    stages, TaskAdmission, TaskKind, TaskOutput, TaskSpec, TaskState,
};

#[tokio::test]
async fn test_execute_task_direct_admission_fast_path() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-service-direct-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path.clone())
        .await
        .expect("open service");

    let spec = TaskSpec::new(TaskKind::Other, None).with_admission(TaskAdmission::Direct);

    let handle = service
        .execute_task(spec, |cx| async move {
            // 验证业务代码内的埋点在 Direct 模式下天然穿透 (No-Op)
            let stage = cx.enter_stage(stages::DISCOVER);
            stage.progress(1, Some(10), Some("测试进度".to_string()));
            cx.check_cancellation()
                .map_err(crate::backend::infrastructure::InfraError::external)?;
            Ok(TaskOutput::new(42usize))
        })
        .await
        .expect("execute direct task");

    // Fast-path 验证：已完成，携带业务数据
    assert!(handle.is_completed());
    assert_eq!(*handle.completed_data().expect("completed data"), 42usize);

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("finished snapshot");
    assert_eq!(snapshot.state, TaskState::Succeeded);

    drop(service);
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn test_execute_task_shadow_admission_is_not_user_visible() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-service-shadow-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path.clone())
        .await
        .expect("open service");

    let spec =
        TaskSpec::new(TaskKind::ConversationUsageScan, None).with_admission(TaskAdmission::Shadow);

    let handle = service
        .execute_task(spec, |_cx| async move {
            Ok(TaskOutput::new("shadow_result".to_string()))
        })
        .await
        .expect("execute shadow task");

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("finished snapshot");

    assert_eq!(snapshot.state, TaskState::Succeeded);
    // 验证 Shadow 模式下 user_visible 被强制设置为 false
    assert!(!snapshot.user_visible);

    drop(service);
    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn test_execute_task_managed_admission() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-service-managed-test-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let service = AppService::open_with_db_path(db_path.clone())
        .await
        .expect("open service");

    let spec = TaskSpec::new(TaskKind::Scan, None).with_admission(TaskAdmission::Managed);

    let handle = service
        .execute_task(spec, |_cx| async move {
            Ok(TaskOutput::new("managed_result".to_string()))
        })
        .await
        .expect("execute managed task");

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("finished snapshot");

    assert_eq!(snapshot.state, TaskState::Succeeded);
    assert!(snapshot.user_visible);

    drop(service);
    let _ = std::fs::remove_file(db_path);
}
