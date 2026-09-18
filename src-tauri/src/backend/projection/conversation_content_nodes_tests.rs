use super::*;
use crate::backend::dto::ConversationCardRenderer;
use crate::backend::models::{ConversationGroupingOrigin, ConversationPartKind};

fn test_part(id: &str, turn_id: &str, part_index: i64) -> ConversationPart {
    ConversationPart {
        id: id.to_string(),
        turn_id: turn_id.to_string(),
        part_index,
        role: ConversationPartRole::Assistant,
        kind: ConversationPartKind::Text,
        text: Some(id.to_string()),
        language: None,
        command: None,
        cwd: None,
        status: None,
        exit_code: None,
        command_label: None,
        source_execution_id: Some("execution-1".to_string()),
        content_card: None,
        metadata_json: None,
        translated_text: None,
    }
}

fn candidate(node_type: &str, content: &str) -> ConversationContentNodeCandidate {
    ConversationContentNodeCandidate {
        node_type: node_type.to_string(),
        semantic_role: Some(node_type.to_string()),
        renderer: ConversationCardRenderer::Plain,
        role: ConversationPartRole::Assistant,
        content: content.to_string(),
        language: None,
        cwd: None,
        status: None,
        exit_code: None,
        source_execution_id: Some("execution-1".to_string()),
        command_label: None,
        translated_content: None,
        legacy_anchor_ids: vec![format!("part-1-{node_type}")],
    }
}

#[test]
fn projects_zero_one_or_many_nodes_without_array_indexes() {
    let part = test_part("part-1", "turn-1", 4);
    let nodes = project_content_nodes_for_part(
        "question-1",
        3,
        &part,
        &[
            candidate("summary", "summary"),
            candidate("detail", "detail"),
        ],
    );

    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].node_id, "part-1-node-0");
    assert_eq!(nodes[1].node_id, "part-1-node-1");
    assert_eq!(nodes[0].question_id, "question-1");
    assert_eq!(nodes[0].turn_id, "turn-1");
    assert_eq!(nodes[0].part_id, "part-1");
    assert_eq!(nodes[0].turn_order, 3);
    assert_eq!(nodes[0].part_order, 4);
    assert_eq!(nodes[0].node_order, 0);
    assert!(nodes[0].legacy_anchor_ids.contains(&"part-1".to_string()));
    assert_eq!(nodes[1].node_order, 1);
    assert_eq!(nodes[1].content, "detail");
    assert_eq!(nodes[1].legacy_anchor_ids, vec!["part-1-detail"]);

    assert!(project_content_nodes_for_part("question-1", 3, &part, &[]).is_empty());
}

#[test]
fn projects_a_single_node_with_the_source_part_identity() {
    let part = test_part("part-single", "turn-1", 0);
    let mut single_candidate = candidate("answer", "answer");
    single_candidate.legacy_anchor_ids = vec!["part-single-answer".to_string()];
    let nodes = project_content_nodes_for_part("question-1", 0, &part, &[single_candidate]);

    assert_eq!(nodes[0].node_id, "part-single");
    assert_eq!(nodes[0].locator.node_order, 0);
    assert_eq!(
        nodes[0].legacy_anchor_ids,
        vec!["part-single-answer", "part-single-node-0"]
    );
}

#[test]
fn orders_nodes_by_question_membership_part_and_node_order() {
    let parts = vec![
        test_part("part-t1-1", "turn-1", 1),
        test_part("part-empty", "turn-2", 1),
        test_part("part-t2-0", "turn-2", 0),
        test_part("part-t1-0", "turn-1", 0),
    ];
    let question_turns = vec![
        ConversationQuestionTurn {
            question_id: "question-1".to_string(),
            turn_id: "turn-1".to_string(),
            turn_order: 1,
            assignment_origin: ConversationGroupingOrigin::Imported,
            assigned_at: "2026-08-25T00:00:00Z".to_string(),
            updated_at: "2026-08-25T00:00:00Z".to_string(),
        },
        ConversationQuestionTurn {
            question_id: "question-1".to_string(),
            turn_id: "turn-2".to_string(),
            turn_order: 0,
            assignment_origin: ConversationGroupingOrigin::Imported,
            assigned_at: "2026-08-25T00:00:00Z".to_string(),
            updated_at: "2026-08-25T00:00:00Z".to_string(),
        },
    ];
    let nodes = project_conversation_content_nodes("question-1", &question_turns, &parts, |part| {
        if part.id == "part-empty" {
            Ok(Vec::new())
        } else if part.id == "part-t1-0" {
            Ok(vec![
                candidate("summary", "summary"),
                candidate("detail", "detail"),
            ])
        } else {
            Ok(vec![candidate("answer", "answer")])
        }
    })
    .expect("project content nodes");

    assert_eq!(
        nodes
            .iter()
            .map(|node| node.part_id.as_str())
            .collect::<Vec<_>>(),
        vec!["part-t2-0", "part-t1-0", "part-t1-0", "part-t1-1"]
    );
    assert_eq!(nodes[0].turn_order, 0);
    assert_eq!(nodes[1].turn_order, 1);
    assert_eq!(nodes[2].turn_order, 1);
    assert_eq!(nodes[3].turn_order, 1);
    assert_eq!(nodes[1].node_order, 0);
    assert_eq!(nodes[2].node_order, 1);
    assert_eq!(nodes[3].node_order, 0);
    assert!(!nodes.iter().any(|node| node.part_id == "part-empty"));
}
