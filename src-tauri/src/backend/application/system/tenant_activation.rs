use std::sync::Arc;

use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::conversations::ConversationAdapterCatalog;
use crate::backend::infrastructure::runtime::{AppRuntime, RequestContextSnapshot};

pub(crate) async fn activate_tenant(runtime: &AppRuntime, tenant_id: &str) -> AppResult<Tenant> {
    activate_tenant_with_fault(runtime, tenant_id, None).await
}

pub(crate) async fn activate_tenant_with_fault(
    runtime: &AppRuntime,
    tenant_id: &str,
    fault_point: Option<&str>,
) -> AppResult<Tenant> {
    let _update_guard = runtime.context_update_gate().lock().await;
    let previous = runtime.context();
    let principal_id = previous.request_context.principal.id.clone();
    let previous_tenant_id = previous.tenant.id.clone();
    let tenant_id = tenant_id.to_string();
    let pool = runtime.pool().clone();

    let transition = async {
        let tenant =
            crate::backend::store::set_active_tenant_sqlx(&pool, &principal_id, &tenant_id).await?;
        if fault_point == Some("after_set_active_tenant") {
            return Err(AppError::External(
                "fault injected: after_set_active_tenant".to_string(),
            ));
        }
        let next_context = crate::backend::store::load_local_request_context_sqlx(&pool).await?;
        if fault_point == Some("after_load_request_context") {
            return Err(AppError::External(
                "fault injected: after_load_request_context".to_string(),
            ));
        }
        let adapters =
            crate::backend::store::list_conversation_adapters_sqlx(&pool, &next_context.tenant.id)
                .await?;
        if fault_point == Some("after_list_adapters") {
            return Err(AppError::External(
                "fault injected: after_list_adapters".to_string(),
            ));
        }
        let next_snapshot = RequestContextSnapshot {
            tenant: next_context.tenant.clone(),
            request_context: next_context,
            agent_runtime: previous.agent_runtime.clone(),
            agent_runtime_manager: previous.agent_runtime_manager.clone(),
            conversation_adapter_catalog: Arc::new(ConversationAdapterCatalog::new(adapters)),
        };
        AppResult::Ok((tenant, next_snapshot))
    }
    .await;

    let (tenant, next_snapshot) = match transition {
        Ok(transition) => Ok(transition),
        Err(error) => {
            crate::backend::store::set_active_tenant_sqlx(
                &pool,
                &principal_id,
                &previous_tenant_id,
            )
            .await
            .map_err(|rollback_error| {
                AppError::Conflict(format!(
                    "租户上下文构造失败且回滚 active tenant 失败: {error}; {rollback_error}"
                ))
            })?;
            Err(error)
        }
    }?;

    runtime.commit_tenant_snapshot(next_snapshot);
    Ok(tenant)
}

#[cfg(test)]
#[path = "tenant_activation_tests.rs"]
mod tests;
