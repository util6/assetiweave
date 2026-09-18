use super::prelude::*;
use crate::backend::{
    ai_execution::{
        execute_agent, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionPurpose, AiExecutionRequest,
    },
    dto::{MemoryProjectView, MemoryRebuildResult, MemoryTaskView},
    models::{GlobalConsolidationInput, MemoryWorkOrderV2, ProjectConsolidationInput},
    runtime::tasks::{CancelOutcome, TaskFilter, TaskKind, TaskState},
    store,
};
use serde::de::DeserializeOwned;
use tokio_util::sync::CancellationToken;

impl AppService {
    pub(crate) async fn get_memory_project(
        &self,
        params: MemoryProjectGetParams,
    ) -> AppResult<Option<MemoryProjectView>> {
        let project_path = self
            .resolve_context_project_path(Some(&params.project_path))
            .await?
            .ok_or_else(|| AppError::Validation("project_path is required".to_string()))?;
        self.get_project_memory_l2(&project_path).await
    }

    /// 获取当前项目的 L2 长期记忆视图 (M35-L2-01 ~ M35-L2-06)
    pub(crate) async fn get_project_memory_l2(
        &self,
        project_path: &str,
    ) -> AppResult<Option<crate::backend::models::L2ProjectMemoryView>> {
        let normalized_path = self
            .resolve_context_project_path(Some(project_path))
            .await?
            .unwrap_or_else(|| project_path.to_string());
        let tenant_id = self.tenant_id();
        crate::backend::application::project_consolidation_pipeline::load_l2_project_memory_view(
            self.db.pool(),
            tenant_id,
            &normalized_path,
        )
        .await
    }

    /// 协调并执行指定项目的 L2 Consolidation
    pub(crate) async fn reconcile_project_consolidation(
        &self,
        project_key: &str,
        project_path: Option<&str>,
    ) -> AppResult<Option<crate::backend::models::L2ProjectMemoryView>> {
        let lock_map = crate::backend::application::project_consolidation_pipeline::global_project_consolidation_lock_map();
        crate::backend::application::project_consolidation_pipeline::reconcile_project_consolidation_default(
            self.db.pool(),
            self.tenant_id(),
            project_key,
            project_path,
            lock_map,
        )
        .await
    }

    pub(crate) async fn reconcile_project_consolidation_with_agent(
        &self,
        project_key: &str,
        project_path: Option<&str>,
        work_order: MemoryWorkOrderV2,
        skill_text: String,
        cancellation: CancellationToken,
        maintenance_lease: Option<(&str, &str)>,
        frozen_input: Option<ProjectConsolidationInput>,
    ) -> AppResult<Option<crate::backend::models::L2ProjectMemoryView>> {
        let settings = self.app_settings_value();
        let (agent_id, model) = crate::backend::ai_execution::composition::resolve_agent_for(
            &crate::backend::ai_execution::composition::ActionId::new("memory.generation"),
            &settings,
        )?;
        let runtime = self.agent_runtime.clone();
        let tenant_id = self.tenant_id().to_string();
        let lock_map = crate::backend::application::project_consolidation_pipeline::
            global_project_consolidation_lock_map();
        crate::backend::application::project_consolidation_pipeline::reconcile_project_consolidation_with_lease(
            self.db.pool(),
            self.tenant_id(),
            project_key,
            project_path,
            lock_map,
            Some(move |input: ProjectConsolidationInput| {
                let runtime = runtime.clone();
                let agent_id = agent_id.clone();
                let model = model.clone();
                let tenant_id = tenant_id.clone();
                let work_order = work_order.clone();
                let skill_text = skill_text.clone();
                let cancellation = cancellation.clone();
                async move {
                    if cancellation.is_cancelled() {
                        return Err(AppError::Cancelled(
                            "Project consolidation was cancelled before execution".to_string(),
                        ));
                    }
                    let prompt = serde_json::to_string(&json!({
                        "contract": "memory.project.consolidation.v2",
                        "instruction": "Return exactly one JSON object matching ProjectConsolidationResult. Do not return Markdown, prose, or code fences.",
                        "skill": skill_text,
                        "work_order": work_order.clone(),
                        "evidence": input,
                    }))
                    .map_err(AppError::external)?;
                    let request = AiExecutionRequest {
                        execution_id: format!(
                            "memory-project-execution-{}",
                            work_order.work_order_id
                        ),
                        agent_id,
                        purpose: AiExecutionPurpose::MemoryGeneration,
                        session_mode: AgentSessionMode::OneShot,
                        prompt,
                        model,
                        limits: AiExecutionLimits::default(),
                        cancellation: AiExecutionCancellation::from_token(cancellation),
                        progress: None,
                        tenant_id: Some(tenant_id),
                        execution_context_key: None,
                        binding: None,
                        replay: false,
                        restore_only: false,
                        team_tools: None,
                        recall_tools: None,
                        memory_generation_tools: None,
                    };
                    let execution = execute_agent(runtime, request).await.map_err(|error| {
                        let view = error.to_view();
                        AppError::Domain {
                            code: view.code,
                            message: view.message,
                            retryable: view.retryable,
                            details: None,
                        }
                    })?;
                    parse_consolidation_output(&execution.text, "ProjectConsolidationResult")
                }
            }),
            maintenance_lease,
            frozen_input,
        )
        .await
    }

    /// 获取当前 Tenant 的 L3 全局长期记忆视图 (M35-L3-01 ~ M35-L3-06)
    pub(crate) async fn get_global_memory_l3(
        &self,
    ) -> AppResult<Option<crate::backend::models::L3GlobalMemoryView>> {
        crate::backend::application::global_consolidation_pipeline::get_global_memory_l3_view(
            self.db.pool(),
            self.tenant_id(),
        )
        .await
    }

    /// 协调并执行当前 Tenant 的 Global Consolidation (M35-L3-01 ~ M35-L3-06)
    pub(crate) async fn reconcile_global_consolidation(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        is_manual_rebuild: bool,
    ) -> AppResult<Option<crate::backend::models::L3GlobalMemoryView>> {
        crate::backend::application::global_consolidation_pipeline::reconcile_global_consolidation(
            self.db.pool(),
            self.tenant_id(),
            now,
            is_manual_rebuild,
            None,
        )
        .await
    }

    pub(crate) async fn reconcile_global_consolidation_with_agent(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        work_order: MemoryWorkOrderV2,
        skill_text: String,
        cancellation: CancellationToken,
        maintenance_lease: Option<(&str, &str)>,
        frozen_input: Option<GlobalConsolidationInput>,
    ) -> AppResult<Option<crate::backend::models::L3GlobalMemoryView>> {
        let settings = self.app_settings_value();
        let (agent_id, model) = crate::backend::ai_execution::composition::resolve_agent_for(
            &crate::backend::ai_execution::composition::ActionId::new("memory.generation"),
            &settings,
        )?;
        let runtime = self.agent_runtime.clone();
        let tenant_id = self.tenant_id().to_string();
        let work_order_for_runner = work_order.clone();
        let skill_text_for_runner = skill_text.clone();
        let cancellation_for_runner = cancellation.clone();
        crate::backend::application::global_consolidation_pipeline::
            reconcile_global_consolidation_with_runner_and_lease(
                self.db.pool(),
                self.tenant_id(),
                now,
                true,
                None,
                Some(move |input: GlobalConsolidationInput| {
                    let runtime = runtime.clone();
                    let agent_id = agent_id.clone();
                    let model = model.clone();
                    let tenant_id = tenant_id.clone();
                    let work_order = work_order_for_runner.clone();
                    let skill_text = skill_text_for_runner.clone();
                    let cancellation = cancellation_for_runner.clone();
                    async move {
                        if cancellation.is_cancelled() {
                            return Err(AppError::Cancelled(
                                "Global consolidation was cancelled before execution".to_string(),
                            ));
                        }
                        let prompt = serde_json::to_string(&json!({
                            "contract": "memory.global.consolidation.v2",
                            "instruction": "Return exactly one JSON object matching GlobalConsolidationResult. Do not return Markdown, prose, or code fences. Only use source_refs from the frozen evidence. A global_rule operation must retain an available canonical user reference. A cross_project_pattern operation must retain available references from at least two distinct real project keys and state the generalized rule in the statement; never promote a project-specific fact as global.",
                            "skill": skill_text,
                            "work_order": work_order.clone(),
                            "evidence": input,
                        }))
                        .map_err(AppError::external)?;
                        let request = AiExecutionRequest {
                            execution_id: format!(
                                "memory-global-execution-{}",
                                work_order.work_order_id
                            ),
                            agent_id,
                            purpose: AiExecutionPurpose::MemoryGeneration,
                            session_mode: AgentSessionMode::OneShot,
                            prompt,
                            model,
                            limits: AiExecutionLimits::default(),
                            cancellation: AiExecutionCancellation::from_token(cancellation),
                            progress: None,
                            tenant_id: Some(tenant_id),
                            execution_context_key: None,
                            binding: None,
                            replay: false,
                            restore_only: false,
                            team_tools: None,
                            recall_tools: None,
                            memory_generation_tools: None,
                        };
                        let execution = execute_agent(runtime, request).await.map_err(|error| {
                            let view = error.to_view();
                            AppError::Domain {
                                code: view.code,
                                message: view.message,
                                retryable: view.retryable,
                                details: None,
                            }
                        })?;
                        parse_consolidation_output(&execution.text, "GlobalConsolidationResult")
                    }
                }),
                maintenance_lease,
                frozen_input,
            )
            .await
    }

    /// 协调长期记忆来源失效 (M35-L3-04)
    pub(crate) async fn reconcile_memory_source_invalidation(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<usize> {
        let settings = self.app_settings_value();
        let memory_settings = settings
            .get("memory")
            .and_then(|v| {
                serde_json::from_value::<crate::backend::app_settings::MemorySettings>(v.clone())
                    .ok()
            })
            .unwrap_or_default();

        crate::backend::application::global_consolidation_pipeline::reconcile_source_invalidation(
            self.db.pool(),
            self.tenant_id(),
            now,
            &memory_settings.excluded_source_ids,
            &memory_settings.excluded_session_ids,
        )
        .await
    }

    /// 重建 Markdown 投影文件 (M35-PROJ-01 ~ M35-PROJ-03)
    pub(crate) async fn rebuild_markdown_projections(
        &self,
    ) -> AppResult<crate::backend::application::memory_projection_v2::MemoryProjectionPaths> {
        crate::backend::application::memory_projection_v2::rebuild_markdown_projections(
            self.db.pool(),
            self.tenant_id(),
            None,
        )
        .await
    }

    /// 清理过期的 Recent Snapshot 历史 (M35-PROJ-04，默认 30 天)
    pub(crate) async fn purge_expired_memory_snapshots(
        &self,
        retention_days: i64,
    ) -> AppResult<usize> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days);
        crate::backend::application::memory_projection_v2::purge_stale_recent_memory_snapshots(
            self.db.pool(),
            self.tenant_id(),
            cutoff,
        )
        .await
    }

    pub(crate) async fn rebuild_memory_scope(
        &self,
        params: MemoryScopeRebuildParams,
    ) -> AppResult<MemoryRebuildResult> {
        if !self.backend_settings()?.is_memory_generation_enabled() {
            return Ok(MemoryRebuildResult {
                accepted: false,
                scheduled_task_ids: Vec::new(),
                target_watermark: None,
                reused: false,
            });
        }
        let now = Utc::now();
        let project_path = params
            .project_path
            .as_deref()
            .or(params.scope.project_path.as_deref());
        let target = params.target.unwrap_or_else(|| {
            if project_path.is_some() {
                MemoryRebuildTarget::Project
            } else {
                MemoryRebuildTarget::Recent
            }
        });
        let narrow_scope = params.scope.app_id.is_some()
            || params.scope.source_id.is_some()
            || params.scope.session_id.is_some();
        if narrow_scope {
            return Err(AppError::Validation(
                "Memory v2 rebuild does not support app/source/session scopes".to_string(),
            ));
        }
        if matches!(params.reason, Some(MemoryRebuildReason::ProjectionRepair)) {
            let scheduled_task_ids = self.schedule_memory_projection_rebuild().await?;
            return Ok(MemoryRebuildResult {
                accepted: true,
                scheduled_task_ids,
                target_watermark: None,
                reused: false,
            });
        }

        let (scheduled_task_ids, target_watermark) = match target {
            MemoryRebuildTarget::Recent => {
                if project_path.is_some() {
                    return Err(AppError::Validation(
                        "Recent rebuild does not accept project_path; use target=project"
                            .to_string(),
                    ));
                }
                let phase1_target = self
                    .ensure_recent_snapshot_phase1_jobs_at(now, true)
                    .await?
                    .map(|(target, _)| target);
                self.reconcile_session_memory_jobs_for_tenant_at(self.tenant_id(), now)
                    .await?;

                let preparation = self
                    .prepare_recent_snapshot_generation_with_options(Some(now), true)
                    .await?;
                let target_watermark = phase1_target
                    .map(|target| target.target_watermark_utc.to_rfc3339())
                    .or_else(|| {
                        preparation
                            .as_ref()
                            .map(|value| value.target.target_watermark_utc.to_rfc3339())
                    });
                let mut scheduled_task_ids = Vec::new();
                if let Some(preparation) = preparation {
                    let job_id = self
                        .enqueue_recent_snapshot_generation(&preparation, now)
                        .await?;
                    let _ = store::restart_recent_memory_job_for_rebuild_sqlx(
                        self.db.pool(),
                        self.tenant_id(),
                        &job_id,
                        &now.to_rfc3339(),
                    )
                    .await?;
                    scheduled_task_ids.push(format!("memory-v2-recent-{job_id}"));
                }
                self.reconcile_recent_memory_jobs_for_tenant_at(self.tenant_id(), now)
                    .await?;
                if scheduled_task_ids.is_empty() {
                    if let Ok(active_memory_tasks) = self.list_memory_task_views(
                        crate::backend::application::params::MemoryTaskListParams {
                            active_only: true,
                        },
                    ) {
                        for task in active_memory_tasks {
                            if task.status == "running" || task.status == "pending" {
                                scheduled_task_ids.push(task.id);
                            }
                        }
                    }
                }
                (scheduled_task_ids, target_watermark)
            }
            MemoryRebuildTarget::Project => {
                let path = project_path.ok_or_else(|| {
                    AppError::Validation("Project rebuild requires project_path".to_string())
                })?;
                (self.schedule_project_memory_rebuild(path, now).await?, None)
            }
            MemoryRebuildTarget::Global => {
                if project_path.is_some() {
                    return Err(AppError::Validation(
                        "Global rebuild does not accept project_path".to_string(),
                    ));
                }
                (self.schedule_global_memory_rebuild(now).await?, None)
            }
            MemoryRebuildTarget::All => {
                if project_path.is_some() {
                    return Err(AppError::Validation(
                        "All rebuild does not accept project_path".to_string(),
                    ));
                }
                let phase1_target = self
                    .ensure_recent_snapshot_phase1_jobs_at(now, true)
                    .await?
                    .map(|(target, _)| target);
                self.reconcile_session_memory_jobs_for_tenant_at(self.tenant_id(), now)
                    .await?;

                let preparation = self
                    .prepare_recent_snapshot_generation_with_options(Some(now), true)
                    .await?;
                let target_watermark = phase1_target
                    .map(|target| target.target_watermark_utc.to_rfc3339())
                    .or_else(|| {
                        preparation
                            .as_ref()
                            .map(|value| value.target.target_watermark_utc.to_rfc3339())
                    });
                let mut scheduled_task_ids = Vec::new();
                if let Some(preparation) = preparation {
                    let job_id = self
                        .enqueue_recent_snapshot_generation(&preparation, now)
                        .await?;
                    let _ = store::restart_recent_memory_job_for_rebuild_sqlx(
                        self.db.pool(),
                        self.tenant_id(),
                        &job_id,
                        &now.to_rfc3339(),
                    )
                    .await?;
                    scheduled_task_ids.push(format!("memory-v2-recent-{job_id}"));
                }
                self.reconcile_recent_memory_jobs_for_tenant_at(self.tenant_id(), now)
                    .await?;
                for project_path in
                    store::list_memory_v2_project_paths_sqlx(self.db.pool(), self.tenant_id())
                        .await?
                {
                    scheduled_task_ids.extend(
                        self.schedule_project_memory_rebuild(&project_path, now)
                            .await?,
                    );
                }
                scheduled_task_ids.extend(self.schedule_global_memory_rebuild(now).await?);
                (scheduled_task_ids, target_watermark)
            }
        };
        let mut scheduled_task_ids = scheduled_task_ids;
        scheduled_task_ids.sort();
        scheduled_task_ids.dedup();
        Ok(MemoryRebuildResult {
            accepted: true,
            scheduled_task_ids,
            target_watermark,
            reused: false,
        })
    }

    pub(crate) fn list_memory_task_views(
        &self,
        params: MemoryTaskListParams,
    ) -> AppResult<Vec<MemoryTaskView>> {
        let snapshots = self.runtime.task_runtime().list_for_tenant(
            self.tenant_id(),
            TaskFilter {
                kind: Some(TaskKind::Memory),
                active_only: params.active_only,
                ..Default::default()
            },
        );
        snapshots.into_iter().map(memory_task_view).collect()
    }

    pub(crate) fn get_memory_task_view(
        &self,
        params: MemoryTaskGetParams,
    ) -> AppResult<Option<MemoryTaskView>> {
        let snapshot = self
            .runtime
            .task_runtime()
            .get_for_tenant(self.tenant_id(), &params.task_id);
        snapshot.map(memory_task_view).transpose()
    }

    pub(crate) fn cancel_memory_task_view(
        &self,
        params: MemoryTaskGetParams,
    ) -> AppResult<MemoryTaskView> {
        match self
            .runtime
            .task_runtime()
            .cancel_for_tenant(self.tenant_id(), &params.task_id)
        {
            CancelOutcome::Requested(snapshot) | CancelOutcome::AlreadyFinished(snapshot) => {
                memory_task_view(snapshot)
            }
            CancelOutcome::NotFound => Err(AppError::NotFound(format!(
                "Memory task not found: {}",
                params.task_id
            ))),
        }
    }

    pub(crate) async fn retry_memory_task(
        &self,
        params: MemoryTaskRetryParams,
    ) -> AppResult<MemoryTaskView> {
        let snapshot = self
            .runtime
            .task_runtime()
            .get_for_tenant(self.tenant_id(), &params.task_id)
            .ok_or_else(|| {
                AppError::NotFound(format!("Memory task not found: {}", params.task_id))
            })?;
        if snapshot.state.is_active() {
            return Err(AppError::Conflict(
                "Memory task is still active".to_string(),
            ));
        }
        let domain = snapshot
            .detail
            .get("domain")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::Validation("Memory task has no retryable domain".to_string())
            })?;
        let tenant_id = self.tenant_id().to_string();
        let job_id = snapshot.detail.get("job_id").and_then(Value::as_str);
        let maintenance_job_id = snapshot
            .detail
            .get("maintenance_job_id")
            .and_then(Value::as_str);
        if matches!(domain, "project_memory" | "global_memory") && maintenance_job_id.is_some() {
            let maintenance_job_id = maintenance_job_id.expect("checked above");
            let changed = store::retry_memory_v2_maintenance_job_sqlx(
                self.db.pool(),
                &tenant_id,
                maintenance_job_id,
                &Utc::now().to_rfc3339(),
            )
            .await?;
            if !changed {
                return Err(AppError::Conflict(
                    "Memory maintenance task is not in a retryable durable state".to_string(),
                ));
            }
            let _ = self.runtime.task_runtime().remove_terminal(&params.task_id);
            self.reconcile_memory_v2_maintenance_jobs_for_tenant_at(&tenant_id, Utc::now())
                .await?;
            return self
                .get_memory_task_view(MemoryTaskGetParams {
                    task_id: params.task_id,
                })?
                .ok_or_else(|| AppError::NotFound("Retried Memory task was pruned".to_string()));
        }
        if matches!(
            domain,
            "project_memory" | "global_memory" | "memory_projection"
        ) {
            let _ = self.runtime.task_runtime().remove_terminal(&params.task_id);
            let now = Utc::now();
            let scheduled_task_ids = match domain {
                "project_memory" => {
                    let project_path = snapshot
                        .detail
                        .get("project_path")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            AppError::Validation(
                                "Project Memory task has no project path".to_string(),
                            )
                        })?;
                    self.schedule_project_memory_rebuild(project_path, now)
                        .await?
                }
                "global_memory" => self.schedule_global_memory_rebuild(now).await?,
                "memory_projection" => self.schedule_memory_projection_rebuild().await?,
                _ => Vec::new(),
            };
            if scheduled_task_ids.is_empty() {
                return Err(AppError::Conflict(
                    "Memory task could not be scheduled again".to_string(),
                ));
            }
            return self
                .get_memory_task_view(MemoryTaskGetParams {
                    task_id: params.task_id,
                })?
                .ok_or_else(|| AppError::NotFound("Retried Memory task was pruned".to_string()));
        }
        let job_id = job_id
            .ok_or_else(|| AppError::Validation("Memory task has no durable job id".to_string()))?;
        let changed = match domain {
            "recent_snapshot" => {
                store::retry_recent_memory_job_sqlx(
                    self.db.pool(),
                    &tenant_id,
                    job_id,
                    &Utc::now().to_rfc3339(),
                )
                .await?
            }
            "session_memory" => {
                store::retry_session_memory_job_sqlx(self.db.pool(), &tenant_id, job_id).await?
            }
            "memory_recall" => {
                store::retry_memory_recall_turn_sqlx(self.db.pool(), &tenant_id, job_id).await?
            }
            _ => false,
        };
        if !changed {
            return Err(AppError::Conflict(
                "Memory task is not in a retryable durable state".to_string(),
            ));
        }
        let _ = self.runtime.task_runtime().remove_terminal(&params.task_id);
        let now = Utc::now();
        match domain {
            "recent_snapshot" => {
                self.reconcile_recent_memory_jobs_for_tenant_at(&tenant_id, now)
                    .await?;
            }
            "session_memory" => {
                self.reconcile_session_memory_jobs_for_tenant_at(&tenant_id, now)
                    .await?;
            }
            "memory_recall" => {
                self.schedule_memory_recall_turn_for_tenant(&tenant_id, job_id)
                    .await?;
            }
            _ => {}
        }
        self.find_memory_task_by_durable_job(&tenant_id, domain, job_id)
            .ok_or_else(|| AppError::NotFound("Retried Memory task was pruned".to_string()))
    }

    fn find_memory_task_by_durable_job(
        &self,
        tenant_id: &str,
        domain: &str,
        job_id: &str,
    ) -> Option<MemoryTaskView> {
        self.runtime
            .task_runtime()
            .list_for_tenant(
                tenant_id,
                TaskFilter {
                    kind: Some(TaskKind::Memory),
                    active_only: false,
                    ..Default::default()
                },
            )
            .into_iter()
            .filter(|snapshot| {
                snapshot.detail.get("domain").and_then(Value::as_str) == Some(domain)
                    && snapshot.detail.get("job_id").and_then(Value::as_str) == Some(job_id)
            })
            .max_by(|left, right| {
                left.started_at
                    .cmp(&right.started_at)
                    .then_with(|| left.task_id.cmp(&right.task_id))
            })
            .and_then(|snapshot| memory_task_view(snapshot).ok())
    }
}

fn parse_consolidation_output<T: DeserializeOwned>(raw: &str, contract: &str) -> AppResult<T> {
    let trimmed = raw.trim();
    let json_text = if let Some(rest) = trimmed.strip_prefix("```") {
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        let rest = rest.strip_prefix('\n').unwrap_or(rest);
        rest.strip_suffix("```").map(str::trim).ok_or_else(|| {
            AppError::Validation(format!(
                "MEMORY_OUTPUT_INVALID: unterminated {contract} JSON code fence"
            ))
        })?
    } else {
        trimmed
    };
    if json_text.is_empty() {
        return Err(AppError::Validation(format!(
            "MEMORY_OUTPUT_INVALID: empty {contract} output"
        )));
    }
    serde_json::from_str(json_text).map_err(|error| {
        AppError::Validation(format!(
            "MEMORY_OUTPUT_INVALID: expected one {contract} JSON value: {error}"
        ))
    })
}

fn memory_task_view(
    snapshot: crate::backend::runtime::tasks::TaskSnapshot,
) -> AppResult<MemoryTaskView> {
    let status = match snapshot.state {
        TaskState::Pending => "pending",
        TaskState::Running => "running",
        TaskState::Cancelling => "cancelling",
        TaskState::Succeeded => "succeeded",
        TaskState::Failed => "failed",
        TaskState::Canceled => "cancelled",
    };
    let kind = match snapshot.kind {
        TaskKind::Memory => "memory",
        _ => return Err(AppError::Validation("not a Memory task".to_string())),
    };
    Ok(MemoryTaskView {
        id: snapshot.task_id,
        status: status.to_string(),
        kind: kind.to_string(),
        progress: snapshot.progress,
        started_at: snapshot.started_at,
        finished_at: snapshot.finished_at,
        result: snapshot.result,
        error: snapshot.error,
        detail: snapshot.detail,
    })
}
