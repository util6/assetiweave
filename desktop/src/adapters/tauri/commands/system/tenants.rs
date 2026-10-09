use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;

#[tauri::command]
pub(crate) async fn list_tenants(state: State<'_, AppState>) -> RuntimeAppResult<Vec<Tenant>> {
    AppService::from_runtime(&state.runtime)
        .list_tenants()
        .await
}

#[tauri::command]
pub(crate) async fn get_active_tenant(state: State<'_, AppState>) -> RuntimeAppResult<Tenant> {
    AppService::from_runtime(&state.runtime)
        .active_tenant()
        .await
}

#[tauri::command]
pub(crate) async fn create_tenant(
    state: State<'_, AppState>,
    params: TenantCreateParams,
) -> RuntimeAppResult<Tenant> {
    let tenant_name = params.name.clone();
    let result = AppService::from_runtime(&state.runtime)
        .create_tenant(params)
        .await;
    match &result {
        Ok(tenant) => tracing::info!(
            action = "tenant.create",
            tenant_id = %tenant.id,
            "创建租户成功"
        ),
        Err(error) => tracing::error!(
            action = "tenant.create",
            name = %tenant_name,
            error = %error,
            "创建租户失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn switch_tenant(
    state: State<'_, AppState>,
    tenant_id: String,
) -> RuntimeAppResult<Tenant> {
    let result = AppService::from_runtime(&state.runtime)
        .switch_tenant(tenant_id.clone())
        .await;
    match &result {
        Ok(tenant) => tracing::info!(
            action = "tenant.switch",
            tenant_id = %tenant.id,
            "切换租户成功"
        ),
        Err(error) => tracing::error!(
            action = "tenant.switch",
            tenant_id = %tenant_id,
            error = %error,
            "切换租户失败"
        ),
    }
    result
}
