use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::conversations::{
    ConversationContentNodeLocator, ConversationPartRole, ConversationSessionDetail,
    NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
};
use crate::backend::domain::memory::evidence::{BoundedEvidenceInitialPack, ShortEvidenceRef};
use crate::backend::domain::memory::MemoryRecipe;
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
pub(crate) fn build_session_memory_prompt(
    detail: &ConversationSessionDetail,
    evidence: &[EvidenceReference],
) -> AppResult<String> {
    let title =
        crate::backend::domain::memory::evidence::redact_memory_text(&detail.session.title).text;
    let evidence = evidence
        .iter()
        .take(MAX_EVIDENCE_ITEMS)
        .map(|item| PromptEvidence {
            reference_key: &item.key,
            locator: &item.locator,
            content: &item.content,
        })
        .collect::<Vec<_>>();
    let prompt = json!({
        "contract_version": SESSION_MEMORY_CONTRACT_VERSION,
        "prompt_version": SESSION_MEMORY_PROMPT_VERSION,
        "task": "Extract a concise structured Session Memory from canonical Conversation evidence.",
        "session": { "title": title },
        "evidence": evidence,
        "output": {
            "summary": "string",
            "goal": "string",
            "result": "string",
            "decisions": ["string"],
            "verification": ["string"],
            "blockers": ["string"],
            "follow_up": ["string"],
            "topics": ["string"],
            "source_references": [{ "reference_key": "one evidence reference_key" }],
            "events": [{
                "category": "progress|decision|research|verification|blocker|follow_up",
                "title": "string",
                "summary": "string",
                "source_reference": "optional evidence reference_key"
            }]
        }
    });
    serde_json::to_string(&prompt).map_err(AppError::external)
}

pub(crate) fn build_evidence_references(
    detail: &ConversationSessionDetail,
) -> Vec<EvidenceReference> {
    let mut references = Vec::new();
    for question in &detail.questions {
        for node in &question.projected_content_nodes {
            let key = format!("node:{}", node.node_id);
            let content =
                crate::backend::domain::memory::evidence::redact_memory_text(&node.content).text;
            references.push(EvidenceReference {
                key,
                locator: node.locator.clone(),
                node_id: Some(node.node_id.clone()),
                content,
            });
        }
        if question.projected_content_nodes.is_empty() {
            for turn in &question.turns {
                let key = format!("turn:{}", turn.id);
                let locator = ConversationContentNodeLocator {
                    question_id: question.question.id.clone(),
                    turn_id: turn.id.clone(),
                    part_id: String::new(),
                    node_order: 0,
                };
                let content =
                    crate::backend::domain::memory::evidence::redact_memory_text(&turn.user_text)
                        .text;
                references.push(EvidenceReference {
                    key,
                    locator,
                    node_id: None,
                    content,
                });
            }
        }
    }
    references.sort_by(|left, right| left.key.cmp(&right.key));
    references.dedup_by(|left, right| left.key == right.key);
    references
}

pub(crate) fn session_detail_to_normalized(
    detail: &ConversationSessionDetail,
) -> NormalizedConversationSession {
    let mut turns = Vec::new();
    for question in &detail.questions {
        for turn in &question.turns {
            let parts = question
                .parts
                .iter()
                .filter(|p| p.turn_id == turn.id)
                .map(|p| NormalizedConversationPart {
                    role: p.role,
                    kind: p.kind,
                    text: p.text.clone(),
                    language: p.language.clone(),
                    command: p.command.clone(),
                    cwd: p.cwd.clone(),
                    status: p.status.clone(),
                    exit_code: p.exit_code,
                    command_label: p.command_label.clone(),
                    source_execution_id: p.source_execution_id.clone(),
                    content_card: None,
                    metadata_json: p.metadata_json.clone(),
                })
                .collect();

            turns.push(NormalizedConversationTurn {
                external_id: turn.external_id.clone(),
                turn_index: turn.turn_index,
                user_text: turn.user_text.clone(),
                title: turn.title.clone(),
                started_at: turn.started_at.clone(),
                ended_at: turn.ended_at.clone(),
                model: turn.model.clone(),
                parts,
            });
        }
    }

    NormalizedConversationSession {
        external_id: detail.session.external_id.clone(),
        title: Some(detail.session.title.clone()),
        project_path: detail.session.project_path.clone(),
        started_at: detail.session.started_at.clone(),
        updated_at: detail.session.updated_at.clone(),
        source_locator: detail.session.source_locator.clone(),
        source_fingerprint: detail.session.source_fingerprint.clone(),
        turns,
        ..Default::default()
    }
}

pub(crate) fn build_bounded_evidence_prompt(
    pack: &BoundedEvidenceInitialPack,
    recipe: &MemoryRecipe,
) -> AppResult<String> {
    let prompt = json!({
        "contract_version": SESSION_MEMORY_CONTRACT_VERSION,
        "prompt_version": SESSION_MEMORY_PROMPT_VERSION,
        "work_order_id": pack.work_order_id,
        "task_boundary": pack.task_boundary,
        "recipe": {
            "name": recipe.name,
            "focus_areas": recipe.focus_areas,
            "ignored_topics": recipe.ignored_topics,
            "terminology": recipe.terminology,
            "custom_instructions": recipe.custom_instructions,
        },
        "initial_evidence_pack": {
            "intent_and_corrections": pack.intent_and_corrections,
            "outcomes_and_verifications": pack.outcomes_and_verifications,
            "index": pack.index,
            "coverage": pack.coverage,
        },
        "instructions": [
            "Extract concise structured Session Memory from the bounded evidence pack.",
            "Cite evidence exclusively using the provided ref_key values (e.g. ref-t1-u, ref-t1-p1). Do not invent IDs.",
            "CRITICAL - User Decisions: Only record decisions that were confirmed by the user. If the user corrected, rejected, or modified an earlier proposal, DO NOT record the rejected/superseded proposal as a confirmed decision.",
            "CRITICAL - Verification: Distinguish between verified facts backed by test/tool evidence and unverified claims. If an agent claimed a task was completed or passed without verification evidence, do not record it as verified.",
            "CRITICAL - Strict Output Format: You MUST output ONLY a single valid raw JSON object strictly conforming to output_format. Do NOT wrap output in markdown code blocks like ```json or ```. Do NOT include any greetings, explanations, notes, or any text before or after the JSON.",
            "CRITICAL - Tool Prohibition: You are strictly forbidden from calling or invoking any tools, executing commands, reading files, or requesting user input. Produce the final JSON directly from the provided evidence.",
            "If the session has no meaningful user content or all nodes are unavailable, output empty arrays and empty summary."
        ],
        "output_format": {
            "summary": "string",
            "goal": "string",
            "result": "string",
            "decisions": ["string (confirmed user decisions only)"],
            "verification": ["string (verified with test/tool outputs)"],
            "blockers": ["string"],
            "follow_up": ["string"],
            "topics": ["string"],
            "source_references": [{ "reference_key": "ref_key" }],
            "events": [{
                "category": "progress|decision|research|verification|blocker|follow_up",
                "title": "string",
                "summary": "string",
                "source_reference": "optional ref_key"
            }]
        }
    });

    serde_json::to_string(&prompt).map_err(AppError::external)
}

pub(crate) fn build_bounded_evidence_references(
    detail: &ConversationSessionDetail,
    short_refs: &HashMap<String, ShortEvidenceRef>,
) -> Vec<EvidenceReference> {
    let mut references = Vec::new();

    for (ref_key, sref) in short_refs {
        if sref.status == EvidenceReadStatus::Unavailable {
            continue;
        }

        let mut matched_locator = None;
        let mut matched_node_id = None;
        let mut matched_content = String::new();

        for question in &detail.questions {
            if let Some(turn) = question
                .turns
                .iter()
                .find(|t| t.external_id == sref.turn_id)
            {
                if sref.ref_key.ends_with("-u") {
                    matched_locator = Some(ConversationContentNodeLocator {
                        question_id: question.question.id.clone(),
                        turn_id: turn.id.clone(),
                        part_id: String::new(),
                        node_order: 0,
                    });
                    matched_content = turn.user_text.clone();
                    if let Some(node) = question
                        .projected_content_nodes
                        .iter()
                        .find(|n| n.turn_id == turn.id && n.role == ConversationPartRole::User)
                    {
                        matched_node_id = Some(node.node_id.clone());
                    }
                    break;
                } else {
                    let part_opt = question
                        .parts
                        .iter()
                        .find(|p| p.turn_id == turn.id && p.part_index == sref.part_index as i64);
                    if let Some(part) = part_opt {
                        let node_order = question
                            .projected_content_nodes
                            .iter()
                            .find(|n| n.part_id == part.id)
                            .map(|n| n.node_order)
                            .unwrap_or(sref.part_index);

                        matched_locator = Some(ConversationContentNodeLocator {
                            question_id: question.question.id.clone(),
                            turn_id: turn.id.clone(),
                            part_id: part.id.clone(),
                            node_order,
                        });
                        matched_content = part.text.clone().unwrap_or_default();
                        if let Some(node) = question
                            .projected_content_nodes
                            .iter()
                            .find(|n| n.part_id == part.id)
                        {
                            matched_node_id = Some(node.node_id.clone());
                        }
                        break;
                    }
                }
            }
        }

        if let Some(locator) = matched_locator {
            references.push(EvidenceReference {
                key: ref_key.clone(),
                locator,
                node_id: matched_node_id,
                content: crate::backend::domain::memory::evidence::redact_memory_text(
                    &matched_content,
                )
                .text,
            });
        }
    }

    references.sort_by(|a, b| a.key.cmp(&b.key));
    references
}
