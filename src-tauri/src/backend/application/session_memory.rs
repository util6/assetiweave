use super::service::AppService;
use crate::backend::{
    ai_execution::{
        execute_agent, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest, SessionCleanupStatus,
    },
    dto::{AgentSessionRef, ConversationContentNodeLocator, ConversationSessionDetail},
    evidence::{
        build_bounded_evidence_initial_pack, BoundedEvidenceInitialPack, BoundedEvidenceNode,
        EvidenceNodeKind, EvidenceReadStatus, ShortEvidenceRef,
    },
    models::{
        BoundedMemoryBudgetPolicy, ConversationPartRole, MemoryExecutionWorkOrder, MemoryRecipe,
        NormalizedConversationPart, NormalizedConversationSession, NormalizedConversationTurn,
        RecentMemoryEventCategory, SessionMemory, SessionMemoryJob, SessionMemoryJobStatus,
    },
    runtime::{
        tasks::{
            StageStatus, TaskActivity, TaskCapabilities, TaskContext, TaskFailure, TaskMetric,
            TaskOutcome, TaskStage,
        },
        AppError, AppResult,
    },
    store::{
        self, RecentMemoryEventInput, SessionMemoryPersistInput, SessionMemoryReferenceInput,
        SESSION_MEMORY_CONTRACT_VERSION, SESSION_MEMORY_PROMPT_VERSION,
    },
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
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

fn is_error_retryable(error: &AppError) -> bool {
    match error {
        AppError::Validation(_) => false,
        AppError::Domain {
            retryable, code, ..
        } => {
            if !*retryable {
                return false;
            }
            !matches!(
                code.as_str(),
                "agent_not_found"
                    | "tool_use_denied"
                    | "model_not_found"
                    | "protocol_unsupported"
                    | "config_invalid"
            )
        }
        AppError::Cancelled(_) => false,
        _ => true,
    }
}

struct SessionMemoryAgentExecutionResult {
    raw_text: String,
    short_refs: std::collections::HashMap<String, ShortEvidenceRef>,
    pack: BoundedEvidenceInitialPack,
    session_cleanup: SessionCleanupStatus,
    is_empty: bool,
}

fn sanitize_memory_failure(
    code: &str,
    raw_message: &str,
    stage: &str,
    retryable: bool,
) -> TaskFailure {
    let sanitized_code = if code.is_empty() {
        "memory_execution_failed".to_string()
    } else {
        code.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .take(64)
            .collect::<String>()
    };
    let safe_message = if raw_message.contains("prompt")
        || raw_message.contains("bearer")
        || raw_message.contains("token")
        || raw_message.contains("secret")
    {
        "执行过程中发生受控错误".to_string()
    } else {
        raw_message.chars().take(200).collect::<String>()
    };
    TaskFailure {
        code: sanitized_code,
        message: safe_message,
        stage: stage.to_string(),
        identity: None,
        retryable,
        path: None,
        timestamp: Utc::now().to_rfc3339(),
    }
}

use crate::backend::application::memory_agent_session::{
    ActiveMemoryAgentSession, MemoryAgentSessionParams,
};

struct SessionMemoryLeaseGuard {
    task: tokio::task::JoinHandle<()>,
}

impl SessionMemoryLeaseGuard {
    fn start(
        database: crate::backend::store::Database,
        tenant_id: String,
        job_id: String,
        ownership_token: String,
        task_cancellation: CancellationToken,
    ) -> Self {
        let task = tokio::spawn(async move {
            let pool = database.pool().clone();
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(StdDuration::from_secs(1)) => {
                        if task_cancellation.is_cancelled() {
                            break;
                        }
                        let now = Utc::now().to_rfc3339();
                        let healthy = store::heartbeat_session_memory_job_sqlx(
                            &pool,
                            &tenant_id,
                            &job_id,
                            &ownership_token,
                            &now,
                            store::SESSION_MEMORY_JOB_LEASE,
                        )
                        .await;
                        if !healthy.unwrap_or(false) {
                            break;
                        }
                    }
                    _ = task_cancellation.cancelled() => {
                        break;
                    }
                }
            }
        });
        Self { task }
    }
}

impl Drop for SessionMemoryLeaseGuard {
    fn drop(&mut self) {
        self.task.abort();
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
pub(crate) struct EvidenceReference {
    pub(crate) key: String,
    pub(crate) locator: ConversationContentNodeLocator,
    pub(crate) node_id: Option<String>,
    pub(crate) content: String,
}

#[derive(Debug, Clone, Serialize)]
struct PromptEvidence<'a> {
    reference_key: &'a str,
    locator: &'a ConversationContentNodeLocator,
    content: &'a str,
}

impl AppService {
    #[cfg(test)]
    pub(crate) async fn enqueue_session_memory_jobs_at(
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
        let internal_agent_workspace =
            crate::backend::ai_execution::agent_execution_workspace_root(&self.db_path);
        store::enqueue_session_memory_jobs_sqlx(
            &pool,
            &tenant_id,
            source_id,
            sync_run_id,
            source_revision,
            source_event_id,
            changed_session_ids,
            &internal_agent_workspace,
            &now.to_rfc3339(),
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn run_session_memory_phase1_at(
        &self,
        job_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<Option<SessionMemory>> {
        let tenant_id = self.tenant_id().to_string();
        self.run_session_memory_phase1_for_tenant_at(
            &tenant_id,
            job_id,
            now,
            TaskContext::untracked(),
        )
        .await
    }

    pub(crate) async fn run_session_memory_phase1_for_tenant_at(
        &self,
        tenant_id: &str,
        job_id: &str,
        now: DateTime<Utc>,
        context: TaskContext,
    ) -> AppResult<Option<SessionMemory>> {
        let now_text = now.to_rfc3339();
        let pool = self.db.pool().clone();
        let job = store::load_session_memory_job_sqlx(&pool, tenant_id, job_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Session Memory job not found".to_string()))?;
        let settings = self.backend_settings()?;
        if !settings.is_memory_generation_enabled()
            || settings.is_session_excluded(&job.session_id)
            || settings.is_source_excluded(&job.source_id)
        {
            store::cancel_session_memory_job_sqlx(&pool, tenant_id, job_id, &now_text).await?;
            return Ok(None);
        }
        if matches!(
            job.status,
            SessionMemoryJobStatus::Succeeded
                | SessionMemoryJobStatus::Skipped
                | SessionMemoryJobStatus::Canceled
                | SessionMemoryJobStatus::Running
        ) {
            return store::load_session_memory_for_job_sqlx(&pool, tenant_id, &job).await;
        }

        let progress = context.progress();
        let stages = vec![
            TaskStage {
                id: "claim".to_string(),
                name: "领取工作与租约".to_string(),
                status: StageStatus::Pending,
                started_at: None,
                finished_at: None,
                duration_ms: None,
                progress: None,
                current_activities: Vec::new(),
                metrics: Vec::new(),
                failures: Vec::new(),
                skipped: Vec::new(),
                agent_session_ref: None,
            },
            TaskStage {
                id: "load_facts".to_string(),
                name: "加载上下文与事实".to_string(),
                status: StageStatus::Pending,
                started_at: None,
                finished_at: None,
                duration_ms: None,
                progress: None,
                current_activities: Vec::new(),
                metrics: Vec::new(),
                failures: Vec::new(),
                skipped: Vec::new(),
                agent_session_ref: None,
            },
            TaskStage {
                id: "agent_execution".to_string(),
                name: "调用 Agent 提取".to_string(),
                status: StageStatus::Pending,
                started_at: None,
                finished_at: None,
                duration_ms: None,
                progress: None,
                current_activities: Vec::new(),
                metrics: Vec::new(),
                failures: Vec::new(),
                skipped: Vec::new(),
                agent_session_ref: None,
            },
            TaskStage {
                id: "validation".to_string(),
                name: "校验记忆卡片".to_string(),
                status: StageStatus::Pending,
                started_at: None,
                finished_at: None,
                duration_ms: None,
                progress: None,
                current_activities: Vec::new(),
                metrics: Vec::new(),
                failures: Vec::new(),
                skipped: Vec::new(),
                agent_session_ref: None,
            },
            TaskStage {
                id: "publish".to_string(),
                name: "持久化与发布".to_string(),
                status: StageStatus::Pending,
                started_at: None,
                finished_at: None,
                duration_ms: None,
                progress: None,
                current_activities: Vec::new(),
                metrics: Vec::new(),
                failures: Vec::new(),
                skipped: Vec::new(),
                agent_session_ref: None,
            },
            TaskStage {
                id: "cleanup_session".to_string(),
                name: "清理 Agent 会话".to_string(),
                status: StageStatus::Pending,
                started_at: None,
                finished_at: None,
                duration_ms: None,
                progress: None,
                current_activities: Vec::new(),
                metrics: Vec::new(),
                failures: Vec::new(),
                skipped: Vec::new(),
                agent_session_ref: None,
            },
        ];
        progress.set_stages(stages);

        let worker_id = format!("memory:{}", job_id);
        progress.update_stage_status("claim", StageStatus::Running);
        progress.record_activity(TaskActivity {
            stage_id: "claim".to_string(),
            worker_id: worker_id.clone(),
            operation: "claim_lease".to_string(),
            path: None,
            display_path: None,
            started_at: Utc::now().to_rfc3339(),
            current: Some(0),
            total: None,
        });

        let detail =
            store::load_conversation_session_detail_sqlx(&pool, tenant_id, &job.session_id).await?;
        let roots = store::load_sources_sqlx(&pool, tenant_id)
            .await?
            .into_iter()
            .filter_map(|source| source.repo_root)
            .collect::<Vec<_>>();
        let registered_roots = roots;

        let completed = session_has_completion_signal(&detail);
        let idle_ready = session_idle_ready(&detail, now);
        if !completed && !idle_ready {
            progress.finish_stage(
                "claim",
                StageStatus::Skipped,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            progress.remove_activity("claim", &worker_id);
            return Ok(None);
        }
        let ownership_token = format!("session-memory-owner-{}", Uuid::new_v4());
        let claimed = store::claim_session_memory_job_with_lease_sqlx(
            &pool,
            tenant_id,
            job_id,
            &now_text,
            completed,
            &ownership_token,
            store::SESSION_MEMORY_JOB_LEASE,
        )
        .await?;
        let Some(job) = claimed else {
            progress.finish_stage(
                "claim",
                StageStatus::Skipped,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            progress.remove_activity("claim", &worker_id);
            return Ok(None);
        };
        progress.finish_stage(
            "claim",
            StageStatus::Succeeded,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        progress.remove_activity("claim", &worker_id);
        progress.progress(0, Some(3), Some("claimed"));

        // Stage 2: load_facts
        progress.update_stage_status("load_facts", StageStatus::Running);
        progress.record_activity(TaskActivity {
            stage_id: "load_facts".to_string(),
            worker_id: worker_id.clone(),
            operation: "load_conversation_facts".to_string(),
            path: None,
            display_path: None,
            started_at: Utc::now().to_rfc3339(),
            current: Some(1),
            total: Some(1),
        });
        progress.finish_stage(
            "load_facts",
            StageStatus::Succeeded,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        progress.remove_activity("load_facts", &worker_id);

        let lease_guard = SessionMemoryLeaseGuard::start(
            self.db.clone(),
            tenant_id.to_string(),
            job.id.clone(),
            ownership_token.clone(),
            context.cancellation(),
        );

        // Stage 3: agent_execution
        let session_ref = AgentSessionRef::new(format!("sm-{}", job.id));
        progress.set_stage_agent_session_ref("agent_execution", Some(session_ref.clone()));
        progress.update_stage_status("agent_execution", StageStatus::Running);
        progress.record_activity(TaskActivity {
            stage_id: "agent_execution".to_string(),
            worker_id: worker_id.clone(),
            operation: "extract_session_memory".to_string(),
            path: None,
            display_path: None,
            started_at: Utc::now().to_rfc3339(),
            current: Some(0),
            total: None,
        });

        let (agent_id, model) = crate::backend::ai_execution::composition::resolve_agent_for(
            &crate::backend::ai_execution::composition::ActionId::new(SESSION_MEMORY_ACTION),
            &self.app_settings_value(),
        )
        .map(|(id, m)| (id.to_string(), m))
        .unwrap_or_else(|_| ("builtin:assistant".to_string(), None));

        let memory_session = ActiveMemoryAgentSession::start(
            &self.runtime,
            MemoryAgentSessionParams {
                tenant_id,
                scope: "session",
                job_id: &job.id,
                task_id: Some(context.task_id()),
                agent_id: &agent_id,
                display_name: Some("Session Memory Agent".to_string()),
                model,
                prompt_summary: "提取会话记忆与事实上下文",
                custom_session_ref: Some(session_ref.clone()),
                persistent: false,
            },
        );

        let result = self
            .execute_session_memory_agent(
                &job,
                &detail,
                context.cancellation(),
                Some(memory_session.sink.clone()),
            )
            .await;
        let execution = match result {
            Ok(output) => {
                memory_session.finish_succeeded(&self.runtime);
                progress.finish_stage(
                    "agent_execution",
                    StageStatus::Succeeded,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
                progress.remove_activity("agent_execution", &worker_id);
                output
            }
            Err(error) => {
                drop(lease_guard);
                progress.remove_activity("agent_execution", &worker_id);
                if context.is_cancelled() {
                    memory_session.finish_cancelled(&self.runtime);
                    progress.finish_stage(
                        "agent_execution",
                        StageStatus::Canceled,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    );
                    progress.set_outcome(
                        TaskOutcome::Canceled,
                        None,
                        Some("任务已被取消".to_string()),
                    );
                    store::cancel_session_memory_job_sqlx(&pool, tenant_id, job_id, &now_text)
                        .await?;
                    return Err(AppError::Cancelled(
                        "Session Memory task was canceled".to_string(),
                    ));
                }
                let code = error
                    .view()
                    .code
                    .chars()
                    .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
                    .collect::<String>();
                let failure_code = if code.is_empty() {
                    "phase1_failed".to_string()
                } else {
                    code
                };
                let retryable = is_error_retryable(&error);
                let safe_failure = sanitize_memory_failure(
                    &failure_code,
                    &error.to_string(),
                    "agent_execution",
                    retryable,
                );
                memory_session.finish_failed(
                    &self.runtime,
                    &failure_code,
                    &safe_failure.message,
                    false,
                );
                progress.finish_stage(
                    "agent_execution",
                    StageStatus::Failed,
                    Vec::new(),
                    vec![safe_failure.clone()],
                    Vec::new(),
                );
                progress.set_outcome(TaskOutcome::Failure, None, Some(safe_failure.message));
                store::mark_session_memory_job_failed_with_lease_sqlx(
                    &pool,
                    tenant_id,
                    job_id,
                    &ownership_token,
                    &failure_code,
                    &now_text,
                    retryable,
                )
                .await?;
                return Err(error);
            }
        };
        progress.progress(1, Some(3), Some("agent_completed"));
        if context.is_cancelled() {
            drop(lease_guard);
            progress.finish_stage(
                "validation",
                StageStatus::Canceled,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            progress.set_outcome(
                TaskOutcome::Canceled,
                None,
                Some("任务已被取消".to_string()),
            );
            store::cancel_session_memory_job_sqlx(&pool, tenant_id, job_id, &now_text).await?;
            return Err(AppError::Cancelled(
                "Session Memory task was canceled".to_string(),
            ));
        }

        // Stage 4: validation
        progress.update_stage_status("validation", StageStatus::Running);
        let output = if execution.is_empty {
            SessionMemoryAgentOutput {
                summary: "No content available in this session.".to_string(),
                goal: String::new(),
                result: String::new(),
                decisions: Vec::new(),
                verification: Vec::new(),
                blockers: Vec::new(),
                follow_up: Vec::new(),
                topics: Vec::new(),
                source_references: Vec::new(),
                events: Vec::new(),
            }
        } else {
            let parsed_output = parse_session_memory_agent_output(&execution.raw_text);
            let mut parsed = match parsed_output {
                Ok(out) => out,
                Err(err) => {
                    drop(lease_guard);
                    let line = err.line();
                    let column = err.column();
                    let category = match err.classify() {
                        serde_json::error::Category::Io => "io",
                        serde_json::error::Category::Syntax => "syntax",
                        serde_json::error::Category::Data => "data",
                        serde_json::error::Category::Eof => "eof",
                    };
                    let raw_len = execution.raw_text.len();
                    let raw_sha256 = digest(&execution.raw_text);
                    let err_msg = format!(
                        "Session Memory Agent output JSON validation failed: {err} (cat={category}, line={line}, col={column}, len={raw_len}, sha={:.8})",
                        raw_sha256
                    );
                    let safe_failure = sanitize_memory_failure(
                        "session_memory_validation_failed",
                        &err_msg,
                        "validation",
                        false,
                    );
                    progress.finish_stage(
                        "validation",
                        StageStatus::Failed,
                        Vec::new(),
                        vec![safe_failure.clone()],
                        Vec::new(),
                    );
                    progress.set_outcome(TaskOutcome::Failure, None, Some(safe_failure.message));
                    store::mark_session_memory_job_failed_with_lease_sqlx(
                        &pool,
                        tenant_id,
                        job_id,
                        &ownership_token,
                        "session_memory_validation_failed",
                        &now_text,
                        false,
                    )
                    .await?;
                    return Err(AppError::Validation(format!(
                        "Session Memory Agent output is invalid: {err_msg}"
                    )));
                }
            };

            // 准入校验与事实过滤：
            // 1. 过滤被用户更正/否决的提案 (E02)
            let corrections: Vec<BoundedEvidenceNode> = execution
                .pack
                .intent_and_corrections
                .iter()
                .filter(|n| n.kind == EvidenceNodeKind::UserCorrection)
                .cloned()
                .collect();
            parsed.decisions = sanitize_decisions_with_corrections(parsed.decisions, &corrections);

            // 2. 检查是否有 VerificationEvidence，无则过滤虚假通过 (E03)
            let has_verification_evidence = execution
                .pack
                .outcomes_and_verifications
                .iter()
                .any(|n| n.kind == EvidenceNodeKind::VerificationEvidence);
            parsed.verification = sanitize_verifications_with_evidence(
                parsed.verification,
                has_verification_evidence,
            );

            parsed
        };

        let evidence = if !execution.short_refs.is_empty() {
            let mut refs = build_bounded_evidence_references(&detail, &execution.short_refs);
            refs.extend(build_evidence_references(&detail));
            refs
        } else {
            build_evidence_references(&detail)
        };
        let project_path = session_project_path(&detail, &registered_roots);
        let persist =
            match validated_persist_input(&job, &output, &evidence, project_path, &now_text) {
                Ok(persist) => {
                    progress.finish_stage(
                        "validation",
                        StageStatus::Succeeded,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    );
                    persist
                }
                Err(error) => {
                    drop(lease_guard);
                    let safe_failure = sanitize_memory_failure(
                        "session_memory_validation_failed",
                        &error.to_string(),
                        "validation",
                        false,
                    );
                    progress.finish_stage(
                        "validation",
                        StageStatus::Failed,
                        Vec::new(),
                        vec![safe_failure.clone()],
                        Vec::new(),
                    );
                    progress.set_outcome(TaskOutcome::Failure, None, Some(safe_failure.message));
                    store::mark_session_memory_job_failed_with_lease_sqlx(
                        &pool,
                        tenant_id,
                        job_id,
                        &ownership_token,
                        "session_memory_validation_failed",
                        &now_text,
                        false,
                    )
                    .await?;
                    return Err(error);
                }
            };
        progress.progress(2, Some(3), Some("validated"));

        // Stage 5: publish
        progress.update_stage_status("publish", StageStatus::Running);
        if let Err(error) = store::persist_session_memory_sqlx(&pool, &persist).await {
            drop(lease_guard);
            if context.is_cancelled() {
                progress.finish_stage(
                    "publish",
                    StageStatus::Canceled,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
                progress.set_outcome(
                    TaskOutcome::Canceled,
                    None,
                    Some("任务已被取消".to_string()),
                );
                store::cancel_session_memory_job_sqlx(&pool, tenant_id, job_id, &now_text).await?;
                return Err(AppError::Cancelled(
                    "Session Memory task was canceled".to_string(),
                ));
            }
            let safe_failure = sanitize_memory_failure(
                "session_memory_persist_failed",
                &error.to_string(),
                "publish",
                true,
            );
            progress.finish_stage(
                "publish",
                StageStatus::Failed,
                Vec::new(),
                vec![safe_failure.clone()],
                Vec::new(),
            );
            progress.set_outcome(TaskOutcome::Failure, None, Some(safe_failure.message));
            store::mark_session_memory_job_failed_with_lease_sqlx(
                &pool,
                tenant_id,
                job_id,
                &ownership_token,
                "session_memory_persist_failed",
                &now_text,
                true,
            )
            .await?;
            return Err(error);
        }
        drop(lease_guard);
        progress.finish_stage(
            "publish",
            StageStatus::Succeeded,
            vec![TaskMetric {
                code: "memories_created".to_string(),
                value: 1,
            }],
            Vec::new(),
            Vec::new(),
        );

        // Stage 6: cleanup_session
        progress.update_stage_status("cleanup_session", StageStatus::Running);
        match execution.session_cleanup {
            SessionCleanupStatus::Deleted => {
                progress.finish_stage(
                    "cleanup_session",
                    StageStatus::Succeeded,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
            }
            SessionCleanupStatus::Unsupported => {
                tracing::warn!(
                    action = "session_memory.cleanup_session",
                    job_id = %job.id,
                    "Agent backend reported session deletion unsupported; continuing with partial success"
                );
                progress.finish_stage(
                    "cleanup_session",
                    StageStatus::PartialSuccess,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
            }
            SessionCleanupStatus::Failed(ref reason) => {
                tracing::warn!(
                    action = "session_memory.cleanup_session",
                    job_id = %job.id,
                    reason = %reason,
                    "Agent backend reported session cleanup warning; continuing with partial success"
                );
                progress.finish_stage(
                    "cleanup_session",
                    StageStatus::PartialSuccess,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
            }
            SessionCleanupStatus::Skipped => {
                progress.finish_stage(
                    "cleanup_session",
                    StageStatus::Skipped,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
            }
        }

        progress.set_outcome(
            TaskOutcome::Success,
            Some("会话记忆已成功生成并发布".to_string()),
            None,
        );
        progress.progress(3, Some(3), Some("persisted"));
        store::load_session_memory_for_job_sqlx(&pool, tenant_id, &job).await
    }

    /// Reconcile durable Session Memory jobs into the in-memory TaskRuntime.
    /// SQLite remains the queue authority; rebuilding or clearing TaskRuntime
    /// only causes this bounded pass to register the work again.
    pub(crate) async fn reconcile_session_memory_jobs_for_tenant_at(
        &self,
        tenant_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<usize> {
        if !self.backend_settings()?.is_memory_generation_enabled() {
            return Ok(0);
        }
        let pool = self.db.pool().clone();
        let now_text = now.to_rfc3339();
        store::recover_expired_session_memory_leases_sqlx(&pool, tenant_id, &now_text).await?;
        const MAX_SESSION_MEMORY_HOURLY_BUDGET: i64 = 60;
        let one_hour_ago = (now - Duration::hours(1)).to_rfc3339();
        let hourly_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM session_memory_jobs WHERE tenant_id = ?1 AND updated_at >= ?2 AND status IN ('running', 'succeeded', 'failed')",
        )
        .bind(tenant_id)
        .bind(&one_hour_ago)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

        if hourly_count >= MAX_SESSION_MEMORY_HOURLY_BUDGET {
            tracing::warn!(
                tenant_id,
                hourly_count,
                "Session memory hourly budget exceeded; pausing dispatch for this tenant"
            );
            return Ok(0);
        }

        let recent_failures: Vec<Option<String>> = sqlx::query_scalar(
            "SELECT last_error FROM session_memory_jobs WHERE tenant_id = ?1 AND status = 'failed' AND retry_at IS NOT NULL AND updated_at >= ?2 AND last_error IS NOT NULL ORDER BY updated_at DESC LIMIT 3",
        )
        .bind(tenant_id)
        .bind(&one_hour_ago)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        if recent_failures.len() >= 3 {
            let first_err = recent_failures[0].as_deref().unwrap_or("");
            if !first_err.is_empty()
                && recent_failures
                    .iter()
                    .all(|e| e.as_deref() == Some(first_err))
            {
                tracing::warn!(
                    tenant_id,
                    error = first_err,
                    "Session memory circuit breaker tripped: 3 consecutive identical failures. Pausing dispatch."
                );
                return Ok(0);
            }
        }

        let job_ids =
            store::list_session_memory_job_ids_for_scheduler_sqlx(&pool, tenant_id, &now_text, 32)
                .await?;
        let mut scheduled = 0usize;
        for job_id in job_ids {
            if self
                .runtime
                .task_runtime()
                .list_for_tenant(
                    tenant_id,
                    crate::backend::runtime::tasks::TaskFilter {
                        kind: Some(crate::backend::runtime::tasks::TaskKind::Memory),
                        active_only: true,
                        ..Default::default()
                    },
                )
                .len()
                >= MAX_SESSION_MEMORY_CONCURRENCY
            {
                break;
            }
            let Some(job) = store::load_session_memory_job_sqlx(&pool, tenant_id, &job_id).await?
            else {
                continue;
            };
            let detail = match store::load_conversation_session_detail_sqlx(
                &pool,
                tenant_id,
                &job.session_id,
            )
            .await
            {
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
            let task_id = format!("session-memory-{}", job.id);
            let runtime = self.runtime.clone();
            let job_id_for_task = job.id.clone();
            let tenant_id_for_task = tenant_id.to_string();
            let session_id = job.session_id.clone();
            let run_at = now;
            let short_id: String = session_id.chars().take(8).collect();
            let title = format!("会话记忆生成 (#{short_id})");
            let _ = self.runtime.task_runtime().remove_terminal(&task_id);
            let mut spec = crate::backend::runtime::tasks::TaskSpec::new(
                crate::backend::runtime::tasks::TaskKind::Memory,
                Some(format!("session-memory-job:{tenant_id}:{job_id}")),
            )
            .with_task_id(task_id)
            .with_tenant_id(tenant_id.to_string())
            .with_title(title)
            .with_capabilities(TaskCapabilities {
                cancellable: true,
                retryable: true,
                clearable: true,
            })
            .with_conflict_key(format!("session-memory-session:{tenant_id}:{session_id}"));
            spec.detail = json!({
                "domain": "session_memory",
                "scope": "session",
                "job_id": job.id,
                "session_id": session_id,
                "attempt_count": job.attempt_count,
            });
            match self
                .runtime
                .task_runtime()
                .spawn_async(spec, move |context| async move {
                    let service = AppService::from_runtime(&runtime)
                        .for_tenant(&tenant_id_for_task)
                        .await?;
                    let result = service
                        .run_session_memory_phase1_for_tenant_at(
                            &tenant_id_for_task,
                            &job_id_for_task,
                            run_at,
                            context,
                        )
                        .await;
                    let phase1_terminal = store::load_session_memory_job_sqlx(
                        service.db.pool(),
                        &tenant_id_for_task,
                        &job_id_for_task,
                    )
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|job| {
                        matches!(
                            job.status,
                            SessionMemoryJobStatus::Succeeded
                                | SessionMemoryJobStatus::Skipped
                                | SessionMemoryJobStatus::Failed
                                | SessionMemoryJobStatus::Canceled
                        )
                    });
                    if phase1_terminal {
                        if let Err(error) = service
                            .reconcile_recent_memory_jobs_for_tenant_at(
                                &tenant_id_for_task,
                                run_at,
                            )
                            .await
                        {
                            tracing::warn!(
                                action = "session_memory.reconcile_recent_after_terminal",
                                tenant_id = %tenant_id_for_task,
                                job_id = %job_id_for_task,
                                error = %error,
                                "Recent Snapshot reconciliation after Session Memory terminal state failed"
                            );
                        }
                    }
                    result.map(|memory| {
                        json!({
                            "domain": "session_memory",
                            "job_id": job_id_for_task,
                            "projected": memory.is_some(),
                        })
                    })
                }) {
                Ok(crate::backend::runtime::tasks::SpawnOutcome::Started) => {
                    scheduled += 1;
                }
                Ok(crate::backend::runtime::tasks::SpawnOutcome::Existing) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(scheduled)
    }

    async fn execute_session_memory_agent(
        &self,
        job: &SessionMemoryJob,
        detail: &ConversationSessionDetail,
        cancellation: CancellationToken,
        progress: Option<Arc<dyn AiExecutionProgressSink>>,
    ) -> AppResult<SessionMemoryAgentExecutionResult> {
        let recipe = if let Some(work_order_json) = &job.work_order_json {
            if let Ok(wo) = serde_json::from_str::<MemoryExecutionWorkOrder>(work_order_json) {
                wo.recipe.to_recipe()
            } else {
                MemoryRecipe::default_builtin()
            }
        } else {
            MemoryRecipe::default_builtin()
        };

        let normalized = session_detail_to_normalized(detail);
        let budget = BoundedMemoryBudgetPolicy::default();
        let work_order = MemoryExecutionWorkOrder::new(
            format!("wo-{}", job.id),
            job.session_id.clone(),
            job.source_id.clone(),
            job.source_revision,
            job.source_fingerprint.clone(),
            &recipe,
            budget,
            Utc::now().to_rfc3339(),
        );

        let (pack, short_refs) = build_bounded_evidence_initial_pack(&normalized, &work_order);

        // 如果完全没有有效节点（所有节点全为空或不可用）
        if pack.nodes_count == 0 && pack.coverage.indexed_nodes == 0 {
            return Ok(SessionMemoryAgentExecutionResult {
                raw_text: String::new(),
                short_refs,
                pack,
                session_cleanup: SessionCleanupStatus::Skipped,
                is_empty: true,
            });
        }

        let prompt = build_bounded_evidence_prompt(&pack, &recipe)?;
        let settings = self.app_settings_value();
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
            progress,
            tenant_id: Some(job.tenant_id.clone()),
            execution_context_key: None,
            binding: None,
            replay: false,
            restore_only: false,
            team_tools: None,
            recall_tools: None,
            memory_generation_tools: None,
        };
        let result = execute_agent(self.agent_runtime.clone(), request)
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
        if result.text.chars().count() > MAX_AGENT_OUTPUT_LENGTH {
            return Err(AppError::Validation(
                "Session Memory Agent output is too large".to_string(),
            ));
        }

        Ok(SessionMemoryAgentExecutionResult {
            raw_text: result.text,
            short_refs,
            pack,
            session_cleanup: result.session_cleanup,
            is_empty: false,
        })
    }
}

pub(crate) fn build_session_memory_prompt(
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

pub(crate) fn build_evidence_references(
    detail: &ConversationSessionDetail,
) -> Vec<EvidenceReference> {
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
                content: crate::backend::memory_redaction::redact_memory_text(&matched_content)
                    .text,
            });
        }
    }

    references.sort_by(|a, b| a.key.cmp(&b.key));
    references
}

fn sanitize_decisions_with_corrections(
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

fn sanitize_verifications_with_evidence(
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
    if references.is_empty() && !evidence.is_empty() {
        let fallback = &evidence[0];
        references.push(SessionMemoryReferenceInput {
            source_id: job.source_id.clone(),
            session_id: job.session_id.clone(),
            question_id: Some(fallback.locator.question_id.clone()),
            turn_id: Some(fallback.locator.turn_id.clone()),
            part_id: (!fallback.locator.part_id.is_empty())
                .then(|| fallback.locator.part_id.clone()),
            node_id: fallback.node_id.clone(),
            node_order: Some(fallback.locator.node_order),
            reference_key: fallback.key.clone(),
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
        recipe_id: job.recipe_id.clone(),
        recipe_content_hash: job.recipe_content_hash.clone(),
        work_order_json: job.work_order_json.clone(),
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
        .and_then(crate::backend::models::parse_conversation_timestamp)
        .is_some_and(|updated| now >= updated + Duration::minutes(30))
}

fn strip_json_fence(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return trimmed;
    }

    // Agent 偶尔会在 JSON 字符串值中直接输出未转义的引号。此时外层对象
    // 虽然暂时无法被 serde_json 解析，结构边界仍然是完整的。必须优先保留
    // 这个外层对象，不能继续向内扫描并把某个 Recent Event 子对象误当成
    // 整份 Session Memory。
    if trimmed.starts_with('{') {
        if let Some(candidate) = json_object_from_start(trimmed) {
            if is_parseable_or_repairable_memory_candidate(candidate) {
                return candidate;
            }
        }
    }

    // 1. 如果整体已经是 Session Memory JSON 对象，直接返回
    if first_valid_json_object(trimmed).is_some_and(|candidate| candidate == trimmed) {
        return trimmed;
    }

    // 2. 扫描所有代码围栏 (```json 或 ```)，优先提取第一个能解析成合法 JSON 对象的块
    let mut search_pos = 0;
    while let Some(start_rel) = trimmed[search_pos..].find("```") {
        let block_start = search_pos + start_rel;
        let content_start = if trimmed[block_start..].starts_with("```json") {
            block_start + 7
        } else {
            block_start + 3
        };
        if let Some(end_rel) = trimmed[content_start..].find("```") {
            let block_end = content_start + end_rel;
            let block_content = trimmed[content_start..block_end].trim();
            if block_content.starts_with('{') {
                if let Some(candidate) = json_object_from_start(block_content) {
                    if is_parseable_or_repairable_memory_candidate(candidate) {
                        return candidate;
                    }
                }
            }
            if let Some(candidate) = first_session_memory_json_object(block_content)
                .or_else(|| first_valid_json_object(block_content))
            {
                return candidate;
            }
            search_pos = block_end + 3;
        } else {
            break;
        }
    }

    // 3. 从前往后扫描平衡的大括号，避免前置思考文本中的无关 `{...}`
    // 把真正的业务 JSON 与尾部内容拼成一个不可解析的大区间。
    if let Some(candidate) =
        first_session_memory_json_object(trimmed).or_else(|| first_valid_json_object(trimmed))
    {
        return candidate;
    }

    trimmed
}

fn parse_session_memory_agent_output(
    value: &str,
) -> Result<SessionMemoryAgentOutput, serde_json::Error> {
    let candidate = strip_json_fence(value);
    match serde_json::from_str(candidate) {
        Ok(output) => Ok(output),
        Err(original_error) => {
            let repaired = repair_unescaped_json_string_quotes(candidate);
            if repaired == candidate {
                return Err(original_error);
            }
            serde_json::from_str(&repaired).map_err(|_| original_error)
        }
    }
}

/// Repairs the narrow, common model-output defect where a quote inside a JSON
/// string was emitted without a backslash. A quote is treated as the end of a
/// JSON string only when the next non-whitespace character is a legal
/// structural delimiter. The repaired text is still deserialized into the
/// typed contract afterwards, so this does not admit invalid field shapes.
fn repair_unescaped_json_string_quotes(value: &str) -> String {
    let mut repaired = String::with_capacity(value.len());
    let mut in_string = false;
    let mut escaped = false;
    let characters = value.char_indices().collect::<Vec<_>>();

    for (index, (byte_offset, character)) in characters.iter().copied().enumerate() {
        if !in_string {
            repaired.push(character);
            if character == '"' {
                in_string = true;
            }
            continue;
        }

        if escaped {
            repaired.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            repaired.push(character);
            escaped = true;
            continue;
        }
        if character != '"' {
            repaired.push(character);
            continue;
        }

        let next_non_whitespace = characters[index + 1..]
            .iter()
            .map(|(_, next)| *next)
            .find(|next| !next.is_whitespace());
        if next_non_whitespace.is_none_or(|next| matches!(next, ':' | ',' | '}' | ']')) {
            repaired.push(character);
            in_string = false;
        } else {
            repaired.push('\\');
            repaired.push(character);
        }

        debug_assert!(value.is_char_boundary(byte_offset));
    }

    repaired
}

fn is_parseable_or_repairable_memory_candidate(value: &str) -> bool {
    if serde_json::from_str::<serde_json::Map<String, Value>>(value).is_ok() {
        return true;
    }
    let repaired = repair_unescaped_json_string_quotes(value);
    serde_json::from_str::<serde_json::Map<String, Value>>(&repaired)
        .is_ok_and(|object| is_session_memory_json_object(&object))
}

fn json_object_from_start(value: &str) -> Option<&str> {
    if !value.starts_with('{') {
        return None;
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, current) in value.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '"' {
                in_string = false;
            }
            continue;
        }
        match current {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(&value[..offset + current.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

fn first_valid_json_object(value: &str) -> Option<&str> {
    first_json_object_matching(value, |_| true)
}

fn first_session_memory_json_object(value: &str) -> Option<&str> {
    first_json_object_matching(value, is_session_memory_json_object)
}

fn is_session_memory_json_object(object: &serde_json::Map<String, Value>) -> bool {
    let has_memory_field = object.keys().any(|key| {
        matches!(
            key.as_str(),
            "goal"
                | "result"
                | "decisions"
                | "verification"
                | "blockers"
                | "follow_up"
                | "followUp"
                | "topics"
                | "source_references"
                | "sourceReferences"
                | "events"
                | "recentEvents"
        )
    });
    let is_standalone_summary = object.contains_key("summary")
        && !object.contains_key("category")
        && !object.contains_key("title")
        && !object.contains_key("source_reference")
        && !object.contains_key("sourceReference");
    has_memory_field || is_standalone_summary
}

fn first_json_object_matching(
    value: &str,
    predicate: impl Fn(&serde_json::Map<String, Value>) -> bool,
) -> Option<&str> {
    for (start, character) in value.char_indices() {
        if character != '{' {
            continue;
        }
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for (offset, current) in value[start..].char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if current == '\\' {
                    escaped = true;
                } else if current == '"' {
                    in_string = false;
                }
                continue;
            }
            match current {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let end = start + offset + current.len_utf8();
                        let candidate = &value[start..end];
                        if serde_json::from_str::<serde_json::Map<String, Value>>(candidate)
                            .is_ok_and(|object| predicate(&object))
                        {
                            return Some(candidate);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
#[path = "session_memory_tests.rs"]
mod tests;
