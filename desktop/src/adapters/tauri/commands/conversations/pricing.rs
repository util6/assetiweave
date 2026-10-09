//! 模型定价与计费相关 Tauri Commands
//!
//! 负责模型价格在线同步、汇率维护、本地单价自定修改与价格状态查询。

use crate::adapters::app_state::AppState;
use crate::backend::application::conversations::{
    ModelPriceEntryDto, ModelPricingCatalogDto, ModelPricingStatusDto,
};
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::application::{AppError, AppService};
use tauri::{AppHandle, Emitter, State};

fn start_model_pricing_sync_background(
    app: AppHandle,
    runtime: std::sync::Arc<crate::backend::infrastructure::runtime::AppRuntime>,
    background_tasks: std::sync::Arc<
        crate::adapters::tauri::background_tasks::BackgroundTaskRegistry,
    >,
    force: bool,
    models: Option<Vec<String>>,
) -> RuntimeAppResult<crate::adapters::tauri::background_tasks::ModelPricingSyncTaskSnapshot> {
    let tenant_id = runtime.context().tenant.id.clone();
    let (snapshot, should_start) =
        background_tasks.begin_model_pricing_sync_for_tenant(&tenant_id, force)?;
    if !should_start {
        return Ok(snapshot);
    }

    let task_id = snapshot.id.clone();
    let task_runtime = background_tasks
        .task_runtime()
        .ok_or_else(|| AppError::Conflict("TaskRuntime 未初始化".to_string()))?;
    let task_detail = task_runtime
        .get(&task_id)
        .map(|task| task.detail)
        .ok_or_else(|| AppError::NotFound(format!("background task not found: {task_id}")))?;
    let task_background_tasks = background_tasks.clone();
    let task_app = app.clone();
    let task_id_for_runtime = task_id.clone();
    let target_models = models;
    let outcome =
        task_runtime.start_external_with_async(&task_id, task_detail, move |context| async move {
            let progress_app = task_app.clone();
            let progress_tasks = task_background_tasks.clone();
            let progress_task_id = task_id_for_runtime.clone();

            let mut on_progress = move |phase_str: &str, msg: &str| {
                let phase = match phase_str {
                    "fetching" => {
                        crate::adapters::tauri::background_tasks::ModelPricingSyncProgressPhase::Fetching
                    }
                    "persisting" => {
                        crate::adapters::tauri::background_tasks::ModelPricingSyncProgressPhase::Persisting
                    }
                    "recalculating" => {
                        crate::adapters::tauri::background_tasks::ModelPricingSyncProgressPhase::Recalculating
                    }
                    "completed" => {
                        crate::adapters::tauri::background_tasks::ModelPricingSyncProgressPhase::Completed
                    }
                    _ => {
                        crate::adapters::tauri::background_tasks::ModelPricingSyncProgressPhase::Preparing
                    }
                };
                match progress_tasks.update_model_pricing_sync_progress(
                    &progress_task_id,
                    phase,
                    Some(msg.to_string()),
                    None,
                    0,
                    0,
                    7.25,
                ) {
                    Ok(snapshot) => {
                        if let Err(error) =
                            progress_app.emit("model-pricing-sync-task-updated", &snapshot)
                        {
                            tracing::error!(
                                action = "conversation.pricing.sync",
                                task_id = %progress_task_id,
                                error = %error,
                                "推送后台价格同步进度失败"
                            );
                        }
                    }
                    Err(error) => tracing::error!(
                        action = "conversation.pricing.sync",
                        task_id = %progress_task_id,
                        error = %error,
                        "更新后台价格同步进度失败"
                    ),
                }
            };

            let cancellation = context.cancellation();
            if context.is_cancelled() {
                return Err(AppError::Cancelled(
                    "model pricing sync cancelled".to_string(),
                ));
            }

            let result = AppService::from_runtime(&runtime)
                .sync_model_pricing_catalog_with_control(
                    force,
                    target_models,
                    Some(&cancellation),
                    Some(&task_id_for_runtime),
                    &mut on_progress,
                )
                .await;

            match &result {
                Ok(value) => tracing::info!(
                    action = "conversation.pricing.sync",
                    task_id = %task_id_for_runtime,
                    models_count = value.models_count,
                    recalculated = value.recalculated_events_count,
                    "后台同步模型价格成功"
                ),
                Err(error) => tracing::error!(
                    action = "conversation.pricing.sync",
                    task_id = %task_id_for_runtime,
                    error = %error,
                    "后台同步模型价格失败"
                ),
            }

            let json_result =
                result.map(|dto| serde_json::to_value(dto).unwrap_or(serde_json::Value::Null));

            match task_background_tasks
                .finish_model_pricing_sync(&task_id_for_runtime, json_result)
            {
                Ok(snapshot) => {
                    if let Err(error) = task_app.emit("model-pricing-sync-task-updated", &snapshot)
                    {
                        tracing::error!(
                            action = "conversation.pricing.sync",
                            task_id = %task_id_for_runtime,
                            error = %error,
                            "推送后台价格同步任务状态失败"
                        );
                    }
                    Ok(serde_json::Value::Null)
                }
                Err(error) => Err(error),
            }
        });

    if let Err(error) = outcome {
        return Err(error.into());
    }

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn update_model_pricing_catalog(
    app: AppHandle,
    state: State<'_, AppState>,
    force: Option<bool>,
    models: Option<Vec<String>>,
) -> RuntimeAppResult<crate::adapters::tauri::background_tasks::ModelPricingSyncTaskSnapshot> {
    start_model_pricing_sync_background(
        app,
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
) -> RuntimeAppResult<Option<crate::adapters::tauri::background_tasks::ModelPricingSyncTaskSnapshot>>
{
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .model_pricing_sync_snapshot_for_tenant(&tenant_id)
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
