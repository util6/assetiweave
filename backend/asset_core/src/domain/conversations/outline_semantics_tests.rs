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
    let row: SessionTurnRow = serde_json::from_value(json!([
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
    assert_eq!(row.turn_kind(), Some("subagent"));
    assert_eq!(row.agent_role(), Some("guard_audit"));
    assert_eq!(row.parent_turn_id(), Some("turn-parent"));

    let full: SessionTurnRow = serde_json::from_value(json!([
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
    assert_eq!(full.answer(), Some("Answer E"));
    assert_eq!(full.turn_kind(), Some("subagent"));
    assert_eq!(full.agent_role(), None);
    assert_eq!(full.parent_turn_id(), Some("turn-parent"));
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
            "turn-parent"
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
            "parent_turn_id"
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
            "parent_turn_id"
        ]
    );
}
