use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::backend::{
    application::{AppResult, AppService},
    infrastructure::runtime::AppRuntime,
    store,
};

/// Rehydrate durable Session Memory work independently of the Memory page.
/// The loop only owns scheduling; SQLite owns leases, retries, watermarks,
/// and terminal state, so an interrupted process can be rebuilt safely.
pub fn start_session_memory_coordinator(runtime: &Arc<AppRuntime>) {
    let cancellation = CancellationToken::new();
    let task_cancellation = cancellation.clone();
    let runtime_clone = runtime.clone();
    let runner = async move {
        const BASE_COORDINATOR_INTERVAL_SECS: u64 = 15;
        const MAX_COORDINATOR_BACKOFF_SECS: u64 = 60;
        let mut current_interval_secs = BASE_COORDINATOR_INTERVAL_SECS;

        while !task_cancellation.is_cancelled() {
            let service = AppService::from_runtime(&runtime_clone);
            let principal_id = runtime_clone.context().request_context.principal.id.clone();
            let mut dispatched_any = false;
            let result: AppResult<()> = async {
                // 1. 检查当前是否已经有任何租户的 Memory 流水线在运行
                let any_memory_pipeline_active = runtime_clone
                    .task_runtime()
                    .list(crate::backend::infrastructure::tasks::TaskFilter {
                        kind: Some(crate::backend::infrastructure::tasks::TaskKind::Memory),
                        active_only: true,
                        user_visible_only: false,
                    })
                    .iter()
                    .any(|t| t.task_id.starts_with("memory-pipeline:"));

                if any_memory_pipeline_active {
                    return Ok(());
                }

                // 2. 获取当前活跃租户并优先调度
                let active_tenant_id =
                    store::load_active_tenant_id_sqlx(runtime_clone.pool(), &principal_id)
                        .await
                        .ok()
                        .flatten();

                let mut tenants =
                    store::list_tenants_for_principal_sqlx(runtime_clone.pool(), &principal_id)
                        .await?;

                if let Some(active_id) = active_tenant_id.as_deref() {
                    tenants.sort_by(|a, b| {
                        let a_active = a.id == active_id;
                        let b_active = b.id == active_id;
                        b_active.cmp(&a_active)
                    });
                }

                for tenant in tenants {
                    let tenant_service = match service.for_tenant(&tenant.id).await {
                        Ok(bound) => bound,
                        Err(error) => {
                            tracing::warn!(
                                action = "memory.coordinator.tenant_binding",
                                tenant_id = %tenant.id,
                                error = %error,
                                "Memory tenant binding failed"
                            );
                            continue;
                        }
                    };
                    if tenant_service.is_memory_pipeline_active() {
                        continue;
                    }

                    // 检查上一次 memory-pipeline 是否以失败告终，若是则进入冷却退避（避免死循环风暴）
                    let task_id = format!("memory-pipeline:{}", tenant.id);
                    if let Some(last_pipeline) = runtime_clone
                        .task_runtime()
                        .get_for_tenant(&tenant.id, &task_id)
                    {
                        if last_pipeline.state
                            == crate::backend::infrastructure::tasks::TaskState::Failed
                        {
                            if let Some(finished_at_str) = last_pipeline.finished_at.as_deref() {
                                if let Ok(finished_at) =
                                    chrono::DateTime::parse_from_rfc3339(finished_at_str)
                                {
                                    let elapsed = chrono::Utc::now().signed_duration_since(
                                        finished_at.with_timezone(&chrono::Utc),
                                    );
                                    if elapsed < chrono::Duration::seconds(180) {
                                        continue;
                                    }
                                }
                            }
                        }
                    }

                    let now = chrono::Utc::now();
                    match tenant_service.has_pending_memory_work(now).await {
                        Ok(true) => {
                            if let Err(error) = tenant_service
                                .start_memory_pipeline(
                                    super::memory_pipeline::MemoryPipelineParams::background(),
                                )
                                .await
                            {
                                tracing::warn!(
                                    action = "memory.pipeline.background_dispatch",
                                    tenant_id = %tenant.id,
                                    error = %error,
                                    "Memory background pipeline dispatch failed"
                                );
                            } else {
                                dispatched_any = true;
                                // 成功启动了一个租户的流水线，立即退出本轮，保证单流水线运行
                                break;
                            }
                        }
                        Ok(false) => {}
                        Err(error) => {
                            tracing::warn!(
                                action = "memory.pipeline.check_pending_work",
                                tenant_id = %tenant.id,
                                error = %error,
                                "Failed to check pending memory work"
                            );
                        }
                    }
                    if let Err(error) = tenant_service
                        .recover_memory_recall_turns_for_tenant(&tenant.id)
                        .await
                    {
                        tracing::warn!(
                            action = "memory_recall.coordinator.recovery",
                            tenant_id = %tenant.id,
                            error = %error,
                            "Recall durable workflow reconciliation failed"
                        );
                    }
                }
                Ok(())
            }
            .await;

            if let Err(error) = result {
                tracing::warn!(
                    action = "session_memory.coordinator.tenants",
                    error = %error,
                    "Session Memory tenant enumeration failed"
                );
                current_interval_secs =
                    (current_interval_secs * 2).min(MAX_COORDINATOR_BACKOFF_SECS);
            } else if dispatched_any {
                current_interval_secs = BASE_COORDINATOR_INTERVAL_SECS;
            } else {
                current_interval_secs =
                    (current_interval_secs + 15).min(MAX_COORDINATOR_BACKOFF_SECS);
            }

            for _ in 0..(current_interval_secs * 10) {
                if task_cancellation.is_cancelled() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    };
    let join = if let Some(handle) = runtime.task_runtime().runtime_handle() {
        handle.spawn(runner)
    } else if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(runner)
    } else {
        tracing::warn!("No Tokio runtime handle available to start session memory coordinator");
        return;
    };
    runtime.register_session_memory_coordinator(cancellation, join);
}
