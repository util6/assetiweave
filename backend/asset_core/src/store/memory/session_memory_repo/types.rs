use super::*;

pub const SESSION_MEMORY_CONTRACT_VERSION: &str = "session-memory.v1";
pub const SESSION_MEMORY_PROMPT_VERSION: &str = "session-memory-prompt.v1";
pub const SESSION_MEMORY_JOB_LEASE: Duration = Duration::minutes(2);
pub const SESSION_IDLE_DELAY: Duration = Duration::minutes(30);

#[derive(Debug, Clone, Default)]
pub struct SessionMemoryJobCandidate {
    pub session_id: String,
    pub source_id: String,
    pub source_revision: i64,
    pub source_fingerprint: String,
    pub not_before: String,
    pub recipe_id: Option<String>,
    pub recipe_revision: Option<i64>,
    pub recipe_content_hash: Option<String>,
    pub budget_policy_version: Option<String>,
    pub work_order_json: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionMemoryPersistInput {
    pub memory_id: String,
    pub tenant_id: String,
    pub session_id: String,
    pub source_id: String,
    pub source_revision: i64,
    pub source_fingerprint: String,
    pub contract_version: String,
    pub prompt_version: String,
    pub project_path: Option<String>,
    pub summary: String,
    pub goal: String,
    pub result: String,
    pub decisions_json: String,
    pub verification_json: String,
    pub blockers_json: String,
    pub follow_up_json: String,
    pub topics_json: String,
    pub raw_output_json: String,
    pub generated_at: String,
    pub ownership_token: String,
    pub references: Vec<SessionMemoryReferenceInput>,
    pub events: Vec<RecentMemoryEventInput>,
    pub recipe_id: Option<String>,
    pub recipe_content_hash: Option<String>,
    pub work_order_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionMemoryReferenceInput {
    pub source_id: String,
    pub session_id: String,
    pub question_id: Option<String>,
    pub turn_id: Option<String>,
    pub part_id: Option<String>,
    pub node_id: Option<String>,
    pub node_order: Option<usize>,
    pub reference_key: String,
    pub source_revision: i64,
}

#[derive(Debug, Clone)]
pub struct RecentMemoryEventInput {
    pub category: RecentMemoryEventCategory,
    pub title: String,
    pub summary: String,
    pub occurred_at: String,
    pub source_reference_id: Option<String>,
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct SessionMemorySourceCandidate {
    pub id: String,
    pub source_id: String,
    pub source_fingerprint: Option<String>,
    pub updated_at: Option<String>,
}

pub struct MemoryGenerationPolicy {
    pub enabled: bool,
    pub excluded_session_ids: HashSet<String>,
    pub excluded_source_ids: HashSet<String>,
}

#[derive(Debug, FromRow)]
pub struct SessionMemorySourceCandidateRow {
    pub id: String,
    pub source_id: String,
    pub source_fingerprint: Option<String>,
    pub updated_at: Option<String>,
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
pub struct RecentMemoryEventTargetRow {
    pub session_id: String,
    pub question_id: Option<String>,
    pub turn_id: Option<String>,
    pub node_id: Option<String>,
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
pub struct SessionMemoryJobRow {
    pub tenant_id: String,
    pub id: String,
    pub session_id: String,
    pub source_id: String,
    pub source_revision: i64,
    pub source_fingerprint: String,
    pub contract_version: String,
    pub prompt_version: String,
    pub source_event_id: String,
    pub source_sync_run_id: String,
    pub status: String,
    pub not_before: String,
    pub attempt_count: i64,
    pub last_error: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub ownership_token: Option<String>,
    pub lease_expires_at: Option<String>,
    pub heartbeat_at: Option<String>,
    pub retry_count: i64,
    pub retry_at: Option<String>,
    pub watermark: Option<i64>,
    pub recipe_id: Option<String>,
    pub recipe_revision: Option<i64>,
    pub recipe_content_hash: Option<String>,
    pub budget_policy_version: Option<String>,
    pub work_order_json: Option<String>,
}

impl SessionMemoryJobRow {
    pub fn try_into_job(self) -> StoreResult<SessionMemoryJob> {
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
pub struct SessionMemoryRow {
    pub tenant_id: String,
    pub id: String,
    pub session_id: String,
    pub source_id: String,
    pub source_revision: i64,
    pub source_fingerprint: String,
    pub contract_version: String,
    pub prompt_version: String,
    pub status: String,
    pub project_path: Option<String>,
    pub summary: String,
    pub goal: String,
    pub result: String,
    pub decisions_json: String,
    pub verification_json: String,
    pub blockers_json: String,
    pub follow_up_json: String,
    pub topics_json: String,
    pub generated_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub recipe_id: Option<String>,
    pub recipe_content_hash: Option<String>,
    pub work_order_json: Option<String>,
}

impl SessionMemoryRow {
    pub fn try_into_memory(self) -> StoreResult<SessionMemory> {
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
pub struct SessionMemorySourceReferenceRow {
    pub tenant_id: String,
    pub id: String,
    pub memory_id: String,
    pub source_id: String,
    pub session_id: String,
    pub question_id: Option<String>,
    pub turn_id: Option<String>,
    pub part_id: Option<String>,
    pub node_id: Option<String>,
    pub node_order: Option<i64>,
    pub reference_key: String,
    pub source_revision: i64,
    pub created_at: String,
}

impl SessionMemorySourceReferenceRow {
    pub fn into_reference(self) -> SessionMemorySourceReference {
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
pub struct RecentMemoryEventRow {
    pub tenant_id: String,
    pub id: String,
    pub memory_id: String,
    pub session_id: String,
    pub category: String,
    pub title: String,
    pub summary: String,
    pub occurred_at: String,
    pub source_reference_id: Option<String>,
    pub fingerprint: String,
    pub created_at: String,
}

impl RecentMemoryEventRow {
    pub fn try_into_event(self) -> StoreResult<RecentMemoryEvent> {
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

pub fn decode_string_array(value: &str) -> StoreResult<Vec<String>> {
    serde_json::from_str(value).map_err(StoreError::external)
}

pub fn parse_job_status(value: &str) -> StoreResult<SessionMemoryJobStatus> {
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

pub fn parse_memory_status(value: &str) -> StoreResult<SessionMemoryStatus> {
    match value {
        "active" => Ok(SessionMemoryStatus::Active),
        "invalid" => Ok(SessionMemoryStatus::Invalid),
        "failed" => Ok(SessionMemoryStatus::Failed),
        _ => Err(StoreError::external("invalid Session Memory status")),
    }
}
