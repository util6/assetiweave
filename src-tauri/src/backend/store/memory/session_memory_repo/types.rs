use super::*;

pub(crate) const SESSION_MEMORY_CONTRACT_VERSION: &str = "session-memory.v1";
pub(crate) const SESSION_MEMORY_PROMPT_VERSION: &str = "session-memory-prompt.v1";
pub(crate) const SESSION_MEMORY_JOB_LEASE: Duration = Duration::minutes(2);
pub(crate) const SESSION_IDLE_DELAY: Duration = Duration::minutes(30);

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionMemoryJobCandidate {
    pub(crate) session_id: String,
    pub(crate) source_id: String,
    pub(crate) source_revision: i64,
    pub(crate) source_fingerprint: String,
    pub(crate) not_before: String,
    pub(crate) recipe_id: Option<String>,
    pub(crate) recipe_revision: Option<i64>,
    pub(crate) recipe_content_hash: Option<String>,
    pub(crate) budget_policy_version: Option<String>,
    pub(crate) work_order_json: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionMemoryPersistInput {
    pub(crate) memory_id: String,
    pub(crate) tenant_id: String,
    pub(crate) session_id: String,
    pub(crate) source_id: String,
    pub(crate) source_revision: i64,
    pub(crate) source_fingerprint: String,
    pub(crate) contract_version: String,
    pub(crate) prompt_version: String,
    pub(crate) project_path: Option<String>,
    pub(crate) summary: String,
    pub(crate) goal: String,
    pub(crate) result: String,
    pub(crate) decisions_json: String,
    pub(crate) verification_json: String,
    pub(crate) blockers_json: String,
    pub(crate) follow_up_json: String,
    pub(crate) topics_json: String,
    pub(crate) raw_output_json: String,
    pub(crate) generated_at: String,
    pub(crate) ownership_token: String,
    pub(crate) references: Vec<SessionMemoryReferenceInput>,
    pub(crate) events: Vec<RecentMemoryEventInput>,
    pub(crate) recipe_id: Option<String>,
    pub(crate) recipe_content_hash: Option<String>,
    pub(crate) work_order_json: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionMemoryReferenceInput {
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) question_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) part_id: Option<String>,
    pub(crate) node_id: Option<String>,
    pub(crate) node_order: Option<usize>,
    pub(crate) reference_key: String,
    pub(crate) source_revision: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct RecentMemoryEventInput {
    pub(crate) category: RecentMemoryEventCategory,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) occurred_at: String,
    pub(crate) source_reference_id: Option<String>,
    pub(crate) fingerprint: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionMemorySourceCandidate {
    pub(crate) id: String,
    pub(crate) source_id: String,
    pub(crate) source_fingerprint: Option<String>,
    pub(crate) updated_at: Option<String>,
}

pub(crate) struct MemoryGenerationPolicy {
    pub(crate) enabled: bool,
    pub(crate) excluded_session_ids: HashSet<String>,
    pub(crate) excluded_source_ids: HashSet<String>,
}

#[derive(Debug, FromRow)]
pub(crate) struct SessionMemorySourceCandidateRow {
    pub(crate) id: String,
    pub(crate) source_id: String,
    pub(crate) source_fingerprint: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl From<SessionMemorySourceCandidateRow> for SessionMemorySourceCandidate {
    fn from(row: SessionMemorySourceCandidateRow) -> Self {
        Self {
            id: row.id,
            source_id: row.source_id,
            source_fingerprint: row.source_fingerprint,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct RecentMemoryEventTargetRow {
    pub(crate) session_id: String,
    pub(crate) question_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) node_id: Option<String>,
}

impl From<RecentMemoryEventTargetRow> for crate::backend::domain::memory::RecentMemoryEventTarget {
    fn from(row: RecentMemoryEventTargetRow) -> Self {
        Self {
            record_kind: "session".to_string(),
            session_id: row.session_id,
            question_id: row.question_id,
            turn_id: row.turn_id,
            block_id: row.node_id,
        }
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct SessionMemoryJobRow {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) source_id: String,
    pub(crate) source_revision: i64,
    pub(crate) source_fingerprint: String,
    pub(crate) contract_version: String,
    pub(crate) prompt_version: String,
    pub(crate) source_event_id: String,
    pub(crate) source_sync_run_id: String,
    pub(crate) status: String,
    pub(crate) not_before: String,
    pub(crate) attempt_count: i64,
    pub(crate) last_error: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) ownership_token: Option<String>,
    pub(crate) lease_expires_at: Option<String>,
    pub(crate) heartbeat_at: Option<String>,
    pub(crate) retry_count: i64,
    pub(crate) retry_at: Option<String>,
    pub(crate) watermark: Option<i64>,
    pub(crate) recipe_id: Option<String>,
    pub(crate) recipe_revision: Option<i64>,
    pub(crate) recipe_content_hash: Option<String>,
    pub(crate) budget_policy_version: Option<String>,
    pub(crate) work_order_json: Option<String>,
}

impl SessionMemoryJobRow {
    pub(crate) fn try_into_job(self) -> StoreResult<SessionMemoryJob> {
        Ok(SessionMemoryJob {
            tenant_id: self.tenant_id,
            id: self.id,
            session_id: self.session_id,
            source_id: self.source_id,
            source_revision: self.source_revision,
            source_fingerprint: self.source_fingerprint,
            contract_version: self.contract_version,
            prompt_version: self.prompt_version,
            source_event_id: self.source_event_id,
            source_sync_run_id: self.source_sync_run_id,
            status: parse_job_status(&self.status)?,
            not_before: self.not_before,
            attempt_count: self.attempt_count,
            last_error: self.last_error,
            started_at: self.started_at,
            finished_at: self.finished_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            ownership_token: self.ownership_token,
            lease_expires_at: self.lease_expires_at,
            heartbeat_at: self.heartbeat_at,
            retry_count: self.retry_count,
            retry_at: self.retry_at,
            watermark: self.watermark,
            recipe_id: self.recipe_id,
            recipe_revision: self.recipe_revision,
            recipe_content_hash: self.recipe_content_hash,
            budget_policy_version: self.budget_policy_version,
            work_order_json: self.work_order_json,
        })
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct SessionMemoryRow {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) source_id: String,
    pub(crate) source_revision: i64,
    pub(crate) source_fingerprint: String,
    pub(crate) contract_version: String,
    pub(crate) prompt_version: String,
    pub(crate) status: String,
    pub(crate) project_path: Option<String>,
    pub(crate) summary: String,
    pub(crate) goal: String,
    pub(crate) result: String,
    pub(crate) decisions_json: String,
    pub(crate) verification_json: String,
    pub(crate) blockers_json: String,
    pub(crate) follow_up_json: String,
    pub(crate) topics_json: String,
    pub(crate) generated_at: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) recipe_id: Option<String>,
    pub(crate) recipe_content_hash: Option<String>,
    pub(crate) work_order_json: Option<String>,
}

impl SessionMemoryRow {
    pub(crate) fn try_into_memory(self) -> StoreResult<SessionMemory> {
        Ok(SessionMemory {
            tenant_id: self.tenant_id,
            id: self.id,
            session_id: self.session_id,
            source_id: self.source_id,
            source_revision: self.source_revision,
            source_fingerprint: self.source_fingerprint,
            contract_version: self.contract_version,
            prompt_version: self.prompt_version,
            status: parse_memory_status(&self.status)?,
            project_path: self.project_path,
            summary: self.summary,
            goal: self.goal,
            result: self.result,
            decisions: decode_string_array(&self.decisions_json)?,
            verification: decode_string_array(&self.verification_json)?,
            blockers: decode_string_array(&self.blockers_json)?,
            follow_up: decode_string_array(&self.follow_up_json)?,
            topics: decode_string_array(&self.topics_json)?,
            generated_at: self.generated_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            recipe_id: self.recipe_id,
            recipe_content_hash: self.recipe_content_hash,
            work_order_json: self.work_order_json,
        })
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct SessionMemorySourceReferenceRow {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) memory_id: String,
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) question_id: Option<String>,
    pub(crate) turn_id: Option<String>,
    pub(crate) part_id: Option<String>,
    pub(crate) node_id: Option<String>,
    pub(crate) node_order: Option<i64>,
    pub(crate) reference_key: String,
    pub(crate) source_revision: i64,
    pub(crate) created_at: String,
}

impl SessionMemorySourceReferenceRow {
    pub(crate) fn into_reference(self) -> SessionMemorySourceReference {
        SessionMemorySourceReference {
            tenant_id: self.tenant_id,
            id: self.id,
            memory_id: self.memory_id,
            source_id: self.source_id,
            session_id: self.session_id,
            question_id: self.question_id,
            turn_id: self.turn_id,
            part_id: self.part_id,
            node_id: self.node_id,
            node_order: self
                .node_order
                .and_then(|value| usize::try_from(value).ok()),
            reference_key: self.reference_key,
            source_revision: self.source_revision,
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct RecentMemoryEventRow {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) memory_id: String,
    pub(crate) session_id: String,
    pub(crate) category: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) occurred_at: String,
    pub(crate) source_reference_id: Option<String>,
    pub(crate) fingerprint: String,
    pub(crate) created_at: String,
}

impl RecentMemoryEventRow {
    pub(crate) fn try_into_event(self) -> StoreResult<RecentMemoryEvent> {
        Ok(RecentMemoryEvent {
            tenant_id: self.tenant_id,
            id: self.id,
            memory_id: self.memory_id,
            session_id: self.session_id,
            category: RecentMemoryEventCategory::parse(&self.category)
                .ok_or_else(|| StoreError::external("invalid Recent Event category"))?,
            title: self.title,
            summary: self.summary,
            occurred_at: self.occurred_at,
            source_reference_id: self.source_reference_id,
            fingerprint: self.fingerprint,
            created_at: self.created_at,
        })
    }
}

pub(crate) fn decode_string_array(value: &str) -> StoreResult<Vec<String>> {
    serde_json::from_str(value).map_err(StoreError::external)
}

pub(crate) fn parse_job_status(value: &str) -> StoreResult<SessionMemoryJobStatus> {
    match value {
        "queued" => Ok(SessionMemoryJobStatus::Queued),
        "running" => Ok(SessionMemoryJobStatus::Running),
        "succeeded" => Ok(SessionMemoryJobStatus::Succeeded),
        "failed" => Ok(SessionMemoryJobStatus::Failed),
        "skipped" => Ok(SessionMemoryJobStatus::Skipped),
        "canceled" => Ok(SessionMemoryJobStatus::Canceled),
        _ => Err(StoreError::external("invalid Session Memory job status")),
    }
}

pub(crate) fn parse_memory_status(value: &str) -> StoreResult<SessionMemoryStatus> {
    match value {
        "active" => Ok(SessionMemoryStatus::Active),
        "invalid" => Ok(SessionMemoryStatus::Invalid),
        "failed" => Ok(SessionMemoryStatus::Failed),
        _ => Err(StoreError::external("invalid Session Memory status")),
    }
}
