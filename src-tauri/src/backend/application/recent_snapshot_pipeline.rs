use super::prelude::*;
use super::recent::resolve_project_directory;
use crate::backend::{
    ai_execution::{
        execute_agent, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest,
    },
    dto::RecentMemorySnapshotView,
    models::{
        CandidateSession, CandidateSessionSummary, ContinuableMemoryItemView, L2ProjectMemoryView,
        L3MemoryItemView, MemoryGenerationResultV2, MemoryItemCategory, MemoryItemStatus,
        MemoryJobPurpose, MemoryPromotionNomination, MemoryScopeV2, MemorySkillBinding,
        MemoryWindowV2, MemoryWorkOrderV2, RecentSnapshotSessionEvidence,
        RecentSnapshotWorkOrderEvidencePack, RecentSnapshotWorkOrderPayload, ResolvedEvidenceRef,
        SessionMemory, SessionMemorySourceReference, ALLOWED_MEMORY_GENERATION_TOOLS,
    },
    runtime::{AppError, AppResult},
    store,
};
use chrono::{DateTime, Duration, Offset, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub(crate) struct WatermarkTarget {
    pub(crate) target_watermark_utc: DateTime<Utc>,
    pub(crate) local_watermark_date: String,
    pub(crate) local_watermark_time: String,
    pub(crate) timezone_offset_minutes: i64,
    pub(crate) window_hours: i64,
    pub(crate) window_start_utc: DateTime<Utc>,
    pub(crate) window_end_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct RecentSnapshotPreparation {
    pub(crate) target: WatermarkTarget,
    pub(crate) skill_binding: MemorySkillBinding,
    pub(crate) target_fingerprint: String,
    pub(crate) content_fingerprint: String,
    pub(crate) evidence: RecentSnapshotWorkOrderEvidencePack,
    pub(crate) skill_text: String,
}

fn build_recent_generation_prompt(
    envelope: &serde_json::Value,
    payload: &RecentSnapshotWorkOrderPayload,
) -> AppResult<String> {
    let output_schema = schemars::schema_for!(MemoryGenerationResultV2);
    let evidence = model_visible_recent_snapshot_evidence(&payload.evidence)?;
    serde_json::to_string(&serde_json::json!({
        "contract": "memory.contract.v2",
        "instruction": "Return exactly one JSON object matching the supplied output_schema. Do not return Markdown, prose, or code fences. Consume the frozen evidence first. Never inspect the workspace or execute shell commands. coverage.coveredSessions, coverage.noMemorySessions, and every project.sourceSessions must use candidate.sessionRef values such as s1. Every item.sourceRefs entry must use only sourceReferences[].reference_key or MCP nodeRef values such as s1.r1; never emit internal IDs or session-memory-ref-* values. Each project must use a valid candidate projectKey; project.sourceSessions and item.sourceRefs must strictly belong to that exact projectKey (do not mix sessions across projects). Keep descriptions concise and focused on high-signal items: at most 3 to 5 most important items per project; keep summary and rationale concise (1-2 sentences) to ensure the JSON completes cleanly within token limits. For each project with actionable items, assign recommendationRank (1, 2, or 3) to the top next action items. For projectKey=unassigned, every promotionNomination must be none. All evidence required for this generation is fully provided in the frozen evidence pack; do NOT invoke any shell, workspace, filesystem, external tools, or MCP commands. Generate the JSON output directly.",
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

pub(super) fn source_reference_alias(session_ref: &str, index: usize) -> String {
    format!("{session_ref}.r{}", index + 1)
}

fn model_visible_recent_snapshot_evidence(
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

fn extend_source_reference_aliases(
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

fn normalize_agent_memory_generation_result(
    result: &mut MemoryGenerationResultV2,
    candidates: &[CandidateSession],
    ref_map: &HashMap<String, ResolvedEvidenceRef>,
) {
    let candidate_by_ref = candidates
        .iter()
        .map(|c| (c.short_ref.as_str(), c))
        .collect::<HashMap<_, _>>();
    let candidate_project_keys = candidates
        .iter()
        .map(|c| c.project_key.as_str())
        .collect::<HashSet<_>>();

    let mut seen_keys = HashSet::new();
    result.projects.retain_mut(|project| {
        let key = project.project_key.trim();
        if key.is_empty() {
            return false;
        }
        if !candidates.is_empty() && key != "unassigned" && !candidate_project_keys.contains(key) {
            return false;
        }
        seen_keys.insert(key.to_string())
    });

    for project in &mut result.projects {
        let project_key = project.project_key.trim().to_string();
        project.project_key = project_key.clone();

        // 仅保留属于当前 project 的 session_ref
        project.source_sessions.retain(|session_ref| {
            candidate_by_ref
                .get(session_ref.as_str())
                .map_or(false, |c| c.project_key == project_key)
        });

        if project.source_sessions.is_empty() {
            for c in candidates {
                if c.project_key == project_key {
                    project.source_sessions.push(c.short_ref.clone());
                }
            }
        }

        let project_available_refs = ref_map
            .iter()
            .filter(|(_, res)| res.project_key == project_key)
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();

        let mut rank_set = HashSet::new();
        for item in &mut project.items {
            item.source_refs.retain(|ref_key| {
                ref_map
                    .get(ref_key.as_str())
                    .map_or(false, |res| res.project_key == project_key)
            });

            if item.source_refs.is_empty() && !project_available_refs.is_empty() {
                item.source_refs.push(project_available_refs[0].clone());
            }

            if let Some(rank) = item.recommendation_rank {
                if (1..=3).contains(&rank) && !item.source_refs.is_empty() && rank_set.insert(rank) {
                    // 保留有效建议
                } else {
                    item.recommendation_rank = None;
                }
            }

            if project_key == "unassigned" || item.source_refs.is_empty() {
                item.promotion_nomination = MemoryPromotionNomination::None;
            }
        }

        if rank_set.len() < 3 && !project_available_refs.is_empty() {
            let mut available_ranks = Vec::new();
            for r in (1..=3).rev() {
                if !rank_set.contains(&r) {
                    available_ranks.push(r);
                }
            }
            for item in &mut project.items {
                if item.recommendation_rank.is_some() {
                    continue;
                }
                let is_actionable = matches!(
                    item.category,
                    MemoryItemCategory::FollowUp | MemoryItemCategory::Blocker
                );
                if is_actionable {
                    if let Some(next_rank) = available_ranks.pop() {
                        item.recommendation_rank = Some(next_rank);
                        rank_set.insert(next_rank);
                        if item.source_refs.is_empty() {
                            item.source_refs.push(project_available_refs[0].clone());
                        }
                    }
                }
                if available_ranks.is_empty() {
                    break;
                }
            }
        }
    }
}


fn memory_generation_tools_for_job(
    job: &store::RecentMemoryJob,
    database_path: &Path,
) -> AppResult<crate::backend::ai_execution::AiMemoryGenerationTools> {
    Ok(crate::backend::ai_execution::AiMemoryGenerationTools {
        tenant_id: job.tenant_id.clone(),
        job_id: job.id.clone(),
        ownership_token: job.ownership_token.clone().ok_or_else(|| {
            AppError::Validation(
                "MEMORY_WORK_ORDER_INVALID: running job has no ownership token".to_string(),
            )
        })?,
        database_path: database_path.to_string_lossy().into_owned(),
    })
}

#[derive(Debug, Default)]
struct RecentSnapshotFrozenContext {
    session_memories: BTreeMap<String, SessionMemory>,
    source_references: BTreeMap<String, Vec<SessionMemorySourceReference>>,
    recent_events: BTreeMap<String, Vec<crate::backend::models::RecentMemoryEvent>>,
    current_l2_projects: Vec<L2ProjectMemoryView>,
    current_l3_items: Vec<L3MemoryItemView>,
}

fn has_complete_recent_snapshot_evidence(
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

fn resolve_local_time_with_dst<Tz: chrono::TimeZone>(
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

#[derive(Debug, Serialize)]
struct TargetFingerprintSessionRef<'a> {
    session_id: &'a str,
    source_revision: i64,
}

#[derive(Debug, Serialize)]
struct TargetFingerprintItemRef<'a> {
    item_id: &'a str,
    revision_id: &'a str,
}

#[derive(Debug, Serialize)]
struct TargetFingerprintPayload<'a> {
    budget_policy_version: &'static str,
    candidate_sessions: Vec<TargetFingerprintSessionRef<'a>>,
    contract_version: &'static str,
    prior_active_items: Vec<TargetFingerprintItemRef<'a>>,
    projection_policy_version: &'static str,
    skill_asset_id: Option<&'a str>,
    skill_content_hash: Option<&'a str>,
    skill_revision: i64,
    target_watermark_utc: &'a str,
    tenant_id: &'a str,
    window_hours: i64,
}

#[derive(Debug, Serialize)]
struct ContentFingerprintSessionRef<'a> {
    last_activity_at: &'a str,
    session_id: &'a str,
    source_revision: i64,
}

#[derive(Debug, Serialize)]
struct ContentFingerprintItemRef<'a> {
    item_id: &'a str,
    remaining_lifetime_bucket: i64,
    revision_id: &'a str,
}

#[derive(Debug, Serialize)]
struct ContentFingerprintPayload<'a> {
    candidate_sessions: Vec<ContentFingerprintSessionRef<'a>>,
    carry_over_items: Vec<ContentFingerprintItemRef<'a>>,
    contract_version: &'static str,
    current_l2_l3_revisions: Vec<&'a str>,
    excluded_session_ids: &'a [String],
    excluded_source_ids: &'a [String],
    skill_asset_id: Option<&'a str>,
    skill_content_hash: Option<&'a str>,
    skill_revision: i64,
    window_hours: i64,
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

fn compute_recent_snapshot_evidence_fingerprint(
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

impl AppService {
    async fn load_current_long_term_revision_ids(&self) -> AppResult<Vec<String>> {
        sqlx::query_scalar(
            "SELECT current_revision_id FROM memory_items
             WHERE tenant_id = ?1 AND layer IN ('l2', 'l3')
               AND lifecycle = 'current' AND current_revision_id IS NOT NULL
             ORDER BY layer ASC, project_key ASC, id ASC",
        )
        .bind(self.tenant_id())
        .fetch_all(self.db.pool())
        .await
        .map_err(AppError::external)
    }

    /// Ensures that every Session candidate in the exact frozen Recent
    /// watermark window has Phase-1 work. The candidate set must come from
    /// the same watermark calculation as Phase 2; deriving a second rolling
    /// `now - 48h` window can silently omit sessions near the lower bound.
    pub(crate) async fn ensure_recent_snapshot_phase1_jobs_at<Tz: chrono::TimeZone>(
        &self,
        now: DateTime<Tz>,
        restart_terminal: bool,
    ) -> AppResult<Option<(WatermarkTarget, usize)>> {
        let memory_settings = self.backend_settings()?.memory.clone();
        if !memory_settings.generation_enabled {
            return Ok(None);
        }

        let now_text = now.with_timezone(&Utc).to_rfc3339();
        let target = resolve_target_watermark(
            now,
            memory_settings.recent_window_hours,
            &memory_settings.watermark_time_1,
            &memory_settings.watermark_time_2,
        )?;
        let (candidates, _) = self
            .collect_recent_snapshot_candidates(target.target_watermark_utc, target.window_hours)
            .await?;
        let session_ids = candidates
            .into_iter()
            .map(|candidate| candidate.session_id)
            .collect::<Vec<_>>();
        let internal_agent_workspace =
            crate::backend::ai_execution::agent_execution_workspace_root(&self.db_path);
        let prepared = store::ensure_session_memory_jobs_for_sessions_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &session_ids,
            &internal_agent_workspace,
            &now_text,
            restart_terminal,
        )
        .await?;

        Ok(Some((target, prepared)))
    }

    /// 根据目标水位与窗口小时数收集候选 Session 及对应短引用映射表 (M35-L1-03/04)
    pub(crate) async fn collect_recent_snapshot_candidates(
        &self,
        target_watermark_utc: DateTime<Utc>,
        window_hours: i64,
    ) -> AppResult<(Vec<CandidateSession>, HashMap<String, ResolvedEvidenceRef>)> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let settings = self.app_settings_value();

        let cutoff = target_watermark_utc - Duration::hours(window_hours);
        let cutoff_str = cutoff.to_rfc3339();
        let watermark_str = target_watermark_utc.to_rfc3339();

        let internal_agent_workspace =
            crate::backend::ai_execution::agent_execution_workspace_root(&self.db_path);

        let records = store::list_recent_conversation_sessions_sqlx(
            pool,
            tenant_id,
            &cutoff_str,
            &watermark_str,
            &internal_agent_workspace,
        )
        .await?;

        let registered_roots = store::load_sources_sqlx(pool, tenant_id)
            .await?
            .into_iter()
            .filter_map(|source| source.repo_root)
            .collect::<Vec<_>>();

        let memory_settings = settings
            .get("memory")
            .and_then(|v| {
                serde_json::from_value::<crate::backend::app_settings::MemorySettings>(v.clone())
                    .ok()
            })
            .unwrap_or_default();

        let excluded_sessions = memory_settings
            .excluded_session_ids
            .into_iter()
            .collect::<HashSet<_>>();
        let excluded_sources = memory_settings
            .excluded_source_ids
            .into_iter()
            .collect::<HashSet<_>>();

        let mut candidates = Vec::new();
        for record in records {
            let session_id = &record.session.session.id;
            let source_id = &record.session.session.source_id;

            if excluded_sessions.contains(session_id) || excluded_sources.contains(source_id) {
                continue;
            }

            let last_activity_at = match crate::backend::models::parse_conversation_timestamp(
                &record.last_activity_at,
            ) {
                Some(dt) => dt,
                None => continue,
            };

            // M35-L1-03: 左闭右闭 window_start_utc <= last_activity_at <= window_end_utc
            if last_activity_at < cutoff || last_activity_at > target_watermark_utc {
                continue;
            }

            let raw_project_path = record.cwd.as_deref().or(record
                .session
                .session
                .project_path
                .as_deref());

            let project_path = raw_project_path
                .and_then(|path| resolve_project_directory(path, &registered_roots));
            let project_key = project_path
                .clone()
                .unwrap_or_else(|| "unassigned".to_string());

            candidates.push(CandidateSession {
                tenant_id: tenant_id.to_string(),
                session_id: session_id.clone(),
                source_id: source_id.clone(),
                session_title: record.session.session.title.clone(),
                source_agent: record.source_agent.clone(),
                project_key,
                project_path,
                last_activity_at: last_activity_at.to_rfc3339(),
                source_revision: 1,
                short_ref: String::new(),
            });
        }

        // 稳定排序: project_key -> last_activity_at 降序 -> session_id 升序
        candidates.sort_by(|left, right| {
            left.project_key
                .cmp(&right.project_key)
                .then_with(|| right.last_activity_at.cmp(&left.last_activity_at))
                .then_with(|| left.session_id.cmp(&right.session_id))
        });

        let session_ids = candidates
            .iter()
            .map(|candidate| candidate.session_id.clone())
            .collect::<Vec<_>>();
        let source_references = store::list_session_memory_source_references_for_sessions_sqlx(
            pool,
            tenant_id,
            &session_ids,
        )
        .await?;

        let mut ref_map = HashMap::new();
        for (idx, candidate) in candidates.iter_mut().enumerate() {
            let short_ref = format!("s{}", idx + 1);
            candidate.short_ref = short_ref.clone();

            let canonical_reference = source_references
                .get(&candidate.session_id)
                .and_then(|references| references.first());
            if let Some(reference) = canonical_reference {
                candidate.source_revision = reference.source_revision;
            }

            ref_map.insert(
                short_ref.clone(),
                ResolvedEvidenceRef {
                    short_ref,
                    source_id: canonical_reference
                        .map(|reference| reference.source_id.clone())
                        .unwrap_or_else(|| candidate.source_id.clone()),
                    session_id: candidate.session_id.clone(),
                    project_key: candidate.project_key.clone(),
                    session_title: candidate.session_title.clone(),
                    source_agent: candidate.source_agent.clone(),
                    last_activity_at: candidate.last_activity_at.clone(),
                    reference_key: canonical_reference
                        .map(|reference| reference.reference_key.clone())
                        .unwrap_or_else(|| {
                            format!("{}/{}", candidate.source_id, candidate.session_id)
                        }),
                    source_revision: candidate.source_revision,
                    question_id: canonical_reference
                        .and_then(|reference| reference.question_id.clone()),
                    turn_id: canonical_reference.and_then(|reference| reference.turn_id.clone()),
                    node_id: canonical_reference.and_then(|reference| reference.node_id.clone()),
                },
            );
            if let Some(references) = source_references.get(&candidate.session_id) {
                extend_source_reference_aliases(&mut ref_map, candidate, references);
            }
        }

        Ok((candidates, ref_map))
    }

    /// M35-L3-04: 同步来源与会话有效性，并将来源已失效的未晋升 L1 条目退出 (lifecycle = 'retired')
    /// 注意：已晋升到 L2/L3 的条目保持知识不删，仅引用标记为 unavailable
    pub(crate) async fn sync_source_availability_and_retire_unpromoted_items(
        &self,
        now_str: &str,
    ) -> AppResult<()> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        // 1. conversation_source 被禁用 (enabled = 0)
        sqlx::query(
            "UPDATE memory_item_source_references \
             SET availability = 'unavailable', \
                 unavailable_reason = 'source_disabled', \
                 unavailable_at = COALESCE(unavailable_at, ?2) \
             WHERE tenant_id = ?1 \
               AND availability = 'available' \
               AND source_id IN (SELECT id FROM conversation_sources WHERE tenant_id = ?1 AND enabled = 0)",
        )
        .bind(tenant_id)
        .bind(now_str)
        .execute(pool)
        .await
        .map_err(AppError::external)?;

        // 2. conversation_session 缺失 (missing = 1)
        sqlx::query(
            "UPDATE memory_item_source_references \
             SET availability = 'unavailable', \
                 unavailable_reason = 'missing', \
                 unavailable_at = COALESCE(unavailable_at, ?2) \
             WHERE tenant_id = ?1 \
               AND availability = 'available' \
               AND session_id IN (SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND missing = 1)",
        )
        .bind(tenant_id)
        .bind(now_str)
        .execute(pool)
        .await
        .map_err(AppError::external)?;

        // 3. conversation_source 或 conversation_session 被删除
        sqlx::query(
            "UPDATE memory_item_source_references \
             SET availability = 'unavailable', \
                 unavailable_reason = 'deleted', \
                 unavailable_at = COALESCE(unavailable_at, ?2) \
             WHERE tenant_id = ?1 \
               AND availability = 'available' \
               AND ( \
                   source_id NOT IN (SELECT id FROM conversation_sources WHERE tenant_id = ?1) \
                   OR session_id NOT IN (SELECT id FROM conversation_sessions WHERE tenant_id = ?1) \
               )",
        )
        .bind(tenant_id)
        .bind(now_str)
        .execute(pool)
        .await
        .map_err(AppError::external)?;

        // 4. 设置中排除的 source 或 session
        let settings = self.app_settings_value();
        let memory_settings = settings
            .get("memory")
            .and_then(|v| {
                serde_json::from_value::<crate::backend::app_settings::MemorySettings>(v.clone())
                    .ok()
            })
            .unwrap_or_default();

        for excluded_source in &memory_settings.excluded_source_ids {
            sqlx::query(
                "UPDATE memory_item_source_references \
                 SET availability = 'unavailable', \
                     unavailable_reason = 'excluded', \
                     unavailable_at = COALESCE(unavailable_at, ?2) \
                 WHERE tenant_id = ?1 \
                   AND availability = 'available' \
                   AND source_id = ?3",
            )
            .bind(tenant_id)
            .bind(now_str)
            .bind(excluded_source)
            .execute(pool)
            .await
            .map_err(AppError::external)?;
        }

        for excluded_session in &memory_settings.excluded_session_ids {
            sqlx::query(
                "UPDATE memory_item_source_references \
                 SET availability = 'unavailable', \
                     unavailable_reason = 'excluded', \
                     unavailable_at = COALESCE(unavailable_at, ?2) \
                 WHERE tenant_id = ?1 \
                   AND availability = 'available' \
                   AND session_id = ?3",
            )
            .bind(tenant_id)
            .bind(now_str)
            .bind(excluded_session)
            .execute(pool)
            .await
            .map_err(AppError::external)?;
        }

        // 5. M35-L3-04: 来源失效使未晋升 L1 退出 (layer = 'l1' AND lifecycle = 'current')
        // 已晋升的 L2/L3 保持不变，仅引用标记 unavailable
        sqlx::query(
            "UPDATE memory_items \
             SET lifecycle = 'retired', \
                 updated_at = ?2 \
             WHERE tenant_id = ?1 \
               AND layer = 'l1' \
               AND lifecycle = 'current' \
               AND NOT EXISTS ( \
                   SELECT 1 \
                   FROM memory_item_source_references sr \
                   JOIN memory_item_revisions mir ON sr.item_revision_id = mir.id \
                   WHERE mir.tenant_id = memory_items.tenant_id \
                     AND mir.item_id = memory_items.id \
                     AND sr.availability = 'available' \
               )",
        )
        .bind(tenant_id)
        .bind(now_str)
        .execute(pool)
        .await
        .map_err(AppError::external)?;

        Ok(())
    }

    /// M35-L1-08 / M35-L1-10: 收集未完成事项 (active/blocked/waiting)，跨窗口延续最长 7 天
    /// 超过 7 天则自动退休，输入完全基于结构化事实与引用，严禁读取旧 Markdown
    pub(crate) async fn collect_continuable_items(
        &self,
        target_watermark_utc: &DateTime<Utc>,
    ) -> AppResult<Vec<ContinuableMemoryItemView>> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let target_watermark_str = target_watermark_utc.to_rfc3339();

        // 1. 同步来源有效性并淘汰来源失效的未晋升 L1
        self.sync_source_availability_and_retire_unpromoted_items(&target_watermark_str)
            .await?;

        // 2. 查询活跃/阻塞/等待中的当前 L1 条目
        let rows = sqlx::query(
            "SELECT mi.id as item_id, mi.project_key, mi.first_seen_at, mi.last_seen_at, \
                    mir.id as rev_id, mir.revision_number, mir.category, mir.status, \
                    mir.title, mir.summary, mir.rationale, mir.evidence_fingerprint \
             FROM memory_items mi \
             JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
             WHERE mi.tenant_id = ?1 \
               AND mi.layer = 'l1' \
               AND mi.lifecycle = 'current' \
               AND mir.status IN ('active', 'blocked', 'waiting') \
             ORDER BY mir.occurred_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;

        let mut continuable = Vec::new();

        for row in rows {
            let item_id: String = row.get("item_id");
            let first_seen_at: String = row.get("first_seen_at");
            let last_seen_at: String = row.get("last_seen_at");
            let rev_id: String = row.get("rev_id");
            let current_revision_number: i64 = row.get("revision_number");
            let project_key: Option<String> = row.get("project_key");
            let category_str: String = row.get("category");
            let status_str: String = row.get("status");
            let title: String = row.get("title");
            let summary: String = row.get("summary");
            let rationale: String = row.get("rationale");
            let evidence_fingerprint: String = row.get("evidence_fingerprint");

            // 计算生命周期 (最长 7 天)
            let fs_dt = match DateTime::parse_from_rfc3339(&first_seen_at) {
                Ok(dt) => dt.with_timezone(&Utc),
                Err(_) => continue,
            };

            let seconds_elapsed = (*target_watermark_utc - fs_dt).num_seconds();
            if seconds_elapsed > 7 * 86400 {
                // 超过 7 天上限，退休
                sqlx::query(
                    "UPDATE memory_items SET lifecycle = 'retired', updated_at = ?3 \
                     WHERE tenant_id = ?1 AND id = ?2",
                )
                .bind(tenant_id)
                .bind(&item_id)
                .bind(&target_watermark_str)
                .execute(pool)
                .await
                .map_err(AppError::external)?;
                continue;
            }

            let days_since_first_seen = (seconds_elapsed.max(0) / 86400).min(7);
            let remaining_days = (7 - days_since_first_seen).max(0);

            // 查询有效引用
            let ref_rows = sqlx::query(
                "SELECT reference_key FROM memory_item_source_references \
                 WHERE tenant_id = ?1 AND item_revision_id = ?2 AND availability = 'available'",
            )
            .bind(tenant_id)
            .bind(&rev_id)
            .fetch_all(pool)
            .await
            .map_err(AppError::external)?;

            let source_refs: Vec<String> = ref_rows
                .into_iter()
                .map(|r| r.get("reference_key"))
                .collect();

            let category = match category_str.as_str() {
                "decision" => MemoryItemCategory::Decision,
                "research" => MemoryItemCategory::Research,
                "verification" => MemoryItemCategory::Verification,
                "blocker" => MemoryItemCategory::Blocker,
                "follow_up" => MemoryItemCategory::FollowUp,
                _ => MemoryItemCategory::Progress,
            };

            let status = match status_str.as_str() {
                "blocked" => MemoryItemStatus::Blocked,
                "waiting" => MemoryItemStatus::Waiting,
                _ => MemoryItemStatus::Active,
            };

            continuable.push(ContinuableMemoryItemView {
                item_id,
                project_key: project_key.unwrap_or_else(|| "unassigned".to_string()),
                category,
                status,
                title,
                summary,
                rationale,
                first_seen_at,
                last_seen_at,
                days_since_first_seen,
                remaining_days,
                current_revision_id: rev_id,
                current_revision_number,
                evidence_fingerprint,
                source_refs,
            });
        }

        Ok(continuable)
    }

    /// M35-L1-10 / M35-L1-11: 构建 Work Order 证据首包，包含结构化候选 Session 与可续接条目，绝不读取 Markdown
    pub(crate) fn build_recent_snapshot_work_order_evidence_pack(
        &self,
        target: &WatermarkTarget,
        candidates: &[CandidateSession],
        continuable_items: &[ContinuableMemoryItemView],
    ) -> RecentSnapshotWorkOrderEvidencePack {
        self.build_recent_snapshot_work_order_evidence_pack_with_context(
            target,
            candidates,
            continuable_items,
            &RecentSnapshotFrozenContext::default(),
        )
    }

    fn build_recent_snapshot_work_order_evidence_pack_with_context(
        &self,
        target: &WatermarkTarget,
        candidates: &[CandidateSession],
        continuable_items: &[ContinuableMemoryItemView],
        context: &RecentSnapshotFrozenContext,
    ) -> RecentSnapshotWorkOrderEvidencePack {
        let candidate_sessions = candidates
            .iter()
            .map(|c| CandidateSessionSummary {
                session_ref: c.short_ref.clone(),
                session_id: c.session_id.clone(),
                project_key: c.project_key.clone(),
                title: c.session_title.clone(),
                last_activity_at: c.last_activity_at.clone(),
                source_id: c.source_id.clone(),
                source_agent: c.source_agent.clone(),
                source_revision: c.source_revision,
            })
            .collect::<Vec<_>>();

        let session_evidence = candidates
            .iter()
            .map(|candidate| {
                let memory = context.session_memories.get(&candidate.session_id);
                RecentSnapshotSessionEvidence {
                    candidate: CandidateSessionSummary {
                        session_ref: candidate.short_ref.clone(),
                        session_id: candidate.session_id.clone(),
                        project_key: candidate.project_key.clone(),
                        title: candidate.session_title.clone(),
                        last_activity_at: candidate.last_activity_at.clone(),
                        source_id: candidate.source_id.clone(),
                        source_agent: candidate.source_agent.clone(),
                        source_revision: candidate.source_revision,
                    },
                    memory_source_revision: memory.map(|value| value.source_revision),
                    summary: memory.map(|value| value.summary.clone()),
                    goal: memory.map(|value| value.goal.clone()),
                    result: memory.map(|value| value.result.clone()),
                    decisions: memory
                        .map(|value| value.decisions.clone())
                        .unwrap_or_default(),
                    verification: memory
                        .map(|value| value.verification.clone())
                        .unwrap_or_default(),
                    blockers: memory
                        .map(|value| value.blockers.clone())
                        .unwrap_or_default(),
                    follow_up: memory
                        .map(|value| value.follow_up.clone())
                        .unwrap_or_default(),
                    topics: memory.map(|value| value.topics.clone()).unwrap_or_default(),
                    source_references: context
                        .source_references
                        .get(&candidate.session_id)
                        .cloned()
                        .unwrap_or_default(),
                    recent_events: context
                        .recent_events
                        .get(&candidate.session_id)
                        .cloned()
                        .unwrap_or_default(),
                }
            })
            .collect();

        let mut project_keys: Vec<String> = candidates
            .iter()
            .map(|c| c.project_key.clone())
            .chain(continuable_items.iter().map(|i| i.project_key.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        project_keys.sort();

        let allowed_tools = ALLOWED_MEMORY_GENERATION_TOOLS
            .iter()
            .map(|&s| s.to_string())
            .collect();

        RecentSnapshotWorkOrderEvidencePack {
            target_watermark_utc: target.target_watermark_utc.to_rfc3339(),
            window_start_utc: target.window_start_utc.to_rfc3339(),
            window_end_utc: target.window_end_utc.to_rfc3339(),
            window_hours: target.window_hours as u32,
            project_keys,
            candidate_sessions,
            session_evidence,
            continuable_items: continuable_items.to_vec(),
            current_l2_projects: context.current_l2_projects.clone(),
            current_l3_items: context.current_l3_items.clone(),
            allowed_tools,
            output_schema_version: 2,
        }
    }

    async fn load_recent_snapshot_frozen_context(
        &self,
        target: &WatermarkTarget,
        candidates: &[CandidateSession],
    ) -> AppResult<RecentSnapshotFrozenContext> {
        let session_ids = candidates
            .iter()
            .map(|candidate| candidate.session_id.clone())
            .collect::<Vec<_>>();
        let session_memories = store::list_active_session_memories_for_sessions_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &session_ids,
        )
        .await?;
        let source_references = store::list_session_memory_source_references_for_sessions_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &session_ids,
        )
        .await?;
        let recent_events = store::list_recent_memory_events_for_sessions_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &session_ids,
            &target.window_start_utc.to_rfc3339(),
            &target.window_end_utc.to_rfc3339(),
        )
        .await?;

        let project_keys = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT project_key FROM memory_items WHERE tenant_id = ?1 AND layer = 'l2' AND lifecycle = 'current' AND project_key IS NOT NULL ORDER BY project_key ASC",
        )
        .bind(self.tenant_id())
        .fetch_all(self.db.pool())
        .await
        .map_err(AppError::external)?;
        let mut current_l2_projects = Vec::new();
        for project_key in project_keys {
            if let Some(view) = crate::backend::application::project_consolidation_pipeline::
                load_l2_project_memory_view(self.db.pool(), self.tenant_id(), &project_key)
                .await?
            {
                current_l2_projects.push(view);
            }
        }
        let current_l3_items =
            crate::backend::application::global_consolidation_pipeline::get_global_memory_l3_view(
                self.db.pool(),
                self.tenant_id(),
            )
            .await?
            .map(|view| view.items)
            .unwrap_or_default();

        Ok(RecentSnapshotFrozenContext {
            session_memories,
            source_references,
            recent_events,
            current_l2_projects,
            current_l3_items,
        })
    }

    /// 准备一个 v2 Recent Snapshot 任务。该方法只负责确定性输入、指纹和
    /// reuse；真正的 Agent 调用由 durable job worker 执行。
    pub(crate) async fn prepare_recent_snapshot_generation<Tz: chrono::TimeZone>(
        &self,
        now: Option<DateTime<Tz>>,
    ) -> AppResult<Option<RecentSnapshotPreparation>> {
        self.prepare_recent_snapshot_generation_with_options(now, false)
            .await
    }

    pub(crate) async fn prepare_recent_snapshot_generation_with_options<Tz: chrono::TimeZone>(
        &self,
        now: Option<DateTime<Tz>>,
        force_rebuild: bool,
    ) -> AppResult<Option<RecentSnapshotPreparation>> {
        let settings = self.app_settings_value();
        let memory_settings = settings
            .get("memory")
            .and_then(|value| {
                serde_json::from_value::<crate::backend::app_settings::MemorySettings>(
                    value.clone(),
                )
                .ok()
            })
            .unwrap_or_default();

        if !memory_settings.generation_enabled {
            return Ok(None);
        }

        let current = now
            .map(|value| value.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        let target = resolve_target_watermark(
            current,
            memory_settings.recent_window_hours,
            &memory_settings.watermark_time_1,
            &memory_settings.watermark_time_2,
        )?;
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let state = store::load_recent_memory_state_sqlx(pool, tenant_id).await?;
        let (candidates, _ref_map) = self
            .collect_recent_snapshot_candidates(target.target_watermark_utc, target.window_hours)
            .await?;
        let skill_binding = self.get_active_generation_skill_binding().await?;
        let skill_text = self.load_active_generation_skill_text().await?;
        let continuable_items = self
            .collect_continuable_items(&target.target_watermark_utc)
            .await?;
        let prior_items: Vec<(&str, &str)> = continuable_items
            .iter()
            .map(|item| (item.item_id.as_str(), item.current_revision_id.as_str()))
            .collect();
        let carry_over: Vec<(&str, &str, i64)> = continuable_items
            .iter()
            .map(|item| {
                (
                    item.item_id.as_str(),
                    item.current_revision_id.as_str(),
                    item.remaining_days,
                )
            })
            .collect();
        let frozen_context = self
            .load_recent_snapshot_frozen_context(&target, &candidates)
            .await?;
        // Recent generation consumes successful Phase-1 facts and canonical
        // Conversation locators only. If Phase 1 is still pending, leave the
        // durable watermark untouched so the next coordinator pass can retry
        // after the prerequisite job reaches a terminal state.
        if !has_complete_recent_snapshot_evidence(&candidates, &frozen_context) {
            return Ok(None);
        }
        let evidence = self.build_recent_snapshot_work_order_evidence_pack_with_context(
            &target,
            &candidates,
            &continuable_items,
            &frozen_context,
        );
        let current_l2_l3_revisions = self.load_current_long_term_revision_ids().await?;
        let current_l2_l3_revision_refs = current_l2_l3_revisions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let target_fingerprint = compute_target_fingerprint(
            tenant_id,
            &target.target_watermark_utc,
            target.window_hours,
            &candidates,
            &prior_items,
            &skill_binding,
        );
        let base_content_fingerprint = compute_content_fingerprint(
            target.window_hours,
            &candidates,
            &carry_over,
            &current_l2_l3_revision_refs,
            &skill_binding,
            &memory_settings.excluded_session_ids,
            &memory_settings.excluded_source_ids,
        );
        let content_fingerprint =
            compute_recent_snapshot_evidence_fingerprint(&base_content_fingerprint, &evidence);

        if !force_rebuild {
            if let Some(last_snapshot) = state.snapshot {
                if let Some(last_meta) = store::load_recent_snapshot_meta_by_id_sqlx(
                    pool,
                    tenant_id,
                    &last_snapshot.snapshot_id,
                )
                .await?
                {
                    if last_meta.content_fingerprint == content_fingerprint {
                        if last_meta.target_watermark_utc
                            == target.target_watermark_utc.to_rfc3339()
                            || last_meta.target_fingerprint == target_fingerprint
                        {
                            return Ok(None);
                        }
                        self.commit_reused_memory_snapshot(
                            &target,
                            &skill_binding,
                            &last_meta.id,
                            &target_fingerprint,
                            &content_fingerprint,
                        )
                        .await?;
                        return Ok(None);
                    }
                }
            }
        }

        Ok(Some(RecentSnapshotPreparation {
            target,
            skill_binding,
            target_fingerprint,
            content_fingerprint,
            evidence,
            skill_text,
        }))
    }

    /// 将确定性输入固定为 v2 durable job。相同目标水位和输入指纹只生成一个任务。
    pub(crate) async fn enqueue_recent_snapshot_generation(
        &self,
        preparation: &RecentSnapshotPreparation,
        now: DateTime<Utc>,
    ) -> AppResult<String> {
        let work_order = MemoryWorkOrderV2::new(
            format!("recent-snapshot-{}", Uuid::new_v4()),
            self.tenant_id().to_string(),
            MemoryJobPurpose::RecentSnapshot,
            preparation.target.target_watermark_utc.to_rfc3339(),
            MemoryWindowV2 {
                start_utc: preparation.target.window_start_utc.to_rfc3339(),
                end_utc: preparation.target.window_end_utc.to_rfc3339(),
                hours: preparation.target.window_hours as u32,
            },
            MemoryScopeV2 { project_key: None },
            preparation.content_fingerprint.clone(),
            preparation.skill_binding.clone(),
            now.to_rfc3339(),
        );
        let payload = RecentSnapshotWorkOrderPayload {
            target_watermark_utc: preparation.target.target_watermark_utc.to_rfc3339(),
            local_watermark_date: preparation.target.local_watermark_date.clone(),
            local_watermark_time: preparation.target.local_watermark_time.clone(),
            timezone_offset_minutes: preparation.target.timezone_offset_minutes,
            window_hours: preparation.target.window_hours,
            window_start_utc: preparation.target.window_start_utc.to_rfc3339(),
            window_end_utc: preparation.target.window_end_utc.to_rfc3339(),
            target_fingerprint: preparation.target_fingerprint.clone(),
            content_fingerprint: preparation.content_fingerprint.clone(),
            skill: preparation.skill_binding.clone(),
            skill_text: preparation.skill_text.clone(),
            evidence: preparation.evidence.clone(),
        };
        let work_order_json = serde_json::to_string(&serde_json::json!({
            "workOrder": work_order,
            "payload": payload,
        }))
        .map_err(AppError::external)?;
        store::enqueue_recent_memory_job_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &format!("recent-memory-{}", Uuid::new_v4()),
            &preparation.target.target_watermark_utc.to_rfc3339(),
            preparation.target.window_hours,
            &preparation.target_fingerprint,
            &preparation.content_fingerprint,
            &work_order_json,
            &now.to_rfc3339(),
        )
        .await
    }

    pub(super) async fn run_recent_snapshot_generation_job(
        &self,
        job: &store::RecentMemoryJob,
        cancellation: tokio_util::sync::CancellationToken,
        progress: Option<std::sync::Arc<dyn AiExecutionProgressSink>>,
        mut task_progress: Option<
            &mut super::recent_snapshot_task_progress::RecentSnapshotTaskProgress,
        >,
    ) -> AppResult<RecentMemorySnapshotView> {
        let envelope: serde_json::Value =
            serde_json::from_str(&job.work_order_json).map_err(|_| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: invalid JSON".to_string())
            })?;
        let work_order: MemoryWorkOrderV2 =
            serde_json::from_value(envelope.get("workOrder").cloned().ok_or_else(|| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: missing work order".to_string())
            })?)
            .map_err(|_| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: invalid work order".to_string())
            })?;
        let payload: RecentSnapshotWorkOrderPayload =
            serde_json::from_value(envelope.get("payload").cloned().ok_or_else(|| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: missing payload".to_string())
            })?)
            .map_err(|_| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: invalid payload".to_string())
            })?;
        if work_order.tenant_id != self.tenant_id()
            || work_order.purpose != MemoryJobPurpose::RecentSnapshot
            || work_order.scope.project_key.is_some()
            || work_order.target_watermark_utc != payload.target_watermark_utc
            || work_order.window.hours != payload.window_hours as u32
            || work_order.skill != payload.skill
            || work_order.contract_version != "memory.contract.v2"
            || work_order.allowed_tools
                != ALLOWED_MEMORY_GENERATION_TOOLS
                    .iter()
                    .map(|tool| (*tool).to_string())
                    .collect::<Vec<_>>()
        {
            return Err(AppError::Validation(
                "MEMORY_WORK_ORDER_INVALID: work order binding mismatch".to_string(),
            ));
        }
        let expected_input_fingerprint = MemoryWorkOrderV2::compute_input_fingerprint(
            work_order.purpose,
            &work_order.target_watermark_utc,
            work_order.window.hours,
            work_order.scope.project_key.as_deref(),
            &work_order.source_revision_set_hash,
            &work_order.skill.content_hash,
            &work_order.contract_version,
            &work_order.budget_policy_version,
            &work_order.projection_policy_version,
        );
        if expected_input_fingerprint != work_order.input_fingerprint {
            return Err(AppError::Validation(
                "MEMORY_WORK_ORDER_INVALID: input fingerprint mismatch".to_string(),
            ));
        }

        let target_watermark = DateTime::parse_from_rfc3339(&payload.target_watermark_utc)
            .map_err(|_| {
                AppError::Validation("MEMORY_WORK_ORDER_INVALID: invalid watermark".to_string())
            })?
            .with_timezone(&Utc);
        let target = WatermarkTarget {
            target_watermark_utc: target_watermark,
            local_watermark_date: payload.local_watermark_date.clone(),
            local_watermark_time: payload.local_watermark_time.clone(),
            timezone_offset_minutes: payload.timezone_offset_minutes,
            window_hours: payload.window_hours,
            window_start_utc: DateTime::parse_from_rfc3339(&payload.window_start_utc)
                .map_err(|_| {
                    AppError::Validation(
                        "MEMORY_WORK_ORDER_INVALID: invalid window start".to_string(),
                    )
                })?
                .with_timezone(&Utc),
            window_end_utc: DateTime::parse_from_rfc3339(&payload.window_end_utc)
                .map_err(|_| {
                    AppError::Validation(
                        "MEMORY_WORK_ORDER_INVALID: invalid window end".to_string(),
                    )
                })?
                .with_timezone(&Utc),
        };
        let current_skill = self.get_active_generation_skill_binding().await?;
        if current_skill != payload.skill {
            return Err(AppError::Domain {
                code: "MEMORY_RESULT_STALE".to_string(),
                message: "Memory Generation Skill changed after the job was queued".to_string(),
                retryable: false,
                details: None,
            });
        }
        let (candidates, ref_map) = self
            .collect_recent_snapshot_candidates(target.target_watermark_utc, target.window_hours)
            .await?;
        let continuable_items = self
            .collect_continuable_items(&target.target_watermark_utc)
            .await?;
        let prior_items = continuable_items
            .iter()
            .map(|item| (item.item_id.as_str(), item.current_revision_id.as_str()))
            .collect::<Vec<_>>();
        let carry_over = continuable_items
            .iter()
            .map(|item| {
                (
                    item.item_id.as_str(),
                    item.current_revision_id.as_str(),
                    item.remaining_days,
                )
            })
            .collect::<Vec<_>>();
        let current_l2_l3_revisions = self.load_current_long_term_revision_ids().await?;
        let current_l2_l3_revision_refs = current_l2_l3_revisions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let current_frozen_context = self
            .load_recent_snapshot_frozen_context(&target, &candidates)
            .await?;
        if !has_complete_recent_snapshot_evidence(&candidates, &current_frozen_context) {
            return Err(AppError::Domain {
                code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                message: "Recent Snapshot requires successful Phase-1 facts and canonical references for every candidate session".to_string(),
                retryable: true,
                details: None,
            });
        }
        let current_evidence = self.build_recent_snapshot_work_order_evidence_pack_with_context(
            &target,
            &candidates,
            &continuable_items,
            &current_frozen_context,
        );
        let settings = self.app_settings_value();
        let memory_settings = settings
            .get("memory")
            .and_then(|value| {
                serde_json::from_value::<crate::backend::app_settings::MemorySettings>(
                    value.clone(),
                )
                .ok()
            })
            .unwrap_or_default();
        let current_target_fingerprint = compute_target_fingerprint(
            self.tenant_id(),
            &target.target_watermark_utc,
            target.window_hours,
            &candidates,
            &prior_items,
            &current_skill,
        );
        let current_base_content_fingerprint = compute_content_fingerprint(
            target.window_hours,
            &candidates,
            &carry_over,
            &current_l2_l3_revision_refs,
            &current_skill,
            &memory_settings.excluded_session_ids,
            &memory_settings.excluded_source_ids,
        );
        let current_content_fingerprint = compute_recent_snapshot_evidence_fingerprint(
            &current_base_content_fingerprint,
            &current_evidence,
        );
        if current_target_fingerprint != payload.target_fingerprint
            || current_content_fingerprint != payload.content_fingerprint
        {
            return Err(AppError::Domain {
                code: "MEMORY_RESULT_STALE".to_string(),
                message: "Memory evidence changed after the job was queued".to_string(),
                retryable: false,
                details: None,
            });
        }

        if cancellation.is_cancelled() {
            return Err(AppError::Cancelled(
                "Recent memory generation was cancelled before execution".to_string(),
            ));
        }

        let mut result = if candidates.is_empty() {
            if let Some(task_progress) = task_progress.as_deref_mut() {
                task_progress.transition("agent_execution");
                task_progress.skip_current_and_transition("validation");
            }
            MemoryGenerationResultV2 {
                schema_version: 2,
                projects: Vec::new(),
                coverage: Default::default(),
                unknowns: Vec::new(),
            }
        } else {
            if let Some(task_progress) = task_progress.as_deref_mut() {
                task_progress.transition("agent_execution");
            }
            let prompt = build_recent_generation_prompt(&envelope, &payload)?;
            let settings = self.app_settings_value();
            let (agent_id, model) = crate::backend::ai_execution::composition::resolve_agent_for(
                &crate::backend::ai_execution::composition::ActionId::new("memory.generation"),
                &settings,
            )?;
            let request = AiExecutionRequest {
                execution_id: format!("recent-memory-execution-{}", job.id),
                agent_id,
                purpose: AiExecutionPurpose::MemoryGeneration,
                session_mode: AgentSessionMode::OneShot,
                prompt,
                model,
                limits: AiExecutionLimits::default(),
                cancellation: AiExecutionCancellation::from_token(cancellation.clone()),
                progress,
                tenant_id: Some(job.tenant_id.clone()),
                execution_context_key: None,
                binding: None,
                replay: false,
                restore_only: false,
                team_tools: None,
                recall_tools: None,
                memory_generation_tools: Some(memory_generation_tools_for_job(job, &self.db_path)?),
            };
            let execution = execute_agent(self.agent_runtime.clone(), request)
                .await
                .map_err(|error| {
                    let view = error.to_view();
                    AppError::Domain {
                        code: view.code,
                        message: view.message,
                        retryable: view.retryable,
                        details: None,
                    }
                })?;
            if let Some(task_progress) = task_progress.as_deref_mut() {
                task_progress.transition("validation");
            }
            parse_memory_generation_output(&execution.text)?
        };

        normalize_agent_memory_generation_result(&mut result, &candidates, &ref_map);

        self.validate_memory_generation_result(&result, &candidates, &ref_map)?;
        if cancellation.is_cancelled() {
            return Err(AppError::Cancelled(
                "Recent memory generation was cancelled before publication".to_string(),
            ));
        }
        if let Some(task_progress) = task_progress.as_deref_mut() {
            task_progress.transition("publish");
        }
        let snapshot = self
            .commit_recent_memory_snapshot(
                &target,
                &payload.skill,
                result,
                &candidates,
                &ref_map,
                &payload.target_fingerprint,
                &payload.content_fingerprint,
                Some(&current_l2_l3_revisions),
                Some((
                    job.id.as_str(),
                    job.ownership_token.as_deref().ok_or_else(|| {
                        AppError::Conflict(
                            "recent memory job has no active ownership token".to_string(),
                        )
                    })?,
                )),
            )
            .await?;
        if let Some(task_progress) = task_progress.as_deref_mut() {
            task_progress.transition("cleanup_session");
            task_progress.finish();
        }
        Ok(snapshot)
    }

    /// 校验 Agent 输出是否符合准入与质量门禁 (M35-L1-07/12, Schema, Coverage, Refs)
    pub(crate) fn validate_memory_generation_result(
        &self,
        result: &MemoryGenerationResultV2,
        candidates: &[CandidateSession],
        ref_map: &HashMap<String, ResolvedEvidenceRef>,
    ) -> AppResult<()> {
        if result.schema_version != 2 {
            return Err(AppError::Validation(format!(
                "Unsupported memory generation schema version: {}",
                result.schema_version
            )));
        }

        // 1. Coverage 门禁: 预算耗尽或不可读 Session 发生时直接拒绝发布
        if result.coverage.budget_exhausted {
            return Err(AppError::Domain {
                code: "MEMORY_BUDGET_EXHAUSTED".to_string(),
                message: "Memory generation budget exhausted, refusing publication".to_string(),
                retryable: false,
                details: None,
            });
        }
        if !result.coverage.unreadable_sessions.is_empty() {
            return Err(AppError::Domain {
                code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                message: format!(
                    "Unreadable sessions detected: {:?}",
                    result.coverage.unreadable_sessions
                ),
                retryable: true,
                details: None,
            });
        }

        // Coverage must be an exact partition of the frozen candidate set.
        // Do not allow an Agent to invent a short ref, silently duplicate a
        // session, or omit a candidate while still returning a valid shape.
        let candidate_by_ref = candidates
            .iter()
            .map(|candidate| (candidate.short_ref.as_str(), candidate))
            .collect::<HashMap<_, _>>();
        let mut covered_set = HashSet::new();
        for short_ref in &result.coverage.covered_sessions {
            if !candidate_by_ref.contains_key(short_ref.as_str()) || !covered_set.insert(short_ref)
            {
                return Err(AppError::Validation(format!(
                    "Invalid or duplicate covered session reference '{short_ref}'"
                )));
            }
        }
        for short_ref in &result.coverage.no_memory_sessions {
            if !candidate_by_ref.contains_key(short_ref.as_str()) || !covered_set.insert(short_ref)
            {
                return Err(AppError::Validation(format!(
                    "Invalid or duplicate no-memory session reference '{short_ref}'"
                )));
            }
        }

        for candidate in candidates {
            if !covered_set.contains(&candidate.short_ref) {
                return Err(AppError::Domain {
                    code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                    message: format!(
                        "Candidate session {} (short ref {}) not covered in generation result",
                        candidate.session_id, candidate.short_ref
                    ),
                    retryable: true,
                    details: None,
                });
            }
        }

        if covered_set.len() != candidates.len() {
            return Err(AppError::Domain {
                code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                message: "Generation coverage contains more or fewer sessions than the frozen candidate set".to_string(),
                retryable: true,
                details: None,
            });
        }

        // 2. Project & Item 校验
        let candidate_project_keys = candidates
            .iter()
            .map(|c| c.project_key.clone())
            .collect::<HashSet<_>>();

        let mut result_project_keys = HashSet::new();
        for project in &result.projects {
            let key = project.project_key.trim();
            if key.is_empty() {
                return Err(AppError::Validation(
                    "Project key in memory generation result cannot be empty".to_string(),
                ));
            }

            // 项目 key 必须来自候选集或为 unassigned
            if !candidate_project_keys.contains(key)
                && key != "unassigned"
                && !candidates.is_empty()
            {
                return Err(AppError::Validation(format!(
                    "Project key '{}' not present in candidate work order",
                    key
                )));
            }
            if !result_project_keys.insert(key.to_string()) {
                return Err(AppError::Validation(format!(
                    "Duplicate project '{}' in memory generation result",
                    key
                )));
            }

            for session_ref in &project.source_sessions {
                let candidate = candidate_by_ref.get(session_ref.as_str()).ok_or_else(|| {
                    AppError::Validation(format!(
                        "Unknown source session reference '{}' in project '{}'",
                        session_ref, key
                    ))
                })?;
                if candidate.project_key != key {
                    return Err(AppError::Validation(format!(
                        "Project '{}' references session '{}' from project '{}'",
                        key, session_ref, candidate.project_key
                    )));
                }
            }

            // 摘要长度
            if !project.no_material_change
                && (project.summary.trim().is_empty() || project.summary.len() > 2000)
            {
                return Err(AppError::Validation(format!(
                    "Project '{}' summary must be 1..2000 chars when no_material_change is false",
                    key
                )));
            }

            // 建议排序 (0..3 个 recommendation_rank, 1..=3 且不重复)
            let mut rank_set = HashSet::new();
            for item in &project.items {
                if let Some(rank) = item.recommendation_rank {
                    if !(1..=3).contains(&rank) {
                        return Err(AppError::Validation(format!(
                            "Recommendation rank {} in project '{}' is out of range 1..=3",
                            rank, key
                        )));
                    }
                    if !rank_set.insert(rank) {
                        return Err(AppError::Validation(format!(
                            "Duplicate recommendation rank {} in project '{}'",
                            rank, key
                        )));
                    }
                    if item.source_refs.is_empty() {
                        return Err(AppError::Validation(format!(
                            "Recommendation item '{}' in project '{}' must have at least one source ref",
                            item.title, key
                        )));
                    }
                }

                // 字段长度
                let title = item.title.trim();
                if title.is_empty() || title.len() > 200 {
                    return Err(AppError::Validation(format!(
                        "Item title must be 1..200 chars, got length {}",
                        title.len()
                    )));
                }
                if item.summary.trim().is_empty() || item.summary.len() > 2000 {
                    return Err(AppError::Validation(format!(
                        "Item summary must be 1..2000 chars, got length {}",
                        item.summary.len()
                    )));
                }
                if item.rationale.trim().is_empty() || item.rationale.len() > 2000 {
                    return Err(AppError::Validation(format!(
                        "Item rationale must be 1..2000 chars, got length {}",
                        item.rationale.len()
                    )));
                }

                // 引用校验: 所有 source_refs 必须合法在 ref_map 中存在
                for ref_key in &item.source_refs {
                    let resolved = ref_map.get(ref_key).ok_or_else(|| {
                        AppError::Validation(format!(
                            "Unknown source reference '{}' in item '{}'",
                            ref_key, item.title
                        ))
                    })?;
                    if resolved.project_key != key {
                        return Err(AppError::Validation(format!(
                            "Item '{}' in project '{}' references session '{}' from project '{}'",
                            item.title, key, ref_key, resolved.project_key
                        )));
                    }
                }

                if item.promotion_nomination != MemoryPromotionNomination::None
                    && item.source_refs.is_empty()
                {
                    return Err(AppError::Validation(format!(
                        "Promoted item '{}' must have at least one source ref",
                        item.title
                    )));
                }

                // M35-L1-12: unassigned 项目不能被提名晋升 L2
                if key == "unassigned"
                    && item.promotion_nomination != MemoryPromotionNomination::None
                {
                    return Err(AppError::Validation(
                        "Items in unassigned project cannot be nominated for promotion".to_string(),
                    ));
                }
            }

            if rank_set.len() > 3 {
                return Err(AppError::Validation(format!(
                    "Project '{}' exceeds maximum 3 recommendations",
                    key
                )));
            }
        }

        if result_project_keys != candidate_project_keys {
            return Err(AppError::Domain {
                code: "MEMORY_COVERAGE_INCOMPLETE".to_string(),
                message:
                    "Every candidate project must have exactly one Recent Snapshot project output"
                        .to_string(),
                retryable: true,
                details: None,
            });
        }

        Ok(())
    }

    async fn remove_conflicting_recent_snapshots(
        conn: &mut sqlx::SqliteConnection,
        tenant_id: &str,
        target_watermark_str: &str,
        window_hours: i64,
        exclude_snapshot_id: Option<&str>,
    ) -> AppResult<()> {
        let existing_snapshot_ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM recent_memory_snapshots \
             WHERE tenant_id = ?1 AND target_watermark_utc = ?2 AND window_hours = ?3 AND contract_version = 'memory.contract.v2'",
        )
        .bind(tenant_id)
        .bind(target_watermark_str)
        .bind(window_hours)
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::external)?;

        for old_id in &existing_snapshot_ids {
            if let Some(exclude) = exclude_snapshot_id {
                if old_id == exclude {
                    continue;
                }
            }

            sqlx::query(
                "UPDATE recent_memory_state SET last_successful_snapshot_id = NULL \
                 WHERE tenant_id = ?1 AND last_successful_snapshot_id = ?2",
            )
            .bind(tenant_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::external)?;

            sqlx::query(
                "UPDATE recent_memory_snapshots SET reused_from_snapshot_id = NULL \
                 WHERE tenant_id = ?1 AND reused_from_snapshot_id = ?2",
            )
            .bind(tenant_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::external)?;

            sqlx::query(
                "DELETE FROM memory_promotion_observations WHERE tenant_id = ?1 AND snapshot_id = ?2",
            )
            .bind(tenant_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::external)?;

            sqlx::query(
                "DELETE FROM recent_memory_snapshot_items WHERE tenant_id = ?1 AND snapshot_id = ?2",
            )
            .bind(tenant_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::external)?;

            sqlx::query(
                "DELETE FROM recent_memory_snapshot_projects WHERE tenant_id = ?1 AND snapshot_id = ?2",
            )
            .bind(tenant_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::external)?;

            sqlx::query(
                "DELETE FROM recent_memory_snapshots WHERE tenant_id = ?1 AND id = ?2",
            )
            .bind(tenant_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::external)?;
        }

        Ok(())
    }

    /// 在单个 SQLite 事务内原子提交生成的 L1 Snapshot，并推进 last-success (M35-AUTH-01/05, M35-L1-07/12)
    pub(crate) async fn commit_recent_memory_snapshot(
        &self,
        target: &WatermarkTarget,
        skill_binding: &MemorySkillBinding,
        result: MemoryGenerationResultV2,
        candidates: &[CandidateSession],
        ref_map: &HashMap<String, ResolvedEvidenceRef>,
        target_fingerprint: &str,
        content_fingerprint: &str,
        expected_long_term_revisions: Option<&[String]>,
        job_lease: Option<(&str, &str)>,
    ) -> AppResult<RecentMemorySnapshotView> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let mut tx = pool.begin().await.map_err(AppError::external)?;

        if let Some((job_id, ownership_token)) = job_lease {
            let now = Utc::now().to_rfc3339();
            let owned = sqlx::query(
                "SELECT 1 FROM recent_memory_jobs WHERE tenant_id = ?1 AND id = ?2 \
                 AND status = 'running' AND ownership_token = ?3 \
                 AND lease_expires_at > ?4",
            )
            .bind(tenant_id)
            .bind(job_id)
            .bind(ownership_token)
            .bind(now)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::external)?
            .is_some();
            if !owned {
                return Err(AppError::Conflict(
                    "recent memory job lease is no longer owned".to_string(),
                ));
            }
        }

        let snapshot_id = format!("snap-{}", uuid::Uuid::new_v4());
        let now = Utc::now();
        let published_at = now.to_rfc3339();
        let content_generated_at = published_at.clone();

        let window_start_utc = target.window_start_utc.to_rfc3339();
        let window_end_utc = target.window_end_utc.to_rfc3339();
        let target_watermark_str = target.target_watermark_utc.to_rfc3339();

        let local_watermark_date = &target.local_watermark_date;
        let local_watermark_time = &target.local_watermark_time;
        let timezone_offset_minutes = target.timezone_offset_minutes;
        let window_hours = target.window_hours;

        // 清理同一 watermark 的历史快照冲突（若存在），支持用户重试任务/重新生成无缝覆盖
        Self::remove_conflicting_recent_snapshots(
            &mut tx,
            tenant_id,
            &target_watermark_str,
            window_hours,
            None,
        )
        .await?;

        // Sequence
        let seq_row = sqlx::query(
            "SELECT COALESCE(MAX(sequence), 0) + 1 AS next_seq FROM recent_memory_snapshots WHERE tenant_id = ?1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::external)?;
        let sequence: i64 = seq_row.get("next_seq");

        // 1. 插入 recent_memory_snapshots
        sqlx::query(
            "INSERT INTO recent_memory_snapshots (\
                tenant_id, id, sequence, target_watermark_utc, local_watermark_date, \
                local_watermark_time, timezone_offset_minutes, window_hours, window_start_utc, \
                window_end_utc, publication_kind, reused_from_snapshot_id, target_fingerprint, \
                content_fingerprint, generation_skill_asset_id, generation_skill_revision, \
                generation_skill_content_hash, contract_version, budget_policy_version, \
                projection_policy_version, content_generated_at, published_at\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'generated', NULL, ?11, ?12, ?13, ?14, ?15, 'memory.contract.v2', 'budget.v1', 'projection.v2', ?16, ?17)",
        )
        .bind(tenant_id)
        .bind(&snapshot_id)
        .bind(sequence)
        .bind(&target_watermark_str)
        .bind(local_watermark_date)
        .bind(local_watermark_time)
        .bind(timezone_offset_minutes)
        .bind(window_hours)
        .bind(&window_start_utc)
        .bind(&window_end_utc)
        .bind(target_fingerprint)
        .bind(content_fingerprint)
        .bind(Some(skill_binding.asset_id.as_str()))
        .bind(skill_binding.asset_revision)
        .bind(Some(skill_binding.content_hash.as_str()))
        .bind(&content_generated_at)
        .bind(&published_at)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        // 2. 插入 Project & Items
        for (proj_idx, project) in result.projects.into_iter().enumerate() {
            let project_row_id = format!("{}-{}", snapshot_id, proj_idx + 1);

            // 计算该项目的实际 candidate session count 与 latest activity
            let project_candidates = candidates
                .iter()
                .filter(|c| c.project_key == project.project_key)
                .collect::<Vec<_>>();

            let actual_session_count = project_candidates.len() as i64;
            let latest_activity_at = project_candidates
                .iter()
                .map(|c| c.last_activity_at.as_str())
                .max()
                .unwrap_or(&target_watermark_str);

            let project_path = project_candidates
                .first()
                .and_then(|c| c.project_path.clone());

            let project_title = if project.project_key == "unassigned" {
                "未归属会话".to_string()
            } else {
                project_path
                    .as_deref()
                    .and_then(|p| std::path::Path::new(p).file_name()?.to_str())
                    .unwrap_or(&project.project_key)
                    .to_string()
            };

            let summary = if project.no_material_change && project.summary.trim().is_empty() {
                "本时间窗口内无重大变更".to_string()
            } else {
                project.summary
            };

            sqlx::query(
                "INSERT INTO recent_memory_snapshot_projects (\
                    tenant_id, id, snapshot_id, project_key, project_title, project_path, summary, \
                    no_material_change, latest_activity_at, source_session_count, sort_order\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )
            .bind(tenant_id)
            .bind(&project_row_id)
            .bind(&snapshot_id)
            .bind(&project.project_key)
            .bind(&project_title)
            .bind(&project_path)
            .bind(&summary)
            .bind(if project.no_material_change { 1 } else { 0 })
            .bind(latest_activity_at)
            .bind(actual_session_count)
            .bind(proj_idx as i64)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;

            for (item_idx, item) in project.items.into_iter().enumerate() {
                let mut resolved_item_id = None;
                let mut resolved_rev_num = 1i64;
                let mut resolved_supersedes_id = None;
                let mut first_seen_str = target_watermark_str.clone();

                if let Some(ref prior_id) = item.continues_item_id {
                    let prior_row = sqlx::query(
                        "SELECT mi.id, mi.project_key, mi.lifecycle, mi.first_seen_at, mir.revision_number, mir.status, mir.id as rev_id \
                         FROM memory_items mi \
                         JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
                         WHERE mi.tenant_id = ?1 AND mi.id = ?2",
                    )
                    .bind(tenant_id)
                    .bind(prior_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(AppError::external)?;

                    if let Some(row) = prior_row {
                        let prior_proj: Option<String> = row.get("project_key");
                        let prior_lifecycle: String = row.get("lifecycle");
                        let prior_first_seen: String = row.get("first_seen_at");
                        let prior_status: String = row.get("status");
                        let prior_rev: i64 = row.get("revision_number");
                        let prior_rev_id: String = row.get("rev_id");

                        let proj_match = prior_proj.as_deref().unwrap_or("unassigned")
                            == project.project_key.as_str();
                        let is_current = prior_lifecycle == "current";
                        let was_continuable_status =
                            matches!(prior_status.as_str(), "active" | "blocked" | "waiting");

                        let within_7_days = match (
                            DateTime::parse_from_rfc3339(&target_watermark_str),
                            DateTime::parse_from_rfc3339(&prior_first_seen),
                        ) {
                            (Ok(tw), Ok(fs)) => {
                                (tw.signed_duration_since(fs)).num_seconds() <= 7 * 86400
                            }
                            _ => false,
                        };

                        if proj_match && is_current && was_continuable_status && within_7_days {
                            resolved_item_id = Some(prior_id.clone());
                            resolved_rev_num = prior_rev + 1;
                            resolved_supersedes_id = Some(prior_rev_id);
                            first_seen_str = prior_first_seen;
                        } else {
                            // M35-L1-08 §7.2: 超过 7 天上限或原条目已终态/项目不匹配：旧条目 retired
                            sqlx::query(
                                "UPDATE memory_items SET lifecycle = 'retired', updated_at = ?3 \
                                 WHERE tenant_id = ?1 AND id = ?2",
                            )
                            .bind(tenant_id)
                            .bind(prior_id)
                            .bind(&published_at)
                            .execute(&mut *tx)
                            .await
                            .map_err(AppError::external)?;
                        }
                    }
                }

                let item_id =
                    resolved_item_id.unwrap_or_else(|| format!("item-{}", uuid::Uuid::new_v4()));
                let revision_id = format!("rev-{}", uuid::Uuid::new_v4());

                // M35-L1-09: 终态条目在当前 Snapshot 展示一次后 lifecycle 标记为 retired/superseded，退出 L1
                let new_lifecycle = if item.status == MemoryItemStatus::Superseded {
                    "superseded"
                } else if item.status.is_terminal() {
                    "retired"
                } else {
                    "current"
                };

                // Upsert memory_items
                sqlx::query(
                    "INSERT INTO memory_items (\
                        tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                        first_seen_at, last_seen_at, created_at, updated_at\
                     ) VALUES (?1, ?2, 'l1', ?3, ?4, ?5, ?6, ?7, ?8, ?8) \
                     ON CONFLICT (tenant_id, id) DO UPDATE SET \
                        current_revision_id = excluded.current_revision_id, \
                        lifecycle = excluded.lifecycle, \
                        last_seen_at = excluded.last_seen_at, \
                        updated_at = excluded.updated_at",
                )
                .bind(tenant_id)
                .bind(&item_id)
                .bind(&project.project_key)
                .bind(&revision_id)
                .bind(new_lifecycle)
                .bind(&first_seen_str)
                .bind(&target_watermark_str)
                .bind(&published_at)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // Evidence fingerprint for revision
                let mut rev_hasher = Sha256::new();
                rev_hasher.update(item.title.as_bytes());
                rev_hasher.update(item.summary.as_bytes());
                for r in &item.source_refs {
                    rev_hasher.update(r.as_bytes());
                }
                let evidence_fingerprint = format!("{:x}", rev_hasher.finalize());

                // Insert memory_item_revisions
                sqlx::query(
                    "INSERT INTO memory_item_revisions (\
                        tenant_id, id, item_id, revision_number, category, status, title, summary, \
                        rationale, recommendation_rank, promotion_nomination, occurred_at, \
                        evidence_fingerprint, generated_by_snapshot_id, supersedes_revision_id, created_at\
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                )
                .bind(tenant_id)
                .bind(&revision_id)
                .bind(&item_id)
                .bind(resolved_rev_num)
                .bind(item.category.as_str())
                .bind(item.status.as_str())
                .bind(&item.title)
                .bind(&item.summary)
                .bind(&item.rationale)
                .bind(item.recommendation_rank)
                .bind(item.promotion_nomination.as_str())
                .bind(&item.occurred_at)
                .bind(&evidence_fingerprint)
                .bind(&snapshot_id)
                .bind(resolved_supersedes_id)
                .bind(&published_at)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // Insert recent_memory_snapshot_items
                let snapshot_item_row_id = format!("{}-{}", snapshot_id, item_id);
                let display_date: String = item.occurred_at.chars().take(10).collect();
                sqlx::query(
                    "INSERT INTO recent_memory_snapshot_items (\
                        tenant_id, id, snapshot_id, project_key, item_id, item_revision_id, display_date, sort_order\
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .bind(tenant_id)
                .bind(&snapshot_item_row_id)
                .bind(&snapshot_id)
                .bind(&project.project_key)
                .bind(&item_id)
                .bind(&revision_id)
                .bind(&display_date)
                .bind(item_idx as i64)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // Insert source references (去重插入)
                let mut seen_refs = HashSet::new();
                for ref_key in item.source_refs {
                    if !seen_refs.insert(ref_key.clone()) {
                        continue;
                    }
                    if let Some(resolved) = ref_map.get(&ref_key) {
                        let ref_id = format!("ref-{}", uuid::Uuid::new_v4());
                        sqlx::query(
                            "INSERT INTO memory_item_source_references (\
                                tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                                question_id, turn_id, part_id, node_id, node_order, reference_key, \
                                source_revision, availability, unavailable_reason, unavailable_at, created_at\
                             ) VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, NULL, ?8, NULL, ?9, ?10, 'available', NULL, NULL, ?11)",
                        )
                        .bind(tenant_id)
                        .bind(&ref_id)
                        .bind(&revision_id)
                        .bind(&resolved.source_id)
                        .bind(&resolved.session_id)
                        .bind(&resolved.question_id)
                        .bind(&resolved.turn_id)
                        .bind(&resolved.node_id)
                        .bind(&resolved.reference_key)
                        .bind(resolved.source_revision)
                        .bind(&published_at)
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::external)?;
                    }
                }

                // 晋升观察 (非 unassigned 且有 nomination)
                if item.promotion_nomination != MemoryPromotionNomination::None
                    && project.project_key != "unassigned"
                {
                    let obs_id = format!("obs-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_promotion_observations (\
                            tenant_id, id, item_id, item_revision_id, snapshot_id, nomination, \
                            evidence_fingerprint, project_key, observed_at\
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    )
                    .bind(tenant_id)
                    .bind(&obs_id)
                    .bind(&item_id)
                    .bind(&revision_id)
                    .bind(&snapshot_id)
                    .bind(item.promotion_nomination.as_str())
                    .bind(&evidence_fingerprint)
                    .bind(&project.project_key)
                    .bind(&published_at)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
        }

        // 计算提交后有效内容指纹 (包含当前生成条目状态)，确保后续无变化水位能够精准触发复用
        let settings = self.app_settings_value();
        let memory_settings = settings
            .get("memory")
            .and_then(|v| {
                serde_json::from_value::<crate::backend::app_settings::MemorySettings>(v.clone())
                    .ok()
            })
            .unwrap_or_default();

        let active_rows = sqlx::query(
            "SELECT mi.id as item_id, mir.id as rev_id, mi.first_seen_at \
             FROM memory_items mi \
             JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
             WHERE mi.tenant_id = ?1 \
               AND mi.layer = 'l1' \
               AND mi.lifecycle = 'current' \
               AND mir.status IN ('active', 'blocked', 'waiting')",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::external)?;

        let mut carry_over: Vec<(String, String, i64)> = Vec::new();
        for r in active_rows {
            let item_id: String = r.get("item_id");
            let rev_id: String = r.get("rev_id");
            let fs: String = r.get("first_seen_at");
            let days_elapsed = match DateTime::parse_from_rfc3339(&fs) {
                Ok(dt) => {
                    (target.target_watermark_utc - dt.with_timezone(&Utc))
                        .num_seconds()
                        .max(0)
                        / 86400
                }
                Err(_) => 0,
            };
            let rem_bucket = (7 - days_elapsed).max(0);
            carry_over.push((item_id, rev_id, rem_bucket));
        }

        let carry_over_refs: Vec<(&str, &str, i64)> = carry_over
            .iter()
            .map(|(i, r, b)| (i.as_str(), r.as_str(), *b))
            .collect();

        let current_long_term_revisions: Vec<String> = sqlx::query_scalar(
            "SELECT mir.id FROM memory_items mi
             JOIN memory_item_revisions mir
               ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id
             WHERE mi.tenant_id = ?1 AND mi.layer IN ('l2', 'l3')
               AND mi.lifecycle = 'current' ORDER BY mi.id ASC",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::external)?;
        let current_long_term_revision_refs = current_long_term_revisions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        let effective_content_fp = compute_content_fingerprint(
            target.window_hours,
            candidates,
            &carry_over_refs,
            &current_long_term_revision_refs,
            skill_binding,
            &memory_settings.excluded_session_ids,
            &memory_settings.excluded_source_ids,
        );

        if let Some(expected) = expected_long_term_revisions {
            if expected != current_long_term_revisions.as_slice() {
                return Err(AppError::Domain {
                    code: "MEMORY_RESULT_STALE".to_string(),
                    message: "Long-term memory changed before publication".to_string(),
                    retryable: false,
                    details: None,
                });
            }
        }

        sqlx::query(
            "UPDATE recent_memory_snapshots SET content_fingerprint = ?3 \
             WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(tenant_id)
        .bind(&snapshot_id)
        .bind(&effective_content_fp)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        // 3. 更新 recent_memory_state (原子更新 last-success，清除错误)
        let state_id = format!("state-{}", tenant_id);
        sqlx::query(
            "INSERT INTO recent_memory_state (\
                tenant_id, id, last_successful_snapshot_id, latest_attempt_task_id, \
                latest_attempt_error_code, latest_attempt_error_message, created_at, updated_at\
             ) VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4, ?4) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
                last_successful_snapshot_id = excluded.last_successful_snapshot_id, \
                latest_attempt_error_code = NULL, \
                latest_attempt_error_message = NULL, \
                updated_at = excluded.updated_at",
        )
        .bind(tenant_id)
        .bind(&state_id)
        .bind(&snapshot_id)
        .bind(&published_at)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        if let Some((job_id, ownership_token)) = job_lease {
            let finalized = sqlx::query(
                "UPDATE recent_memory_jobs SET status = 'succeeded', retry_at = NULL, \
                 last_error_code = NULL, last_error_message = NULL, finished_at = ?1, \
                 ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?1 \
                 WHERE tenant_id = ?2 AND id = ?3 AND status = 'running' \
                 AND ownership_token = ?4 AND lease_expires_at > ?1",
            )
            .bind(&published_at)
            .bind(tenant_id)
            .bind(job_id)
            .bind(ownership_token)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
            if finalized.rows_affected() != 1 {
                return Err(AppError::Conflict(
                    "recent memory job lease is no longer owned".to_string(),
                ));
            }
        }

        tx.commit().await.map_err(AppError::external)?;

        let loaded = store::load_recent_snapshot_by_id_sqlx(pool, tenant_id, &snapshot_id)
            .await?
            .ok_or_else(|| {
                AppError::external("Committed snapshot not found immediately after commit")
            })?;

        Ok(loaded)
    }

    /// 执行固定水位 L1 Snapshot 管线并记录失败审计 (M35-L1-01/03/04/07/12)
    pub(crate) async fn execute_recent_memory_snapshot_pipeline(
        &self,
        target_watermark_utc: DateTime<Utc>,
        window_hours: i64,
        result: MemoryGenerationResultV2,
    ) -> AppResult<RecentMemorySnapshotView> {
        let target = WatermarkTarget {
            target_watermark_utc,
            local_watermark_date: target_watermark_utc.format("%Y-%m-%d").to_string(),
            local_watermark_time: target_watermark_utc.format("%H:%M").to_string(),
            timezone_offset_minutes: 0,
            window_hours,
            window_start_utc: target_watermark_utc - Duration::hours(window_hours),
            window_end_utc: target_watermark_utc,
        };

        let (candidates, ref_map) = self
            .collect_recent_snapshot_candidates(target.target_watermark_utc, target.window_hours)
            .await?;

        // 校验门禁
        if let Err(e) = self.validate_memory_generation_result(&result, &candidates, &ref_map) {
            let (code, msg) = match &e {
                AppError::Domain { code, message, .. } => (code.clone(), message.clone()),
                other => ("VALIDATION_FAILED".to_string(), other.to_string()),
            };
            self.record_recent_memory_failure(&code, &msg).await?;
            return Err(e);
        }

        let skill_binding = self.get_active_generation_skill_binding().await?;

        let target_fingerprint = compute_target_fingerprint(
            self.tenant_id(),
            &target.target_watermark_utc,
            target.window_hours,
            &candidates,
            &[],
            &skill_binding,
        );

        let content_fingerprint = compute_content_fingerprint(
            target.window_hours,
            &candidates,
            &[],
            &[],
            &skill_binding,
            &[],
            &[],
        );

        let snapshot_view = self
            .commit_recent_memory_snapshot(
                &target,
                &skill_binding,
                result,
                &candidates,
                &ref_map,
                &target_fingerprint,
                &content_fingerprint,
                None,
                None,
            )
            .await?;

        Ok(snapshot_view)
    }

    /// 在单个 SQLite 事务内原子提交无变化复用的 L1 Snapshot (M35-L1-06)
    /// 推进水位与 last-success，保留原始 content_generated_at，且不增加晋升观察次数
    pub(crate) async fn commit_reused_memory_snapshot(
        &self,
        target: &WatermarkTarget,
        skill_binding: &MemorySkillBinding,
        prior_snapshot_id: &str,
        target_fingerprint: &str,
        content_fingerprint: &str,
    ) -> AppResult<RecentMemorySnapshotView> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let mut tx = pool.begin().await.map_err(AppError::external)?;

        let prior_row = sqlx::query(
            "SELECT content_generated_at FROM recent_memory_snapshots WHERE tenant_id = ?1 AND id = ?2",
        )
        .bind(tenant_id)
        .bind(prior_snapshot_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::external)?;

        let content_generated_at: String = prior_row.get("content_generated_at");

        let snapshot_id = format!("snap-{}", uuid::Uuid::new_v4());
        let now = Utc::now();
        let published_at = now.to_rfc3339();

        let window_start_utc = target.window_start_utc.to_rfc3339();
        let window_end_utc = target.window_end_utc.to_rfc3339();
        let target_watermark_str = target.target_watermark_utc.to_rfc3339();

        // 清理同一 watermark 的历史快照冲突（若存在，排除 prior_snapshot_id）
        Self::remove_conflicting_recent_snapshots(
            &mut tx,
            tenant_id,
            &target_watermark_str,
            target.window_hours,
            Some(prior_snapshot_id),
        )
        .await?;

        let seq_row = sqlx::query(
            "SELECT COALESCE(MAX(sequence), 0) + 1 AS next_seq FROM recent_memory_snapshots WHERE tenant_id = ?1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::external)?;
        let sequence: i64 = seq_row.get("next_seq");

        sqlx::query(
            "INSERT INTO recent_memory_snapshots (\
                tenant_id, id, sequence, target_watermark_utc, local_watermark_date, \
                local_watermark_time, timezone_offset_minutes, window_hours, window_start_utc, \
                window_end_utc, publication_kind, reused_from_snapshot_id, target_fingerprint, \
                content_fingerprint, generation_skill_asset_id, generation_skill_revision, \
                generation_skill_content_hash, contract_version, budget_policy_version, \
                projection_policy_version, content_generated_at, published_at\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'reused', ?11, ?12, ?13, ?14, ?15, ?16, 'memory.contract.v2', 'budget.v1', 'projection.v2', ?17, ?18)",
        )
        .bind(tenant_id)
        .bind(&snapshot_id)
        .bind(sequence)
        .bind(&target_watermark_str)
        .bind(&target.local_watermark_date)
        .bind(&target.local_watermark_time)
        .bind(target.timezone_offset_minutes)
        .bind(target.window_hours)
        .bind(&window_start_utc)
        .bind(&window_end_utc)
        .bind(Some(prior_snapshot_id))
        .bind(target_fingerprint)
        .bind(content_fingerprint)
        .bind(Some(skill_binding.asset_id.as_str()))
        .bind(skill_binding.asset_revision)
        .bind(Some(skill_binding.content_hash.as_str()))
        .bind(&content_generated_at)
        .bind(&published_at)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        // 复制 project rows
        sqlx::query(
            "INSERT INTO recent_memory_snapshot_projects (\
                tenant_id, id, snapshot_id, project_key, project_title, project_path, summary, \
                no_material_change, latest_activity_at, source_session_count, sort_order\
             ) SELECT tenant_id, ?1 || '-' || sort_order, ?1, project_key, project_title, project_path, \
                      summary, no_material_change, latest_activity_at, source_session_count, sort_order \
               FROM recent_memory_snapshot_projects WHERE tenant_id = ?2 AND snapshot_id = ?3",
        )
        .bind(&snapshot_id)
        .bind(tenant_id)
        .bind(prior_snapshot_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        // 复制 item rows
        sqlx::query(
            "INSERT INTO recent_memory_snapshot_items (\
                tenant_id, id, snapshot_id, project_key, item_id, item_revision_id, display_date, sort_order\
             ) SELECT tenant_id, ?1 || '-' || sort_order, ?1, project_key, item_id, item_revision_id, display_date, sort_order \
               FROM recent_memory_snapshot_items WHERE tenant_id = ?2 AND snapshot_id = ?3",
        )
        .bind(&snapshot_id)
        .bind(tenant_id)
        .bind(prior_snapshot_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        // M35-L1-06 / M35-L2-04: reused Snapshot 不增加 L2/L3 晋升观察次数

        // 推进 last-success
        let state_id = format!("state-{}", tenant_id);
        sqlx::query(
            "INSERT INTO recent_memory_state (\
                tenant_id, id, last_successful_snapshot_id, latest_attempt_task_id, \
                latest_attempt_error_code, latest_attempt_error_message, created_at, updated_at\
             ) VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4, ?4) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
                last_successful_snapshot_id = excluded.last_successful_snapshot_id, \
                latest_attempt_error_code = NULL, \
                latest_attempt_error_message = NULL, \
                updated_at = excluded.updated_at",
        )
        .bind(tenant_id)
        .bind(&state_id)
        .bind(&snapshot_id)
        .bind(&published_at)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        tx.commit().await.map_err(AppError::external)?;

        let loaded = store::load_recent_snapshot_by_id_sqlx(pool, tenant_id, &snapshot_id)
            .await?
            .ok_or_else(|| {
                AppError::external("Committed reused snapshot not found immediately after commit")
            })?;

        Ok(loaded)
    }

    /// 评估并执行双水位调度与无变化复用 (M35-L1-01–06)
    pub(crate) async fn evaluate_and_run_recent_snapshot<Tz: chrono::TimeZone>(
        &self,
        now: Option<DateTime<Tz>>,
        mock_result: Option<MemoryGenerationResultV2>,
    ) -> AppResult<Option<RecentMemorySnapshotView>> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let settings = self.app_settings_value();

        let memory_settings = settings
            .get("memory")
            .and_then(|v| {
                serde_json::from_value::<crate::backend::app_settings::MemorySettings>(v.clone())
                    .ok()
            })
            .unwrap_or_default();

        if !memory_settings.generation_enabled {
            return Ok(None);
        }

        let target = if let Some(custom_now) = now {
            resolve_target_watermark(
                custom_now,
                memory_settings.recent_window_hours,
                &memory_settings.watermark_time_1,
                &memory_settings.watermark_time_2,
            )?
        } else {
            let utc_now = Utc::now();
            resolve_target_watermark(
                utc_now,
                memory_settings.recent_window_hours,
                &memory_settings.watermark_time_1,
                &memory_settings.watermark_time_2,
            )?
        };

        let state = store::load_recent_memory_state_sqlx(pool, tenant_id).await?;

        let (candidates, ref_map) = self
            .collect_recent_snapshot_candidates(target.target_watermark_utc, target.window_hours)
            .await?;

        let skill_binding = self.get_active_generation_skill_binding().await?;

        let continuable_items = self
            .collect_continuable_items(&target.target_watermark_utc)
            .await?;

        let prior_items_for_target: Vec<(&str, &str)> = continuable_items
            .iter()
            .map(|i| (i.item_id.as_str(), i.current_revision_id.as_str()))
            .collect();

        let carry_over_items_for_content: Vec<(&str, &str, i64)> = continuable_items
            .iter()
            .map(|i| {
                (
                    i.item_id.as_str(),
                    i.current_revision_id.as_str(),
                    i.remaining_days,
                )
            })
            .collect();
        let current_l2_l3_revisions = self.load_current_long_term_revision_ids().await?;
        let current_l2_l3_revision_refs = current_l2_l3_revisions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();

        let target_fingerprint = compute_target_fingerprint(
            tenant_id,
            &target.target_watermark_utc,
            target.window_hours,
            &candidates,
            &prior_items_for_target,
            &skill_binding,
        );

        let content_fingerprint = compute_content_fingerprint(
            target.window_hours,
            &candidates,
            &carry_over_items_for_content,
            &current_l2_l3_revision_refs,
            &skill_binding,
            &memory_settings.excluded_session_ids,
            &memory_settings.excluded_source_ids,
        );

        let _evidence_pack = self.build_recent_snapshot_work_order_evidence_pack(
            &target,
            &candidates,
            &continuable_items,
        );

        // 幂等检查 (M35-L1-05): 如果已有成功 Snapshot 且目标水位已处理或 target_fingerprint 相同，跳过
        if let Some(ref last_snap) = state.snapshot {
            if let Some(last_meta) =
                store::load_recent_snapshot_meta_by_id_sqlx(pool, tenant_id, &last_snap.snapshot_id)
                    .await?
            {
                if last_meta.target_watermark_utc == target.target_watermark_utc.to_rfc3339()
                    || last_meta.target_fingerprint == target_fingerprint
                {
                    return Ok(None);
                }

                // Reuse 检查: 如果 content_fingerprint 相同，执行无变化复用 (M35-L1-06)
                if last_meta.content_fingerprint == content_fingerprint {
                    let reused = self
                        .commit_reused_memory_snapshot(
                            &target,
                            &skill_binding,
                            &last_meta.id,
                            &target_fingerprint,
                            &content_fingerprint,
                        )
                        .await?;
                    return Ok(Some(reused));
                }
            }
        }

        // 内容变化或首次生成
        if let Some(result) = mock_result {
            if let Err(e) = self.validate_memory_generation_result(&result, &candidates, &ref_map) {
                let (code, msg) = match &e {
                    AppError::Domain { code, message, .. } => (code.clone(), message.clone()),
                    other => ("VALIDATION_FAILED".to_string(), other.to_string()),
                };
                self.record_recent_memory_failure(&code, &msg).await?;
                return Err(e);
            }

            let snap = self
                .commit_recent_memory_snapshot(
                    &target,
                    &skill_binding,
                    result,
                    &candidates,
                    &ref_map,
                    &target_fingerprint,
                    &content_fingerprint,
                    None,
                    None,
                )
                .await?;
            return Ok(Some(snap));
        }

        Ok(None)
    }

    /// 记录最近生成失败（保持原有 last-success 不变）
    pub(crate) async fn record_recent_memory_failure(
        &self,
        error_code: &str,
        error_message: &str,
    ) -> AppResult<()> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let state_id = format!("state-{}", tenant_id);
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO recent_memory_state (\
                tenant_id, id, last_successful_snapshot_id, latest_attempt_task_id, \
                latest_attempt_error_code, latest_attempt_error_message, created_at, updated_at\
             ) VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?5, ?5) \
             ON CONFLICT (tenant_id) DO UPDATE SET \
                latest_attempt_error_code = excluded.latest_attempt_error_code, \
                latest_attempt_error_message = excluded.latest_attempt_error_message, \
                updated_at = excluded.updated_at",
        )
        .bind(tenant_id)
        .bind(&state_id)
        .bind(error_code)
        .bind(error_message)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(AppError::external)?;

        Ok(())
    }
}

pub(crate) fn parse_memory_generation_output(raw: &str) -> AppResult<MemoryGenerationResultV2> {
    let trimmed = raw.trim();
    let json_text = if let Some(rest) = trimmed.strip_prefix("```") {
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        let rest = rest.strip_prefix('\n').unwrap_or(rest);
        rest.strip_suffix("```").map(str::trim).ok_or_else(|| {
            AppError::Validation("MEMORY_OUTPUT_INVALID: unterminated JSON code fence".to_string())
        })?
    } else {
        trimmed
    };
    if json_text.is_empty() {
        return Err(AppError::Validation(
            "MEMORY_OUTPUT_INVALID: empty Agent output".to_string(),
        ));
    }
    match serde_json::from_str::<MemoryGenerationResultV2>(json_text) {
        Ok(result) => Ok(result),
        Err(orig_err) => {
            if let Some(repaired) = attempt_repair_truncated_json(json_text) {
                if let Ok(result) = serde_json::from_str::<MemoryGenerationResultV2>(&repaired) {
                    tracing::warn!("Successfully repaired truncated JSON in memory generation output");
                    return Ok(result);
                }
            }
            Err(AppError::Validation(format!(
                "MEMORY_OUTPUT_INVALID: expected one MemoryGenerationResultV2 JSON value: {orig_err}"
            )))
        }
    }
}

fn attempt_repair_truncated_json(input: &str) -> Option<String> {
    let s = input.trim();
    if !s.starts_with('{') {
        return None;
    }

    let mut last_brace = s.rfind('}')?;
    while last_brace > 0 {
        let candidate = &s[..=last_brace];
        let mut stack = Vec::new();
        let mut in_string = false;
        let mut escape = false;
        let mut valid = true;

        for ch in candidate.chars() {
            if in_string {
                if escape {
                    escape = false;
                } else if ch == '\\' {
                    escape = true;
                } else if ch == '"' {
                    in_string = false;
                }
            } else {
                match ch {
                    '"' => in_string = true,
                    '{' => stack.push('}'),
                    '[' => stack.push(']'),
                    '}' | ']' => {
                        if stack.pop() != Some(ch) {
                            valid = false;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }

        if valid && !in_string {
            let mut repaired = candidate.to_string();
            while let Some(closing) = stack.pop() {
                repaired.push(closing);
            }
            return Some(repaired);
        }

        last_brace = s[..last_brace].rfind('}')?;
    }
    None
}

#[cfg(test)]
#[path = "recent_snapshot_pipeline_tests.rs"]
mod tests;
