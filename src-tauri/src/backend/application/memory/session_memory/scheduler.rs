use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::memory::evidence::{BoundedEvidenceNode, EvidenceNodeKind};
use crate::backend::domain::memory::{SessionMemory, SessionMemoryJob, SessionMemoryJobStatus};
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
impl AppService {
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
            crate::backend::infrastructure::agent_execution::agent_execution_workspace_root(
                &self.db_path,
            );
        Ok(store::enqueue_session_memory_jobs_sqlx(
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
        .await?)
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
            return Ok(store::load_session_memory_for_job_sqlx(&pool, tenant_id, &job).await?);
        }

        let pipeline = crate::backend::infrastructure::tasks::PipelineDescriptor::builder()
            .stage("claim", "领取工作与租约")
            .stage("load_facts", "加载上下文与事实")
            .stage("agent_execution", "调用 Agent 提取")
            .stage("validation", "校验记忆卡片")
            .stage("publish", "持久化与发布")
            .stage("cleanup_session", "清理 Agent 会话")
            .build();
        context.progress().set_stages(pipeline.to_initial_stages());

        let worker_id = format!("memory:{}", job_id);
        let (detail, registered_roots, ownership_token, job) = {
            let mut guard = context.enter_stage("claim");
            let worker = guard.worker(&worker_id);
            worker.report("claim_lease", Some(0), None, None);

            let detail =
                store::load_conversation_session_detail_sqlx(&pool, tenant_id, &job.session_id)
                    .await?;
            let roots = store::load_sources_sqlx(&pool, tenant_id)
                .await?
                .into_iter()
                .filter_map(|source| source.repo_root)
                .collect::<Vec<_>>();
            let registered_roots = roots;

            let completed = session_has_completion_signal(&detail);
            let idle_ready = session_idle_ready(&detail, now);
            if !completed && !idle_ready {
                guard.skip("not_ready", "会话未完成且未达到空闲阈值");
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
                guard.skip("lease_conflict", "工作项已被其他 Worker 认领");
                return Ok(None);
            };

            (detail, registered_roots, ownership_token, job)
        };
        context.progress().progress(0, Some(3), Some("claimed"));

        // Stage 2: load_facts
        {
            let guard = context.enter_stage("load_facts");
            let worker = guard.worker(&worker_id);
            worker.report("load_conversation_facts", Some(1), Some(1), None);
        }

        let lease_guard = SessionMemoryLeaseGuard::start(
            self.db.clone(),
            tenant_id.to_string(),
            job.id.clone(),
            ownership_token.clone(),
            context.cancellation(),
        );

        // Stage 3: agent_execution
        let session_ref = AgentSessionRef::new(format!("sm-{}", job.id));
        let execution = {
            let mut guard = context.enter_stage("agent_execution");
            guard.set_agent_session_ref(Some(session_ref.clone()));
            let worker = guard.worker(&worker_id);
            worker.report("extract_session_memory", Some(0), None, None);

            let (agent_id, model) =
                crate::backend::application::agents::composition::resolve_agent_for(
                    &crate::backend::domain::agents::ActionId::new(SESSION_MEMORY_ACTION),
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

            match result {
                Ok(output) => {
                    memory_session.finish_succeeded(&self.runtime);
                    worker.complete();
                    output
                }
                Err(error) => {
                    drop(worker);
                    drop(lease_guard);
                    if context.is_cancelled() {
                        memory_session.finish_cancelled(&self.runtime);
                        context.set_outcome(
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
                    guard.record_failure(
                        safe_failure.code.clone(),
                        safe_failure.message.clone(),
                        safe_failure.retryable,
                    );
                    context.set_outcome(TaskOutcome::Failure, None, Some(safe_failure.message));
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
            }
        };
        context
            .progress()
            .progress(1, Some(3), Some("agent_completed"));

        // Stage 4: validation
        let persist = {
            let mut guard = context.enter_stage("validation");
            if context.is_cancelled() {
                drop(lease_guard);
                context.set_outcome(
                    TaskOutcome::Canceled,
                    None,
                    Some("任务已被取消".to_string()),
                );
                store::cancel_session_memory_job_sqlx(&pool, tenant_id, job_id, &now_text).await?;
                return Err(AppError::Cancelled(
                    "Session Memory task was canceled".to_string(),
                ));
            }

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
                        guard.record_failure(
                            safe_failure.code.clone(),
                            safe_failure.message.clone(),
                            safe_failure.retryable,
                        );
                        context.set_outcome(TaskOutcome::Failure, None, Some(safe_failure.message));
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
                parsed.decisions =
                    sanitize_decisions_with_corrections(parsed.decisions, &corrections);

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
                    Ok(persist) => persist,
                    Err(error) => {
                        drop(lease_guard);
                        let safe_failure = sanitize_memory_failure(
                            "session_memory_validation_failed",
                            &error.to_string(),
                            "validation",
                            false,
                        );
                        guard.record_failure(
                            safe_failure.code.clone(),
                            safe_failure.message.clone(),
                            safe_failure.retryable,
                        );
                        context.set_outcome(TaskOutcome::Failure, None, Some(safe_failure.message));
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

            persist
        };
        context.progress().progress(2, Some(3), Some("validated"));

        // Stage 5: publish
        {
            let mut guard = context.enter_stage("publish");
            if let Err(error) = store::persist_session_memory_sqlx(&pool, &persist).await {
                drop(lease_guard);
                if context.is_cancelled() {
                    context.set_outcome(
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
                let safe_failure = sanitize_memory_failure(
                    "session_memory_persist_failed",
                    &error.to_string(),
                    "publish",
                    true,
                );
                guard.record_failure(
                    safe_failure.code.clone(),
                    safe_failure.message.clone(),
                    safe_failure.retryable,
                );
                context.set_outcome(TaskOutcome::Failure, None, Some(safe_failure.message));
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
                return Err(AppError::from(error));
            }
            drop(lease_guard);
            guard.record_metric("memories_created", 1);
        }

        // Stage 6: cleanup_session
        {
            let mut guard = context.enter_stage("cleanup_session");
            match execution.session_cleanup {
                SessionCleanupStatus::Deleted => {}
                SessionCleanupStatus::Unsupported => {
                    tracing::warn!(
                        action = "session_memory.cleanup_session",
                        job_id = %job.id,
                        "Agent backend reported session deletion unsupported; continuing with partial success"
                    );
                    guard.finish_with_status(StageStatus::PartialSuccess);
                }
                SessionCleanupStatus::Failed(ref reason) => {
                    tracing::warn!(
                        action = "session_memory.cleanup_session",
                        job_id = %job.id,
                        reason = %reason,
                        "Agent backend reported session cleanup warning; continuing with partial success"
                    );
                    guard.finish_with_status(StageStatus::PartialSuccess);
                }
                SessionCleanupStatus::Skipped => {
                    guard.finish_with_status(StageStatus::Skipped);
                }
            }
        }

        context.set_outcome(
            TaskOutcome::Success,
            Some("会话记忆已成功生成并发布".to_string()),
            None,
        );
        context.progress().progress(3, Some(3), Some("persisted"));
        Ok(store::load_session_memory_for_job_sqlx(&pool, tenant_id, &job).await?)
    }
}
