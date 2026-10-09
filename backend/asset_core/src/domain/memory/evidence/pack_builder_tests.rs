use super::*;
use crate::backend::domain::conversations::{
    ConversationContentCardDescriptor, ConversationPartKind, ConversationPartRole,
    NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
};
use crate::backend::domain::memory::evidence::EvidenceReadStatus;
use crate::backend::domain::{BoundedMemoryBudgetPolicy, MemoryExecutionWorkOrder, MemoryRecipe};

fn make_test_work_order() -> MemoryExecutionWorkOrder {
    let recipe = MemoryRecipe::default_builtin();
    let budget = BoundedMemoryBudgetPolicy::default();
    MemoryExecutionWorkOrder::new(
        "wo-test".to_string(),
        "session-1".to_string(),
        "source-1".to_string(),
        1,
        "fp-1".to_string(),
        &recipe,
        budget,
        "2026-10-03T00:00:00Z".to_string(),
    )
}

#[test]
fn test_ambient_exploration_part_is_downgraded_to_indexed_only() {
    let work_order = make_test_work_order();
    let ambient_part = NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Command,
        text: Some("view_file /src/large_log.txt\n[file content 1000 lines...]".to_string()),
        language: None,
        command: Some("view_file /src/large_log.txt".to_string()),
        cwd: None,
        status: Some("success".to_string()),
        exit_code: Some(0),
        command_label: None,
        source_execution_id: None,
        content_card: None,
        metadata_json: Some(r#"{"signal":"ambient","tool_name":"view_file"}"#.to_string()),
    };

    let mutate_part = NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Command,
        text: Some("cargo build --release\nCompiling...".to_string()),
        language: None,
        command: Some("cargo build --release".to_string()),
        cwd: None,
        status: Some("success".to_string()),
        exit_code: Some(0),
        command_label: None,
        source_execution_id: None,
        content_card: None,
        metadata_json: None,
    };

    let session = NormalizedConversationSession {
        external_id: "session-1".to_string(),
        title: Some("Ambient Filter Test".to_string()),
        project_path: None,
        started_at: None,
        updated_at: None,
        source_locator: None,
        source_fingerprint: None,
        execution_origin: None,
        execution_purpose: None,
        user_visible: None,
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-1".to_string(),
            turn_index: 0,
            user_text: "Please inspect and build".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            model: None,
            parts: vec![ambient_part, mutate_part],
        }],
    };

    let (pack, short_refs) = build_bounded_evidence_initial_pack(&session, &work_order);

    // 1. Mutate part is high priority and read in initial pack
    let mutate_ref = short_refs
        .get("ref-t1-p2")
        .expect("mutate ref should exist");
    assert_eq!(mutate_ref.status, EvidenceReadStatus::ReadInInitialPack);

    // 2. Ambient exploration part is downgraded and indexed only, saving initial pack budget
    let ambient_ref = short_refs
        .get("ref-t1-p1")
        .expect("ambient ref should exist");
    assert_eq!(ambient_ref.status, EvidenceReadStatus::IndexedOnly);

    // 3. Outcomes and verifications only contains mutate part
    assert_eq!(pack.outcomes_and_verifications.len(), 1);
    assert_eq!(pack.outcomes_and_verifications[0].ref_key, "ref-t1-p2");
    assert!(pack.outcomes_and_verifications[0]
        .text
        .contains("cargo build"));

    // 4. Index contains ambient part entry
    assert!(pack.index.iter().any(|idx| idx.ref_key == "ref-t1-p1"));
}

#[test]
fn test_subagent_part_is_prioritized_with_subagent_title() {
    let work_order = make_test_work_order();
    let subagent_part = NormalizedConversationPart {
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Text,
        text: Some(
            r#"{"agent_role":"Tester","task":"run all unit tests","child_session_id":"sub-99"}"#
                .to_string(),
        ),
        language: None,
        command: None,
        cwd: None,
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: None,
        content_card: Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "subagent".to_string(),
            semantic_role: Some("subagent".to_string()),
            renderer: Some("subagent_tree".to_string()),
        }),
        metadata_json: Some(r#"{"card_type":"subagent","agent_role":"Tester"}"#.to_string()),
    };

    let session = NormalizedConversationSession {
        external_id: "session-1".to_string(),
        title: Some("Subagent Test".to_string()),
        project_path: None,
        started_at: None,
        updated_at: None,
        source_locator: None,
        source_fingerprint: None,
        execution_origin: None,
        execution_purpose: None,
        user_visible: None,
        turns: vec![NormalizedConversationTurn {
            external_id: "turn-1".to_string(),
            turn_index: 0,
            user_text: "Delegate test run".to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            model: None,
            parts: vec![subagent_part],
        }],
    };

    let (pack, short_refs) = build_bounded_evidence_initial_pack(&session, &work_order);

    let sub_ref = short_refs
        .get("ref-t1-p1")
        .expect("subagent ref should exist");
    assert_eq!(sub_ref.status, EvidenceReadStatus::ReadInInitialPack);

    assert_eq!(pack.outcomes_and_verifications.len(), 1);
    let node = &pack.outcomes_and_verifications[0];
    assert_eq!(node.ref_key, "ref-t1-p1");
    assert!(node.title.contains("(Subagent)"));
    assert!(node.text.contains("Tester"));
}
