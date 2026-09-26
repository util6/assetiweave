use super::*;

#[test]
fn ses_01_projection_merges_duplicate_and_out_of_order_events() {
    let projection = SessionEventProjection::new(SessionEventProjectionLimits {
        max_items: 8,
        max_events: 32,
        max_bytes: 4096,
    });
    let first = event(
        2,
        "text",
        SessionEventKind::AssistantTextDelta {
            text: "world".to_string(),
        },
        SessionEventDelivery::Live,
    );
    let second = event(
        1,
        "text",
        SessionEventKind::AssistantTextDelta {
            text: "hello ".to_string(),
        },
        SessionEventDelivery::Live,
    );

    assert_eq!(
        projection.apply(first.clone()),
        SessionEventApplyResult::Applied
    );
    assert_eq!(projection.apply(first), SessionEventApplyResult::Duplicate);
    assert_eq!(projection.apply(second), SessionEventApplyResult::Applied);

    let snapshot = projection.snapshot();
    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].text.as_deref(), Some("hello world"));
    assert_eq!(snapshot.items[0].sequence, 2);
    assert_eq!(snapshot.event_count, 2);
}

#[test]
fn ses_02_projection_attaches_tool_result_and_separates_concurrent_executions() {
    let projection = SessionEventProjection::new(SessionEventProjectionLimits {
        max_items: 8,
        max_events: 32,
        max_bytes: 4096,
    });
    projection.apply(event(
        1,
        "tool",
        SessionEventKind::ToolStart {
            name: Some("search".to_string()),
            raw_input: None,
        },
        SessionEventDelivery::Live,
    ));
    projection.apply(event(
        2,
        "tool",
        SessionEventKind::ToolResult {
            success: true,
            detail: Some("done".to_string()),
            raw_output: None,
        },
        SessionEventDelivery::Live,
    ));
    projection.apply(event_for_execution(
        "execution-b",
        1,
        "other-text",
        SessionEventKind::AssistantTextSnapshot {
            text: "other".to_string(),
        },
        SessionEventDelivery::Live,
    ));

    let snapshot = projection.snapshot();
    assert_eq!(snapshot.items.len(), 2);
    let tool = snapshot
        .items
        .iter()
        .find(|item| item.kind == SessionItemKind::Tool)
        .expect("tool item");
    assert_eq!(tool.state, SessionItemState::Succeeded);
    assert_eq!(tool.text.as_deref(), Some("done"));
    assert_eq!(
        snapshot
            .items
            .iter()
            .find(|item| item.identity.execution_id == "execution-b")
            .and_then(|item| item.text.as_deref()),
        Some("other")
    );
}

#[test]
fn ses_03_replay_and_live_events_merge_without_duplicate_items() {
    let projection = SessionEventProjection::new(SessionEventProjectionLimits::default());
    projection.apply(event(
        4,
        "text",
        SessionEventKind::AssistantTextSnapshot {
            text: "replayed".to_string(),
        },
        SessionEventDelivery::Replay,
    ));
    projection.apply(event(
        5,
        "text",
        SessionEventKind::AssistantTextDelta {
            text: " live".to_string(),
        },
        SessionEventDelivery::Live,
    ));
    projection.apply(event(
        4,
        "text",
        SessionEventKind::AssistantTextSnapshot {
            text: "replayed".to_string(),
        },
        SessionEventDelivery::Replay,
    ));

    let snapshot = projection.snapshot();
    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].text.as_deref(), Some("replayed live"));
    assert_eq!(snapshot.items[0].delivery, SessionEventDelivery::Live);
}

#[test]
fn ses_04_projection_evicts_oldest_events_and_items_with_explicit_limits() {
    let projection = SessionEventProjection::new(SessionEventProjectionLimits {
        max_items: 2,
        max_events: 2,
        max_bytes: 4096,
    });
    projection.apply(event(
        1,
        "item-a",
        SessionEventKind::Notice {
            code: "a".to_string(),
            detail: None,
        },
        SessionEventDelivery::Live,
    ));
    projection.apply(event(
        2,
        "item-b",
        SessionEventKind::Notice {
            code: "b".to_string(),
            detail: None,
        },
        SessionEventDelivery::Live,
    ));
    projection.apply(event(
        3,
        "item-c",
        SessionEventKind::Notice {
            code: "c".to_string(),
            detail: None,
        },
        SessionEventDelivery::Live,
    ));

    let snapshot = projection.snapshot();
    assert_eq!(snapshot.items.len(), 2);
    assert_eq!(snapshot.event_count, 2);
    assert_eq!(snapshot.items[0].identity.item_id, "item-b");
    assert_eq!(snapshot.items[1].identity.item_id, "item-c");
}

#[test]
fn ses_05_debug_output_redacts_event_and_snapshot_content() {
    let projection = SessionEventProjection::new(SessionEventProjectionLimits::default());
    projection.apply(event(
        1,
        "text",
        SessionEventKind::AssistantTextDelta {
            text: "SESSION_EVENT_SECRET".to_string(),
        },
        SessionEventDelivery::Live,
    ));

    let event_debug = format!(
        "{:?}",
        event(
            1,
            "text",
            SessionEventKind::AssistantTextDelta {
                text: "SESSION_EVENT_SECRET".to_string(),
            },
            SessionEventDelivery::Live,
        )
    );
    let snapshot_debug = format!("{:?}", projection.snapshot());

    assert!(!event_debug.contains("SESSION_EVENT_SECRET"));
    assert!(!snapshot_debug.contains("SESSION_EVENT_SECRET"));
}

#[test]
fn ses_06_projection_broadcasts_snapshot_and_clear_is_in_memory_only() {
    let projection = SessionEventProjection::default();
    let mut subscriber = projection.subscribe();
    projection.apply(event(
        1,
        "text",
        SessionEventKind::AssistantTextSnapshot {
            text: "temporary".to_string(),
        },
        SessionEventDelivery::Live,
    ));

    let published = subscriber.try_recv().expect("published snapshot");
    assert_eq!(published.items.len(), 1);
    projection.clear();
    assert!(projection.snapshot().items.is_empty());
    assert_eq!(projection.snapshot().event_count, 0);
}

#[test]
fn ses_07_projection_accepts_concurrent_events_without_losing_ordered_deltas() {
    let projection =
        std::sync::Arc::new(SessionEventProjection::new(SessionEventProjectionLimits {
            max_items: 2,
            max_events: 16,
            max_bytes: 4096,
        }));
    let handles = (0..8)
        .map(|sequence| {
            let projection = projection.clone();
            std::thread::spawn(move || {
                projection.apply(event(
                    sequence,
                    "text",
                    SessionEventKind::AssistantTextDelta {
                        text: sequence.to_string(),
                    },
                    SessionEventDelivery::Live,
                ))
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        assert_eq!(
            handle.join().expect("projection worker"),
            SessionEventApplyResult::Applied
        );
    }

    let snapshot = projection.snapshot();
    assert_eq!(snapshot.event_count, 8);
    assert_eq!(snapshot.items[0].text.as_deref(), Some("01234567"));
}

#[test]
fn t02_one_complete_tool_step_aggregates_start_update_result_with_typed_payload_and_redaction() {
    let projection = SessionEventProjection::default();
    let secret_input = "SECRET_INPUT_KEYWORD";
    let secret_output = "SECRET_OUTPUT_PAYLOAD";

    // 1. ToolStart with name and raw input
    projection.apply(event(
        1,
        "tool:read_1",
        SessionEventKind::ToolStart {
            name: Some("read_file".to_string()),
            raw_input: Some(serde_json::json!({"path": "src/main.rs", "secret": secret_input})),
        },
        SessionEventDelivery::Live,
    ));

    // 2. ToolUpdate with Running and detail
    projection.apply(event(
        2,
        "tool:read_1",
        SessionEventKind::ToolUpdate {
            state: SessionToolState::Running,
            detail: Some("reading 1024 bytes".to_string()),
            raw_output: None,
        },
        SessionEventDelivery::Live,
    ));

    // 3. ToolResult with Succeeded and raw output
    let result_event = event(
        3,
        "tool:read_1",
        SessionEventKind::ToolResult {
            success: true,
            detail: Some("completed reading".to_string()),
            raw_output: Some(serde_json::json!({"bytes": 1024, "token": secret_output})),
        },
        SessionEventDelivery::Live,
    );
    projection.apply(result_event.clone());

    let snapshot = projection.snapshot();
    // 1 logical tool item
    assert_eq!(snapshot.items.len(), 1);
    let item = &snapshot.items[0];
    assert_eq!(item.kind, SessionItemKind::Tool);
    assert_eq!(item.state, SessionItemState::Succeeded);
    assert_eq!(item.tool_name.as_deref(), Some("read_file"));
    assert_eq!(item.tool_call_id.as_deref(), Some("read_1"));
    assert_eq!(
        item.tool_input,
        Some(serde_json::json!({"path": "src/main.rs", "secret": secret_input}))
    );
    assert_eq!(
        item.tool_output,
        Some(serde_json::json!({"bytes": 1024, "token": secret_output}))
    );

    // State monotonicity: subsequent InProgress update does not retreat state from Succeeded
    projection.apply(event(
        4,
        "tool:read_1",
        SessionEventKind::ToolUpdate {
            state: SessionToolState::Running,
            detail: Some("late running event".to_string()),
            raw_output: None,
        },
        SessionEventDelivery::Live,
    ));
    let snapshot_after = projection.snapshot();
    assert_eq!(snapshot_after.items[0].state, SessionItemState::Succeeded);

    // Redaction assertions: Debug format must NEVER contain secret payload
    let event_debug = format!("{result_event:?}");
    let snapshot_debug = format!("{snapshot:?}");
    let item_debug = format!("{item:?}");

    assert!(!event_debug.contains(secret_input));
    assert!(!event_debug.contains(secret_output));
    assert!(!snapshot_debug.contains(secret_input));
    assert!(!snapshot_debug.contains(secret_output));
    assert!(!item_debug.contains(secret_input));
    assert!(!item_debug.contains(secret_output));
}

#[test]
fn t03_complete_agent_session_canonical_fixture_and_edge_cases() {
    let projection = SessionEventProjection::new(SessionEventProjectionLimits {
        max_items: 32,
        max_events: 128,
        max_bytes: 64 * 1024,
    });

    // 1. User request
    projection.apply(event(
        1,
        "user",
        SessionEventKind::UserMessageAcknowledged {
            accepted: true,
            text: Some("Inspect the workspace and update the target.".to_string()),
        },
        SessionEventDelivery::Live,
    ));

    // 2. Assistant text 1
    projection.apply(event(
        2,
        "assistant_1",
        SessionEventKind::AssistantTextDelta {
            text: "I will inspect".to_string(),
        },
        SessionEventDelivery::Live,
    ));

    // 3. Thinking
    projection.apply(event(
        3,
        "thinking",
        SessionEventKind::ThinkingDelta {
            text: "Analyzing the codebase and tools...".to_string(),
        },
        SessionEventDelivery::Live,
    ));

    // 4. Tool A start
    projection.apply(event(
        4,
        "tool:run_cmd",
        SessionEventKind::ToolStart {
            name: Some("run_command".to_string()),
            raw_input: Some(serde_json::json!({"cmd": "ls"})),
        },
        SessionEventDelivery::Live,
    ));

    // 5. Tool A update
    projection.apply(event(
        5,
        "tool:run_cmd",
        SessionEventKind::ToolUpdate {
            state: SessionToolState::Running,
            detail: Some("running command".to_string()),
            raw_output: Some(serde_json::json!({"stdout": "Cargo.toml\n"})),
        },
        SessionEventDelivery::Live,
    ));

    // 6. Tool A result (success)
    projection.apply(event(
        6,
        "tool:run_cmd",
        SessionEventKind::ToolResult {
            success: true,
            detail: Some("done".to_string()),
            raw_output: Some(serde_json::json!({"exit": 0})),
        },
        SessionEventDelivery::Live,
    ));

    // 7. Assistant text 2
    projection.apply(event(
        7,
        "assistant_2",
        SessionEventKind::AssistantTextDelta {
            text: "I found the file.".to_string(),
        },
        SessionEventDelivery::Live,
    ));

    // 8. Tool B start
    projection.apply(event(
        8,
        "tool:edit",
        SessionEventKind::ToolStart {
            name: Some("file_edit".to_string()),
            raw_input: Some(serde_json::json!({"file": "Cargo.toml"})),
        },
        SessionEventDelivery::Live,
    ));

    // 9. Tool B result
    projection.apply(event(
        9,
        "tool:edit",
        SessionEventKind::ToolResult {
            success: true,
            detail: Some("edited".to_string()),
            raw_output: Some(serde_json::json!({"diff": "+[dependencies]"})),
        },
        SessionEventDelivery::Live,
    ));

    // 10. Tool C result (failure)
    projection.apply(event(
        10,
        "tool:test",
        SessionEventKind::ToolResult {
            success: false,
            detail: Some("test failed".to_string()),
            raw_output: Some(serde_json::json!({"stderr": "failed"})),
        },
        SessionEventDelivery::Live,
    ));

    // 11. Assistant final text
    projection.apply(event(
        11,
        "assistant_3",
        SessionEventKind::AssistantTextDelta {
            text: "All done.".to_string(),
        },
        SessionEventDelivery::Live,
    ));

    // 12. Terminal result with same text as assistant_3
    projection.apply(event(
        12,
        "terminal",
        SessionEventKind::TerminalResult {
            text: Some("All done.".to_string()),
        },
        SessionEventDelivery::Live,
    ));

    let snapshot = projection.snapshot();
    assert_eq!(snapshot.items.len(), 9);

    // Sequence verification
    assert_eq!(snapshot.items[0].kind, SessionItemKind::UserMessage);
    assert_eq!(
        snapshot.items[0].text.as_deref(),
        Some("Inspect the workspace and update the target.")
    );

    assert_eq!(snapshot.items[1].kind, SessionItemKind::AssistantText);
    assert_eq!(snapshot.items[1].text.as_deref(), Some("I will inspect"));

    assert_eq!(snapshot.items[2].kind, SessionItemKind::Thinking);

    assert_eq!(snapshot.items[3].kind, SessionItemKind::Tool);
    assert_eq!(snapshot.items[3].tool_name.as_deref(), Some("run_command"));
    assert_eq!(snapshot.items[3].state, SessionItemState::Succeeded);

    assert_eq!(snapshot.items[4].kind, SessionItemKind::AssistantText);
    assert_eq!(snapshot.items[4].text.as_deref(), Some("I found the file."));

    assert_eq!(snapshot.items[5].kind, SessionItemKind::Tool);
    assert_eq!(snapshot.items[5].tool_name.as_deref(), Some("file_edit"));
    assert_eq!(snapshot.items[5].state, SessionItemState::Succeeded);

    assert_eq!(snapshot.items[6].kind, SessionItemKind::Tool);
    assert_eq!(snapshot.items[6].state, SessionItemState::Failed);

    assert_eq!(snapshot.items[7].kind, SessionItemKind::AssistantText);
    assert_eq!(snapshot.items[7].text.as_deref(), Some("All done."));

    // C-072: terminal text identical to last assistant text is suppressed from duplicating text
    assert_eq!(snapshot.items[8].kind, SessionItemKind::FinalResult);
    assert_eq!(snapshot.items[8].text, None);
    assert_eq!(snapshot.items[8].state, SessionItemState::Completed);

    // Edge case: Conflicting terminal events keep monotonic terminal and record notice code
    projection.apply(event(
        13,
        "tool:run_cmd",
        SessionEventKind::ToolResult {
            success: false,
            detail: Some("conflicting result".to_string()),
            raw_output: None,
        },
        SessionEventDelivery::Live,
    ));
    let snapshot_conflict = projection.snapshot();
    let tool_run_cmd = snapshot_conflict
        .items
        .iter()
        .find(|i| i.tool_name.as_deref() == Some("run_command"))
        .unwrap();
    assert_eq!(tool_run_cmd.state, SessionItemState::Succeeded);
    assert_eq!(
        tool_run_cmd.code.as_deref(),
        Some("conflicting_terminal_event")
    );

    // Edge case: Head/Tail truncation on oversized content
    let small_projection = SessionEventProjection::new(SessionEventProjectionLimits {
        max_items: 8,
        max_events: 16,
        max_bytes: 400,
    });
    let huge_text = "A".repeat(1000);
    let apply_result = small_projection.apply(event(
        1,
        "assistant_huge",
        SessionEventKind::AssistantTextDelta { text: huge_text },
        SessionEventDelivery::Live,
    ));
    assert_eq!(apply_result, SessionEventApplyResult::Applied);
    let small_snapshot = small_projection.snapshot();
    assert_eq!(small_snapshot.items.len(), 1);
    let huge_item = &small_snapshot.items[0];
    assert!(huge_item.truncation.is_some());
    let trunc = huge_item.truncation.as_ref().unwrap();
    assert_eq!(trunc.original_bytes, 1000);
    assert_eq!(trunc.strategy, "headTail");
    assert!(huge_item.text.as_ref().unwrap().contains("[truncated]"));
}

fn event(
    sequence: u64,
    item_id: &str,
    kind: SessionEventKind,
    delivery: SessionEventDelivery,
) -> SessionEvent {
    event_for_execution("execution-a", sequence, item_id, kind, delivery)
}

fn event_for_execution(
    execution_id: &str,
    sequence: u64,
    item_id: &str,
    kind: SessionEventKind,
    delivery: SessionEventDelivery,
) -> SessionEvent {
    SessionEvent {
        identity: SessionEventIdentity {
            session_id: "session".to_string(),
            member_id: "member".to_string(),
            execution_id: execution_id.to_string(),
            turn_id: "turn".to_string(),
            item_id: item_id.to_string(),
            event_id: format!("event-{execution_id}-{item_id}-{sequence}"),
        },
        sequence,
        delivery,
        kind,
        truncation: None,
    }
}
