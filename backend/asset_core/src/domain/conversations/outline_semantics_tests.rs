use super::*;
use serde_json::json;

fn facts(started_at: Option<&str>) -> SessionTurnFacts {
    SessionTurnFacts {
        turn_id: "turn-f".to_string(),
        question: "Question F".to_string(),
        activities: Vec::new(),
        cards: 0,
        answer: None,
        started_at: started_at.map(str::to_string),
    }
}

#[test]
fn serializes_rows_with_started_at_at_the_tail() {
    let questions = SessionTurnRow::from_facts(
        ConversationOutlineDetailLevel::Questions,
        1,
        facts(Some("2026-10-21T10:00:00Z")),
    );
    assert_eq!(
        serde_json::to_value(&questions).unwrap(),
        json!([1, "turn-f", "Question F", "2026-10-21T10:00:00Z"])
    );

    let mut qa_facts = facts(Some("2026-10-21T10:00:00Z"));
    qa_facts.answer = Some("Answer F".to_string());
    let qa = SessionTurnRow::from_facts(ConversationOutlineDetailLevel::Qa, 2, qa_facts);
    assert_eq!(
        serde_json::to_value(&qa).unwrap(),
        json!([
            2,
            "turn-f",
            "Question F",
            "Answer F",
            "2026-10-21T10:00:00Z"
        ])
    );

    let mut default_facts = facts(Some("2026-10-21T10:00:00Z"));
    default_facts.activities = vec![
        "check".to_string(),
        "Subagent(Explore -> 8f3a1c2b)".to_string(),
    ];
    default_facts.cards = 2;
    let default =
        SessionTurnRow::from_facts(ConversationOutlineDetailLevel::Default, 3, default_facts);
    assert_eq!(
        serde_json::to_value(&default).unwrap(),
        json!([
            3,
            "turn-f",
            "Question F",
            ["check", "Subagent(Explore -> 8f3a1c2b)"],
            2,
            "2026-10-21T10:00:00Z"
        ])
    );

    let mut full_facts = facts(Some("2026-10-21T10:05:00Z"));
    full_facts.activities = vec!["check".to_string()];
    full_facts.cards = 1;
    full_facts.answer = Some("Answer G".to_string());
    let full = SessionTurnRow::from_facts(ConversationOutlineDetailLevel::Full, 4, full_facts);
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        json!([
            4,
            "turn-f",
            "Question F",
            ["check"],
            1,
            "Answer G",
            "2026-10-21T10:05:00Z"
        ])
    );
}

#[test]
fn column_sets_match_row_arity() {
    assert_eq!(
        SessionTurnRow::columns(ConversationOutlineDetailLevel::Questions),
        vec!["index", "turn_id", "question", "started_at"]
    );
    assert_eq!(
        SessionTurnRow::columns(ConversationOutlineDetailLevel::Qa),
        vec!["index", "turn_id", "question", "answer", "started_at"]
    );
    assert_eq!(
        SessionTurnRow::columns(ConversationOutlineDetailLevel::Default),
        vec![
            "index",
            "turn_id",
            "question",
            "activities",
            "cards",
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
            "started_at"
        ]
    );
    // 伪轮次语义列被物理删除后，不得再出现在列名单里。
    for level in [
        ConversationOutlineDetailLevel::Default,
        ConversationOutlineDetailLevel::Questions,
        ConversationOutlineDetailLevel::Qa,
        ConversationOutlineDetailLevel::QaDigest,
        ConversationOutlineDetailLevel::Full,
    ] {
        let columns = SessionTurnRow::columns(level);
        for forbidden in ["turn_kind", "agent_role", "parent_turn_id"] {
            assert!(!columns.iter().any(|column| column == forbidden));
        }
    }
}

#[test]
fn deserializes_legacy_three_element_question_row() {
    let questions: SessionTurnRow =
        serde_json::from_value(json!([1, "turn-a", "Question A"])).unwrap();
    assert_eq!(questions.index(), 1);
    assert_eq!(questions.turn_id(), "turn-a");
    assert_eq!(questions.question(), "Question A");
    assert_eq!(questions.started_at(), None);
}

#[test]
fn round_trips_every_detail_level() {
    let rows = [
        SessionTurnRow::from_facts(
            ConversationOutlineDetailLevel::Questions,
            1,
            facts(Some("2026-10-21T10:00:00Z")),
        ),
        {
            let mut f = facts(None);
            f.answer = Some("Answer".to_string());
            SessionTurnRow::from_facts(ConversationOutlineDetailLevel::Qa, 2, f)
        },
        {
            let mut f = facts(Some("2026-10-21T10:01:00Z"));
            f.activities = vec!["run tests".to_string()];
            f.cards = 2;
            SessionTurnRow::from_facts(ConversationOutlineDetailLevel::Default, 3, f)
        },
        {
            let mut f = facts(Some("2026-10-21T10:02:00Z"));
            f.activities = vec!["run tests".to_string()];
            f.cards = 2;
            f.answer = Some("Answer".to_string());
            SessionTurnRow::from_facts(ConversationOutlineDetailLevel::Full, 4, f)
        },
    ];
    for row in rows {
        let json = serde_json::to_value(&row).unwrap();
        let round_trip: SessionTurnRow = serde_json::from_value(json).unwrap();
        assert_eq!(round_trip, row);
    }
}

#[test]
fn rejects_rows_outside_the_contract() {
    for len in [0usize, 2, 8] {
        let value = serde_json::Value::Array(vec![json!(1); len]);
        assert!(serde_json::from_value::<SessionTurnRow>(value).is_err());
    }
}
