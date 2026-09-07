//! Thin Tauri adapter for Agent Market reads and background lifecycle tasks.

use std::{sync::Arc, time::Duration};

use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

use crate::backend::agent_market::types::AgentInstallationView;
use crate::{
    adapters::app_state::AppState,
    backend::{
        agent_market::types::{
            AgentInstallPreviewRequest, AgentInstallStartRequest, AgentLifecycleTaskSnapshot,
            AgentMarketError, AgentMarketListRequest, AgentUninstallStartRequest,
        },
        application::{
            AgentInstallPreview, AgentMarketItemView, AgentUninstallPreview, AppService,
        },
        runtime::{AppError, AppResult},
    },
};

pub(crate) const AGENT_LIFECYCLE_TASK_UPDATED_EVENT: &str = "agent-market://lifecycle-task-updated";
pub(crate) const AGENT_MARKET_REFRESH_TASK_UPDATED_EVENT: &str =
    "agent-market://refresh-task-updated";

pub(crate) async fn list_agent_market(
    state: State<'_, AppState>,
    params: AgentMarketListRequest,
) -> AppResult<Vec<AgentMarketItemView>> {
    AppService::from_runtime(&state.runtime)
        .list_agent_market(params)
        .await
}

pub(crate) async fn inspect_agent_market_item(
    state: State<'_, AppState>,
    agent_id: String,
) -> AppResult<AgentMarketItemView> {
    AppService::from_runtime(&state.runtime)
        .inspect_agent_market_item(agent_id)
        .await
}

pub(crate) fn refresh_agent_market(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot> {
    let (snapshot, should_start) = state.background_tasks.begin_agent_market_refresh()?;
    let _ = app.emit(AGENT_MARKET_REFRESH_TASK_UPDATED_EVENT, &snapshot);
    if should_start {
        let tasks = state.background_tasks.clone();
        let task_id = snapshot.id.clone();
        let runtime = state.runtime.clone();
        tauri::async_runtime::spawn(async move {
            let result = match tauri::async_runtime::spawn_blocking(move || {
                AppService::from_runtime(&runtime).refresh_agent_market_catalog()
            })
            .await
            {
                Ok(result) => result,
                Err(error) => Err(AppError::External(error.to_string())),
            };
            if let Ok(snapshot) = tasks.finish_agent_market_refresh(&task_id, result) {
                let _ = app.emit(AGENT_MARKET_REFRESH_TASK_UPDATED_EVENT, &snapshot);
            }
        });
    }
    Ok(snapshot)
}

pub(crate) fn get_agent_market_refresh_task(
    state: State<'_, AppState>,
    task_id: String,
) -> AppResult<crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot> {
    Ok(state
        .background_tasks
        .agent_market_refresh_snapshot(&task_id)?)
}

pub(crate) fn list_agent_market_refresh_tasks(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot>> {
    Ok(state.background_tasks.agent_market_refresh_snapshots()?)
}

pub(crate) async fn preview_agent_installation(
    state: State<'_, AppState>,
    params: AgentInstallPreviewRequest,
) -> AppResult<AgentInstallPreview> {
    AppService::from_runtime(&state.runtime)
        .preview_agent_installation(params)
        .await
}

pub(crate) async fn list_installed_agents(
    state: State<'_, AppState>,
) -> AppResult<Vec<AgentInstallationView>> {
    AppService::from_runtime(&state.runtime)
        .list_installed_agents()
        .await
}

pub(crate) async fn get_installed_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> AppResult<AgentInstallationView> {
    AppService::from_runtime(&state.runtime)
        .get_installed_agent(agent_id)
        .await
}

pub(crate) async fn check_agent_runtime(
    state: State<'_, AppState>,
    agent_id: String,
) -> AppResult<AgentInstallationView> {
    AppService::from_runtime(&state.runtime)
        .check_agent_runtime(agent_id)
        .await
}

pub(crate) async fn preview_agent_uninstall(
    state: State<'_, AppState>,
    agent_id: String,
) -> AppResult<AgentUninstallPreview> {
    AppService::from_runtime(&state.runtime)
        .preview_agent_uninstall(agent_id)
        .await
}

pub(crate) fn get_agent_lifecycle_task(
    state: State<'_, AppState>,
    task_id: String,
) -> AppResult<AgentLifecycleTaskSnapshot> {
    Ok(state.background_tasks.agent_lifecycle_snapshot(&task_id)?)
}

pub(crate) fn list_agent_lifecycle_tasks(
    state: State<'_, AppState>,
) -> AppResult<Vec<AgentLifecycleTaskSnapshot>> {
    Ok(state.background_tasks.agent_lifecycle_snapshots()?)
}

pub(crate) fn cancel_agent_lifecycle_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> AppResult<AgentLifecycleTaskSnapshot> {
    let snapshot = state.background_tasks.cancel_agent_lifecycle(&task_id)?;
    let _ = app.emit(AGENT_LIFECYCLE_TASK_UPDATED_EVENT, &snapshot);
    Ok(snapshot)
}

pub(crate) fn start_agent_installation(
    app: AppHandle,
    state: State<'_, AppState>,
    params: AgentInstallStartRequest,
) -> AppResult<AgentLifecycleTaskSnapshot> {
    start_agent_installation_with_action(app, state, params, "install")
}

pub(crate) fn start_agent_update(
    app: AppHandle,
    state: State<'_, AppState>,
    params: AgentInstallStartRequest,
) -> AppResult<AgentLifecycleTaskSnapshot> {
    start_agent_installation_with_action(app, state, params, "update")
}

pub(crate) fn start_agent_reinstallation(
    app: AppHandle,
    state: State<'_, AppState>,
    params: AgentInstallStartRequest,
) -> AppResult<AgentLifecycleTaskSnapshot> {
    start_agent_installation_with_action(app, state, params, "reinstall")
}

fn start_agent_installation_with_action(
    app: AppHandle,
    state: State<'_, AppState>,
    params: AgentInstallStartRequest,
    action: &str,
) -> AppResult<AgentLifecycleTaskSnapshot> {
    if params.action != action {
        return Err(AppError::Validation(format!(
            "agent lifecycle action mismatch: expected {action}"
        )));
    }
    let (snapshot, cancellation, should_start) = state.background_tasks.begin_agent_lifecycle(
        params.agent_id.clone(),
        action.to_string(),
        Some(params.catalog_version.clone()),
        Some(params.agent_version.clone()),
        Some(params.distribution_id.clone()),
        None,
        None,
    )?;
    let _ = app.emit(AGENT_LIFECYCLE_TASK_UPDATED_EVENT, &snapshot);
    if !should_start {
        return Ok(snapshot);
    }
    spawn_install_worker(app, state, params, snapshot.id.clone(), cancellation);
    Ok(snapshot)
}

pub(crate) fn start_agent_uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    params: AgentUninstallStartRequest,
) -> AppResult<AgentLifecycleTaskSnapshot> {
    let (snapshot, cancellation, should_start) = state.background_tasks.begin_agent_lifecycle(
        params.agent_id.clone(),
        "uninstall".to_string(),
        None,
        None,
        None,
        None,
        None,
    )?;
    let _ = app.emit(AGENT_LIFECYCLE_TASK_UPDATED_EVENT, &snapshot);
    if !should_start {
        return Ok(snapshot);
    }
    spawn_uninstall_worker(app, state, params, snapshot.id.clone(), cancellation);
    Ok(snapshot)
}

pub(crate) async fn enable_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> AppResult<AgentInstallationView> {
    let service = AppService::from_runtime(&state.runtime);
    let installation = service.set_agent_enabled(agent_id, true).await?;
    service
        .list_installed_agents()
        .await?
        .into_iter()
        .find(|item| item.agent_id == installation.agent_id)
        .ok_or_else(|| AppError::NotFound("Agent installation disappeared".to_string()))
}

pub(crate) async fn disable_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> AppResult<AgentInstallationView> {
    let service = AppService::from_runtime(&state.runtime);
    let installation = service.set_agent_enabled(agent_id, false).await?;
    service
        .list_installed_agents()
        .await?
        .into_iter()
        .find(|item| item.agent_id == installation.agent_id)
        .ok_or_else(|| AppError::NotFound("Agent installation disappeared".to_string()))
}

fn spawn_install_worker(
    app: AppHandle,
    state: State<'_, AppState>,
    params: AgentInstallStartRequest,
    task_id: String,
    cancellation: CancellationToken,
) {
    let tasks = state.background_tasks.clone();
    let phase_tasks = tasks.clone();
    let phase_task_id = task_id.clone();
    let phase_app = app.clone();
    let runtime = state.runtime.clone();
    let task_id_for_runtime = task_id.clone();
    let worker_tasks = tasks.clone();
    let result = tasks.spawn_extension_lifecycle(
        &task_id,
        Box::new(move |context| {
            let (bridge_stop, bridge, cancellation_flag) =
                start_cancellation_bridge(&context, cancellation.clone());
            let _ = worker_tasks.update_agent_lifecycle(
                &task_id_for_runtime,
                crate::backend::agent_market::types::LifecycleTaskPhase::Preparing,
                1,
                None,
                Vec::new(),
            );
            let result = if cancellation.is_cancelled() {
                Err(AppError::Cancelled(
                    "Agent installation cancelled".to_string(),
                ))
            } else {
                tauri::async_runtime::block_on(async {
                    AppService::from_runtime(&runtime)
                        .install_agent_with_cancellation_and_progress(
                            params,
                            Some(cancellation_flag.clone()),
                            Some(Arc::new(move |phase| {
                                if let Ok(snapshot) = phase_tasks.update_agent_lifecycle(
                                    &phase_task_id,
                                    phase,
                                    1,
                                    None,
                                    Vec::new(),
                                ) {
                                    let _ = phase_app
                                        .emit(AGENT_LIFECYCLE_TASK_UPDATED_EVENT, &snapshot);
                                }
                            })),
                        )
                        .await
                })
            };
            bridge_stop.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = bridge.join();
            let terminal = match &result {
                Ok(outcome) => worker_tasks
                    .finish_agent_lifecycle(
                        &task_id_for_runtime,
                        Ok((
                            serde_json::to_value(&outcome.installation).ok(),
                            outcome.warnings.clone(),
                        )),
                    )
                    .ok(),
                Err(error) => worker_tasks
                    .finish_agent_lifecycle(
                        &task_id_for_runtime,
                        Err(market_error_from_app(&error)),
                    )
                    .ok(),
            };
            if let Some(snapshot) = terminal {
                let _ = app.emit(AGENT_LIFECYCLE_TASK_UPDATED_EVENT, &snapshot);
            }
            match result {
                Ok(outcome) => serde_json::to_value(&outcome.installation)
                    .map_err(|error| AppError::External(error.to_string())),
                Err(error) => Err(error),
            }
        }),
    );
    if let Err(error) = result {
        let _ = tasks.finish_agent_lifecycle(&task_id, Err(market_error_from_app(&error)));
    }
}

fn spawn_uninstall_worker(
    app: AppHandle,
    state: State<'_, AppState>,
    params: AgentUninstallStartRequest,
    task_id: String,
    cancellation: CancellationToken,
) {
    let tasks = state.background_tasks.clone();
    let phase_tasks = tasks.clone();
    let phase_task_id = task_id.clone();
    let phase_app = app.clone();
    let runtime = state.runtime.clone();
    let task_id_for_runtime = task_id.clone();
    let worker_tasks = tasks.clone();
    let result = tasks.spawn_extension_lifecycle(
        &task_id,
        Box::new(move |context| {
            let (bridge_stop, bridge, cancellation_flag) =
                start_cancellation_bridge(&context, cancellation.clone());
            let _ = worker_tasks.update_agent_lifecycle(
                &task_id_for_runtime,
                crate::backend::agent_market::types::LifecycleTaskPhase::Preparing,
                1,
                None,
                Vec::new(),
            );
            let result = if cancellation.is_cancelled() {
                Err(AppError::Cancelled("Agent uninstall cancelled".to_string()))
            } else {
                tauri::async_runtime::block_on(async {
                    AppService::from_runtime(&runtime)
                        .uninstall_agent_with_cancellation_and_progress(
                            params,
                            Some(cancellation_flag.clone()),
                            Some(Arc::new(move |phase| {
                                if let Ok(snapshot) = phase_tasks.update_agent_lifecycle(
                                    &phase_task_id,
                                    phase,
                                    1,
                                    None,
                                    Vec::new(),
                                ) {
                                    let _ = phase_app
                                        .emit(AGENT_LIFECYCLE_TASK_UPDATED_EVENT, &snapshot);
                                }
                            })),
                        )
                        .await
                })
            };
            bridge_stop.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = bridge.join();
            let terminal = match &result {
                Ok(installation) => worker_tasks
                    .finish_agent_lifecycle(
                        &task_id_for_runtime,
                        Ok((serde_json::to_value(installation).ok(), Vec::new())),
                    )
                    .ok(),
                Err(error) => worker_tasks
                    .finish_agent_lifecycle(
                        &task_id_for_runtime,
                        Err(market_error_from_app(&error)),
                    )
                    .ok(),
            };
            if let Some(snapshot) = terminal {
                let _ = app.emit(AGENT_LIFECYCLE_TASK_UPDATED_EVENT, &snapshot);
            }
            match result {
                Ok(installation) => serde_json::to_value(installation)
                    .map_err(|error| AppError::External(error.to_string())),
                Err(error) => Err(error),
            }
        }),
    );
    if let Err(error) = result {
        let _ = tasks.finish_agent_lifecycle(&task_id, Err(market_error_from_app(&error)));
    }
}

fn start_cancellation_bridge(
    context: &crate::backend::runtime::tasks::TaskContext,
    cancellation: CancellationToken,
) -> (
    Arc<std::sync::atomic::AtomicBool>,
    std::thread::JoinHandle<()>,
    Arc<std::sync::atomic::AtomicBool>,
) {
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_for_thread = stop.clone();
    let worker_cancellation = Arc::new(std::sync::atomic::AtomicBool::new(
        cancellation.is_cancelled(),
    ));
    let worker_cancellation_for_thread = worker_cancellation.clone();
    let task_cancellation = context.cancellation();
    let bridge = std::thread::spawn(move || {
        while !stop_for_thread.load(std::sync::atomic::Ordering::SeqCst) {
            if task_cancellation.is_cancelled() {
                worker_cancellation_for_thread.store(true, std::sync::atomic::Ordering::SeqCst);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    (stop, bridge, worker_cancellation)
}

fn market_error_from_app(error: &AppError) -> AgentMarketError {
    let view = error.view();
    AgentMarketError::new(&view.code, &view.message, view.retryable).with_details(view.details)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        agent_market::types::AgentMarketErrorView, extension_kernel::ExtensionError,
    };

    #[test]
    fn agent_market_error_preserves_structured_extension_details() {
        let app_error = AppError::from(ExtensionError::OutputLimitExceeded {
            package_id: "private-agent".to_string(),
            stdout: true,
            stderr: false,
        });
        let error = market_error_from_app(&app_error);
        let view = AgentMarketErrorView::from(&error);

        assert_eq!(view.code, "output_limit_exceeded");
        assert!(!view.retryable);
        assert_eq!(view.details.as_ref().unwrap()["stdout"], true);
        assert_eq!(view.details.as_ref().unwrap()["stderr"], false);
    }

    #[tokio::test]
    async fn agent_market_error_parity_asserts_all_five_scenarios() {
        use crate::adapters::engine::transport::EngineError;
        use std::error::Error;

        // --- Scenario 1: SQL failure in Agent Market repository ---
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        let repo = crate::backend::agent_market::AgentInstallationRepository::new(pool);
        let repo_err = repo.get("test_agent").await.unwrap_err();
        let app_err = AppError::from(repo_err);
        assert_eq!(
            app_err.code(),
            "storage_error",
            "SQL failure must map to storage_error"
        );
        assert!(app_err.retryable(), "storage errors must be retryable");

        let mut found_sqlx = false;
        let mut cur: Option<&(dyn Error + 'static)> = app_err.source();
        while let Some(e) = cur {
            if e.is::<sqlx::Error>() {
                found_sqlx = true;
                break;
            }
            cur = e.source();
        }
        assert!(found_sqlx, "AppError source chain must contain sqlx::Error");

        let tauri_view = app_err.view();
        let engine_err = EngineError::from_app(app_err);
        assert_eq!(tauri_view.code, "storage_error");
        assert_eq!(engine_err.code, "storage_error");
        assert_eq!(tauri_view.retryable, engine_err.retryable);
        assert_eq!(tauri_view.message, engine_err.message);
        assert!(!tauri_view.message.to_ascii_lowercase().contains("select"));

        // --- Scenario 2: Illegal / invalid catalog ---
        let catalog_err = AgentMarketError::CatalogValidation {
            message: "Catalog version 99.0.0 unsupported".to_string(),
            agent_id: Some("agent_invalid".to_string()),
            field: Some("catalogVersion".to_string()),
            details: None,
        };
        let app_err = AppError::from(catalog_err);
        assert_eq!(app_err.code(), "catalog_validation_failed");
        assert!(!app_err.retryable());
        let tauri_view = app_err.view();
        let engine_err = EngineError::from_app(app_err);
        assert_eq!(tauri_view.code, engine_err.code);
        assert_eq!(tauri_view.message, engine_err.message);
        assert_eq!(tauri_view.retryable, engine_err.retryable);
        assert_eq!(tauri_view.details, engine_err.details);
        assert_eq!(
            tauri_view.details.as_ref().unwrap()["agentId"],
            "agent_invalid"
        );
        assert_eq!(
            tauri_view.details.as_ref().unwrap()["field"],
            "catalogVersion"
        );

        // --- Scenario 3: Missing installation ---
        let missing_err = AgentMarketError::InstallationNotFound {
            agent_id: "missing_agent_404".to_string(),
        };
        let app_err = AppError::from(missing_err);
        assert_eq!(app_err.code(), "agent_not_installed");
        assert!(!app_err.retryable());
        let tauri_view = app_err.view();
        let engine_err = EngineError::from_app(app_err);
        assert_eq!(tauri_view.code, engine_err.code);
        assert_eq!(tauri_view.message, engine_err.message);
        assert_eq!(tauri_view.retryable, engine_err.retryable);
        assert_eq!(tauri_view.details, engine_err.details);
        assert_eq!(
            tauri_view.details.as_ref().unwrap()["agentId"],
            "missing_agent_404"
        );

        // --- Scenario 4: Artifact / distribution mismatch ---
        let dist_err = AgentMarketError::Distribution {
            code: "distribution_artifact_mismatch".to_string(),
            message: "Checksum mismatch for distribution artifact".to_string(),
            agent_id: Some("agent_hash_fail".to_string()),
            distribution_id: Some("dist_darwin_arm64".to_string()),
            details: None,
        };
        let app_err = AppError::from(dist_err);
        assert_eq!(app_err.code(), "distribution_artifact_mismatch");
        assert!(!app_err.retryable());
        let tauri_view = app_err.view();
        let engine_err = EngineError::from_app(app_err);
        assert_eq!(tauri_view.code, engine_err.code);
        assert_eq!(tauri_view.message, engine_err.message);
        assert_eq!(tauri_view.retryable, engine_err.retryable);
        assert_eq!(tauri_view.details, engine_err.details);
        assert_eq!(
            tauri_view.details.as_ref().unwrap()["agentId"],
            "agent_hash_fail"
        );
        assert_eq!(
            tauri_view.details.as_ref().unwrap()["distributionId"],
            "dist_darwin_arm64"
        );

        // --- Scenario 5: Process timeout ---
        let proc_timeout_err =
            AgentMarketError::Process(crate::backend::host_process::HostProcessError::Timeout {
                stdout: vec![],
                stderr: vec![],
                stdout_truncated: false,
                stderr_truncated: false,
            });
        let app_err = AppError::from(proc_timeout_err);
        assert_eq!(app_err.code(), "timeout");
        assert!(app_err.retryable());
        let tauri_view = app_err.view();
        let engine_err = EngineError::from_app(app_err);
        assert_eq!(tauri_view.code, engine_err.code);
        assert_eq!(tauri_view.message, engine_err.message);
        assert_eq!(tauri_view.retryable, engine_err.retryable);
    }
}
