use super::prelude::*;
use crate::backend::{
    memory_redaction::redact_memory_text,
    models::{
        RecentSnapshotSessionEvidence, RecentSnapshotWorkOrderPayload,
        SessionMemorySourceReference, ALLOWED_MEMORY_GENERATION_TOOLS,
    },
    runtime::{AppError, AppResult},
    store,
};
use serde_json::{json, Value};
use std::collections::HashMap;

const MAX_SEARCH_RESULTS: usize = 24;
const MAX_SEARCH_SNIPPET_CHARS: usize = 600;
const MAX_NODE_CHARS: usize = 12_000;
const MAX_QUESTION_CHARS: usize = 24_000;

#[derive(Debug, Clone)]
struct ScopedEvidenceNode {
    node_ref: String,
    question_ref: String,
    role: String,
    content: String,
}

impl AppService {
    pub(crate) async fn call_memory_generation_tool(
        &self,
        job_id: &str,
        ownership_token: &str,
        tool_name: &str,
        arguments: &Value,
    ) -> AppResult<Value> {
        if !ALLOWED_MEMORY_GENERATION_TOOLS.contains(&tool_name) {
            return Err(AppError::Validation(format!(
                "Memory Generation tool is not allowlisted: {tool_name}"
            )));
        }
        let payload = self
            .load_authorized_memory_generation_payload(job_id, ownership_token)
            .await?;
        let session_ref = required_argument(arguments, "session_ref")?;
        let evidence = payload
            .evidence
            .session_evidence
            .iter()
            .find(|entry| entry.candidate.session_ref == session_ref)
            .ok_or_else(|| {
                AppError::Validation(
                    "Memory Generation session_ref is outside the frozen Work Order".to_string(),
                )
            })?;

        match tool_name {
            "get_session_outline" => Ok(memory_generation_session_outline(evidence)),
            "search_session_content" => {
                let query = required_argument(arguments, "query")?;
                let limit = arguments
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(8)
                    .clamp(1, MAX_SEARCH_RESULTS as u64) as usize;
                let needle = query.to_lowercase();
                let nodes = self.load_scoped_evidence_nodes(evidence).await?;
                let hits = nodes
                    .into_iter()
                    .filter(|node| node.content.to_lowercase().contains(&needle))
                    .take(limit)
                    .map(|node| {
                        json!({
                            "nodeRef": node.node_ref,
                            "questionRef": node.question_ref,
                            "role": node.role,
                            "snippet": take_chars(&node.content, MAX_SEARCH_SNIPPET_CHARS),
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(json!({
                    "sessionRef": session_ref,
                    "query": query,
                    "hits": hits,
                }))
            }
            "read_question_content" => {
                let question_ref = required_argument(arguments, "question_ref")?;
                let offset = arguments.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
                let limit = arguments
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(MAX_QUESTION_CHARS as u64)
                    .clamp(1, MAX_QUESTION_CHARS as u64) as usize;
                let nodes = self.load_scoped_evidence_nodes(evidence).await?;
                let matching = nodes
                    .into_iter()
                    .filter(|node| node.question_ref == question_ref)
                    .collect::<Vec<_>>();
                if matching.is_empty() {
                    return Err(AppError::Validation(
                        "Memory Generation question_ref is outside the frozen Work Order"
                            .to_string(),
                    ));
                }
                let full_text = matching
                    .iter()
                    .map(|node| format!("[{}:{}]\n{}", node.role, node.node_ref, node.content))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let total_chars = full_text.chars().count();
                let content = full_text
                    .chars()
                    .skip(offset)
                    .take(limit)
                    .collect::<String>();
                Ok(json!({
                    "sessionRef": session_ref,
                    "questionRef": question_ref,
                    "offset": offset,
                    "totalChars": total_chars,
                    "hasMore": offset.saturating_add(content.chars().count()) < total_chars,
                    "content": content,
                }))
            }
            "read_content_node" => {
                let node_ref = required_argument(arguments, "node_ref")?;
                let node = self
                    .load_scoped_evidence_nodes(evidence)
                    .await?
                    .into_iter()
                    .find(|node| node.node_ref == node_ref)
                    .ok_or_else(|| {
                        AppError::Validation(
                            "Memory Generation node_ref is outside the frozen Work Order"
                                .to_string(),
                        )
                    })?;
                let total_chars = node.content.chars().count();
                Ok(json!({
                    "sessionRef": session_ref,
                    "nodeRef": node.node_ref,
                    "questionRef": node.question_ref,
                    "role": node.role,
                    "content": take_chars(&node.content, MAX_NODE_CHARS),
                    "truncated": total_chars > MAX_NODE_CHARS,
                    "totalChars": total_chars,
                }))
            }
            _ => unreachable!("allowlist was checked before dispatch"),
        }
    }

    async fn load_authorized_memory_generation_payload(
        &self,
        job_id: &str,
        ownership_token: &str,
    ) -> AppResult<RecentSnapshotWorkOrderPayload> {
        if job_id.trim().is_empty() || ownership_token.trim().is_empty() {
            return Err(AppError::Validation(
                "Memory Generation tool lease binding is missing".to_string(),
            ));
        }
        let job = store::load_recent_memory_job_sqlx(self.db.pool(), self.tenant_id(), job_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound("Memory Generation Work Order is unavailable".to_string())
            })?;
        if job.status != "running"
            || job.ownership_token.as_deref() != Some(ownership_token)
            || job.tenant_id != self.tenant_id()
        {
            return Err(AppError::Validation(
                "Memory Generation Work Order lease is no longer active".to_string(),
            ));
        }
        let envelope: Value = serde_json::from_str(&job.work_order_json)
            .map_err(|error| AppError::Validation(format!("Invalid Memory Work Order: {error}")))?;
        serde_json::from_value(
            envelope
                .get("payload")
                .cloned()
                .ok_or_else(|| AppError::Validation("Missing Memory Work Order payload".into()))?,
        )
        .map_err(|error| {
            AppError::Validation(format!("Invalid Memory Work Order payload: {error}"))
        })
    }

    async fn load_scoped_evidence_nodes(
        &self,
        evidence: &RecentSnapshotSessionEvidence,
    ) -> AppResult<Vec<ScopedEvidenceNode>> {
        let detail = store::load_conversation_session_detail_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &evidence.candidate.session_id,
        )
        .await?;
        if detail.session.source_id != evidence.candidate.source_id {
            return Err(AppError::Validation(
                "Memory Generation source binding changed".to_string(),
            ));
        }

        let mut question_refs = HashMap::new();
        for reference in &evidence.source_references {
            if let Some(question_id) = reference.question_id.as_ref() {
                let next = question_refs.len() + 1;
                question_refs
                    .entry(question_id.clone())
                    .or_insert_with(|| format!("q{next}"));
            }
        }

        let mut nodes = Vec::new();
        for (index, reference) in evidence.source_references.iter().enumerate() {
            if reference.session_id != evidence.candidate.session_id
                || reference.source_id != evidence.candidate.source_id
                || reference.source_revision != evidence.candidate.source_revision
            {
                continue;
            }
            let Some(question_id) = reference.question_id.as_deref() else {
                continue;
            };
            let Some(question_ref) = question_refs.get(question_id).cloned() else {
                continue;
            };
            let node_ref = super::recent_snapshot_pipeline::source_reference_alias(
                &evidence.candidate.session_ref,
                index,
            );
            if let Some(node) =
                resolve_source_reference(&detail, reference, &question_ref, &node_ref)
            {
                nodes.push(node);
            }
        }
        Ok(nodes)
    }
}

fn memory_generation_session_outline(evidence: &RecentSnapshotSessionEvidence) -> Value {
    let mut question_refs = HashMap::new();
    let nodes = evidence
        .source_references
        .iter()
        .enumerate()
        .filter_map(|(index, reference)| {
            let question_id = reference.question_id.as_ref()?;
            let next = question_refs.len() + 1;
            let question_ref = question_refs
                .entry(question_id.clone())
                .or_insert_with(|| format!("q{next}"))
                .clone();
            Some(json!({
                "questionRef": question_ref,
                "nodeRef": super::recent_snapshot_pipeline::source_reference_alias(
                    &evidence.candidate.session_ref,
                    index,
                ),
                "hasContent": true,
            }))
        })
        .collect::<Vec<_>>();
    json!({
        "candidate": evidence.candidate,
        "facts": {
            "summary": evidence.summary,
            "goal": evidence.goal,
            "result": evidence.result,
            "decisions": evidence.decisions,
            "verification": evidence.verification,
            "blockers": evidence.blockers,
            "followUp": evidence.follow_up,
            "topics": evidence.topics,
            "recentEvents": evidence.recent_events,
        },
        "nodes": nodes,
    })
}

fn resolve_source_reference(
    detail: &crate::backend::dto::ConversationSessionDetail,
    reference: &SessionMemorySourceReference,
    question_ref: &str,
    node_ref: &str,
) -> Option<ScopedEvidenceNode> {
    let question_id = reference.question_id.as_deref()?;
    let question = detail
        .questions
        .iter()
        .find(|question| question.question.id == question_id)?;
    let raw = if let Some(part_id) = reference.part_id.as_deref() {
        let node = question.projected_content_nodes.iter().find(|node| {
            node.part_id == part_id
                && reference
                    .node_id
                    .as_deref()
                    .is_none_or(|node_id| node.node_id == node_id)
                && reference
                    .node_order
                    .is_none_or(|order| node.node_order == order)
        })?;
        (node.role.as_str().to_string(), node.content.as_str())
    } else {
        let turn_id = reference.turn_id.as_deref()?;
        let turn = question.turns.iter().find(|turn| turn.id == turn_id)?;
        ("user".to_string(), turn.user_text.as_str())
    };
    let content = redact_memory_text(raw.1).text;
    if content.trim().is_empty() {
        return None;
    }
    Some(ScopedEvidenceNode {
        node_ref: node_ref.to_string(),
        question_ref: question_ref.to_string(),
        role: raw.0,
        content,
    })
}

fn required_argument<'a>(arguments: &'a Value, key: &str) -> AppResult<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 512)
        .ok_or_else(|| AppError::Validation(format!("Missing or invalid tool argument: {key}")))
}

fn take_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
#[path = "memory_generation_tools_tests.rs"]
mod tests;
