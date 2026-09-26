use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::cards::ConversationCard;
use super::error::ProjectionError;
use super::renderer::ConversationCardRenderer;
use crate::backend::domain::{ConversationPart, ConversationPartRole, ConversationQuestionTurn};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationContentNodeLocator {
    pub question_id: String,
    pub turn_id: String,
    pub part_id: String,
    pub node_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationContentNode {
    pub node_id: String,
    pub locator: ConversationContentNodeLocator,
    pub question_id: String,
    pub turn_id: String,
    pub part_id: String,
    pub turn_order: i64,
    pub part_order: i64,
    pub node_order: usize,
    pub node_type: String,
    pub semantic_role: Option<String>,
    pub renderer: ConversationCardRenderer,
    pub role: ConversationPartRole,
    pub content: String,
    pub language: Option<String>,
    pub cwd: Option<String>,
    pub status: Option<String>,
    pub exit_code: Option<i32>,
    pub source_execution_id: Option<String>,
    pub command_label: Option<String>,
    pub translated_content: Option<String>,
    pub legacy_anchor_ids: Vec<String>,
}

/// A renderer-owned candidate that can be emitted as one or more nodes for a source Part.
#[derive(Debug, Clone)]
pub struct ConversationContentNodeCandidate {
    pub node_type: String,
    pub semantic_role: Option<String>,
    pub renderer: ConversationCardRenderer,
    pub role: ConversationPartRole,
    pub content: String,
    pub language: Option<String>,
    pub cwd: Option<String>,
    pub status: Option<String>,
    pub exit_code: Option<i32>,
    pub source_execution_id: Option<String>,
    pub command_label: Option<String>,
    pub translated_content: Option<String>,
    pub legacy_anchor_ids: Vec<String>,
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

pub fn project_content_nodes_for_part(
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

pub fn project_conversation_content_nodes<F>(
    question_id: &str,
    question_turns: &[ConversationQuestionTurn],
    parts: &[ConversationPart],
    mut candidates_for_part: F,
) -> Result<Vec<ConversationContentNode>, ProjectionError>
where
    F: FnMut(&ConversationPart) -> Result<Vec<ConversationContentNodeCandidate>, ProjectionError>,
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
#[path = "nodes_tests.rs"]
mod tests;
