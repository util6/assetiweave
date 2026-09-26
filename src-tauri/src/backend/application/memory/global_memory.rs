use crate::backend::application::prelude::*;
use crate::backend::{
    application::memory::memory_agent_session::{
        ActiveMemoryAgentSession, MemoryAgentSessionParams,
    },
    domain::{GlobalMemoryJob, GlobalMemoryJobStatus, GlobalMemorySource, GlobalMemoryVersion},
    infrastructure::agent_execution::{
        execute_agent, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest,
    },
    infrastructure::tasks::{
        StageStatus, TaskContext, TaskFilter, TaskKind, TaskOutcome, TaskSpec, TaskStage,
    },
    store::{self, GlobalMemoryInputSet, GlobalMemoryPersistInput},
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

const GLOBAL_MEMORY_ACTION: &str = "memory.global";
const MAX_GLOBAL_MEMORY_OUTPUT_LENGTH: usize = 100_000;

pub(crate) use super::global_memory_documents::*;

impl AppService {
    pub(crate) async fn reconcile_global_memory_jobs_for_tenant_at(
        &self,
        tenant_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<usize> {
        if !self.backend_settings()?.is_memory_generation_enabled() {
            return Ok(0);
        }
        let pool = self.db.pool().clone();
        let now_text = now.to_rfc3339();
        store::recover_expired_global_memory_leases_sqlx(&pool, tenant_id, &now_text).await?;
        let job_ids =
            store::list_global_memory_job_ids_for_scheduler_sqlx(&pool, tenant_id, &now_text)
                .await?;
        let Some(job_id) = job_ids.into_iter().next() else {
            return Ok(0);
        };
        if self
            .runtime
            .task_runtime()
            .list(TaskFilter {
                kind: Some(TaskKind::Memory),
                active_only: true,
                ..Default::default()
            })
            .iter()
            .any(|snapshot| snapshot.dedup_key.as_deref() == Some("global-memory"))
        {
            return Ok(0);
        }
        let runtime = self.runtime.clone();
        let tenant_for_task = tenant_id.to_string();
        let job_for_task = job_id.clone();
        let spec = TaskSpec::new(TaskKind::Memory, Some("global-memory".to_string()))
            .with_task_id(format!("global-memory-job-{tenant_id}"))
            .with_tenant_id(tenant_id.to_string())
            .with_conflict_key(format!("global-memory-tenant:{tenant_id}"));
        match self
            .runtime
            .task_runtime()
            .spawn_async(spec, move |context| async move {
                AppService::from_runtime(&runtime)
                    .run_global_memory_for_tenant_at(&tenant_for_task, &job_for_task, now, context)
                    .await
                    .map(|version| {
                        json!({
                            "domain": "global_memory",
                            "job_id": job_for_task,
                            "projected": version.is_some(),
                        })
                    })
            })? {
            crate::backend::infrastructure::tasks::SpawnOutcome::Started => Ok(1),
            crate::backend::infrastructure::tasks::SpawnOutcome::Existing => Ok(0),
        }
    }

    pub(crate) async fn run_global_memory_for_tenant_at(
        &self,
        tenant_id: &str,
        job_id: &str,
        now: DateTime<Utc>,
        context: TaskContext,
    ) -> AppResult<Option<GlobalMemoryVersion>> {
        let pool = self.db.pool().clone();
        let now_text = now.to_rfc3339();
        let Some(job) = store::load_global_memory_job_sqlx(&pool, tenant_id, job_id).await? else {
            return Err(AppError::NotFound(
                "Global Memory job not found".to_string(),
            ));
        };
        if matches!(
            job.status,
            GlobalMemoryJobStatus::Succeeded | GlobalMemoryJobStatus::Canceled
        ) {
            return Ok(None);
        }
        if context.is_cancelled() {
            store::cancel_global_memory_job_sqlx(&pool, tenant_id, job_id, &now_text).await?;
            return Err(AppError::Cancelled(
                "Global Memory task was canceled".to_string(),
            ));
        }
        let ownership_token = format!("global-memory-owner-{}", Uuid::new_v4());
        let Some(job) = store::claim_global_memory_job_with_lease_sqlx(
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
        progress.set_stages(global_memory_task_stages());
        progress.update_stage_status("claim", StageStatus::Running);
        progress.finish_stage(
            "claim",
            StageStatus::Succeeded,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let lease_guard = GlobalMemoryLeaseGuard::start(
            self.db.clone(),
            tenant_id.to_string(),
            job.id.clone(),
            ownership_token.clone(),
            context.cancellation(),
        );

        progress.update_stage_status("load_inputs", StageStatus::Running);
        let inputs = store::load_global_memory_inputs_sqlx(&pool, tenant_id).await?;
        if inputs.projects.is_empty() {
            drop(lease_guard);
            progress.finish_stage(
                "load_inputs",
                StageStatus::Skipped,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
            store::cancel_global_memory_job_sqlx(&pool, tenant_id, job_id, &now_text).await?;
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
                &crate::backend::domain::agents::ActionId::new(GLOBAL_MEMORY_ACTION),
                &self.app_settings_value(),
            )
            .map(|(id, m)| (id.to_string(), m))
            .unwrap_or_else(|_| ("builtin:assistant".to_string(), None));

        let memory_session = ActiveMemoryAgentSession::start(
            &self.runtime,
            MemoryAgentSessionParams {
                tenant_id,
                scope: "global",
                job_id,
                task_id: Some(context.task_id()),
                agent_id: &agent_id,
                display_name: Some("Global Memory Agent".to_string()),
                model: model.clone(),
                prompt_summary: "总结全局核心经验与项目索引",
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
            .execute_global_memory_agent(
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
                    store::cancel_global_memory_job_sqlx(&pool, tenant_id, job_id, &now_text)
                        .await?;
                    return Err(AppError::Cancelled(
                        "Global Memory task was canceled".to_string(),
                    ));
                }
                let code = error
                    .view()
                    .code
                    .chars()
                    .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
                    .collect::<String>();
                let failure_code = if code.is_empty() {
                    "global_memory_failed".to_string()
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
                let _ = store::mark_global_memory_job_failed_with_lease_sqlx(
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
            store::cancel_global_memory_job_sqlx(&pool, tenant_id, job_id, &now_text).await?;
            return Err(AppError::Cancelled(
                "Global Memory task was canceled".to_string(),
            ));
        }

        progress.update_stage_status("validation", StageStatus::Running);
        let version_number =
            store::next_global_memory_version_number_sqlx(&pool, tenant_id).await?;
        let paths = global_document_paths(&self.db_path, tenant_id, version_number);
        if let Err(error) = write_global_version_files(
            &paths.version_summary_path,
            &paths.version_memory_path,
            &output.summary_markdown,
            &output.memory_markdown,
        ) {
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
                Some("global_memory_write_failed".to_string()),
                Some(error.to_string()),
            );
            return Err(error);
        }

        let persist = GlobalMemoryPersistInput {
            tenant_id: tenant_id.to_string(),
            input_fingerprint: inputs.fingerprint,
            source_watermark: inputs.watermark,
            summary_markdown: output.summary_markdown,
            memory_markdown: output.memory_markdown,
            raw_output_json: output.raw_output_json,
            summary_document_path: paths.summary_document_path.to_string_lossy().to_string(),
            memory_document_path: paths.memory_document_path.to_string_lossy().to_string(),
            ownership_token,
            sources: inputs
                .projects
                .iter()
                .enumerate()
                .map(|(sort_order, project)| GlobalMemorySource {
                    project_id: project.project_id.clone(),
                    project_path: project.project_path.clone(),
                    project_version_id: project.project_version_id.clone(),
                    project_watermark: project.project_watermark,
                    sort_order: sort_order as i64,
                })
                .collect(),
        };
        let version =
            match store::persist_global_memory_success_sqlx(&pool, &persist, &now_text).await {
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
                        Some("global_memory_persist_failed".to_string()),
                        Some(error.to_string()),
                    );
                    if context.is_cancelled() {
                        store::cancel_global_memory_job_sqlx(&pool, tenant_id, job_id, &now_text)
                            .await?;
                        return Err(AppError::Cancelled(
                            "Global Memory task was canceled".to_string(),
                        ));
                    }
                    let app_error = AppError::from(error);
                    let retryable = is_error_retryable(&app_error);
                    let _ = store::mark_global_memory_job_failed_with_lease_sqlx(
                        &pool,
                        tenant_id,
                        job_id,
                        &persist.ownership_token,
                        "global_memory_persist_failed",
                        &now_text,
                        retryable,
                    )
                    .await?;
                    return Err(app_error);
                }
            };
        drop(lease_guard);

        progress.update_stage_status("publish", StageStatus::Running);
        if let Err(error) = publish_global_documents(
            &paths.summary_document_path,
            &paths.memory_document_path,
            &paths.version_summary_path,
            &paths.version_memory_path,
            &persist.summary_markdown,
            &persist.memory_markdown,
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
                Some("global_memory_publish_failed".to_string()),
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
#[path = "global_memory_tests.rs"]
mod tests;
