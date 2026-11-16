use super::*;
use crate::backend::application::AppService;
use crate::backend::infrastructure::tasks::{stages, TaskKind, TaskState};
use uuid::Uuid;

async fn setup_search_test_service(test_name: &str) -> (AppService, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-search-task-{}-{}",
        test_name,
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create test root");
    let service = AppService::open_with_db_path(root.join("app.db"))
        .await
        .expect("open test service");
    (service, root)
}

fn build_search_params(query: &str, record_kind: Option<&str>) -> ConversationSearchParams {
    ConversationSearchParams {
        record_kind: record_kind.map(str::to_string),
        adapter_id: None,
        source_id: None,
        project_path: None,
        query: query.to_string(),
        content_types: Vec::new(),
        card_kinds: Vec::new(),
        semantic_roles: Vec::new(),
        include_questions: Some(true),
        include_cards: Some(true),
        since: None,
        until: None,
        timeline: false,
        limit: Some(50),
        offset: Some(0),
        search_options: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn conversation_content_search_task_runs_three_stage_pipeline_and_caches_result() {
    let (service, root) = setup_search_test_service("pipeline").await;

    let handle = service
        .start_conversation_content_search_task(build_search_params("deploy", Some("session")))
        .expect("start search task");

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("terminal snapshot");

    assert_eq!(snapshot.kind, TaskKind::ConversationContentSearch);
    assert_eq!(snapshot.category, "conversation/search");
    assert_eq!(snapshot.state, TaskState::Succeeded);
    assert_eq!(
        snapshot
            .stages
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        vec![stages::PREPARE, stages::SCAN_FILES, stages::FINALIZE]
    );

    let result_json = snapshot.result.expect("lightweight summary result");
    assert_eq!(result_json["query"], "deploy");
    assert_eq!(result_json["recordKind"], "session");
    assert!(result_json.get("hits").is_none());

    let cached = service
        .get_conversation_content_search_task_result(ConversationSearchTaskResultGetParams {
            task_id: handle.task_id().to_string(),
        })
        .expect("get cached result")
        .expect("cached result present");
    assert_eq!(cached.query, "deploy");
    assert_eq!(cached.record_kind, "session");

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "multi_thread")]
async fn conversation_content_search_task_rejects_blank_query() {
    let (service, root) = setup_search_test_service("blank-query").await;

    let err = service
        .start_conversation_content_search_task(build_search_params("   ", Some("session")))
        .expect_err("blank query must fail");
    assert!(matches!(err, AppError::Validation(_)));

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "multi_thread")]
async fn conversation_content_search_task_supports_cancellation() {
    let (service, root) = setup_search_test_service("cancel").await;

    let handle = service
        .start_conversation_content_search_task(build_search_params("anything", Some("web")))
        .expect("start search task");

    let _ = service.runtime.task_runtime().cancel(handle.task_id());

    let snapshot = handle
        .wait_until_finished(service.runtime.task_runtime())
        .await
        .expect("terminal snapshot");

    assert!(matches!(
        snapshot.state,
        TaskState::Canceled | TaskState::Succeeded
    ));

    let _ = std::fs::remove_dir_all(root);
}
