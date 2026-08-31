use super::service::AppService;
use crate::backend::{
    ai_execution::{
        execute_agent_blocking, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionPurpose, AiExecutionRequest,
    },
    app_settings,
    dto::{ConversationContentNodeLocator, ConversationSessionDetail},
    models::{RecentMemoryEventCategory, SessionMemory, SessionMemoryJob, SessionMemoryJobStatus},
    runtime::{tasks::TaskContext, AppError, AppResult},
    store::{
        self, RecentMemoryEventInput, SessionMemoryPersistInput, SessionMemoryReferenceInput,
        SESSION_MEMORY_CONTRACT_VERSION, SESSION_MEMORY_PROMPT_VERSION,
    },
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    thread,
    time::Duration as StdDuration,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const SESSION_MEMORY_ACTION: &str = "memory.extraction";
const MAX_EVIDENCE_ITEMS: usize = 512;
const MAX_OUTPUT_ITEMS: usize = 64;
const MAX_ITEM_LENGTH: usize = 4000;
const MAX_AGENT_OUTPUT_LENGTH: usize = 200_000;
const MAX_SESSION_MEMORY_CONCURRENCY: usize = 4;

struct SessionMemoryLeaseGuard {
    stop: CancellationToken,
    join: Option<thread::JoinHandle<()>>,
}

impl SessionMemoryLeaseGuard {
    fn start(
        database: crate::backend::store::Database,
        tenant_id: String,
        job_id: String,
        ownership_token: String,
        task_cancellation: CancellationToken,
    ) -> Self {
        let stop = CancellationToken::new();
        let thread_stop = stop.clone();
        let join = thread::Builder::new()
            .name("aiw-session-memory-heartbeat".to_string())
            .spawn(move || {
                while !thread_stop.is_cancelled() && !task_cancellation.is_cancelled() {
                    thread::sleep(StdDuration::from_secs(1));
                    if thread_stop.is_cancelled() || task_cancellation.is_cancelled() {
                        break;
                    }
                    let now = Utc::now().to_rfc3339();
                    let healthy = database.run_sync(store::heartbeat_session_memory_job_sqlx(
                        database.pool(),
                        &tenant_id,
                        &job_id,
                        &ownership_token,
                        &now,
                        store::SESSION_MEMORY_JOB_LEASE,
                    ));
                    if !healthy.unwrap_or(false) {
                        break;
                    }
                }
            })
            .expect("Session Memory heartbeat thread must start");
        Self {
            stop,
            join: Some(join),
        }
    }
}

impl Drop for SessionMemoryLeaseGuard {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SessionMemoryAgentOutput {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    goal: String,
    #[serde(default)]
    result: String,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    verification: Vec<String>,
    #[serde(default)]
    blockers: Vec<String>,
    #[serde(default, alias = "followUp")]
    follow_up: Vec<String>,
    #[serde(default)]
    topics: Vec<String>,
    #[serde(default, alias = "sourceReferences")]
    source_references: Vec<AgentSourceReference>,
    #[serde(default, alias = "recentEvents")]
    events: Vec<AgentRecentEvent>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentSourceReference {
    #[serde(alias = "referenceKey", alias = "sourceReference")]
    reference_key: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentRecentEvent {
    category: String,
    title: String,
    summary: String,
    #[serde(default, alias = "occurredAt")]
    occurred_at: Option<String>,
    #[serde(default, alias = "sourceReference")]
    source_reference: Option<String>,
    #[serde(default)]
    fingerprint: Option<String>,
}

#[derive(Debug, Clone)]
struct EvidenceReference {
    key: String,
    locator: ConversationContentNodeLocator,
    node_id: Option<String>,
    content: String,
}

#[derive(Debug, Clone, Serialize)]
struct PromptEvidence<'a> {
    reference_key: &'a str,
    locator: &'a ConversationContentNodeLocator,
    content: &'a str,
}

impl AppService {
    pub(crate) fn enqueue_session_memory_jobs_at(
        &self,
        source_id: &str,
        sync_run_id: &str,
        source_revision: i64,
        source_event_id: &str,
        changed_session_ids: Option<&[String]>,
        now: DateTime<Utc>,
    ) -> AppResult<usize> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.runtime
            .run_sync(store::enqueue_session_memory_jobs_sqlx(
                &pool,
                &tenant_id,
                source_id,
                sync_run_id,
                source_revision,
                source_event_id,
                changed_session_ids,
                &now.to_rfc3339(),
            ))
    }

    pub(crate) fn run_session_memory_phase1(
        &self,
        job_id: &str,
    ) -> AppResult<Option<SessionMemory>> {
        self.run_session_memory_phase1_at(job_id, Utc::now())
    }

    pub(crate) fn run_session_memory_phase1_at(
        &self,
        job_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Option<SessionMemory>> {
        let tenant_id = self.tenant_id().to_string();
        self.run_session_memory_phase1_for_tenant_at(
            &tenant_id,
            job_id,
            now,
            TaskContext::detached(),
        )
    }

    pub(crate) fn run_session_memory_phase1_for_tenant_at(
        &self,
        tenant_id: &str,
        job_id: &str,
        now: DateTime<Utc>,
        context: TaskContext,
    ) -> AppResult<Option<SessionMemory>> {
        let now_text = now.to_rfc3339();
        let pool = self.db.pool().clone();
        let job = self
            .runtime
            .run_sync(store::load_session_memory_job_sqlx(
                &pool, tenant_id, job_id,
            ))?
            .ok_or_else(|| AppError::NotFound("Session Memory job not found".to_string()))?;
        if matches!(
            job.status,
            SessionMemoryJobStatus::Succeeded
                | SessionMemoryJobStatus::Skipped
                | SessionMemoryJobStatus::Canceled
                | SessionMemoryJobStatus::Running
        ) {
            return self
                .runtime
                .run_sync(store::load_session_memory_for_job_sqlx(
                    &pool, tenant_id, &job,
                ));
        }

        let (detail, registered_roots) = self.runtime.run_sync(async {
            let detail =
                store::load_conversation_session_detail_sqlx(&pool, tenant_id, &job.session_id)
                    .await?;
            let roots = store::load_sources_sqlx(&pool, tenant_id)
                .await?
                .into_iter()
                .filter_map(|source| source.repo_root)
                .collect::<Vec<_>>();
            Ok::<_, AppError>((detail, roots))
        })?;
        let completed = session_has_completion_signal(&detail);
        let idle_ready = session_idle_ready(&detail, now);
        if !completed && !idle_ready {
            return Ok(None);
        }
        let ownership_token = format!("session-memory-owner-{}", Uuid::new_v4());
        let claimed = self
            .runtime
            .run_sync(store::claim_session_memory_job_with_lease_sqlx(
                &pool,
                tenant_id,
                job_id,
                &now_text,
                completed,
                &ownership_token,
                store::SESSION_MEMORY_JOB_LEASE,
            ))?;
        let Some(job) = claimed else {
            return Ok(None);
        };
        let progress = context.progress();
        progress.progress(0, Some(3), Some("claimed"));

        let lease_guard = SessionMemoryLeaseGuard::start(
            self.db.clone(),
            tenant_id.to_string(),
            job.id.clone(),
            ownership_token.clone(),
            context.cancellation(),
        );
        let result = self.execute_session_memory_agent(&job, &detail, context.cancellation());
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                drop(lease_guard);
                if context.is_cancelled() {
                    self.runtime
                        .run_sync(store::cancel_session_memory_job_sqlx(
                            &pool, tenant_id, job_id, &now_text,
                        ))?;
                    return Err(AppError::Canceled(
                        "Session Memory task was canceled".to_string(),
                    ));
                }
                let code = error
                    .view()
                    .code
                    .chars()
                    .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
                    .collect::<String>();
                self.runtime
                    .run_sync(store::mark_session_memory_job_failed_with_lease_sqlx(
                        &pool,
                        tenant_id,
                        job_id,
                        &ownership_token,
                        if code.is_empty() {
                            "phase1_failed"
                        } else {
                            &code
                        },
                        &now_text,
                    ))?;
                return Err(error);
            }
        };
        progress.progress(1, Some(3), Some("agent_completed"));
        if context.is_cancelled() {
            drop(lease_guard);
            self.runtime
                .run_sync(store::cancel_session_memory_job_sqlx(
                    &pool, tenant_id, job_id, &now_text,
                ))?;
            return Err(AppError::Canceled(
                "Session Memory task was canceled".to_string(),
            ));
        }
        let evidence = build_evidence_references(&detail);
        let project_path = session_project_path(&detail, &registered_roots);
        let persist =
            match validated_persist_input(&job, &output, &evidence, project_path, &now_text) {
                Ok(persist) => persist,
                Err(error) => {
                    drop(lease_guard);
                    self.runtime.run_sync(
                        store::mark_session_memory_job_failed_with_lease_sqlx(
                            &pool,
                            tenant_id,
                            job_id,
                            &ownership_token,
                            "session_memory_validation_failed",
                            &now_text,
                        ),
                    )?;
                    return Err(error);
                }
            };
        progress.progress(2, Some(3), Some("validated"));
        if let Err(error) = self
            .runtime
            .run_sync(store::persist_session_memory_sqlx(&pool, &persist))
        {
            drop(lease_guard);
            if context.is_cancelled() {
                self.runtime
                    .run_sync(store::cancel_session_memory_job_sqlx(
                        &pool, tenant_id, job_id, &now_text,
                    ))?;
                return Err(AppError::Canceled(
                    "Session Memory task was canceled".to_string(),
                ));
            }
            self.runtime
                .run_sync(store::mark_session_memory_job_failed_with_lease_sqlx(
                    &pool,
                    tenant_id,
                    job_id,
                    &ownership_token,
                    "session_memory_persist_failed",
                    &now_text,
                ))?;
            return Err(error);
        }
        drop(lease_guard);
        progress.progress(3, Some(3), Some("persisted"));
        self.runtime
            .run_sync(store::load_session_memory_for_job_sqlx(
                &pool, tenant_id, &job,
            ))
    }

    /// Reconcile durable Session Memory jobs into the in-memory TaskRuntime.
    /// SQLite remains the queue authority; rebuilding or clearing TaskRuntime
    /// only causes this bounded pass to register the work again.
    pub(crate) fn reconcile_session_memory_jobs_for_tenant_at(
        &self,
        tenant_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<usize> {
        let pool = self.db.pool().clone();
        let now_text = now.to_rfc3339();
        self.runtime
            .run_sync(store::recover_expired_session_memory_leases_sqlx(
                &pool, tenant_id, &now_text,
            ))?;
        let job_ids =
            self.runtime
                .run_sync(store::list_session_memory_job_ids_for_scheduler_sqlx(
                    &pool, tenant_id, &now_text, 32,
                ))?;
        let mut scheduled = 0usize;
        for job_id in job_ids {
            if self
                .runtime
                .task_runtime()
                .list(crate::backend::runtime::tasks::TaskFilter {
                    kind: Some(crate::backend::runtime::tasks::TaskKind::Memory),
                    active_only: true,
                })
                .len()
                >= MAX_SESSION_MEMORY_CONCURRENCY
            {
                break;
            }
            let Some(job) = self.runtime.run_sync(store::load_session_memory_job_sqlx(
                &pool, tenant_id, &job_id,
            ))?
            else {
                continue;
            };
            let detail = match self
                .runtime
                .run_sync(store::load_conversation_session_detail_sqlx(
                    &pool,
                    tenant_id,
                    &job.session_id,
                )) {
                Ok(detail) => detail,
                Err(AppError::NotFound(_)) => continue,
                Err(error) => return Err(error),
            };
            let completed = session_has_completion_signal(&detail);
            let idle_ready = session_idle_ready(&detail, now);
            let not_before_ready = DateTime::parse_from_rfc3339(&job.not_before)
                .map(|value| now >= value.with_timezone(&Utc))
                .unwrap_or(false);
            if !completed && (!idle_ready || !not_before_ready) {
                continue;
            }
            let task_id = format!("session-memory-{}-{}", job.id, job.attempt_count);
            let runtime = self.runtime.clone();
            let job_id_for_task = job.id.clone();
            let tenant_id_for_task = tenant_id.to_string();
            let session_id = job.session_id.clone();
            let run_at = now;
            let spec = crate::backend::runtime::tasks::TaskSpec::new(
                crate::backend::runtime::tasks::TaskKind::Memory,
                Some(format!("session-memory-job:{tenant_id}:{job_id}")),
            )
            .with_task_id(task_id)
            .with_tenant_id(tenant_id.to_string())
            .with_conflict_key(format!("session-memory-session:{tenant_id}:{session_id}"));
            let mut spec = spec;
            spec.detail = json!({
                "domain": "session_memory",
                "job_id": job.id,
                "session_id": session_id,
            });
            match self.runtime.task_runtime().spawn(
                spec,
                Box::new(move |context| {
                    AppService::from_runtime(&runtime)
                        .run_session_memory_phase1_for_tenant_at(
                            &tenant_id_for_task,
                            &job_id_for_task,
                            run_at,
                            context,
                        )
                        .map(|memory| {
                            json!({
                                "domain": "session_memory",
                                "job_id": job_id_for_task,
                                "projected": memory.is_some(),
                            })
                        })
                }),
            ) {
                Ok(crate::backend::runtime::tasks::SpawnOutcome::Started) => {
                    scheduled += 1;
                }
                Ok(crate::backend::runtime::tasks::SpawnOutcome::Existing) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(scheduled)
    }

    fn execute_session_memory_agent(
        &self,
        job: &SessionMemoryJob,
        detail: &ConversationSessionDetail,
        cancellation: CancellationToken,
    ) -> AppResult<SessionMemoryAgentOutput> {
        let evidence = build_evidence_references(detail);
        if evidence.is_empty() {
            return Err(AppError::Validation(
                "Session Memory requires canonical Conversation evidence".to_string(),
            ));
        }
        let prompt = build_session_memory_prompt(detail, &evidence)?;
        let settings = app_settings::read_app_settings_value_for_database(&self.db)?;
        let (agent_id, model) = crate::backend::ai_execution::composition::resolve_agent_for(
            &crate::backend::ai_execution::composition::ActionId::new(SESSION_MEMORY_ACTION),
            &settings,
        )?;
        let request = AiExecutionRequest {
            execution_id: format!("session-memory-execution-{}", job.id),
            agent_id,
            purpose: AiExecutionPurpose::SessionMemory,
            session_mode: AgentSessionMode::OneShot,
            prompt,
            model,
            limits: AiExecutionLimits::default(),
            cancellation: AiExecutionCancellation::from_token(cancellation),
            progress: None,
            tenant_id: Some(job.tenant_id.clone()),
            execution_context_key: None,
            binding: None,
            replay: false,
            restore_only: false,
            team_tools: None,
        };
        let result =
            execute_agent_blocking(self.agent_runtime.clone(), request).map_err(|error| {
                let view = error.to_view();
                AppError::Domain {
                    code: view.code,
                    message: view.message,
                    retryable: view.retryable,
                    details: None,
                }
            })?;
        if result.text.chars().count() > MAX_AGENT_OUTPUT_LENGTH {
            return Err(AppError::Validation(
                "Session Memory Agent output is too large".to_string(),
            ));
        }
        let json_text = strip_json_fence(&result.text);
        serde_json::from_str(json_text)
            .map_err(|_| AppError::Validation("Session Memory Agent output is invalid".to_string()))
    }
}

fn build_session_memory_prompt(
    detail: &ConversationSessionDetail,
    evidence: &[EvidenceReference],
) -> AppResult<String> {
    let title = crate::backend::memory_redaction::redact_memory_text(&detail.session.title).text;
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

fn build_evidence_references(detail: &ConversationSessionDetail) -> Vec<EvidenceReference> {
    let mut references = Vec::new();
    for question in &detail.questions {
        for node in &question.projected_content_nodes {
            let key = format!("node:{}", node.node_id);
            let content = crate::backend::memory_redaction::redact_memory_text(&node.content).text;
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
                    crate::backend::memory_redaction::redact_memory_text(&turn.user_text).text;
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

fn validated_persist_input(
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
    for reference in output.source_references.iter().take(MAX_OUTPUT_ITEMS) {
        let key = reference.reference_key.trim();
        let Some(evidence) = evidence_by_key.get(key) else {
            return Err(AppError::Validation(
                "Session Memory contains an unknown source reference".to_string(),
            ));
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
    if references.is_empty() {
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
            .map(str::to_string);
        if let Some(reference_key) = source_reference_id.as_deref() {
            if !reference_ids.contains_key(reference_key) {
                return Err(AppError::Validation(
                    "Recent Event must cite a Session Memory source reference".to_string(),
                ));
            }
        }
        let fingerprint = event
            .fingerprint
            .as_deref()
            .map(|value| crate::backend::memory_redaction::redact_memory_text(value).text)
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
    })
}

fn session_memory_id(job: &SessionMemoryJob) -> String {
    format!(
        "session-memory-{}",
        digest(&format!(
            "{}\0{}\0{}",
            job.tenant_id, job.id, job.source_revision
        ))
    )
}

fn session_memory_reference_id(memory_id: &str, reference_key: &str) -> String {
    format!(
        "session-memory-ref-{}",
        digest(&format!("{memory_id}\0{reference_key}"))
    )
}

fn encode_output_list(values: &[String]) -> AppResult<String> {
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

fn clean_output_text(value: &str, max_length: usize, field: &str) -> AppResult<String> {
    let value = crate::backend::memory_redaction::redact_memory_text(value).text;
    let value = value.trim();
    if value.chars().count() > max_length {
        return Err(AppError::Validation(format!("{field} is too long")));
    }
    Ok(value.to_string())
}

fn session_has_completion_signal(detail: &ConversationSessionDetail) -> bool {
    detail
        .questions
        .iter()
        .flat_map(|question| question.parts.iter())
        .filter_map(|part| part.metadata_json.as_deref())
        .filter_map(|metadata| serde_json::from_str::<Value>(metadata).ok())
        .any(|metadata| value_marks_completion(&metadata))
}

fn session_project_path(
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
        .and_then(|path| super::recent::resolve_project_directory(path, registered_roots))
}

fn value_marks_completion(value: &Value) -> bool {
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

fn is_completion_word(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "complete" | "completed" | "done" | "success" | "succeeded"
    )
}

fn session_idle_ready(detail: &ConversationSessionDetail, now: DateTime<Utc>) -> bool {
    detail
        .session
        .updated_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .is_some_and(|updated| now >= updated.with_timezone(&Utc) + Duration::minutes(30))
}

fn strip_json_fence(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix("```json")
        .or_else(|| value.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(value)
}

fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        agents::types::AgentProtocol,
        ai_execution::{
            executor::BackendFuture, AgentExecutionRuntime, AiExecutionRequest, AiExecutionResult,
        },
        models::{
            ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
            ConversationPartKind, ConversationPartRole, ConversationSource, ConversationSourceKind,
            NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
        },
    };
    use std::sync::{Arc, Mutex};

    struct FakeRuntime {
        result_text: Mutex<String>,
        requests: Mutex<Vec<AiExecutionRequest>>,
    }

    impl FakeRuntime {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                result_text: Mutex::new("{}".to_string()),
                requests: Mutex::new(Vec::new()),
            })
        }

        fn set_result(&self, result_text: String) {
            *self.result_text.lock().expect("fake result lock") = result_text;
        }
    }

    impl AgentExecutionRuntime for FakeRuntime {
        fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
            let result_text = self.result_text.lock().expect("fake result lock").clone();
            self.requests
                .lock()
                .expect("fake request lock")
                .push(request.clone());
            Box::pin(async move {
                Ok(AiExecutionResult {
                    text: result_text,
                    agent_id: request.agent_id,
                    protocol: AgentProtocol::Acp,
                    requested_model: request.model,
                    elapsed_ms: 1,
                    persistent_binding: None,
                    replay_text: None,
                })
            })
        }
    }

    #[test]
    fn completion_signal_is_provider_neutral_and_requires_an_explicit_value() {
        let completed = serde_json::json!({ "session_status": "completed" });
        let pending = serde_json::json!({ "status": "completed-command" });
        assert!(value_marks_completion(&completed));
        assert!(!value_marks_completion(&pending));
    }

    #[test]
    fn json_fence_is_removed_without_touching_payload() {
        assert_eq!(strip_json_fence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_json_fence("```\n{\"a\":1}\n```"), "{\"a\":1}");
    }

    #[test]
    fn phase1_worker_honors_idle_boundary_persists_redacted_output_and_is_idempotent() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-session-memory-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create session memory fixture root");
        let db_path = root.join("app.db");
        let fake = FakeRuntime::new();
        let service = AppService::open_with_db_path_and_runtime(db_path.clone(), fake.clone())
            .expect("open app service with fake agent");
        let timestamp = "2026-08-30T23:00:00Z";
        let adapter = ConversationAdapter {
            id: "session-memory-fixture-adapter".to_string(),
            name: "Session Memory Fixture Agent".to_string(),
            kind: ConversationAdapterKind::External,
            version: "1.0.0".to_string(),
            enabled: true,
            manifest_path: None,
            executable_path: None,
            content_hash: None,
            trusted_hash: None,
            trust_state: ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["read_session".to_string()],
            input_kinds: vec![ConversationSourceKind::Directory],
            card_contract_version: None,
            card_kinds: Vec::new(),
            created_at: timestamp.to_string(),
            updated_at: timestamp.to_string(),
        };
        let source = ConversationSource {
            id: "session-memory-fixture-source".to_string(),
            adapter_id: adapter.id.clone(),
            name: "Session Memory Fixture Source".to_string(),
            kind: ConversationSourceKind::Directory,
            location: root.to_string_lossy().to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: timestamp.to_string(),
            updated_at: timestamp.to_string(),
        };
        let session = NormalizedConversationSession {
            external_id: "session-memory-fixture".to_string(),
            title: Some("Session Memory Fixture".to_string()),
            project_path: None,
            started_at: Some("2026-08-30T22:00:00Z".to_string()),
            updated_at: Some(timestamp.to_string()),
            source_locator: Some("fixture://session-memory".to_string()),
            source_fingerprint: Some("fixture-revision-1".to_string()),
            turns: vec![NormalizedConversationTurn {
                external_id: "turn-1".to_string(),
                turn_index: 0,
                user_text: "Implement the Session Memory fixture".to_string(),
                title: None,
                started_at: Some(timestamp.to_string()),
                ended_at: Some(timestamp.to_string()),
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some("The fixture was implemented.".to_string()),
                    language: None,
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                }],
            }],
        };
        let pool = service.db.pool().clone();
        let source_for_import = source.clone();
        let adapter_for_import = adapter.clone();
        service
            .runtime
            .run_sync(async move {
                crate::backend::store::upsert_conversation_adapter_sqlx(
                    &pool,
                    "default",
                    &adapter_for_import,
                )
                .await?;
                crate::backend::store::upsert_conversation_source_sqlx(
                    &pool,
                    "default",
                    &source_for_import,
                )
                .await?;
                crate::backend::store::import_conversation_sessions_sqlx(
                    &pool,
                    "default",
                    &source_for_import,
                    &[session],
                    false,
                )
                .await
                .map(|_| ())
            })
            .expect("import canonical conversation fixture");

        let session_id: String = service.runtime.run_sync(sqlx::query_scalar(
            "SELECT id FROM conversation_sessions WHERE tenant_id = 'default' AND external_id = 'session-memory-fixture'",
        ).fetch_one(service.db.pool())).expect("load imported session id");
        let detail = service
            .runtime
            .run_sync(
                crate::backend::store::load_conversation_session_detail_sqlx(
                    service.db.pool(),
                    "default",
                    &session_id,
                ),
            )
            .expect("load canonical session detail");
        let reference_key = detail.questions[0]
            .projected_content_nodes
            .first()
            .map(|node| format!("node:{}", node.node_id))
            .unwrap_or_else(|| format!("turn:{}", detail.questions[0].turns[0].id));
        let secret = "ghp_12345678901234567890";
        let events = RecentMemoryEventCategory::ALL
            .iter()
            .enumerate()
            .map(|(index, category)| {
                json!({
                    "category": category.as_str(),
                    "title": format!("Event {index}"),
                    "summary": format!("Event summary {index}"),
                    "source_reference": reference_key.clone(),
                    "fingerprint": format!("event-{index}"),
                })
            })
            .collect::<Vec<_>>();
        fake.set_result(
            json!({
                "summary": format!("Completed with {secret}"),
                "goal": "Create a revision-bound memory",
                "result": "Fixture persisted",
                "decisions": ["Use canonical Conversation evidence"],
                "verification": ["Six Recent Event categories validated"],
                "blockers": [],
                "follow_up": ["Review the generated locator"],
                "topics": ["memory"],
                "source_references": [{ "reference_key": reference_key.clone() }],
                "events": events,
            })
            .to_string(),
        );
        let now = DateTime::parse_from_rfc3339("2026-08-30T23:00:00Z")
            .expect("parse controlled clock")
            .with_timezone(&Utc);
        assert_eq!(
            service
                .enqueue_session_memory_jobs_at(
                    &source.id,
                    "sync-session-memory",
                    1,
                    "event-session-memory",
                    Some(std::slice::from_ref(&session_id)),
                    now,
                )
                .expect("enqueue phase1 job"),
            1
        );
        let job_id: String = service.runtime.run_sync(sqlx::query_scalar(
            "SELECT id FROM session_memory_jobs WHERE tenant_id = 'default' AND session_id = ?1",
        ).bind(&session_id).fetch_one(service.db.pool())).expect("load phase1 job id");
        assert!(service
            .run_session_memory_phase1_at(
                &job_id,
                now + Duration::minutes(29) + Duration::seconds(59),
            )
            .expect("idle boundary before deadline")
            .is_none());
        let memory = service
            .run_session_memory_phase1_at(&job_id, now + Duration::minutes(30))
            .expect("run phase1 worker")
            .expect("phase1 memory result");
        assert_eq!(memory.source_revision, 1);
        assert!(!memory.summary.contains(secret));
        assert!(memory.summary.contains("[REDACTED:api_key]"));

        assert_eq!(
            service
                .enqueue_session_memory_jobs_at(
                    &source.id,
                    "sync-session-memory-scheduler",
                    2,
                    "event-session-memory-scheduler",
                    Some(std::slice::from_ref(&session_id)),
                    now + Duration::minutes(30),
                )
                .expect("enqueue scheduler phase1 job"),
            1
        );
        let scheduled_job_id: String = service.runtime.run_sync(sqlx::query_scalar(
            "SELECT id FROM session_memory_jobs WHERE tenant_id = 'default' AND session_id = ?1 AND source_revision = 2",
        ).bind(&session_id).fetch_one(service.db.pool())).expect("load scheduler job id");
        let scheduled_task_id = format!("session-memory-{}-0", scheduled_job_id);
        assert_eq!(
            service
                .reconcile_session_memory_jobs_for_tenant_at("default", now + Duration::minutes(30))
                .expect("schedule durable phase1 job"),
            1
        );
        for _ in 0..100 {
            let status: String = service.runtime.run_sync(sqlx::query_scalar(
                "SELECT status FROM session_memory_jobs WHERE tenant_id = 'default' AND id = ?1",
            ).bind(&scheduled_job_id).fetch_one(service.db.pool())).expect("read scheduled job status");
            if status == "succeeded" {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let scheduled_status: String = service.runtime.run_sync(sqlx::query_scalar(
            "SELECT status FROM session_memory_jobs WHERE tenant_id = 'default' AND id = ?1",
        ).bind(&scheduled_job_id).fetch_one(service.db.pool())).expect("read completed scheduled job");
        assert_eq!(scheduled_status, "succeeded");
        let scheduled_task = (0..100)
            .find_map(|_| {
                let snapshot = service.runtime.task_runtime().get(&scheduled_task_id)?;
                if snapshot.progress.as_ref().map(|value| value.current) == Some(3) {
                    Some(snapshot)
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    None
                }
            })
            .expect("read scheduled TaskRuntime projection");
        assert_eq!(
            scheduled_task.progress.as_ref().map(|value| value.current),
            Some(3)
        );

        let row_counts = service.runtime.run_sync(async {
            let jobs = crate::backend::store::count_session_memory_rows_sqlx(
                service.db.pool(),
                "default",
                "jobs",
            )
            .await?;
            let memories = crate::backend::store::count_session_memory_rows_sqlx(
                service.db.pool(),
                "default",
                "memories",
            )
            .await?;
            let references = crate::backend::store::count_session_memory_rows_sqlx(
                service.db.pool(),
                "default",
                "references",
            )
            .await?;
            let events = crate::backend::store::count_session_memory_rows_sqlx(
                service.db.pool(),
                "default",
                "events",
            )
            .await?;
            Ok::<_, AppError>((jobs, memories, references, events))
        });
        assert_eq!(row_counts.expect("count phase1 rows"), (2, 2, 2, 12));
        let raw_output: String = service
            .runtime
            .run_sync(sqlx::query_scalar(
                "SELECT raw_output_json FROM session_memories WHERE tenant_id = 'default' AND session_id = ?1",
            )
            .bind(&session_id)
            .fetch_one(service.db.pool()))
            .expect("read sanitized phase1 output");
        assert!(!raw_output.contains(secret));
        assert!(raw_output.contains("[REDACTED:api_key]"));
        assert_eq!(
            service
                .enqueue_session_memory_jobs_at(
                    &source.id,
                    "sync-session-memory-replay",
                    1,
                    "event-session-memory-replay",
                    Some(std::slice::from_ref(&session_id)),
                    now + Duration::minutes(31),
                )
                .expect("replay phase1 event"),
            0
        );
        let recent = service
            .list_recent_conversation_sessions_at(
                crate::backend::application::RecentConversationSessionListParams::default(),
                now + Duration::minutes(31),
            )
            .expect("read Recent projection");
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].recent_events.len(), 6);
        let requests = fake.requests.lock().expect("fake request lock");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].purpose, AiExecutionPurpose::SessionMemory);
        assert!(!requests[0].prompt.contains(secret));
        drop(requests);
        drop(service);
        let _ = std::fs::remove_dir_all(root);
    }
}
