use super::*;
use crate::backend::domain::conversations::ConversationGroupingOrigin;

#[test]
fn only_merges_exact_simple_acknowledgements() {
    assert!(should_auto_merge_acknowledgement("继续"));
    assert!(should_auto_merge_acknowledgement("OK"));
    assert!(!should_auto_merge_acknowledgement("继续解释一下原因"));
    assert!(!should_auto_merge_acknowledgement("ok?"));
    assert!(!should_auto_merge_acknowledgement("ok\nnow add tests"));
}

#[test]
fn groups_acknowledgement_turns_with_previous_question() {
    let groups = group_turn_ids_by_question(vec![
        ("t1".to_string(), "How does sync work?".to_string()),
        ("t2".to_string(), "继续".to_string()),
        ("t3".to_string(), "Now export it".to_string()),
    ]);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].turn_ids, vec!["t1", "t2"]);
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[1].turn_ids, vec!["t3"]);
}

#[test]
fn groups_interruption_recovery_and_micro_follow_up_with_previous_question() {
    let groups = group_turn_ids_by_question(vec![
        ("t1".to_string(), "Design the sync boundary".to_string()),
        ("t2".to_string(), "继续上一个问题".to_string()),
        ("t3".to_string(), "改成 Rust".to_string()),
        ("t4".to_string(), "Now export it".to_string()),
    ]);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].turn_ids, vec!["t1", "t2", "t3"]);
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[1].turn_ids, vec!["t4"]);
    assert_eq!(groups[1].origin, ConversationGroupingOrigin::Imported);
}

#[test]
fn dynamic_projection_groups_and_derives_deterministic_question_ids() {
    struct TestTurn {
        id: String,
        prompt: String,
    }
    let turns = vec![
        TestTurn {
            id: "conversation-turn-0001".to_string(),
            prompt: "What is Ticket 01?".to_string(),
        },
        TestTurn {
            id: "conversation-turn-0002".to_string(),
            prompt: "继续".to_string(),
        },
        TestTurn {
            id: "conversation-turn-0003".to_string(),
            prompt: "Now explain Ticket 02".to_string(),
        },
    ];

    let groups =
        group_turns_by_dynamic_projection("conversation-question", turns, |t| &t.prompt, |t| &t.id);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].question_id, "conversation-question-0001");
    assert_eq!(groups[0].origin, ConversationGroupingOrigin::AutoMerged);
    assert_eq!(groups[0].turns.len(), 2);
    assert_eq!(groups[0].turns[0].id, "conversation-turn-0001");
    assert_eq!(groups[0].turns[1].id, "conversation-turn-0002");

    assert_eq!(groups[1].question_id, "conversation-question-0003");
    assert_eq!(groups[1].origin, ConversationGroupingOrigin::Imported);
    assert_eq!(groups[1].turns.len(), 1);
    assert_eq!(groups[1].turns[0].id, "conversation-turn-0003");

    assert_eq!(
        derive_turn_id_from_question_id("conversation-question-0001"),
        Some("conversation-turn-0001".to_string())
    );
    assert_eq!(
        derive_turn_id_from_question_id("web-record-question-0001"),
        Some("web-record-turn-0001".to_string())
    );
    assert_eq!(derive_turn_id_from_question_id("invalid-question"), None);
}

#[test]
fn test_derive_question_id_from_turn() {
    assert_eq!(
        derive_question_id_from_turn("conversation-turn-abc12345"),
        "conversation-question-abc12345"
    );
    assert_eq!(
        derive_question_id_from_turn("web-record-turn-def67890"),
        "web-record-question-def67890"
    );
    assert_eq!(derive_question_id_from_turn("custom-q-1"), "custom-q-1");
}
