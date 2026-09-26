use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::memory::work_order::{
    RecentSnapshotWorkOrderEvidencePack, RecentSnapshotWorkOrderPayload,
    ALLOWED_MEMORY_GENERATION_TOOLS,
};
use crate::backend::domain::memory::SessionMemorySourceReference;
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
pub(crate) fn build_recent_generation_prompt(
    envelope: &serde_json::Value,
    payload: &RecentSnapshotWorkOrderPayload,
) -> AppResult<String> {
    let output_schema = schemars::schema_for!(MemoryGenerationResult);
    let evidence = model_visible_recent_snapshot_evidence(&payload.evidence)?;
    serde_json::to_string(&serde_json::json!({
        "contract": "memory.contract.v2",
        "instruction": "Return exactly one JSON object matching the supplied output_schema. Do not return Markdown, prose, or code fences. Consume the frozen evidence first. Never inspect the workspace or execute shell commands. coverage.coveredSessions and coverage.noMemorySessions must form a strict, disjoint partition of candidateSessions (every candidate sessionRef such as s1 must appear in exactly one of these two arrays, never in both, and without duplicates). Place sessions that produced memory items in coveredSessions, and sessions with no material changes in noMemorySessions. Every project.sourceSessions must use candidate.sessionRef values such as s1. Every item.sourceRefs entry must use only sourceReferences[].reference_key or MCP nodeRef values such as s1.r1; never emit internal IDs or session-memory-ref-* values. You must include an entry in projects for EVERY projectKey in candidateSessions; if a project has no new material changes, set noMaterialChange: true, items: [], and a brief summary. Each project must use a valid candidate projectKey; project.sourceSessions and item.sourceRefs must strictly belong to that exact projectKey (do not mix sessions across projects). Keep descriptions concise and focused on high-signal items: at most 3 to 5 most important items per project; keep summary and rationale concise (1-2 sentences) to ensure the JSON completes cleanly within token limits. For each project with actionable items, assign recommendationRank (1, 2, or 3) to the top next action items. For projectKey=unassigned, every promotionNomination must be none. All evidence required for this generation is fully provided in the frozen evidence pack; do NOT invoke any shell, workspace, filesystem, external tools, or MCP commands. Generate the JSON output directly.",
        "execution_policy": {
            "tool_mode": "allowlisted_read_only_mcp",
            "allowed_tools": ALLOWED_MEMORY_GENERATION_TOOLS,
            "forbidden_capabilities": ["shell", "filesystem", "network", "subagent", "global_tool_inventory", "database_write"],
        },
        "output_schema": output_schema,
        "skill": payload.skill_text,
        "work_order": envelope.get("workOrder"),
        "evidence": evidence,
    }))
    .map_err(AppError::external)
}

pub(crate) fn source_reference_alias(session_ref: &str, index: usize) -> String {
    format!("{session_ref}.r{}", index + 1)
}

pub(crate) fn model_visible_recent_snapshot_evidence(
    evidence: &RecentSnapshotWorkOrderEvidencePack,
) -> AppResult<serde_json::Value> {
    let mut value = serde_json::to_value(evidence).map_err(AppError::external)?;
    let Some(session_evidence) = value
        .get_mut("sessionEvidence")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(value);
    };

    let mut visible_aliases = HashMap::new();
    for session in session_evidence {
        let session_ref = session
            .get("candidate")
            .and_then(|candidate| candidate.get("sessionRef"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        if let Some(references) = session
            .get_mut("sourceReferences")
            .and_then(serde_json::Value::as_array_mut)
        {
            for (index, reference) in references.iter_mut().enumerate() {
                let alias = source_reference_alias(&session_ref, index);
                if let Some(object) = reference.as_object_mut() {
                    if let Some(id) = object.get("id").and_then(serde_json::Value::as_str) {
                        visible_aliases.insert(id.to_string(), alias.clone());
                    }
                    if let Some(key) = object
                        .get("reference_key")
                        .and_then(serde_json::Value::as_str)
                    {
                        visible_aliases.insert(key.to_string(), alias.clone());
                    }
                    object.retain(|key, _| {
                        matches!(key.as_str(), "reference_key" | "source_revision")
                    });
                    object.insert(
                        "reference_key".to_string(),
                        serde_json::Value::String(alias),
                    );
                }
            }
        }
        if let Some(events) = session
            .get_mut("recentEvents")
            .and_then(serde_json::Value::as_array_mut)
        {
            for event in events {
                if let Some(object) = event.as_object_mut() {
                    let alias = object
                        .get("source_reference_id")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|id| visible_aliases.get(id))
                        .cloned();
                    object.insert(
                        "source_reference_id".to_string(),
                        alias.map_or(serde_json::Value::Null, serde_json::Value::String),
                    );
                }
            }
        }
    }
    if let Some(items) = value
        .get_mut("continuableItems")
        .and_then(serde_json::Value::as_array_mut)
    {
        for item in items {
            if let Some(source_refs) = item
                .get_mut("sourceRefs")
                .and_then(serde_json::Value::as_array_mut)
            {
                *source_refs = source_refs
                    .iter()
                    .filter_map(|reference| {
                        let reference = reference.as_str()?;
                        visible_aliases
                            .get(reference)
                            .cloned()
                            .or_else(|| {
                                (!reference.starts_with("session-memory-ref-"))
                                    .then(|| reference.to_string())
                            })
                            .map(serde_json::Value::String)
                    })
                    .collect();
            }
        }
    }
    Ok(value)
}

pub(crate) fn extend_source_reference_aliases(
    ref_map: &mut HashMap<String, ResolvedEvidenceRef>,
    candidate: &CandidateSession,
    references: &[SessionMemorySourceReference],
) {
    for (index, reference) in references.iter().enumerate() {
        let alias = source_reference_alias(&candidate.short_ref, index);
        let resolved = ResolvedEvidenceRef {
            short_ref: alias.clone(),
            source_id: reference.source_id.clone(),
            session_id: reference.session_id.clone(),
            project_key: candidate.project_key.clone(),
            session_title: candidate.session_title.clone(),
            source_agent: candidate.source_agent.clone(),
            last_activity_at: candidate.last_activity_at.clone(),
            reference_key: reference.reference_key.clone(),
            source_revision: reference.source_revision,
            question_id: reference.question_id.clone(),
            turn_id: reference.turn_id.clone(),
            node_id: reference.node_id.clone(),
        };
        ref_map.insert(alias, resolved.clone());
        // Compatibility for immutable Work Orders queued before short source
        // references were enforced. New prompts never expose this key.
        ref_map
            .entry(reference.reference_key.clone())
            .or_insert(resolved);
    }
}

pub(crate) use crate::backend::application::memory::recent::recent_snapshot_normalization::normalize_agent_memory_generation_result;

pub(crate) fn memory_generation_tools_for_job(
    job: &store::RecentMemoryJob,
    database_path: &Path,
) -> AppResult<crate::backend::infrastructure::agent_execution::AiMemoryGenerationTools> {
    Ok(
        crate::backend::infrastructure::agent_execution::AiMemoryGenerationTools {
            tenant_id: job.tenant_id.clone(),
            job_id: job.id.clone(),
            ownership_token: job.ownership_token.clone().ok_or_else(|| {
                AppError::Validation(
                    "MEMORY_WORK_ORDER_INVALID: running job has no ownership token".to_string(),
                )
            })?,
            database_path: database_path.to_string_lossy().into_owned(),
        },
    )
}

pub(crate) fn has_complete_recent_snapshot_evidence(
    candidates: &[CandidateSession],
    context: &RecentSnapshotFrozenContext,
) -> bool {
    candidates.iter().all(|candidate| {
        context.session_memories.contains_key(&candidate.session_id)
            && context
                .source_references
                .get(&candidate.session_id)
                .is_some_and(|references| {
                    references.iter().any(|reference| {
                        reference.question_id.is_some()
                            || reference.turn_id.is_some()
                            || reference.part_id.is_some()
                            || reference.node_id.is_some()
                    })
                })
    })
}

pub(crate) fn parse_hh_mm(time_str: &str) -> AppResult<chrono::NaiveTime> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        return Err(AppError::Validation(format!(
            "Invalid time format '{}', expected HH:MM",
            time_str
        )));
    }
    let h: u32 = parts[0].parse().map_err(|_| {
        AppError::Validation(format!(
            "Invalid hour '{}' in time '{}'",
            parts[0], time_str
        ))
    })?;
    let m: u32 = parts[1].parse().map_err(|_| {
        AppError::Validation(format!(
            "Invalid minute '{}' in time '{}'",
            parts[1], time_str
        ))
    })?;
    if parts[0].len() != 2 || parts[1].len() != 2 || h >= 24 || m >= 60 {
        return Err(AppError::Validation(format!(
            "Invalid time '{}', hour must be 00-23 and minute 00-59",
            time_str
        )));
    }
    chrono::NaiveTime::from_hms_opt(h, m, 0)
        .ok_or_else(|| AppError::Validation(format!("Invalid time '{}'", time_str)))
}

pub(crate) fn resolve_local_time_with_dst<Tz: chrono::TimeZone>(
    tz: &Tz,
    naive_dt: chrono::NaiveDateTime,
) -> Option<DateTime<Tz>> {
    match tz.from_local_datetime(&naive_dt) {
        chrono::LocalResult::Single(dt) => Some(dt),
        chrono::LocalResult::Ambiguous(earliest, _latest) => {
            // M35 spec §2.2: 歧义时间选择第一次出现的 instant
            Some(earliest)
        }
        chrono::LocalResult::None => {
            // M35 spec §2.2: 不存在的本地墙上时间顺延到该日期第一个有效 instant
            let mut probe = naive_dt + chrono::Duration::minutes(1);
            let end_of_day = naive_dt.date().and_hms_opt(23, 59, 59)?;
            while probe <= end_of_day {
                match tz.from_local_datetime(&probe) {
                    chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
                        return Some(dt);
                    }
                    chrono::LocalResult::None => {
                        probe += chrono::Duration::minutes(1);
                    }
                }
            }
            None
        }
    }
}

pub(crate) fn resolve_target_watermark<Tz: chrono::TimeZone>(
    now: DateTime<Tz>,
    window_hours: u32,
    watermark_1: &str,
    watermark_2: &str,
) -> AppResult<WatermarkTarget> {
    if window_hours != 24 && window_hours != 48 && window_hours != 72 {
        return Err(AppError::Validation(format!(
            "MEMORY_SCHEDULE_INVALID: invalid window hours {}, allowed: 24, 48, 72",
            window_hours
        )));
    }
    if watermark_1 == watermark_2 {
        return Err(AppError::Validation(
            "MEMORY_SCHEDULE_INVALID: watermark times must be different".to_string(),
        ));
    }
    let t1 = parse_hh_mm(watermark_1)?;
    let t2 = parse_hh_mm(watermark_2)?;

    let tz = now.timezone();
    let today = now.date_naive();
    let yesterday = today - chrono::Duration::days(1);

    let naive_candidates = [
        yesterday.and_time(t1),
        yesterday.and_time(t2),
        today.and_time(t1),
        today.and_time(t2),
    ];

    let mut valid_candidates: Vec<DateTime<Tz>> = Vec::new();
    for ndt in naive_candidates {
        if let Some(dt) = resolve_local_time_with_dst(&tz, ndt) {
            if dt.with_timezone(&Utc) <= now.with_timezone(&Utc) {
                valid_candidates.push(dt);
            }
        }
    }

    valid_candidates.sort_by_key(|dt| dt.with_timezone(&Utc));

    let chosen_dt = valid_candidates.into_iter().last().ok_or_else(|| {
        AppError::Validation("No valid watermark candidate found <= current time".to_string())
    })?;

    let target_watermark_utc = chosen_dt.with_timezone(&Utc);
    let local_watermark_date = chosen_dt.naive_local().format("%Y-%m-%d").to_string();
    let local_watermark_time = chosen_dt.naive_local().format("%H:%M").to_string();
    let timezone_offset_minutes = chosen_dt.offset().fix().local_minus_utc() as i64 / 60;
    let window_end_utc = target_watermark_utc;
    let window_start_utc = target_watermark_utc - chrono::Duration::hours(window_hours as i64);

    Ok(WatermarkTarget {
        target_watermark_utc,
        local_watermark_date,
        local_watermark_time,
        timezone_offset_minutes,
        window_hours: window_hours as i64,
        window_start_utc,
        window_end_utc,
    })
}

pub(crate) fn compute_target_fingerprint(
    tenant_id: &str,
    target_watermark_utc: &DateTime<Utc>,
    window_hours: i64,
    candidates: &[CandidateSession],
    prior_active_items: &[(&str, &str)],
    skill_binding: &MemorySkillBinding,
) -> String {
    let mut sorted_candidates: Vec<TargetFingerprintSessionRef> = candidates
        .iter()
        .map(|c| TargetFingerprintSessionRef {
            session_id: &c.session_id,
            source_revision: c.source_revision,
        })
        .collect();
    sorted_candidates.sort_by_key(|c| c.session_id);

    let mut sorted_items: Vec<TargetFingerprintItemRef> = prior_active_items
        .iter()
        .map(|(item_id, rev_id)| TargetFingerprintItemRef {
            item_id,
            revision_id: rev_id,
        })
        .collect();
    sorted_items.sort_by_key(|i| i.item_id);

    let target_watermark_str = target_watermark_utc.to_rfc3339();

    let payload = TargetFingerprintPayload {
        budget_policy_version: "budget.v1",
        candidate_sessions: sorted_candidates,
        contract_version: "memory.contract.v2",
        prior_active_items: sorted_items,
        projection_policy_version: "projection.v2",
        skill_asset_id: Some(skill_binding.asset_id.as_str()),
        skill_content_hash: Some(skill_binding.content_hash.as_str()),
        skill_revision: skill_binding.asset_revision,
        target_watermark_utc: &target_watermark_str,
        tenant_id,
        window_hours,
    };

    let canonical_json = serde_json::to_string(&payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical_json.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn compute_content_fingerprint(
    window_hours: i64,
    candidates: &[CandidateSession],
    carry_over_items: &[(&str, &str, i64)],
    current_l2_l3_revisions: &[&str],
    skill_binding: &MemorySkillBinding,
    excluded_session_ids: &[String],
    excluded_source_ids: &[String],
) -> String {
    let mut sorted_candidates: Vec<ContentFingerprintSessionRef> = candidates
        .iter()
        .map(|c| ContentFingerprintSessionRef {
            session_id: &c.session_id,
            last_activity_at: &c.last_activity_at,
            source_revision: c.source_revision,
        })
        .collect();
    sorted_candidates.sort_by_key(|c| c.session_id);

    let mut sorted_items: Vec<ContentFingerprintItemRef> = carry_over_items
        .iter()
        .map(|(item_id, rev_id, bucket)| ContentFingerprintItemRef {
            item_id,
            revision_id: rev_id,
            remaining_lifetime_bucket: *bucket,
        })
        .collect();
    sorted_items.sort_by_key(|i| i.item_id);

    let mut sorted_l2_l3 = current_l2_l3_revisions.to_vec();
    sorted_l2_l3.sort();

    let payload = ContentFingerprintPayload {
        candidate_sessions: sorted_candidates,
        carry_over_items: sorted_items,
        contract_version: "memory.contract.v2",
        current_l2_l3_revisions: sorted_l2_l3,
        excluded_session_ids,
        excluded_source_ids,
        skill_asset_id: Some(skill_binding.asset_id.as_str()),
        skill_content_hash: Some(skill_binding.content_hash.as_str()),
        skill_revision: skill_binding.asset_revision,
        window_hours,
    };

    let canonical_json = serde_json::to_string(&payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical_json.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn compute_recent_snapshot_evidence_fingerprint(
    base_content_fingerprint: &str,
    evidence: &RecentSnapshotWorkOrderEvidencePack,
) -> String {
    let canonical_json = serde_json::to_string(&(
        base_content_fingerprint,
        &evidence.session_evidence,
        &evidence.current_l2_projects,
        &evidence.current_l3_items,
    ))
    .unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical_json.as_bytes());
    format!("{:x}", hasher.finalize())
}
