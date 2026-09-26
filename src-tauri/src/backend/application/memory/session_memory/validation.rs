use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::memory::evidence::BoundedEvidenceNode;
use crate::backend::domain::memory::{RecentMemoryEventCategory, SessionMemoryJob};
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
pub(crate) fn sanitize_decisions_with_corrections(
    decisions: Vec<String>,
    corrections: &[BoundedEvidenceNode],
) -> Vec<String> {
    if corrections.is_empty() {
        return decisions;
    }
    const NEGATION_PREFIXES: &[&str] = &[
        "don't use",
        "do not use",
        "dont use",
        "never use",
        "stop using",
        "don't",
        "do not",
        "不要用",
        "不要使用",
        "不用",
        "不要",
        "并非",
    ];

    decisions
        .into_iter()
        .filter(|d| {
            let d_lower = d.to_lowercase();
            !corrections.iter().any(|c| {
                let c_lower = c.text.to_lowercase();
                NEGATION_PREFIXES.iter().any(|prefix| {
                    if let Some(pos) = c_lower.find(prefix) {
                        let negated_part = &c_lower[pos + prefix.len()..];
                        d_lower.split(|ch: char| !ch.is_alphanumeric()).any(|word| {
                            word.len() >= 3 && negated_part.trim_start().starts_with(word)
                        })
                    } else {
                        false
                    }
                })
            })
        })
        .collect()
}

pub(crate) fn sanitize_verifications_with_evidence(
    verifications: Vec<String>,
    has_verification_evidence: bool,
) -> Vec<String> {
    if has_verification_evidence {
        verifications
    } else {
        verifications
            .into_iter()
            .filter(|v| {
                let v_lower = v.to_lowercase();
                let is_pass_claim = v_lower.contains("pass")
                    || v_lower.contains("通过")
                    || v_lower.contains("success")
                    || v_lower.contains("verified");
                !is_pass_claim
            })
            .collect()
    }
}

pub(crate) fn validated_persist_input(
    job: &SessionMemoryJob,
    output: &SessionMemoryAgentOutput,
    evidence: &[EvidenceReference],
    project_path: Option<String>,
    generated_at: &str,
) -> AppResult<SessionMemoryPersistInput> {
    let evidence_by_key = evidence
        .iter()
        .map(|item| (item.key.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut references = Vec::new();
    let mut seen_references = BTreeSet::new();
    let requested_reference_keys = output
        .source_references
        .iter()
        .take(MAX_OUTPUT_ITEMS)
        .map(|reference| reference.reference_key.as_str())
        .chain(
            output
                .events
                .iter()
                .take(MAX_OUTPUT_ITEMS)
                .filter_map(|event| event.source_reference.as_deref()),
        );
    for requested_key in requested_reference_keys {
        let key = requested_key.trim();
        let Some(evidence) = evidence_by_key.get(key) else {
            continue;
        };
        if !seen_references.insert(key.to_string()) {
            continue;
        }
        references.push(SessionMemoryReferenceInput {
            source_id: job.source_id.clone(),
            session_id: job.session_id.clone(),
            question_id: Some(evidence.locator.question_id.clone()),
            turn_id: Some(evidence.locator.turn_id.clone()),
            part_id: (!evidence.locator.part_id.is_empty())
                .then(|| evidence.locator.part_id.clone()),
            node_id: evidence.node_id.clone(),
            node_order: Some(evidence.locator.node_order),
            reference_key: key.to_string(),
            source_revision: job.source_revision,
        });
    }
    let is_empty_session = output.source_references.is_empty()
        && output
            .events
            .iter()
            .all(|event| event.source_reference.as_deref().is_none_or(str::is_empty))
        && (output.summary.is_empty()
            || output.summary == "No content available in this session."
            || evidence.is_empty());

    if !is_empty_session && references.is_empty() {
        return Err(AppError::Validation(
            "Session Memory must cite at least one source reference".to_string(),
        ));
    }

    let memory_id = session_memory_id(job);
    let reference_ids = references
        .iter()
        .map(|reference| {
            (
                reference.reference_key.clone(),
                session_memory_reference_id(&memory_id, &reference.reference_key),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut events = Vec::new();
    let mut seen_events = BTreeSet::new();
    for event in output.events.iter().take(MAX_OUTPUT_ITEMS) {
        let category = RecentMemoryEventCategory::parse(&event.category).ok_or_else(|| {
            AppError::Validation(
                "Session Memory contains an invalid Recent Event category".to_string(),
            )
        })?;
        let title = clean_output_text(&event.title, 500, "Recent Event title")?;
        let summary = clean_output_text(&event.summary, MAX_ITEM_LENGTH, "Recent Event summary")?;
        let source_reference_id = event
            .source_reference
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|reference_key| {
                if reference_ids.contains_key(reference_key) {
                    Some(reference_key.to_string())
                } else {
                    reference_ids.keys().next().cloned()
                }
            });
        let fingerprint = event
            .fingerprint
            .as_deref()
            .map(|value| crate::backend::domain::memory::evidence::redact_memory_text(value).text)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                digest(&format!(
                    "{}\0{}\0{}\0{:?}",
                    category.as_str(),
                    title,
                    summary,
                    source_reference_id
                ))
            });
        if !seen_events.insert(fingerprint.clone()) {
            continue;
        }
        events.push(RecentMemoryEventInput {
            category,
            title,
            summary,
            occurred_at: event
                .occurred_at
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| generated_at.to_string()),
            source_reference_id: source_reference_id
                .as_deref()
                .and_then(|key| reference_ids.get(key).cloned()),
            fingerprint,
        });
    }

    let summary = clean_output_text(&output.summary, 12000, "Session Memory summary")?;
    if summary.is_empty() {
        return Err(AppError::Validation(
            "Session Memory summary is empty".to_string(),
        ));
    }
    let goal = clean_output_text(&output.goal, 12000, "Session Memory goal")?;
    let result = clean_output_text(&output.result, 12000, "Session Memory result")?;
    let decisions_json = encode_output_list(&output.decisions)?;
    let verification_json = encode_output_list(&output.verification)?;
    let blockers_json = encode_output_list(&output.blockers)?;
    let follow_up_json = encode_output_list(&output.follow_up)?;
    let topics_json = encode_output_list(&output.topics)?;
    let raw_output_json = serde_json::to_string(&json!({
        "summary": summary,
        "goal": goal,
        "result": result,
        "decisions": serde_json::from_str::<Value>(&decisions_json).map_err(AppError::external)?,
        "verification": serde_json::from_str::<Value>(&verification_json).map_err(AppError::external)?,
        "blockers": serde_json::from_str::<Value>(&blockers_json).map_err(AppError::external)?,
        "follow_up": serde_json::from_str::<Value>(&follow_up_json).map_err(AppError::external)?,
        "topics": serde_json::from_str::<Value>(&topics_json).map_err(AppError::external)?,
        "source_references": references.iter().map(|reference| &reference.reference_key).collect::<Vec<_>>(),
        "events": events.iter().map(|event| json!({
            "category": event.category.as_str(),
            "title": event.title,
            "summary": event.summary,
            "occurred_at": event.occurred_at,
            "source_reference": event.source_reference_id,
            "fingerprint": event.fingerprint,
        })).collect::<Vec<_>>(),
    }))
    .map_err(AppError::external)?;
    Ok(SessionMemoryPersistInput {
        memory_id,
        tenant_id: job.tenant_id.clone(),
        session_id: job.session_id.clone(),
        source_id: job.source_id.clone(),
        source_revision: job.source_revision,
        source_fingerprint: job.source_fingerprint.clone(),
        contract_version: job.contract_version.clone(),
        prompt_version: job.prompt_version.clone(),
        project_path,
        summary,
        goal,
        result,
        decisions_json,
        verification_json,
        blockers_json,
        follow_up_json,
        topics_json,
        raw_output_json,
        generated_at: generated_at.to_string(),
        ownership_token: job.ownership_token.clone().ok_or_else(|| {
            AppError::Conflict("Session Memory job has no ownership token".to_string())
        })?,
        references,
        events,
        recipe_id: job.recipe_id.clone(),
        recipe_content_hash: job.recipe_content_hash.clone(),
        work_order_json: job.work_order_json.clone(),
    })
}

pub(crate) fn session_memory_id(job: &SessionMemoryJob) -> String {
    format!(
        "session-memory-{}",
        digest(&format!(
            "{}\0{}\0{}",
            job.tenant_id, job.id, job.source_revision
        ))
    )
}

pub(crate) fn session_memory_reference_id(memory_id: &str, reference_key: &str) -> String {
    format!(
        "session-memory-ref-{}",
        digest(&format!("{memory_id}\0{reference_key}"))
    )
}

pub(crate) fn encode_output_list(values: &[String]) -> AppResult<String> {
    let values = values
        .iter()
        .take(MAX_OUTPUT_ITEMS)
        .map(|value| clean_output_text(value, MAX_ITEM_LENGTH, "Session Memory list item"))
        .collect::<AppResult<Vec<_>>>()?
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    serde_json::to_string(&values).map_err(AppError::external)
}

pub(crate) fn clean_output_text(value: &str, max_length: usize, field: &str) -> AppResult<String> {
    let value = crate::backend::domain::memory::evidence::redact_memory_text(value).text;
    let value = value.trim();
    if value.chars().count() > max_length {
        return Err(AppError::Validation(format!("{field} is too long")));
    }
    Ok(value.to_string())
}

pub(crate) fn session_has_completion_signal(detail: &ConversationSessionDetail) -> bool {
    detail
        .questions
        .iter()
        .flat_map(|question| question.parts.iter())
        .filter_map(|part| part.metadata_json.as_deref())
        .filter_map(|metadata| serde_json::from_str::<Value>(metadata).ok())
        .any(|metadata| value_marks_completion(&metadata))
}

pub(crate) fn session_project_path(
    detail: &ConversationSessionDetail,
    registered_roots: &[String],
) -> Option<String> {
    detail
        .session
        .project_path
        .as_deref()
        .or_else(|| {
            detail
                .questions
                .iter()
                .flat_map(|question| question.parts.iter())
                .filter_map(|part| part.cwd.as_deref())
                .find(|path| !path.trim().is_empty())
        })
        .and_then(|path| {
            crate::backend::application::memory::recent::recent::resolve_project_directory(
                path,
                registered_roots,
            )
        })
}

pub(crate) fn value_marks_completion(value: &Value) -> bool {
    match value {
        Value::Object(values) => values.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            if key == "completed" && value.as_bool() == Some(true) {
                return true;
            }
            if matches!(key.as_str(), "session_status" | "completion_status")
                && value.as_str().is_some_and(is_completion_word)
            {
                return true;
            }
            value_marks_completion(value)
        }),
        Value::Array(values) => values.iter().any(value_marks_completion),
        _ => false,
    }
}

pub(crate) fn is_completion_word(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "complete" | "completed" | "done" | "success" | "succeeded"
    )
}

pub(crate) fn session_idle_ready(detail: &ConversationSessionDetail, now: DateTime<Utc>) -> bool {
    detail
        .session
        .updated_at
        .as_deref()
        .and_then(crate::backend::domain::parse_conversation_timestamp)
        .is_some_and(|updated| now >= updated + Duration::minutes(30))
}
