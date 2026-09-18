use super::*;
use tokio::sync::mpsc;

fn session() -> SessionId {
    SessionId::new("session")
}

fn text(session_id: &SessionId, value: &str) -> AcpRuntimeEvent {
    AcpRuntimeEvent::AgentText {
        session_id: session_id.clone(),
        text: value.to_owned(),
    }
}

fn complete(session_id: &SessionId) -> AcpRuntimeEvent {
    AcpRuntimeEvent::TurnCompleted {
        session_id: session_id.clone(),
        stop_reason: StopReason::EndTurn,
    }
}

#[test]
fn evt_01_one_text_chunk_returns_exact_text() {
    let session = session();
    let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);

    assert!(matches!(
        aggregator.apply(text(&session, "answer")),
        AggregatorAction::Continue
    ));

    assert_eq!(aggregator.finish().unwrap(), "answer");
}

#[test]
fn evt_02_multiple_chunks_preserve_order() {
    let session = session();
    let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
    aggregator.apply(text(&session, "first "));
    aggregator.apply(text(&session, "second"));

    assert_eq!(aggregator.finish().unwrap(), "first second");
}

#[test]
fn evt_03_unicode_chunks_remain_valid_and_ordered() {
    let session = session();
    let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
    aggregator.apply(text(&session, "你"));
    aggregator.apply(text(&session, "好🌍"));

    assert_eq!(aggregator.finish().unwrap(), "你好🌍");
}

#[test]
fn evt_04_thinking_is_counted_but_not_returned() {
    let session = session();
    let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
    aggregator.apply(AcpRuntimeEvent::AgentThought {
        session_id: session.clone(),
        text: Some("provider thought".to_string()),
    });
    aggregator.apply(text(&session, "visible"));

    assert_eq!(aggregator.diagnostics(), (1, 1, 0));
    assert_eq!(aggregator.finish().unwrap(), "visible");
}

#[test]
fn evt_05_wrong_session_is_ignored() {
    let session = session();
    let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
    aggregator.apply(text(&SessionId::new("other"), "wrong"));
    aggregator.apply(text(&session, "right"));

    assert_eq!(aggregator.diagnostics(), (1, 0, 1));
    assert_eq!(aggregator.finish().unwrap(), "right");
}

#[test]
fn evt_06_permission_requires_cancel_and_fails_closed() {
    let session = session();
    let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);

    assert!(matches!(
        aggregator.apply(AcpRuntimeEvent::PermissionRequested {
            session_id: session
        }),
        AggregatorAction::CancelAndFail(AiExecutionError::PermissionDenied)
    ));
}

#[test]
fn evt_07_tool_activity_requires_cancel_and_fails_closed() {
    let session = session();
    let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);

    assert!(matches!(
        aggregator.apply(AcpRuntimeEvent::ToolCall {
            session_id: session.clone(),
            tool_call_id: "tool".to_string(),
            title: "tool".to_string(),
            status: crate::backend::agents::protocol::acp::AcpToolStatus::Pending,
            raw_input: None,
            raw_output: None,
        }),
        AggregatorAction::CancelAndFail(AiExecutionError::ToolUseDenied)
    ));
}

#[test]
fn evt_08_empty_output_fails() {
    let aggregator = TranslationTextAggregator::new(session(), 64);

    assert!(matches!(
        aggregator.finish(),
        Err(AiExecutionError::EmptyOutput { .. })
    ));
}

#[test]
fn evt_09_whitespace_only_output_fails() {
    let session = session();
    let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
    aggregator.apply(text(&session, " \n\t "));

    assert!(matches!(
        aggregator.finish(),
        Err(AiExecutionError::EmptyOutput { .. })
    ));
}

#[test]
fn evt_10_exact_byte_cap_succeeds() {
    let session = session();
    let value = "你好";
    let mut aggregator = TranslationTextAggregator::new(session.clone(), value.len());
    aggregator.apply(text(&session, value));

    assert_eq!(aggregator.finish().unwrap(), value);
}

#[test]
fn evt_11_cap_plus_one_fails_without_appending_partial_chunk() {
    let session = session();
    let mut aggregator = TranslationTextAggregator::new(session.clone(), 5);
    aggregator.apply(text(&session, "12345"));

    assert!(matches!(
        aggregator.apply(text(&session, "6")),
        AggregatorAction::CancelAndFail(AiExecutionError::OutputLimit { limit: 5 })
    ));
    assert_eq!(aggregator.finish().unwrap(), "12345");
}

#[tokio::test(flavor = "current_thread")]
async fn evt_12_completion_marker_includes_the_late_final_chunk_without_sleep() {
    let session = session();
    let mut aggregator = TranslationTextAggregator::new(session.clone(), 64);
    let (tx, mut rx) = mpsc::channel(4);
    tx.send(text(&session, "before ")).await.unwrap();
    tx.send(text(&session, "late")).await.unwrap();
    tx.send(complete(&session)).await.unwrap();
    drop(tx);

    loop {
        let event = rx.recv().await.expect("completion marker");
        match aggregator.apply(event) {
            AggregatorAction::Continue => {}
            AggregatorAction::Complete { stop_reason } => {
                assert_eq!(stop_reason, StopReason::EndTurn);
                break;
            }
            AggregatorAction::CancelAndFail(error) => {
                panic!("unexpected aggregation failure: {error}")
            }
        }
    }

    assert_eq!(aggregator.finish().unwrap(), "before late");
}

#[test]
fn recall_evt_01_tool_activity_is_allowed_for_read_only_mcp() {
    let session = session();
    let mut aggregator = ReadOnlyToolTextAggregator::new(
        session.clone(),
        64,
        ["memory_recall_search", "memory_recall_block"],
    );

    assert!(matches!(
        aggregator.apply(AcpRuntimeEvent::ToolCall {
            session_id: session.clone(),
            tool_call_id: "tool".to_string(),
            title: "memory_recall_search".to_string(),
            status: crate::backend::agents::protocol::acp::AcpToolStatus::Pending,
            raw_input: None,
            raw_output: None,
        }),
        AggregatorAction::Continue
    ));
    aggregator.apply(text(&session, "answer"));
    assert_eq!(aggregator.finish().unwrap(), "answer");
}

#[test]
fn recall_evt_02_permission_is_still_denied() {
    let session = session();
    let mut aggregator = ReadOnlyToolTextAggregator::new(
        session.clone(),
        64,
        ["memory_recall_search", "memory_recall_block"],
    );

    assert!(matches!(
        aggregator.apply(AcpRuntimeEvent::PermissionRequested {
            session_id: session
        }),
        AggregatorAction::CancelAndFail(AiExecutionError::PermissionDenied)
    ));
}

#[test]
fn memory_generation_evt_01_only_declared_mcp_tools_are_allowed() {
    let session = session();
    let mut aggregator = ReadOnlyToolTextAggregator::new(
        session.clone(),
        64,
        [
            "get_session_outline",
            "search_session_content",
            "read_question_content",
            "read_content_node",
        ],
    );

    assert!(matches!(
        aggregator.apply(AcpRuntimeEvent::ToolCall {
            session_id: session.clone(),
            tool_call_id: "outline".to_string(),
            title: "assetiweave-memory-generation/get_session_outline".to_string(),
            status: crate::backend::agents::protocol::acp::AcpToolStatus::Pending,
            raw_input: None,
            raw_output: None,
        }),
        AggregatorAction::Continue
    ));
    assert!(matches!(
        aggregator.apply(AcpRuntimeEvent::ToolCallUpdate {
            session_id: session,
            tool_call_id: "outline".to_string(),
            title: None,
            status: Some(crate::backend::agents::protocol::acp::AcpToolStatus::Completed),
            raw_input: None,
            raw_output: None,
        }),
        AggregatorAction::Continue
    ));
}

#[test]
fn memory_generation_evt_02_builtin_exec_is_denied() {
    let session = session();
    let mut aggregator = ReadOnlyToolTextAggregator::new(
        session.clone(),
        64,
        [
            "get_session_outline",
            "search_session_content",
            "read_question_content",
            "read_content_node",
        ],
    );

    assert!(matches!(
        aggregator.apply(AcpRuntimeEvent::ToolCall {
            session_id: session,
            tool_call_id: "exec".to_string(),
            title: "Run JavaScript".to_string(),
            status: crate::backend::agents::protocol::acp::AcpToolStatus::Pending,
            raw_input: None,
            raw_output: None,
        }),
        AggregatorAction::CancelAndFail(AiExecutionError::ToolUseDenied)
    ));
}

#[test]
fn memory_generation_evt_03_unknown_update_is_denied() {
    let session = session();
    let mut aggregator =
        ReadOnlyToolTextAggregator::new(session.clone(), 64, ["get_session_outline"]);

    assert!(matches!(
        aggregator.apply(AcpRuntimeEvent::ToolCallUpdate {
            session_id: session,
            tool_call_id: "unknown".to_string(),
            title: None,
            status: Some(crate::backend::agents::protocol::acp::AcpToolStatus::InProgress),
            raw_input: None,
            raw_output: None,
        }),
        AggregatorAction::CancelAndFail(AiExecutionError::ToolUseDenied)
    ));
}

#[test]
fn memory_generation_evt_04_namespaced_tool_with_args_is_allowed() {
    let session = session();
    let mut aggregator =
        ReadOnlyToolTextAggregator::new(session.clone(), 64, ["get_session_outline"]);

    assert!(matches!(
        aggregator.apply(AcpRuntimeEvent::ToolCall {
            session_id: session.clone(),
            tool_call_id: "call-1".to_string(),
            title: "assetiweave_memory_generation_get_session_outline(sessionId: \"abc\")"
                .to_string(),
            status: crate::backend::agents::protocol::acp::AcpToolStatus::Pending,
            raw_input: None,
            raw_output: None,
        }),
        AggregatorAction::Continue
    ));

    assert!(matches!(
        aggregator.apply(AcpRuntimeEvent::ToolCall {
            session_id: session,
            tool_call_id: "call-2".to_string(),
            title: "assetiweave-memory-generation:get_session_outline: {\"id\":\"s1\"}".to_string(),
            status: crate::backend::agents::protocol::acp::AcpToolStatus::Pending,
            raw_input: None,
            raw_output: None,
        }),
        AggregatorAction::Continue
    ));
}
