//! 模型定价与计费相关 Tauri Commands
//!
//! 负责模型价格在线同步、汇率维护、本地单价自定修改与价格状态查询。

use crate::adapters::app_state::AppState;
use crate::backend::application::conversations::{
    ModelPriceEntryDto, ModelPricingCatalogDto, ModelPricingStatusDto,
};
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::application::{AppError, AppService};
use tauri::State;

fn start_model_pricing_sync(
    runtime: std::sync::Arc<crate::backend::infrastructure::runtime::AppRuntime>,
    background_tasks: std::sync::Arc<
        crate::adapters::tauri::background_tasks::BackgroundTaskRegistry,
    >,
    force: bool,
    models: Option<Vec<String>>,
) -> RuntimeAppResult<crate::backend::application::system::TaskView> {
    let handle = AppService::from_runtime(&runtime).start_model_pricing_sync(force, models)?;
    let task_runtime = background_tasks
        .task_runtime()
        .ok_or_else(|| AppError::Conflict("TaskRuntime 未初始化".to_string()))?;
    let snapshot = task_runtime.get(handle.task_id()).ok_or_else(|| {
        AppError::NotFound(format!("background task not found: {}", handle.task_id()))
    })?;
    Ok(crate::backend::application::system::TaskView::from_snapshot(
        &snapshot,
    ))
}

#[tauri::command]
pub(crate) fn update_model_pricing_catalog(
    state: State<'_, AppState>,
    force: Option<bool>,
    models: Option<Vec<String>>,
) -> RuntimeAppResult<crate::backend::application::system::TaskView> {
    start_model_pricing_sync(
        state.runtime.clone(),
        state.background_tasks.clone(),
        force.unwrap_or(false),
        models,
    )
}

#[tauri::command]
pub(crate) async fn get_model_pricing_status(
    state: State<'_, AppState>,
) -> RuntimeAppResult<ModelPricingStatusDto> {
    AppService::from_runtime(&state.runtime)
        .get_model_pricing_status()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) async fn get_model_pricing_catalog(
    state: State<'_, AppState>,
) -> RuntimeAppResult<ModelPricingCatalogDto> {
    AppService::from_runtime(&state.runtime)
        .get_model_pricing_catalog()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) fn get_model_pricing_sync_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<crate::backend::application::system::TaskView>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    Ok(state
        .background_tasks
        .task_runtime()
        .list_for_tenant(
            &tenant_id,
            crate::backend::infrastructure::tasks::TaskFilter {
                kind: Some(crate::backend::infrastructure::tasks::TaskKind::ModelPricingSync),
                ..Default::default()
            },
        )
        .into_iter()
        .max_by(|left, right| left.started_at.cmp(&right.started_at))
        .map(|snapshot| crate::backend::application::system::TaskView::from_snapshot(&snapshot)))
}

#[tauri::command]
pub(crate) async fn set_model_pricing_exchange_rate(
    state: State<'_, AppState>,
    rate: f64,
) -> RuntimeAppResult<ModelPricingStatusDto> {
    AppService::from_runtime(&state.runtime)
        .set_model_pricing_exchange_rate(rate)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) async fn save_model_price_entry(
    state: State<'_, AppState>,
    entry: ModelPriceEntryDto,
) -> RuntimeAppResult<ModelPricingStatusDto> {
    AppService::from_runtime(&state.runtime)
        .save_model_price_entry(entry)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) async fn rebind_model_pricing_alias(
    state: State<'_, AppState>,
    raw_model: String,
    target_model_id: Option<String>,
) -> RuntimeAppResult<ModelPricingStatusDto> {
    AppService::from_runtime(&state.runtime)
        .rebind_model_pricing_alias(raw_model, target_model_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) async fn set_model_pricing_lock(
    state: State<'_, AppState>,
    model_id: String,
    locked: bool,
) -> RuntimeAppResult<ModelPricingStatusDto> {
    AppService::from_runtime(&state.runtime)
        .set_model_pricing_lock(model_id, locked)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) async fn set_model_pricing_ignored(
    state: State<'_, AppState>,
    raw_model: String,
    provider: Option<String>,
    ignored: bool,
) -> RuntimeAppResult<ModelPricingStatusDto> {
    AppService::from_runtime(&state.runtime)
        .set_model_pricing_ignored(raw_model, provider, ignored)
        .await
        .map_err(Into::into)
}
