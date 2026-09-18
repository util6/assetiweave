use super::*;
use crate::backend::agents::types::AgentProtocol;
use crate::backend::ai_execution::{
    executor::BackendFuture, AgentExecutionRuntime, AiExecutionRequest, AiExecutionResult,
};
use std::sync::Arc;

#[test]
fn recall_query_is_redacted_and_bounded() {
    let result = redact_recall_query(" Bearer abcdefghijklmnopqrstuvwxyz1234567890 ");
    assert!(result.is_ok());
    assert!(!result
        .unwrap()
        .contains("abcdefghijklmnopqrstuvwxyz1234567890"));
}

#[test]
fn recall_json_fence_is_removed_without_changing_payload() {
    assert_eq!(strip_json_fence("```json\n{}\n```"), "{}");
}

struct FakeRecallRuntime;

impl AgentExecutionRuntime for FakeRecallRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        Box::pin(async move {
            Ok(AiExecutionResult {
                text: r#"{
                        "answer": "找到了一条相关线索。",
                        "sessionReferences": [],
                        "contentReferences": [],
                        "followUpSuggestions": ["继续缩小时间范围"]
                    }"#
                .to_string(),
                agent_id: request.agent_id,
                protocol: AgentProtocol::Acp,
                requested_model: request.model,
                elapsed_ms: 1,
                persistent_binding: None,
                replay_text: None,
                session_cleanup: crate::backend::ai_execution::SessionCleanupStatus::Deleted,
            })
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn recall_one_turn_returns_quickly_and_reopens_from_conversation_and_workflow_tables() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-memory-recall-workflow-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create Recall fixture root");
    let db_path = root.join("app.db");
    let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(FakeRecallRuntime);
    let service = AppService::open_with_db_path_and_runtime(db_path.clone(), runtime.clone())
        .await
        .expect("open Recall fixture service");
    let invalid = parse_and_validate_recall_output(
        &service,
        "default",
        &MemoryScope::default(),
        r#"{
                "answer": "引用不存在的记录",
                "sessionReferences": [{"recordKind":"session","sessionId":"missing"}],
                "contentReferences": [],
                "followUpSuggestions": []
            }"#,
    )
    .await;
    assert!(invalid
        .expect_err("invalid Recall reference should be rejected")
        .to_string()
        .contains("out-of-scope"));
    let session = service
        .create_memory_recall_session(MemoryRecallSessionCreateParams {
            scope: MemoryScope {
                project_path: Some(root.to_string_lossy().into_owned()),
                ..MemoryScope::default()
            },
        })
        .await
        .expect("create Recall session");
    let queued = service
        .send_memory_recall_turn(MemoryRecallTurnSendParams {
            session_id: session.id.clone(),
            query: "请找出上次关于发布的讨论".to_string(),
        })
        .await
        .expect("queue Recall turn");
    assert_eq!(queued.id, session.id);

    let mut completed = None;
    for _ in 0..100 {
        let current = service
            .get_memory_recall_session(MemoryRecallSessionGetParams {
                session_id: session.id.clone(),
            })
            .await
            .expect("read Recall session");
        if current
            .turns
            .first()
            .is_some_and(|turn| turn.status == MemoryRecallTurnStatus::Completed)
        {
            completed = Some(current);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let completed = completed.expect("Recall turn completion");
    let turn = completed.turns.first().expect("Recall turn");
    let output = turn
        .structured_output
        .as_ref()
        .expect("structured Recall output");
    assert_eq!(output.answer, "找到了一条相关线索。");
    assert!(output.session_references.is_empty());
    assert!(output.content_references.is_empty());
    assert_eq!(output.follow_up_suggestions, vec!["继续缩小时间范围"]);

    let source_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversation_sources WHERE tenant_id='default' AND id='assetiweave-memory-recall'",
        )
        .fetch_one(service.db.pool())
        .await
        .map_err(AppError::external)
        .expect("count sources");
    let turn_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversation_turns WHERE tenant_id='default' AND session_id=?1",
    )
    .bind(&turn.conversation_session_id)
    .fetch_one(service.db.pool())
    .await
    .map_err(AppError::external)
    .expect("count turns");
    assert_eq!((source_count, turn_count), (1, 1));
    assert_eq!(completed.status, MemoryRecallSessionStatus::Active);

    drop(service);
    let reopened = AppService::open_with_db_path_and_runtime(db_path.clone(), runtime)
        .await
        .expect("reopen Recall fixture service");
    let restored = reopened
        .get_memory_recall_session(MemoryRecallSessionGetParams {
            session_id: session.id,
        })
        .await
        .expect("read restored Recall session");
    assert_eq!(restored.turns.len(), 1);
    assert_eq!(
        restored.turns[0]
            .structured_output
            .as_ref()
            .expect("restored Recall output")
            .answer,
        "找到了一条相关线索。"
    );
    drop(reopened);
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn recall_session_supports_sequential_turns_without_replaying_completed_turns() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-memory-recall-multiturn-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create multi-turn fixture root");
    let db_path = root.join("app.db");
    let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(FakeRecallRuntime);
    let service = AppService::open_with_db_path_and_runtime(db_path.clone(), runtime)
        .await
        .expect("open multi-turn service");
    let session = service
        .create_memory_recall_session(MemoryRecallSessionCreateParams::default())
        .await
        .expect("create multi-turn session");

    for (index, query) in ["先找发布讨论", "再找更早的那次"].into_iter().enumerate() {
        service
            .send_memory_recall_turn(MemoryRecallTurnSendParams {
                session_id: session.id.clone(),
                query: query.to_string(),
            })
            .await
            .expect("send sequential Recall turn");
        wait_for_recall_turn(&service, &session.id, index + 1).await;
    }

    let restored = service
        .get_memory_recall_session(MemoryRecallSessionGetParams {
            session_id: session.id,
        })
        .await
        .expect("read multi-turn session");
    assert_eq!(restored.turns.len(), 2);
    assert_eq!(restored.turns[0].user_text, "先找发布讨论");
    assert_eq!(restored.turns[1].user_text, "再找更早的那次");
    assert!(restored
        .turns
        .iter()
        .all(|turn| turn.status == MemoryRecallTurnStatus::Completed));

    drop(service);
    std::fs::remove_dir_all(root).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn recall_cancel_is_durable_and_does_not_allow_late_agent_output() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-memory-recall-cancel-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create cancellation fixture root");
    let db_path = root.join("app.db");
    let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(BlockingRecallRuntime);
    let service = AppService::open_with_db_path_and_runtime(db_path, runtime)
        .await
        .expect("open cancellation service");
    let session = service
        .create_memory_recall_session(MemoryRecallSessionCreateParams::default())
        .await
        .expect("create cancellation session");
    let queued = service
        .send_memory_recall_turn(MemoryRecallTurnSendParams {
            session_id: session.id.clone(),
            query: "等待取消".to_string(),
        })
        .await
        .expect("send cancellable Recall turn");
    let turn_id = queued.active_turn_id.expect("active turn");
    service
        .cancel_memory_recall_turn(MemoryRecallTurnCancelParams { turn_id })
        .await
        .expect("cancel Recall turn");

    let cancelled = wait_for_recall_turn(&service, &session.id, 1).await;
    assert_eq!(cancelled.turns[0].status, MemoryRecallTurnStatus::Cancelled);
    assert!(cancelled.turns[0].structured_output.is_none());
    assert!(cancelled.active_turn_id.is_none());
    service
        .runtime
        .task_runtime()
        .shutdown_with_grace(std::time::Duration::from_secs(2))
        .await;
    drop(service);
    std::fs::remove_dir_all(root).ok();
}

struct BlockingRecallRuntime;

impl AgentExecutionRuntime for BlockingRecallRuntime {
    fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
        Box::pin(async move {
            request.cancellation.cancelled().await;
            Err(crate::backend::ai_execution::AiExecutionError::Cancelled {
                program: std::path::PathBuf::from("recall-fixture"),
            })
        })
    }
}

async fn wait_for_recall_turn(
    service: &AppService,
    session_id: &str,
    expected_turn_count: usize,
) -> MemoryRecallSession {
    for _ in 0..200 {
        let current = service
            .get_memory_recall_session(MemoryRecallSessionGetParams {
                session_id: session_id.to_string(),
            })
            .await
            .expect("read Recall session while waiting");
        if current.turns.len() >= expected_turn_count
            && current
                .turns
                .last()
                .is_some_and(|turn| turn.status.is_terminal())
        {
            return current;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("Recall turn reached terminal state timed out");
}
