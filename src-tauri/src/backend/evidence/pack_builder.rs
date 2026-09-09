use std::collections::HashMap;

use crate::backend::evidence::types::{
    BoundedEvidenceInitialPack, BoundedEvidenceNode, EvidenceCoverage, EvidenceIndexEntry,
    EvidenceNodeKind, EvidenceReadStatus, ShortEvidenceRef,
};
use crate::backend::memory_redaction::redact_memory_text;
use crate::backend::models::{MemoryExecutionWorkOrder, NormalizedConversationSession};

pub fn build_bounded_evidence_initial_pack(
    session: &NormalizedConversationSession,
    work_order: &MemoryExecutionWorkOrder,
) -> (
    BoundedEvidenceInitialPack,
    HashMap<String, ShortEvidenceRef>,
) {
    let budget = &work_order.budget_policy;
    let mut short_refs = HashMap::new();
    let mut intent_and_corrections = Vec::new();
    let mut outcomes_and_verifications = Vec::new();
    let mut index = Vec::new();

    let mut current_chars: usize = 0;
    let mut read_nodes_count: usize = 0;
    let mut indexed_nodes_count: usize = 0;
    let mut truncated_nodes_count: usize = 0;
    let mut unavailable_nodes_count: usize = 0;
    let mut total_nodes: usize = 0;
    let total_turns: usize = session.turns.len();

    let task_boundary = format!(
        "Session: {} | Turns: {} | Project: {}",
        session.external_id,
        session.turns.len(),
        session.project_path.as_deref().unwrap_or("none")
    );
    current_chars += task_boundary.len();

    for (t_idx, turn) in session.turns.iter().enumerate() {
        let turn_id = turn.external_id.clone();
        let turn_index = turn.turn_index;

        // 1. 处理 User Text 节点
        let user_text_trimmed = turn.user_text.trim();
        let user_ref_key = format!("ref-t{}-u", t_idx + 1);
        if !user_text_trimmed.is_empty() {
            total_nodes += 1;
            let kind = classify_user_node_kind(user_text_trimmed);

            let can_fit_in_pack = read_nodes_count < budget.initial_pack_node_limit
                && current_chars + user_text_trimmed.len().min(budget.single_item_max_chars)
                    <= budget.initial_pack_max_chars;

            if can_fit_in_pack {
                let redacted = redact_memory_text(user_text_trimmed).text;
                let (final_text, truncated) =
                    if redacted.chars().count() > budget.single_item_max_chars {
                        truncated_nodes_count += 1;
                        let truncated_str: String = redacted
                            .chars()
                            .take(budget.single_item_max_chars)
                            .collect();
                        (truncated_str, true)
                    } else {
                        (redacted, false)
                    };

                current_chars += final_text.len();
                read_nodes_count += 1;

                intent_and_corrections.push(BoundedEvidenceNode {
                    ref_key: user_ref_key.clone(),
                    role: "user".to_string(),
                    kind,
                    title: format!("Turn {} User", t_idx + 1),
                    text: final_text,
                    truncated,
                });

                short_refs.insert(
                    user_ref_key.clone(),
                    ShortEvidenceRef {
                        ref_key: user_ref_key,
                        turn_id: turn_id.clone(),
                        turn_index,
                        part_index: 0,
                        role: "user".to_string(),
                        status: if truncated {
                            EvidenceReadStatus::Truncated
                        } else {
                            EvidenceReadStatus::ReadInInitialPack
                        },
                    },
                );
            } else {
                indexed_nodes_count += 1;
                let snippet: String = user_text_trimmed.chars().take(120).collect();
                index.push(EvidenceIndexEntry {
                    ref_key: user_ref_key.clone(),
                    turn_id: turn_id.clone(),
                    turn_index,
                    role: "user".to_string(),
                    snippet,
                    total_chars: user_text_trimmed.len(),
                    has_more: user_text_trimmed.chars().count() > 120,
                });

                short_refs.insert(
                    user_ref_key.clone(),
                    ShortEvidenceRef {
                        ref_key: user_ref_key,
                        turn_id: turn_id.clone(),
                        turn_index,
                        part_index: 0,
                        role: "user".to_string(),
                        status: EvidenceReadStatus::IndexedOnly,
                    },
                );
            }
        } else {
            unavailable_nodes_count += 1;
            total_nodes += 1;
            short_refs.insert(
                user_ref_key.clone(),
                ShortEvidenceRef {
                    ref_key: user_ref_key,
                    turn_id: turn_id.clone(),
                    turn_index,
                    part_index: 0,
                    role: "user".to_string(),
                    status: EvidenceReadStatus::Unavailable,
                },
            );
        }

        // 2. 处理 Turn 的各个 Parts
        for (p_idx, part) in turn.parts.iter().enumerate() {
            total_nodes += 1;
            let ref_key = format!("ref-t{}-p{}", t_idx + 1, p_idx + 1);
            let role = part.role.as_str().to_ascii_lowercase();

            let raw_text = part.text.as_deref().unwrap_or("").trim();
            if raw_text.is_empty() {
                unavailable_nodes_count += 1;
                short_refs.insert(
                    ref_key.clone(),
                    ShortEvidenceRef {
                        ref_key,
                        turn_id: turn_id.clone(),
                        turn_index,
                        part_index: p_idx,
                        role: role.clone(),
                        status: EvidenceReadStatus::Unavailable,
                    },
                );
                continue;
            }

            let is_env_dump = is_transient_environment_dump(raw_text);
            let has_command_or_exit = part.command.is_some() || part.exit_code.is_some();
            let kind = classify_assistant_node_kind(raw_text, has_command_or_exit);

            let is_high_priority = match kind {
                EvidenceNodeKind::VerificationEvidence => true,
                EvidenceNodeKind::ExecutionEvidence => !is_env_dump,
                _ => false,
            };

            let can_fit_in_pack = read_nodes_count < budget.initial_pack_node_limit
                && current_chars + raw_text.len().min(budget.single_item_max_chars)
                    <= budget.initial_pack_max_chars;

            if is_high_priority && can_fit_in_pack && !is_env_dump {
                let redacted = redact_memory_text(raw_text).text;
                let (final_text, truncated) =
                    if redacted.chars().count() > budget.single_item_max_chars {
                        truncated_nodes_count += 1;
                        let truncated_str: String = redacted
                            .chars()
                            .take(budget.single_item_max_chars)
                            .collect();
                        (truncated_str, true)
                    } else {
                        (redacted, false)
                    };

                current_chars += final_text.len();
                read_nodes_count += 1;

                outcomes_and_verifications.push(BoundedEvidenceNode {
                    ref_key: ref_key.clone(),
                    role: role.clone(),
                    kind,
                    title: format!("Turn {} Part {} ({})", t_idx + 1, p_idx + 1, role),
                    text: final_text,
                    truncated,
                });

                short_refs.insert(
                    ref_key.clone(),
                    ShortEvidenceRef {
                        ref_key,
                        turn_id: turn_id.clone(),
                        turn_index,
                        part_index: p_idx,
                        role,
                        status: if truncated {
                            EvidenceReadStatus::Truncated
                        } else {
                            EvidenceReadStatus::ReadInInitialPack
                        },
                    },
                );
            } else {
                indexed_nodes_count += 1;
                let snippet: String = raw_text.chars().take(120).collect();
                index.push(EvidenceIndexEntry {
                    ref_key: ref_key.clone(),
                    turn_id: turn_id.clone(),
                    turn_index,
                    role: role.clone(),
                    snippet,
                    total_chars: raw_text.len(),
                    has_more: raw_text.chars().count() > 120,
                });

                short_refs.insert(
                    ref_key.clone(),
                    ShortEvidenceRef {
                        ref_key,
                        turn_id: turn_id.clone(),
                        turn_index,
                        part_index: p_idx,
                        role,
                        status: EvidenceReadStatus::IndexedOnly,
                    },
                );
            }
        }
    }

    let is_fully_covered = indexed_nodes_count == 0 && unavailable_nodes_count == 0;
    let coverage = EvidenceCoverage {
        total_turns,
        total_nodes,
        indexed_nodes: indexed_nodes_count,
        read_nodes: read_nodes_count,
        truncated_nodes: truncated_nodes_count,
        unavailable_nodes: unavailable_nodes_count,
        is_fully_covered,
    };

    let pack = BoundedEvidenceInitialPack {
        work_order_id: work_order.work_order_id.clone(),
        session_id: work_order.session_id.clone(),
        source_revision: work_order.source_revision,
        task_boundary,
        intent_and_corrections,
        outcomes_and_verifications,
        index,
        coverage,
        total_chars: current_chars,
        nodes_count: read_nodes_count,
    };

    (pack, short_refs)
}

fn classify_user_node_kind(text: &str) -> EvidenceNodeKind {
    let lower = text.to_lowercase();
    const CORRECTION_PATTERNS: &[&str] = &[
        "更正",
        "修正",
        "不是",
        "改为",
        "不对",
        "不要",
        "撤回",
        "取消",
        "重新考虑",
        "换成",
        "改成",
        "switch to",
        "instead of",
        "rather than",
        "wrong",
        "do not",
        "don't",
    ];
    if CORRECTION_PATTERNS.iter().any(|p| lower.contains(p)) {
        EvidenceNodeKind::UserCorrection
    } else {
        EvidenceNodeKind::UserIntent
    }
}

fn classify_assistant_node_kind(text: &str, has_command_or_exit: bool) -> EvidenceNodeKind {
    let lower = text.to_lowercase();
    const RUNNER_PATTERNS: &[&str] = &[
        "test result:",
        "failures:",
        "passed;",
        "0 failed",
        "failures: 0",
        "tests passed:",
        "finished `test` profile",
    ];
    if RUNNER_PATTERNS.iter().any(|p| lower.contains(p))
        || (has_command_or_exit
            && (lower.contains("cargo test")
                || lower.contains("pnpm test")
                || lower.contains("go test")
                || lower.contains("npm test")))
    {
        EvidenceNodeKind::VerificationEvidence
    } else {
        EvidenceNodeKind::ExecutionEvidence
    }
}

fn is_transient_environment_dump(text: &str) -> bool {
    let lower = text.to_lowercase();
    (lower.contains("environment:") && lower.contains("os:"))
        || (lower.contains("path=") && lower.contains("/usr/bin"))
        || lower.contains("npm list --depth=0")
        || (lower.contains("system info:") && lower.contains("kernel"))
}
