pub(crate) use super::project_memory_documents::*;
use crate::backend::application::prelude::*;
use crate::backend::{
    application::memory::memory_agent_session::{
        ActiveMemoryAgentSession, MemoryAgentSessionParams,
    },
    domain::{ProjectMemoryJob, ProjectMemoryJobStatus, ProjectMemorySource},
    infrastructure::agent_execution::{
        execute_agent, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest,
    },
    infrastructure::tasks::{
        StageStatus, TaskContext, TaskFilter, TaskKind, TaskOutcome, TaskSpec, TaskStage,
    },
    store::{
        self, ProjectMemoryInputSet, ProjectMemoryPersistInput, PROJECT_MEMORY_CONTRACT_VERSION,
        PROJECT_MEMORY_PROMPT_VERSION,
    },
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const MAX_PROJECT_MEMORY_CONCURRENCY: usize = 4;

impl AppService {
    /// Rehydrates Project Memory work after Session Memory commits or process
    /// restart. The per-project conflict key is the in-memory serialization
    /// boundary; the durable claim remains authoritative across processes.
    pub(crate) async fn reconcile_project_memory_jobs_for_tenant_at(
        &self,
        tenant_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<usize> {
        if !self.backend_settings()?.is_memory_generation_enabled() {
            return Ok(0);
        }
        let pool = self.db.pool().clone();
        let now_text = now.to_rfc3339();
        store::recover_expired_project_memory_leases_sqlx(&pool, tenant_id, &now_text).await?;
        let job_ids =
            store::list_project_memory_job_ids_for_scheduler_sqlx(&pool, tenant_id, &now_text, 32)
                .await?;
        if job_ids.is_empty() {
            return Ok(0);
        }
        let active_count = self
            .runtime
            .task_runtime()
            .list(TaskFilter {
                kind: Some(TaskKind::Memory),
                active_only: true,
                ..Default::default()
            })
            .len();
        let mut scheduled = 0usize;
        for job_id in job_ids {
            if active_count + scheduled >= MAX_PROJECT_MEMORY_CONCURRENCY {
                break;
            }
            let Some(job) = store::load_project_memory_job_sqlx(&pool, tenant_id, &job_id).await?
            else {
                continue;
            };
            let runtime = self.runtime.clone();
            let tenant_id_for_task = tenant_id.to_string();
            let job_id_for_task = job.id.clone();
            let task_id = format!("project-memory-{}-{}", job.id, job.attempt_count);
            let spec = TaskSpec::new(
                TaskKind::Memory,
                Some(format!("project-memory-job:{tenant_id}:{}", job.project_id)),
            )
            .with_task_id(task_id)
            .with_tenant_id(tenant_id.to_string())
            .with_conflict_key(format!(
                "project-memory-project:{tenant_id}:{}",
                job.project_id
            ));
            let mut spec = spec;
            spec.detail = json!({
                "domain": "project_memory",
                "job_id": job.id,
                "project_id": job.project_id,
                "project_path": job.project_path,
            });
            match self
                .runtime
                .task_runtime()
                .spawn_async(spec, move |context| async move {
                    AppService::from_runtime(&runtime)
                        .run_project_memory_for_tenant_at(
                            &tenant_id_for_task,
                            &job_id_for_task,
                            now,
                            context,
                        )
                        .await
                        .map(|version| {
                            json!({
                                "domain": "project_memory",
                                "job_id": job_id_for_task,
                                "projected": version.is_some(),
                            })
                        })
                }) {
                Ok(crate::backend::infrastructure::tasks::SpawnOutcome::Started) => scheduled += 1,
                Ok(crate::backend::infrastructure::tasks::SpawnOutcome::Existing) => {}
                Err(error) => return Err(AppError::Infra(error)),
            }
        }
        Ok(scheduled)
    }

    pub(crate) async fn run_project_memory_for_tenant_at(
        &self,
        tenant_id: &str,
        job_id: &str,
        now: DateTime<Utc>,
        context: TaskContext,
    ) -> AppResult<Option<crate::backend::domain::ProjectMemoryVersion>> {
        let pool = self.db.pool().clone();
        let now_text = now.to_rfc3339();
        let Some(job) = store::load_project_memory_job_sqlx(&pool, tenant_id, job_id).await? else {
            return Err(AppError::NotFound(
                "Project Memory job not found".to_string(),
            ));
        };
        if matches!(
            job.status,
            ProjectMemoryJobStatus::Succeeded | ProjectMemoryJobStatus::Canceled
        ) {
            return Ok(None);
        }
        if context.is_cancelled() {
            store::cancel_project_memory_job_sqlx(&pool, tenant_id, job_id, &now_text).await?;
            return Err(AppError::Cancelled(
                "Project Memory task was canceled".to_string(),
            ));
        }
        let ownership_token = format!("project-memory-owner-{}", Uuid::new_v4());
        let Some(job) = store::claim_project_memory_job_with_lease_sqlx(
            &pool,
            tenant_id,
            job_id,
            &now_text,
            &ownership_token,
        )
        .await?
        else {
            return Ok(None);
        };
        let progress = context.progress();
        progress.set_stages(project_memory_task_stages());
        progress.update_stage_status("claim", StageStatus::Running);
        progress.finish_stage(
            "claim",
            StageStatus::Succeeded,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let lease_guard = ProjectMemoryLeaseGuard::start(
            self.db.clone(),
            tenant_id.to_string(),
            job.id.clone(),
            ownership_token.clone(),
            context.cancellation(),
        );

        progress.update_stage_status("load_inputs", StageStatus::Running);
        let inputs =
            store::load_project_memory_inputs_sqlx(&pool, tenant_id, &job.project_path).await?;
        if inputs.memories.is_empty() {
            drop(lease_guard);
            progress.finish_stage(
                "load_inputs",
                StageStatus::Skipped,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            store::cancel_project_memory_job_sqlx(&pool, tenant_id, job_id, &now_text).await?;
            return Ok(None);
        }
        progress.finish_stage(
            "load_inputs",
            StageStatus::Succeeded,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let (agent_id, model) =
            crate::backend::application::agents::composition::resolve_agent_for(
                &crate::backend::domain::agents::ActionId::new(PROJECT_MEMORY_ACTION),
                &self.app_settings_value(),
            )
            .map(|(id, m)| (id.to_string(), m))
            .unwrap_or_else(|_| ("builtin:assistant".to_string(), None));

        let memory_session = ActiveMemoryAgentSession::start(
            &self.runtime,
            MemoryAgentSessionParams {
                tenant_id,
                scope: "project",
                job_id,
                task_id: Some(context.task_id()),
                agent_id: &agent_id,
                display_name: Some("Project Memory Agent".to_string()),
                model: model.clone(),
                prompt_summary: "提炼项目级 MEMORY.md 知识",
                custom_session_ref: None,
                persistent: false,
            },
        );
        progress.set_stage_agent_session_ref(
            "agent_execution",
            Some(memory_session.session_ref.clone()),
        );
        progress.update_stage_status("agent_execution", StageStatus::Running);

        let output = match self
            .execute_project_memory_agent(
                &job,
                &inputs,
                context.cancellation(),
                Some(memory_session.sink.clone()),
            )
            .await
        {
            Ok(output) => {
                memory_session.finish_succeeded(&self.runtime);
                progress.finish_stage(
                    "agent_execution",
                    StageStatus::Succeeded,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
                output
            }
            Err(error) => {
                drop(lease_guard);
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
                    store::cancel_project_memory_job_sqlx(&pool, tenant_id, job_id, &now_text)
                        .await?;
                    return Err(AppError::Cancelled(
                        "Project Memory task was canceled".to_string(),
                    ));
                }
                let code = error
                    .view()
                    .code
                    .chars()
                    .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
                    .collect::<String>();
                let failure_code = if code.is_empty() {
                    "project_memory_failed".to_string()
                } else {
                    code
                };
                memory_session.finish_failed(
                    &self.runtime,
                    &failure_code,
                    &error.to_string(),
                    false,
                );
                progress.finish_stage(
                    "agent_execution",
                    StageStatus::Failed,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                );
                progress.set_outcome(
                    TaskOutcome::Failure,
                    Some(failure_code.clone()),
                    Some(error.to_string()),
                );
                let retryable = is_error_retryable(&error);
                store::mark_project_memory_job_failed_with_lease_sqlx(
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

        if context.is_cancelled() {
            drop(lease_guard);
            store::cancel_project_memory_job_sqlx(&pool, tenant_id, job_id, &now_text).await?;
            return Err(AppError::Cancelled(
                "Project Memory task was canceled".to_string(),
            ));
        }

        progress.update_stage_status("validation", StageStatus::Running);
        let version_number =
            store::next_project_memory_version_number_sqlx(&pool, tenant_id, &job.project_id)
                .await?;
        let document_paths =
            project_document_paths(&self.db_path, tenant_id, &job.project_path, version_number);
        if let Err(error) =
            write_project_version_file(&document_paths.version_path, &output.content_markdown)
        {
            drop(lease_guard);
            progress.finish_stage(
                "validation",
                StageStatus::Failed,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            progress.set_outcome(
                TaskOutcome::Failure,
                Some("project_memory_write_failed".to_string()),
                Some(error.to_string()),
            );
            return Err(error);
        }
        let persist = ProjectMemoryPersistInput {
            tenant_id: tenant_id.to_string(),
            project_id: job.project_id.clone(),
            project_path: job.project_path.clone(),
            input_fingerprint: inputs.fingerprint.clone(),
            source_watermark: inputs.watermark,
            content_markdown: output.content_markdown,
            raw_output_json: output.raw_output_json,
            document_path: document_paths.document_path.to_string_lossy().to_string(),
            ownership_token,
            sources: inputs
                .memories
                .iter()
                .enumerate()
                .map(|(sort_order, memory)| ProjectMemorySource {
                    session_memory_id: memory.id.clone(),
                    source_revision: memory.source_revision,
                    sort_order: sort_order as i64,
                })
                .collect(),
        };
        let version =
            match store::persist_project_memory_success_sqlx(&pool, &persist, &now_text).await {
                Ok(version) => {
                    progress.finish_stage(
                        "validation",
                        StageStatus::Succeeded,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    );
                    version
                }
                Err(error) => {
                    let error = AppError::Store(error);
                    drop(lease_guard);
                    progress.finish_stage(
                        "validation",
                        StageStatus::Failed,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    );
                    progress.set_outcome(
                        TaskOutcome::Failure,
                        Some("project_memory_persist_failed".to_string()),
                        Some(error.to_string()),
                    );
                    if context.is_cancelled() {
                        store::cancel_project_memory_job_sqlx(&pool, tenant_id, job_id, &now_text)
                            .await?;
                        return Err(AppError::Cancelled(
                            "Project Memory task was canceled".to_string(),
                        ));
                    }
                    let retryable = is_error_retryable(&error);
                    store::mark_project_memory_job_failed_with_lease_sqlx(
                        &pool,
                        tenant_id,
                        job_id,
                        &persist.ownership_token,
                        "project_memory_persist_failed",
                        &now_text,
                        retryable,
                    )
                    .await?;
                    return Err(error);
                }
            };
        drop(lease_guard);

        progress.update_stage_status("publish", StageStatus::Running);
        if let Err(error) = publish_project_document(
            &document_paths.document_path,
            &document_paths.version_path,
            &persist.content_markdown,
        ) {
            progress.finish_stage(
                "publish",
                StageStatus::Failed,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            progress.set_outcome(
                TaskOutcome::Failure,
                Some("project_memory_publish_failed".to_string()),
                Some(error.to_string()),
            );
            return Err(error);
        }
        progress.finish_stage(
            "publish",
            StageStatus::Succeeded,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        progress.set_outcome(TaskOutcome::Success, None, None);
        Ok(Some(version))
    }
}

#[cfg(test)]
#[path = "project_memory_tests.rs"]
mod tests;
