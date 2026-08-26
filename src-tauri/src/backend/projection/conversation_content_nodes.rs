use super::conversation_cards::ConversationCard;
use crate::backend::dto::{
    ConversationCardRenderer, ConversationContentNode, ConversationContentNodeLocator,
};
use crate::backend::models::{ConversationPart, ConversationPartRole, ConversationQuestionTurn};
use std::collections::BTreeMap;

/// A renderer-owned candidate that can be emitted as one or more nodes for a source Part.
///
/// The candidate deliberately has no array index or Card identity. The caller may emit zero,
/// one, or multiple candidates while this module assigns the stable Part-local node order.
#[derive(Debug, Clone)]
pub(crate) struct ConversationContentNodeCandidate {
    pub(crate) node_type: String,
    pub(crate) semantic_role: Option<String>,
    pub(crate) renderer: ConversationCardRenderer,
    pub(crate) role: ConversationPartRole,
    pub(crate) content: String,
    pub(crate) language: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) source_execution_id: Option<String>,
    pub(crate) command_label: Option<String>,
    pub(crate) translated_content: Option<String>,
    pub(crate) legacy_anchor_ids: Vec<String>,
}

impl From<ConversationCard> for ConversationContentNodeCandidate {
    fn from(card: ConversationCard) -> Self {
        Self {
            node_type: card.kind,
            semantic_role: card.semantic_role,
            renderer: card.renderer,
            role: card.role,
            content: card.body,
            language: card.language,
            cwd: card.cwd,
            status: card.status,
            exit_code: card.exit_code,
            source_execution_id: card.source_execution_id,
            command_label: card.command_label,
            translated_content: card.translated_body,
            legacy_anchor_ids: card.legacy_anchor_ids,
        }
    }
}

pub(crate) fn project_content_nodes_for_part(
    question_id: &str,
    turn_order: i64,
    part: &ConversationPart,
    candidates: &[ConversationContentNodeCandidate],
) -> Vec<ConversationContentNode> {
    candidates
        .iter()
        .enumerate()
        .map(|(node_order, candidate)| {
            let legacy_node_id = format!("{}-node-{node_order}", part.id);
            let node_id = if candidates.len() == 1 {
                part.id.clone()
            } else {
                legacy_node_id.clone()
            };
            let mut legacy_anchor_ids = candidate.legacy_anchor_ids.clone();
            if candidates.len() == 1 && !legacy_anchor_ids.contains(&legacy_node_id) {
                legacy_anchor_ids.push(legacy_node_id);
            }
            if candidates.len() > 1 && node_order == 0 && !legacy_anchor_ids.contains(&part.id) {
                legacy_anchor_ids.push(part.id.clone());
            }
            ConversationContentNode {
                node_id,
                locator: ConversationContentNodeLocator {
                    question_id: question_id.to_string(),
                    turn_id: part.turn_id.clone(),
                    part_id: part.id.clone(),
                    node_order,
                },
                question_id: question_id.to_string(),
                turn_id: part.turn_id.clone(),
                part_id: part.id.clone(),
                turn_order,
                part_order: part.part_index,
                node_order,
                node_type: candidate.node_type.clone(),
                semantic_role: candidate.semantic_role.clone(),
                renderer: candidate.renderer,
                role: candidate.role,
                content: candidate.content.clone(),
                language: candidate.language.clone(),
                cwd: candidate.cwd.clone(),
                status: candidate.status.clone(),
                exit_code: candidate.exit_code,
                source_execution_id: candidate.source_execution_id.clone(),
                command_label: candidate.command_label.clone(),
                translated_content: candidate.translated_content.clone(),
                legacy_anchor_ids,
            }
        })
        .collect()
}

pub(crate) fn project_conversation_content_nodes<F>(
    question_id: &str,
    question_turns: &[ConversationQuestionTurn],
    parts: &[ConversationPart],
    mut candidates_for_part: F,
) -> Result<Vec<ConversationContentNode>, String>
where
    F: FnMut(&ConversationPart) -> Result<Vec<ConversationContentNodeCandidate>, String>,
{
    let turn_orders = question_turns
        .iter()
        .map(|membership| (membership.turn_id.as_str(), membership.turn_order))
        .collect::<BTreeMap<_, _>>();
    let mut nodes = Vec::new();
    for part in parts {
        let Some(turn_order) = turn_orders.get(part.turn_id.as_str()).copied() else {
            continue;
        };
        let candidates = candidates_for_part(part)?;
        nodes.extend(project_content_nodes_for_part(
            question_id,
            turn_order,
            part,
            &candidates,
        ));
    }
    nodes.sort_by(|left, right| {
        (
            left.turn_order,
            left.part_order,
            left.node_order,
            left.node_id.as_str(),
        )
            .cmp(&(
                right.turn_order,
                right.part_order,
                right.node_order,
                right.node_id.as_str(),
            ))
    });
    Ok(nodes)
}

#[cfg(test)]
mod tests {
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
        let nodes =
            project_conversation_content_nodes("question-1", &question_turns, &parts, |part| {
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
}
