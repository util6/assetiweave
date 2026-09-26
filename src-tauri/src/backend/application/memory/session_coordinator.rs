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
pub(crate) fn start_session_memory_coordinator(runtime: &Arc<AppRuntime>) {
    let cancellation = CancellationToken::new();
    let task_cancellation = cancellation.clone();
    let runtime_clone = runtime.clone();
    let runner = async move {
        while !task_cancellation.is_cancelled() {
            let service = AppService::from_runtime(&runtime_clone);
            let principal_id = runtime_clone.context().request_context.principal.id.clone();
            let result: AppResult<()> = async {
                let tenants =
                    store::list_tenants_for_principal_sqlx(runtime_clone.pool(), &principal_id)
                        .await?;
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
                    if let Err(error) = tenant_service
                        .reconcile_session_memory_jobs_for_tenant_at(&tenant.id, chrono::Utc::now())
                        .await
                    {
                        tracing::warn!(
                            action = "session_memory.coordinator.recovery",
                            tenant_id = %tenant.id,
                            error = %error,
                            "Session Memory durable coordinator reconciliation failed"
                        );
                    }
                    if let Err(error) = tenant_service
                        .reconcile_recent_memory_jobs_for_tenant_at(&tenant.id, chrono::Utc::now())
                        .await
                    {
                        tracing::warn!(
                            action = "memory.coordinator.recovery",
                            tenant_id = %tenant.id,
                            error = %error,
                            "Memory Recent Snapshot durable coordinator reconciliation failed"
                        );
                    }
                    if let Err(error) = tenant_service
                        .reconcile_memory_maintenance_jobs_for_tenant_at(
                            &tenant.id,
                            chrono::Utc::now(),
                        )
                        .await
                    {
                        tracing::warn!(
                            action = "memory.maintenance.recovery",
                            tenant_id = %tenant.id,
                            error = %error,
                            "Memory Project/Global maintenance reconciliation failed"
                        );
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
            }
            for _ in 0..10 {
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
