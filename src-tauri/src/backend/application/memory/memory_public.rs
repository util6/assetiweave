use super::types::{MemoryProjectView, MemoryRebuildResult, MemoryTaskView};
use crate::backend::application::prelude::*;
use crate::backend::{
    domain::{GlobalConsolidationInput, MemoryWorkOrder, ProjectConsolidationInput},
    infrastructure::agent_execution::{
        execute_agent, AgentSessionMode, AiExecutionCancellation, AiExecutionLimits,
        AiExecutionPurpose, AiExecutionRequest,
    },
    infrastructure::tasks::{CancelOutcome, TaskFilter, TaskKind, TaskState},
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
    ) -> AppResult<Option<crate::backend::domain::L2ProjectMemoryView>> {
        let normalized_path = self
            .resolve_context_project_path(Some(project_path))
            .await?
            .unwrap_or_else(|| project_path.to_string());
        let tenant_id = self.tenant_id();
        crate::backend::application::memory::project_consolidation_pipeline::load_l2_project_memory_view(
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
    ) -> AppResult<Option<crate::backend::domain::L2ProjectMemoryView>> {
        let lock_map = crate::backend::application::memory::project_consolidation_pipeline::global_project_consolidation_lock_map();
        crate::backend::application::memory::project_consolidation_pipeline::reconcile_project_consolidation_default(
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
        work_order: MemoryWorkOrder,
        skill_text: String,
        cancellation: CancellationToken,
        maintenance_lease: Option<(&str, &str)>,
        frozen_input: Option<ProjectConsolidationInput>,
    ) -> AppResult<Option<crate::backend::domain::L2ProjectMemoryView>> {
        let settings = self.app_settings_value();
        let (agent_id, model) =
            crate::backend::application::agents::composition::resolve_agent_for(
                &crate::backend::domain::agents::ActionId::new("memory.generation"),
                &settings,
            )?;
        let runtime = self.agent_runtime.clone();
        let tenant_id = self.tenant_id().to_string();
        let lock_map = crate::backend::application::memory::project_consolidation_pipeline::
            global_project_consolidation_lock_map();
        crate::backend::application::memory::project_consolidation_pipeline::reconcile_project_consolidation_with_lease(
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
    ) -> AppResult<Option<crate::backend::domain::L3GlobalMemoryView>> {
        crate::backend::application::memory::global_consolidation_pipeline::get_global_memory_l3_view(
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
    ) -> AppResult<Option<crate::backend::domain::L3GlobalMemoryView>> {
        crate::backend::application::memory::global_consolidation_pipeline::reconcile_global_consolidation(
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
        work_order: MemoryWorkOrder,
        skill_text: String,
        cancellation: CancellationToken,
        maintenance_lease: Option<(&str, &str)>,
        frozen_input: Option<GlobalConsolidationInput>,
    ) -> AppResult<Option<crate::backend::domain::L3GlobalMemoryView>> {
        let settings = self.app_settings_value();
        let (agent_id, model) =
            crate::backend::application::agents::composition::resolve_agent_for(
                &crate::backend::domain::agents::ActionId::new("memory.generation"),
                &settings,
            )?;
        let runtime = self.agent_runtime.clone();
        let tenant_id = self.tenant_id().to_string();
        let work_order_for_runner = work_order.clone();
        let skill_text_for_runner = skill_text.clone();
        let cancellation_for_runner = cancellation.clone();
        crate::backend::application::memory::global_consolidation_pipeline::
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
                serde_json::from_value::<
                        crate::backend::infrastructure::app_settings::MemorySettings,
                    >(v.clone())
                    .ok()
            })
            .unwrap_or_default();

        crate::backend::application::memory::global_consolidation_pipeline::reconcile_source_invalidation(
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
    ) -> AppResult<crate::backend::application::memory::memory_projection::MemoryProjectionPaths>
    {
        crate::backend::application::memory::memory_projection::rebuild_markdown_projections(
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
        crate::backend::application::memory::memory_projection::purge_stale_recent_memory_snapshots(
            self.db.pool(),
            self.tenant_id(),
            cutoff,
        )
        .await
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
