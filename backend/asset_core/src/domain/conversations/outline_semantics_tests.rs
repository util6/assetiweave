use super::*;
use serde_json::json;

#[test]
fn deserializes_legacy_three_four_five_element_rows() {
    let questions: SessionTurnRow =
        serde_json::from_value(json!([1, "turn-a", "Question A"])).unwrap();
    assert_eq!(questions.index(), 1);
    assert_eq!(questions.turn_id(), "turn-a");
    assert_eq!(questions.question(), "Question A");
    assert_eq!(questions.turn_kind(), None);
    assert_eq!(questions.agent_role(), None);
    assert_eq!(questions.parent_turn_id(), None);

    let qa: SessionTurnRow =
        serde_json::from_value(json!([2, "turn-b", "Question B", "Answer B"])).unwrap();
    assert_eq!(qa.answer(), Some("Answer B"));
    assert_eq!(qa.agent_role(), None);

    let default: SessionTurnRow =
        serde_json::from_value(json!([3, "turn-c", "Question C", ["run tests"], 2])).unwrap();
    assert_eq!(default.activities(), Some(&["run tests".to_string()][..]));
    assert_eq!(default.cards(), Some(2));
    assert_eq!(default.parent_turn_id(), None);
}

#[test]
fn keeps_subagent_semantics_for_current_formats() {
    // 兼容历史 8 元素 (Default 带 3 列语义，无 started_at)
    let legacy_row: SessionTurnRow = serde_json::from_value(json!([
        4,
        "turn-d",
        "Question D",
        ["run tests"],
        1,
        "subagent",
        "guard_audit",
        "turn-parent"
    ]))
    .unwrap();
    assert_eq!(legacy_row.turn_kind(), Some("subagent"));
    assert_eq!(legacy_row.agent_role(), Some("guard_audit"));
    assert_eq!(legacy_row.parent_turn_id(), Some("turn-parent"));
    assert_eq!(legacy_row.started_at(), None);

    // 兼容历史 9 元素 (Full 带 3 列语义，无 started_at)
    let legacy_full: SessionTurnRow = serde_json::from_value(json!([
        5,
        "turn-e",
        "Question E",
        [],
        0,
        "Answer E",
        "subagent",
        null,
        "turn-parent"
    ]))
    .unwrap();
    assert_eq!(legacy_full.answer(), Some("Answer E"));
    assert_eq!(legacy_full.turn_kind(), Some("subagent"));
    assert_eq!(legacy_full.agent_role(), None);
    assert_eq!(legacy_full.parent_turn_id(), Some("turn-parent"));
    assert_eq!(legacy_full.started_at(), None);

    // 当前 9 元素格式 (Default 带 started_at)
    let cur_default: SessionTurnRow = serde_json::from_value(json!([
        6,
        "turn-f",
        "Question F",
        ["run cargo test"],
        2,
        "user",
        null,
        null,
        "2026-10-21T10:00:00Z"
    ]))
    .unwrap();
    assert_eq!(cur_default.cards(), Some(2));
    assert_eq!(cur_default.turn_kind(), Some("user"));
    assert_eq!(cur_default.started_at(), Some("2026-10-21T10:00:00Z"));

    // 当前 10 元素格式 (Full 带 started_at)
    let cur_full: SessionTurnRow = serde_json::from_value(json!([
        7,
        "turn-g",
        "Question G",
        [],
        0,
        "Answer G",
        "subagent",
        "reviewer",
        "turn-parent",
        "2026-10-21T10:05:00Z"
    ]))
    .unwrap();
    assert_eq!(cur_full.answer(), Some("Answer G"));
    assert_eq!(cur_full.turn_kind(), Some("subagent"));
    assert_eq!(cur_full.agent_role(), Some("reviewer"));
    assert_eq!(cur_full.parent_turn_id(), Some("turn-parent"));
    assert_eq!(cur_full.started_at(), Some("2026-10-21T10:05:00Z"));
}

#[test]
fn serializes_rows_with_semantics_columns_at_the_tail() {
    let row = SessionTurnRow::from_facts(
        ConversationOutlineDetailLevel::Questions,
        1,
        SessionTurnFacts {
            turn_id: "turn-f".to_string(),
            question: "Question F".to_string(),
            activities: Vec::new(),
            cards: 0,
            answer: None,
            turn_kind: Some("subagent".to_string()),
            agent_role: Some("reviewer".to_string()),
            parent_turn_id: Some("turn-parent".to_string()),
            started_at: Some("2026-10-21T10:00:00Z".to_string()),
        },
    );
    assert_eq!(
        serde_json::to_value(&row).unwrap(),
        json!([
            1,
            "turn-f",
            "Question F",
            "subagent",
            "reviewer",
            "turn-parent",
            "2026-10-21T10:00:00Z"
        ])
    );
    assert_eq!(
        SessionTurnRow::columns(ConversationOutlineDetailLevel::Questions),
        vec![
            "index",
            "turn_id",
            "question",
            "turn_kind",
            "agent_role",
            "parent_turn_id",
            "started_at"
        ]
    );
    assert_eq!(
        SessionTurnRow::columns(ConversationOutlineDetailLevel::Full),
        vec![
            "index",
            "turn_id",
            "question",
            "activities",
            "cards",
            "answer",
            "turn_kind",
            "agent_role",
            "parent_turn_id",
            "started_at"
        ]
    );
}
