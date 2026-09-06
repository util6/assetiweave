//! Tauri Command 暴露层与 IPC 调用适配模块
//!
//! 该模块包含了所有前端通过 `invoke('plugin:assetiweave|...')` 调用的 Tauri Command 函数实现。
//! 包含应用配置、数据源管理、资产挂载、会话同步与翻译、Memory 以及 CLI 安装等 IPC 交互逻辑。

use crate::adapters::app_state::AppState;
use crate::adapters::prompt_clipboard::{
    copy_prompt_card_to_clipboard as copy_prompt_card_to_clipboard_impl, PromptClipboardParams,
};
use crate::adapters::tauri::app_icon::set_application_icon;
use crate::adapters::tauri::background_tasks::{
    AiExecutionTaskGetParams, AiExecutionTaskSnapshot, BackgroundTaskRegistry,
    BackgroundTaskStatus, BatchMountTaskSnapshot, ConversationDataMaintenanceTaskSnapshot,
    ConversationScriptInstallTaskSnapshot, ConversationSearchIndexTaskSnapshot,
    ConversationSyncTaskSnapshot, RemoteSkillAcquireTaskSnapshot, SkillBackupTaskSnapshot,
    SourceScanScope, SourceScanTaskSnapshot,
};
#[cfg(test)]
use crate::backend::capabilities::{
    apply_skill_group_exclusive_mount_record, apply_skill_group_mount_record,
    assetiweave_library_source_with_root, build_catalog_assets,
    build_skill_group_exclusive_mount_preview_sqlx, ensure_profile_can_be_deleted_sqlx,
    exclusive_item, mount_asset_mount_record, refresh_recorded_assets,
    scan_asset_mount_statuses_sqlx, scan_selected_sources, set_asset_mount_record,
    sync_asset_mount_observations, target_profile_from_input, unmount_asset_mount_record,
};
use crate::{
    backend::agents::types::{
        AgentCatalogEntry, AgentConnectionCheckRequest, AgentConnectionResult, AgentModelsRequest,
        AgentModelsResult,
    },
    backend::ai_execution::{
        AgentExecutionRuntime, AiExecutionCleanupReport, AiExecutionError, AiExecutionLimits,
        AiExecutionPhase, AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest,
    },
    backend::application::{
        AppService, BackgroundTaskGetParams, ConversationAdapterCatalogRefreshParams,
        ConversationAdapterLocalRegisterParams, ConversationAdapterPackageCatalogParams,
        ConversationAdapterPackageChangeParams, ConversationAdapterPackageInspectParams,
        ConversationAdapterPackageInstallParams, ConversationAdapterPackageReleaseListParams,
        ConversationAdapterPackageUninstallParams, ConversationAdapterPackageUpdateCheckParams,
        ConversationAdapterPackageUpdatePolicyParams,
        ConversationAdapterPackageVersionChangeParams, ConversationAdapterUnregisterParams,
        ConversationBlockGetParams, ConversationBlockListParams, ConversationDataAuditParams,
        ConversationDataRepairParams, ConversationDataRollbackParams,
        ConversationPartTranslationUpdateParams, ConversationQuestionGetParams,
        ConversationQuestionListParams, ConversationQuestionMergeParams,
        ConversationQuestionSplitParams, ConversationScriptCatalogParams,
        ConversationScriptInstallParams, ConversationSearchParams, ConversationSearchResult,
        ConversationSessionExportParams, ConversationSessionGetParams,
        ConversationSessionListParams, ConversationSourceDisableParams,
        ConversationSourceUpsertParams, ConversationSyncParams, ListAssetsParams,
        MemoryContextResolveParams, MemoryProjectGetParams, MemoryRecallSearchParams,
        MemoryRecallSessionCreateParams, MemoryRecallSessionGetParams,
        MemoryRecallTurnCancelParams, MemoryRecallTurnSendParams, MemoryScopeRebuildParams,
        MemoryTaskGetParams, MemoryTaskListParams, MemoryTaskRetryParams,
        RecentConversationSessionListParams, SkillAcquireParams, SkillRemoteCheckParams,
        SkillSearchParams, SkillSearchResult, SourceRemoveParams, SourceScanParams,
        TenantCreateParams, UpdateSkillBackupSettingsParams,
    },
    backend::card_translation::{
        prepare_opencode_agent_translation, ConversationTranslationConnectionRequest,
        ConversationTranslationModelsRequest, ConversationTranslationModelsResult,
        ConversationTranslationRequest, OpencodeTranslationAvailability,
        OpencodeTranslationRequest, OpencodeTranslationResult, PromptOptimizationRequest,
        PromptOptimizationResult,
    },
    backend::conversations::{
        ConversationCommandProjection, ConversationCommandProjectionParams,
        ExternalAdapterRegisterParams, ExternalAdapterScaffoldParams, ExternalAdapterTryRunParams,
        ExternalAdapterValidateParams,
    },
    backend::dto::{
        AppOverview, AppShortcut, AssetGroupInput, AssetMountStatus, AssetMountUpdateResult,
        CatalogAsset, ConversationSearchIndexStatus, ExecutionResult, MemoryContextResult,
        MemoryProjectView, MemoryRebuildResult, MemoryTaskView, NavigationModel,
        PhysicalMountStateDto, SkillBackupSettings, SkillGroupExclusiveMountInput,
        SkillGroupExclusiveMountPreview, SkillRemoteSource, SourceInput, TargetProfileInput,
    },
    backend::models::{
        Asset, AssetGroup, AssetGroupDetail, AssetKind, AssetMount, ConversationAdapter,
        ConversationSource, DeploymentPlan, DeploymentStrategy, Source, TargetProfile,
        TargetProfileDescriptor, Tenant,
    },
    backend::runtime::{
        tasks::{TaskContext, TaskFilter, TaskKind},
        AppError,
    },
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, State};

type RuntimeAppResult<T> = crate::backend::runtime::AppResult<T>;

pub(crate) const AI_EXECUTION_TASK_UPDATED_EVENT: &str = "ai-execution://task-updated";
pub(crate) const TEAM_MEMBER_SESSION_UPDATED_EVENT: &str = "team-member-session://updated";

#[tauri::command]
pub(crate) async fn set_app_window_icon(app: AppHandle, icon: Vec<u8>) -> RuntimeAppResult<()> {
    set_application_icon(app, icon).map_err(AppError::external)
}

#[tauri::command]
pub(crate) async fn get_app_overview(state: State<'_, AppState>) -> RuntimeAppResult<AppOverview> {
    AppService::from_runtime(&state.runtime).overview().await
}

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

#[tauri::command]
pub(crate) async fn get_app_settings(
    state: State<'_, AppState>,
) -> RuntimeAppResult<crate::backend::app_settings::AppSettingsFile> {
    AppService::from_runtime(&state.runtime)
        .get_app_settings()
        .await
}

#[tauri::command]
pub(crate) async fn save_app_settings(
    state: State<'_, AppState>,
    settings: serde_json::Value,
) -> RuntimeAppResult<crate::backend::app_settings::AppSettingsFile> {
    AppService::from_runtime(&state.runtime)
        .save_app_settings(settings)
        .await
}

#[tauri::command]
pub(crate) async fn initialize_app_locale_if_unset(
    state: State<'_, AppState>,
    locale: crate::backend::app_settings::AppLocale,
) -> RuntimeAppResult<crate::backend::app_settings::AppSettingsFile> {
    AppService::from_runtime(&state.runtime)
        .initialize_app_locale_if_unset(locale)
        .await
}

#[tauri::command]
pub(crate) fn cancel_app_close_prompt(state: State<'_, AppState>) -> RuntimeAppResult<()> {
    state
        .exit_prompt_open
        .store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub(crate) async fn complete_app_close(
    app: AppHandle,
    state: State<'_, AppState>,
    backup_database: bool,
) -> RuntimeAppResult<()> {
    let shutdown_sync_done = state.shutdown_sync_done.clone();
    let exit_prompt_open = state.exit_prompt_open.clone();
    let allow_close = state.allow_close.clone();
    let allow_exit = state.allow_exit.clone();
    let db_path = state.db_path.clone();
    let background_tasks = state.background_tasks.clone();
    let runtime = state.runtime.clone();

    crate::converge_ai_executions_before_close(background_tasks).await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);

    let unfinished_tasks = runtime.stop_tasks_until(deadline).await;
    if !unfinished_tasks.is_empty() {
        tracing::warn!(
            action = "app.close.tasks",
            unfinished_tasks = unfinished_tasks.len(),
            "关闭前仍有后台任务未收敛"
        );
    }

    if !shutdown_sync_done.swap(true, std::sync::atomic::Ordering::SeqCst) {
        crate::sync_before_close_with_runtime(&runtime, &db_path, backup_database).await;
    }

    let shutdown_report = runtime.shutdown_until(deadline).await;
    if !shutdown_report.is_clean() {
        tracing::warn!(
            action = "app.close.runtime",
            unfinished_tasks = shutdown_report.unfinished_task_ids.len(),
            dispatcher_remaining_events = shutdown_report.dispatcher_remaining_events,
            dispatcher_timed_out = shutdown_report.dispatcher_timed_out,
            unfinished_stages = %shutdown_report.unfinished_stages.join(","),
            "应用运行时在关闭期限内未完全收敛"
        );
    }

    exit_prompt_open.store(false, std::sync::atomic::Ordering::SeqCst);
    allow_close.store(true, std::sync::atomic::Ordering::SeqCst);
    allow_exit.store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub(crate) async fn list_assets(
    state: State<'_, AppState>,
    kind: Option<AssetKind>,
) -> RuntimeAppResult<Vec<CatalogAsset>> {
    AppService::from_runtime(&state.runtime)
        .list_assets(ListAssetsParams { kind })
        .await
}

#[tauri::command]
pub(crate) async fn list_source_assets(
    state: State<'_, AppState>,
    kind: Option<AssetKind>,
) -> RuntimeAppResult<Vec<CatalogAsset>> {
    AppService::from_runtime(&state.runtime)
        .list_source_assets(kind)
        .await
}

#[tauri::command]
pub(crate) async fn list_memory_recent(
    state: State<'_, AppState>,
    params: RecentConversationSessionListParams,
) -> RuntimeAppResult<Vec<crate::backend::application::RecentConversationSession>> {
    AppService::from_runtime(&state.runtime)
        .list_recent_conversation_sessions(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn get_memory_recent_event_target(
    state: State<'_, AppState>,
    event_id: String,
) -> RuntimeAppResult<Option<crate::backend::dto::RecentMemoryEventTarget>> {
    AppService::from_runtime(&state.runtime)
        .get_recent_memory_event_target(event_id)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn search_memory_recall(
    state: State<'_, AppState>,
    params: MemoryRecallSearchParams,
) -> RuntimeAppResult<crate::backend::models::MemoryRecallSearchResult> {
    AppService::from_runtime(&state.runtime)
        .search_memory_recall(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn resolve_memory_context(
    state: State<'_, AppState>,
    params: MemoryContextResolveParams,
) -> RuntimeAppResult<MemoryContextResult> {
    AppService::from_runtime(&state.runtime)
        .resolve_memory_context(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn get_memory_project(
    state: State<'_, AppState>,
    params: MemoryProjectGetParams,
) -> RuntimeAppResult<Option<MemoryProjectView>> {
    AppService::from_runtime(&state.runtime)
        .get_memory_project(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn rebuild_memory_scope(
    state: State<'_, AppState>,
    params: MemoryScopeRebuildParams,
) -> RuntimeAppResult<MemoryRebuildResult> {
    AppService::from_runtime(&state.runtime)
        .rebuild_memory_scope(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) fn list_memory_public_tasks(
    state: State<'_, AppState>,
    params: MemoryTaskListParams,
) -> RuntimeAppResult<Vec<MemoryTaskView>> {
    AppService::from_runtime(&state.runtime).list_memory_task_views(params)
}

#[tauri::command]
pub(crate) fn get_memory_public_task(
    state: State<'_, AppState>,
    params: MemoryTaskGetParams,
) -> RuntimeAppResult<Option<MemoryTaskView>> {
    AppService::from_runtime(&state.runtime).get_memory_task_view(params)
}

#[tauri::command]
pub(crate) fn cancel_memory_public_task(
    state: State<'_, AppState>,
    params: MemoryTaskGetParams,
) -> RuntimeAppResult<MemoryTaskView> {
    AppService::from_runtime(&state.runtime).cancel_memory_task_view(params)
}

#[tauri::command]
pub(crate) async fn retry_memory_public_task(
    state: State<'_, AppState>,
    params: MemoryTaskRetryParams,
) -> RuntimeAppResult<MemoryTaskView> {
    AppService::from_runtime(&state.runtime)
        .retry_memory_task(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn create_memory_recall_session(
    state: State<'_, AppState>,
    params: MemoryRecallSessionCreateParams,
) -> RuntimeAppResult<crate::backend::models::MemoryRecallSession> {
    AppService::from_runtime(&state.runtime)
        .create_memory_recall_session(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn get_memory_recall_session(
    state: State<'_, AppState>,
    params: MemoryRecallSessionGetParams,
) -> RuntimeAppResult<crate::backend::models::MemoryRecallSession> {
    AppService::from_runtime(&state.runtime)
        .get_memory_recall_session(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn send_memory_recall_turn(
    state: State<'_, AppState>,
    params: MemoryRecallTurnSendParams,
) -> RuntimeAppResult<crate::backend::models::MemoryRecallSession> {
    AppService::from_runtime(&state.runtime)
        .send_memory_recall_turn(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn cancel_memory_recall_turn(
    state: State<'_, AppState>,
    params: MemoryRecallTurnCancelParams,
) -> RuntimeAppResult<crate::backend::models::MemoryRecallSession> {
    AppService::from_runtime(&state.runtime)
        .cancel_memory_recall_turn(params)
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn get_skill_backup_settings(
    state: State<'_, AppState>,
) -> RuntimeAppResult<SkillBackupSettings> {
    AppService::from_runtime(&state.runtime)
        .get_skill_backup_settings()
        .await
}

#[tauri::command]
pub(crate) async fn update_skill_backup_settings(
    state: State<'_, AppState>,
    root_path: String,
    migrate: Option<bool>,
) -> RuntimeAppResult<SkillBackupSettings> {
    let root_path_input = root_path.clone();
    let migrate_input = migrate.unwrap_or(true);
    let result = AppService::from_runtime(&state.runtime)
        .update_skill_backup_settings(UpdateSkillBackupSettingsParams {
            root_path,
            migrate: migrate.unwrap_or(true),
        })
        .await;

    match &result {
        Ok(_) => tracing::info!(
            action = "skill.backup.settings.update",
            resource = "skill_backup",
            "更新 Skill 备份目录成功"
        ),
        Err(error) => tracing::error!(
            action = "skill.backup.settings.update",
            resource = "skill_backup",
            migrate = migrate_input,
            error_code = %error.code(),
            "更新 Skill 备份目录失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn backup_skill(
    state: State<'_, AppState>,
    asset_id: String,
) -> RuntimeAppResult<CatalogAsset> {
    let result = AppService::from_runtime(&state.runtime)
        .backup_skill(asset_id.clone())
        .await;

    match &result {
        Ok(asset) => tracing::info!(
            action = "skill.backup",
            asset_id = %asset.asset.id,
            asset_name = %asset.asset.name,
            asset_kind = ?asset.asset.kind,
            "备份 Skill 成功"
        ),
        Err(error) => tracing::error!(
            action = "skill.backup",
            asset_id = %asset_id,
            error = %error,
            "备份 Skill 失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn backup_skills(
    app: AppHandle,
    state: State<'_, AppState>,
    asset_ids: Vec<String>,
) -> RuntimeAppResult<SkillBackupTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let (snapshot, should_start) = state
        .background_tasks
        .begin_skill_backup_for_tenant(&tenant_id, asset_ids)?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let background_tasks = state.background_tasks.clone();
    let task_id = snapshot.id.clone();
    let task_asset_ids = snapshot.asset_ids.clone();
    tauri::async_runtime::spawn(async move {
        let progress_app = app.clone();
        let progress_tasks = background_tasks.clone();
        let progress_task_id = task_id.clone();
        let result = AppService::from_runtime(&runtime)
            .backup_skills_with_progress(task_asset_ids, |completed_count, next_asset_id| {
                match progress_tasks.update_skill_backup_progress(
                    &progress_task_id,
                    completed_count,
                    next_asset_id.map(str::to_string),
                ) {
                    Ok(snapshot) => emit_skill_backup_task(&progress_app, &snapshot),
                    Err(error) => tracing::error!(
                        action = "skill.backup.background",
                        task_id = %progress_task_id,
                        error = %error,
                        "更新 Skill 后台备份进度失败"
                    ),
                }
            })
            .await;
        match &result {
            Ok(assets) => tracing::info!(
                action = "skill.backup.background",
                task_id = %task_id,
                asset_count = assets.len(),
                "后台备份 Skill 成功"
            ),
            Err(error) => tracing::error!(
                action = "skill.backup.background",
                task_id = %task_id,
                error = %error,
                "后台备份 Skill 失败"
            ),
        }
        match background_tasks.finish_skill_backup(&task_id, result) {
            Ok(snapshot) => emit_skill_backup_task(&app, &snapshot),
            Err(error) => tracing::error!(
                action = "skill.backup.background",
                task_id = %task_id,
                error = %error,
                "更新 Skill 后台备份任务状态失败"
            ),
        }
    });

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_skill_backup_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<SkillBackupTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .skill_backup_snapshot_for_tenant(&tenant_id)
}

fn emit_skill_backup_task(app: &AppHandle, snapshot: &SkillBackupTaskSnapshot) {
    if let Err(error) = app.emit("skill-backup-task-updated", snapshot) {
        tracing::error!(
            action = "skill.backup.background",
            task_id = %snapshot.id,
            error = %error,
            "推送 Skill 后台备份任务状态失败"
        );
    }
}

fn emit_conversation_script_install_task(
    app: &AppHandle,
    snapshot: &ConversationScriptInstallTaskSnapshot,
) {
    if let Err(error) = app.emit("conversation-script-install-task-updated", snapshot) {
        tracing::error!(
            action = "conversation.script.install",
            task_id = %snapshot.id,
            error = %error,
            "推送对话脚本后台安装任务状态失败"
        );
    }
}

fn spawn_conversation_lifecycle_task<F>(
    app: AppHandle,
    background_tasks: Arc<BackgroundTaskRegistry>,
    task_id: String,
    operation: &'static str,
    work: F,
) -> RuntimeAppResult<()>
where
    F: FnOnce() -> RuntimeAppResult<Value> + Send + 'static,
{
    let task_id_for_runtime = task_id.clone();
    let background_tasks_for_runtime = background_tasks.clone();
    let app_for_runtime = app.clone();
    let operation_for_runtime = operation;
    let task = Box::new(move |context: TaskContext| {
        let result = if context.is_cancelled() {
            Err(AppError::Cancelled(format!(
                "{operation_for_runtime} task cancelled"
            )))
        } else {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|_| {
                Err(AppError::Process(format!(
                    "{operation_for_runtime} task panicked"
                )))
            })
        };
        match &result {
            Ok(value) => tracing::info!(
                action = operation_for_runtime,
                task_id = %task_id_for_runtime,
                result = %value,
                "扩展生命周期任务成功"
            ),
            Err(error) => tracing::error!(
                action = operation_for_runtime,
                task_id = %task_id_for_runtime,
                error = %error,
                "扩展生命周期任务失败"
            ),
        }
        let projection_result = match &result {
            Ok(value) => Ok(value.clone()),
            Err(error) => Err(AppError::from(error.view())),
        };
        match background_tasks_for_runtime
            .finish_conversation_script_install(&task_id_for_runtime, projection_result)
        {
            Ok(snapshot) => emit_conversation_script_install_task(&app_for_runtime, &snapshot),
            Err(error) => tracing::error!(
                action = operation_for_runtime,
                task_id = %task_id_for_runtime,
                error = %error,
                "更新扩展生命周期任务状态失败"
            ),
        }
        result
    });
    if let Err(error) = background_tasks.spawn_extension_lifecycle(&task_id, task) {
        let projection_error = AppError::from(error.view());
        let _ =
            background_tasks.finish_conversation_script_install(&task_id, Err(projection_error));
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn search_skills(
    state: State<'_, AppState>,
    params: SkillSearchParams,
) -> RuntimeAppResult<SkillSearchResult> {
    let query_input = params.query.clone();
    let result = (|| AppService::from_runtime(&state.runtime).search_skills(params))();

    match &result {
        Ok(result) => tracing::info!(
            action = "skill.search",
            query = %result.query,
            candidate_count = result.candidates.len(),
            "搜索 Skill 成功"
        ),
        Err(error) => tracing::error!(
            action = "skill.search",
            query = %query_input,
            error = %error,
            "搜索 Skill 失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn start_skill_acquire(
    app: AppHandle,
    state: State<'_, AppState>,
    params: SkillAcquireParams,
) -> RuntimeAppResult<RemoteSkillAcquireTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let (snapshot, should_start) = state
        .background_tasks
        .begin_remote_skill_acquire_for_tenant(&tenant_id, &params)?;
    let _ = app.emit("skill-remote://acquire-task-updated", &snapshot);
    if !should_start {
        return Ok(snapshot);
    }

    let tasks = state.background_tasks.clone();
    let runtime = state.runtime.clone();
    let task_id = snapshot.id.clone();
    let cancellation = tasks
        .task_runtime()
        .ok_or_else(|| AppError::Conflict("TaskRuntime 未初始化".to_string()))?
        .cancellation_token(&task_id)?;
    tauri::async_runtime::spawn(async move {
        let emit_app = app.clone();
        let emit_tasks = tasks.clone();
        let update_phase = |phase: &str| {
            if let Ok(snapshot) = emit_tasks.update_remote_skill_acquire_phase(&task_id, phase) {
                let _ = emit_app.emit("skill-remote://acquire-task-updated", &snapshot);
            }
        };
        let result = AppService::from_runtime(&runtime)
            .acquire_skill_with_cancellation_and_progress(
                params,
                Some(&cancellation),
                Some(&update_phase),
            )
            .await;
        if let Err(error) = &result {
            tracing::error!(
                action = "skill.acquire",
                task_id = %task_id,
                error = %error,
                "获取 Skill 失败"
            );
        }
        let snapshot = emit_tasks.finish_remote_skill_acquire(&task_id, result);
        if let Ok(snapshot) = snapshot {
            let _ = emit_app.emit("skill-remote://acquire-task-updated", &snapshot);
        }
    });
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn acquire_skill(
    app: AppHandle,
    state: State<'_, AppState>,
    params: SkillAcquireParams,
) -> RuntimeAppResult<RemoteSkillAcquireTaskSnapshot> {
    start_skill_acquire(app, state, params)
}

#[tauri::command]
pub(crate) fn get_skill_acquire_task(
    state: State<'_, AppState>,
    task_id: Option<String>,
) -> RuntimeAppResult<Option<RemoteSkillAcquireTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    if let Some(task_id) = task_id {
        return Ok(Some(
            state
                .background_tasks
                .remote_skill_acquire_snapshot_for_tenant(&tenant_id, &task_id)?,
        ));
    }
    Ok(state
        .background_tasks
        .remote_skill_acquire_snapshots_for_tenant(&tenant_id)?
        .into_iter()
        .max_by(|left, right| left.started_at.cmp(&right.started_at)))
}

#[tauri::command]
pub(crate) fn list_skill_acquire_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<RemoteSkillAcquireTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .remote_skill_acquire_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_skill_acquire_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<RemoteSkillAcquireTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let snapshot = state
        .background_tasks
        .cancel_remote_skill_acquire_for_tenant(&tenant_id, &task_id)?;
    let _ = app.emit("skill-remote://acquire-task-updated", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn list_skill_remote_sources(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<SkillRemoteSource>> {
    let result = AppService::from_runtime(&state.runtime)
        .list_skill_remote_sources()
        .await;

    match &result {
        Ok(sources) => tracing::info!(
            action = "skill.remote.list",
            source_count = sources.len(),
            "读取远程 Skill 来源成功"
        ),
        Err(error) => tracing::error!(
            action = "skill.remote.list",
            error = %error,
            "读取远程 Skill 来源失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn check_skill_remote_sources(
    state: State<'_, AppState>,
    params: SkillRemoteCheckParams,
) -> RuntimeAppResult<Vec<SkillRemoteSource>> {
    let asset_id_filter = params.asset_id.clone();
    let result = AppService::from_runtime(&state.runtime)
        .check_skill_remote_sources(params)
        .await;

    match &result {
        Ok(sources) => tracing::info!(
            action = "skill.remote.check",
            checked_count = sources.len(),
            changed_count = sources
                .iter()
                .filter(|source| source.status == "changed")
                .count(),
            "检查远程 Skill 来源成功"
        ),
        Err(error) => tracing::error!(
            action = "skill.remote.check",
            asset_id = ?asset_id_filter,
            error = %error,
            "检查远程 Skill 来源失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn update_asset_description(
    state: State<'_, AppState>,
    asset_id: String,
    description: Option<String>,
) -> RuntimeAppResult<Asset> {
    let result = AppService::from_runtime(&state.runtime)
        .update_asset_description(asset_id.clone(), description)
        .await;

    match &result {
        Ok(asset) => tracing::info!(
            action = "asset.update_description",
            asset_id = %asset.id,
            asset_name = %asset.name,
            asset_kind = ?asset.kind,
            "更新资产说明成功"
        ),
        Err(error) => tracing::error!(
            action = "asset.update_description",
            asset_id = %asset_id,
            error = %error,
            "更新资产说明失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn delete_asset(
    state: State<'_, AppState>,
    asset_id: String,
    unmount: Option<bool>,
) -> RuntimeAppResult<Asset> {
    let result = AppService::from_runtime(&state.runtime)
        .delete_asset(asset_id.clone(), unmount.unwrap_or(false))
        .await;

    match &result {
        Ok(asset) => tracing::info!(
            action = "asset.delete",
            asset_id = %asset.id,
            asset_name = %asset.name,
            asset_kind = ?asset.kind,
            "删除资产成功"
        ),
        Err(error) => tracing::error!(
            action = "asset.delete",
            asset_id = %asset_id,
            error = %error,
            "删除资产失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn list_sources(state: State<'_, AppState>) -> RuntimeAppResult<Vec<Source>> {
    AppService::from_runtime(&state.runtime)
        .list_sources()
        .await
}

#[tauri::command]
pub(crate) async fn list_skill_sources(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<Source>> {
    AppService::from_runtime(&state.runtime)
        .list_skill_sources()
        .await
}

#[tauri::command]
pub(crate) async fn create_source(
    state: State<'_, AppState>,
    source: SourceInput,
) -> RuntimeAppResult<Source> {
    let source_name = source.name.clone();
    let source_kind = format!("{:?}", source.kind);
    let result = AppService::from_runtime(&state.runtime)
        .add_source(source)
        .await;

    match &result {
        Ok(source) => tracing::info!(
            action = "source.create",
            source_id = %source.id,
            source_name = %source.name,
            source_kind = ?source.kind,
            "添加数据来源成功"
        ),
        Err(error) => tracing::error!(
            action = "source.create",
            source_name = %source_name,
            source_kind = %source_kind,
            error = %error,
            "添加数据来源失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn update_source(
    state: State<'_, AppState>,
    source: Source,
) -> RuntimeAppResult<Source> {
    let source_id = source.id.clone();
    let source_name = source.name.clone();
    let source_kind = format!("{:?}", source.kind);
    let result = AppService::from_runtime(&state.runtime)
        .update_source(source)
        .await;

    match &result {
        Ok(source) => tracing::info!(
            action = "source.update",
            source_id = %source.id,
            source_name = %source.name,
            source_kind = ?source.kind,
            "更新数据来源成功"
        ),
        Err(error) => tracing::error!(
            action = "source.update",
            source_id = %source_id,
            source_name = %source_name,
            source_kind = %source_kind,
            error = %error,
            "更新数据来源失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn delete_source(state: State<'_, AppState>, id: String) -> RuntimeAppResult<()> {
    let result = AppService::from_runtime(&state.runtime)
        .remove_source(SourceRemoveParams {
            id: id.clone(),
            dry_run: false,
            yes: true,
        })
        .await
        .map(|_| ());

    match &result {
        Ok(()) => tracing::info!(
            action = "source.delete",
            source_id = %id,
            "删除数据来源成功"
        ),
        Err(error) => tracing::error!(
            action = "source.delete",
            source_id = %id,
            error = %error,
            "删除数据来源失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn list_profiles(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<TargetProfile>> {
    AppService::from_runtime(&state.runtime)
        .list_profiles()
        .await
}

#[tauri::command]
pub(crate) fn list_target_profile_descriptors(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<TargetProfileDescriptor>> {
    AppService::from_runtime(&state.runtime).list_target_profile_descriptors()
}

#[tauri::command]
pub(crate) async fn refresh_target_profile_descriptors(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<TargetProfileDescriptor>> {
    AppService::from_runtime(&state.runtime)
        .refresh_target_profile_descriptors()
        .await
}

#[tauri::command]
pub(crate) async fn create_profile(
    state: State<'_, AppState>,
    input: TargetProfileInput,
) -> RuntimeAppResult<TargetProfile> {
    let profile_name = input.name.clone();
    let target_path_count = input
        .target_paths
        .as_ref()
        .map(|paths| paths.len())
        .unwrap_or(0);
    let app_kind = input.app_kind.map(|k| format!("{k:?}"));
    let result = AppService::from_runtime(&state.runtime)
        .create_profile(input)
        .await;

    match &result {
        Ok(profile) => tracing::info!(
            action = "profile.create",
            profile_id = %profile.id,
            profile_name = %profile.name,
            target_path_count = profile.target_paths.len(),
            "添加目标 APP 配置成功"
        ),
        Err(error) => tracing::error!(
            action = "profile.create",
            profile_name = %profile_name,
            target_path_count = target_path_count,
            app_kind = ?app_kind,
            error = %error,
            "添加目标 APP 配置失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn update_profile(
    state: State<'_, AppState>,
    profile: TargetProfile,
) -> RuntimeAppResult<TargetProfile> {
    let profile_id = profile.id.clone();
    let profile_name = profile.name.clone();
    let target_path_count = profile.target_paths.len();
    let result = AppService::from_runtime(&state.runtime)
        .update_profile(profile)
        .await;

    match &result {
        Ok(profile) => tracing::info!(
            action = "profile.update",
            profile_id = %profile.id,
            profile_name = %profile.name,
            target_path_count = profile.target_paths.len(),
            "更新目标 APP 配置成功"
        ),
        Err(error) => tracing::error!(
            action = "profile.update",
            profile_id = %profile_id,
            profile_name = %profile_name,
            target_path_count = target_path_count,
            error = %error,
            "更新目标 APP 配置失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn delete_profile(state: State<'_, AppState>, id: String) -> RuntimeAppResult<()> {
    let result = AppService::from_runtime(&state.runtime)
        .delete_profile(id.clone())
        .await;

    match &result {
        Ok(()) => tracing::info!(
            action = "profile.delete",
            profile_id = %id,
            "删除目标 APP 配置成功"
        ),
        Err(error) => tracing::error!(
            action = "profile.delete",
            profile_id = %id,
            error = %error,
            "删除目标 APP 配置失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn get_navigation_model(
    state: State<'_, AppState>,
) -> RuntimeAppResult<NavigationModel> {
    AppService::from_runtime(&state.runtime)
        .navigation_model()
        .await
}

#[tauri::command]
pub(crate) async fn update_navigation_model(
    state: State<'_, AppState>,
    model: NavigationModel,
) -> RuntimeAppResult<NavigationModel> {
    let active_rail_id = model.active_rail_id.clone();
    let active_header_tab_id = model.active_header_tab_id.clone();
    let active_sub_nav_id = model.active_sub_nav_id.clone();
    let rail_count = model.rail_items.len();
    let result = AppService::from_runtime(&state.runtime)
        .update_navigation_model(model)
        .await;

    match &result {
        Ok(_) => tracing::info!(
            action = "navigation.update",
            active_rail_id = %active_rail_id,
            active_header_tab_id = %active_header_tab_id,
            active_sub_nav_id = %active_sub_nav_id,
            rail_count = rail_count,
            "更新导航配置成功"
        ),
        Err(error) => tracing::error!(
            action = "navigation.update",
            active_rail_id = %active_rail_id,
            active_header_tab_id = %active_header_tab_id,
            active_sub_nav_id = %active_sub_nav_id,
            rail_count = rail_count,
            error = %error,
            "更新导航配置失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn list_app_shortcuts(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<AppShortcut>> {
    AppService::from_runtime(&state.runtime)
        .list_app_shortcuts()
        .await
}

#[tauri::command]
pub(crate) async fn list_app_shortcut_settings(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<AppShortcut>> {
    AppService::from_runtime(&state.runtime)
        .list_app_shortcut_settings()
        .await
}

#[tauri::command]
pub(crate) async fn update_app_shortcuts(
    state: State<'_, AppState>,
    shortcuts: Vec<AppShortcut>,
) -> RuntimeAppResult<Vec<AppShortcut>> {
    let shortcut_count = shortcuts.len();
    let result = AppService::from_runtime(&state.runtime)
        .update_app_shortcuts(shortcuts)
        .await;

    match &result {
        Ok(shortcuts) => tracing::info!(
            action = "settings.app_shortcuts.update",
            shortcut_count = shortcuts.len(),
            "更新 APP 快捷入口配置成功"
        ),
        Err(error) => tracing::error!(
            action = "settings.app_shortcuts.update",
            shortcut_count = shortcut_count,
            error = %error,
            "更新 APP 快捷入口配置失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn list_asset_mounts(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> RuntimeAppResult<Vec<AssetMount>> {
    AppService::from_runtime(&state.runtime)
        .list_asset_mounts(asset_id.as_deref())
        .await
}

#[tauri::command]
pub(crate) async fn list_asset_mount_statuses(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> RuntimeAppResult<Vec<AssetMountStatus>> {
    AppService::from_runtime(&state.runtime)
        .list_asset_mount_statuses(asset_id.as_deref())
        .await
}

#[tauri::command]
pub(crate) async fn refresh_asset_mount_statuses(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> RuntimeAppResult<Vec<AssetMountStatus>> {
    let result = AppService::from_runtime(&state.runtime)
        .refresh_asset_mount_statuses(asset_id.as_deref())
        .await;

    match &result {
        Ok(statuses) => {
            let mounted = statuses
                .iter()
                .filter(|status| status.state == PhysicalMountStateDto::Mounted)
                .count();
            let issues = statuses
                .iter()
                .filter(|status| {
                    matches!(
                        status.state,
                        PhysicalMountStateDto::Conflict | PhysicalMountStateDto::Broken
                    )
                })
                .count();
            tracing::info!(
                action = "mount_status.refresh",
                asset_id = ?asset_id,
                count = statuses.len(),
                mounted = mounted,
                issues = issues,
                "刷新挂载状态成功"
            );
        }
        Err(error) => tracing::error!(
            action = "mount_status.refresh",
            asset_id = ?asset_id,
            error = %error,
            "刷新挂载状态失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn list_skill_groups(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<AssetGroupDetail>> {
    AppService::from_runtime(&state.runtime)
        .list_skill_groups()
        .await
}

#[tauri::command]
pub(crate) async fn create_skill_group(
    state: State<'_, AppState>,
    input: AssetGroupInput,
) -> RuntimeAppResult<AssetGroupDetail> {
    let group_name = input.name.clone();
    let result = AppService::from_runtime(&state.runtime)
        .create_skill_group(input)
        .await;

    match &result {
        Ok(detail) => tracing::info!(
            action = "skill_group.create",
            group_id = %detail.group.id,
            group_name = %detail.group.name,
            member_count = detail.members.len(),
            "添加 skill 分组成功"
        ),
        Err(error) => tracing::error!(
            action = "skill_group.create",
            group_name = %group_name,
            error = %error,
            "添加 skill 分组失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn update_skill_group(
    state: State<'_, AppState>,
    group: AssetGroup,
) -> RuntimeAppResult<AssetGroupDetail> {
    let group_id = group.id.clone();
    let group_name = group.name.clone();
    let result = AppService::from_runtime(&state.runtime)
        .update_skill_group(group)
        .await;

    match &result {
        Ok(detail) => tracing::info!(
            action = "skill_group.update",
            group_id = %detail.group.id,
            group_name = %detail.group.name,
            member_count = detail.members.len(),
            "更新 skill 分组成功"
        ),
        Err(error) => tracing::error!(
            action = "skill_group.update",
            group_id = %group_id,
            group_name = %group_name,
            error = %error,
            "更新 skill 分组失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn delete_skill_group(
    state: State<'_, AppState>,
    group_id: String,
) -> RuntimeAppResult<()> {
    let result = AppService::from_runtime(&state.runtime)
        .delete_skill_group(group_id.clone())
        .await;

    match &result {
        Ok(()) => tracing::info!(
            action = "skill_group.delete",
            group_id = %group_id,
            "删除 skill 分组成功"
        ),
        Err(error) => tracing::error!(
            action = "skill_group.delete",
            group_id = %group_id,
            error = %error,
            "删除 skill 分组失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn set_skill_group_manual_members(
    state: State<'_, AppState>,
    group_id: String,
    asset_ids: Vec<String>,
) -> RuntimeAppResult<AssetGroupDetail> {
    let asset_count = asset_ids.len();
    let result = AppService::from_runtime(&state.runtime)
        .set_skill_group_manual_members(group_id.clone(), asset_ids)
        .await;

    match &result {
        Ok(detail) => tracing::info!(
            action = "skill_group.members.update",
            group_id = %detail.group.id,
            group_name = %detail.group.name,
            member_count = detail.members.len(),
            "更新 skill 分组成员成功"
        ),
        Err(error) => tracing::error!(
            action = "skill_group.members.update",
            group_id = %group_id,
            asset_count = asset_count,
            error = %error,
            "更新 skill 分组成员失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn preview_skill_group_exclusive_mount(
    state: State<'_, AppState>,
    input: SkillGroupExclusiveMountInput,
) -> RuntimeAppResult<SkillGroupExclusiveMountPreview> {
    let profile_id = input.profile_id.clone();
    let group_count = input.group_ids.len();
    let result = AppService::from_runtime(&state.runtime)
        .preview_skill_group_exclusive_mount(input)
        .await;

    match &result {
        Ok(preview) => {
            tracing::info!(
                action = "skill_group.exclusive.preview",
                profile_id = %preview.profile_id,
                group_count = preview.group_ids.len(),
                selected_count = preview.selected_skill_ids.len(),
                keep_count = preview.keep_count,
                mount_count = preview.mount_count,
                unmount_count = preview.unmount_count,
                skipped_count = preview.skipped_count,
                "预览 skill 分组独占挂载成功"
            );
            for item in &preview.skipped {
                tracing::warn!(
                    action = "skill_group.exclusive.skipped",
                    profile_id = %preview.profile_id,
                    asset_id = %item.asset_id,
                    skill_name = %item.name,
                    reason = %item.reason,
                    "skill 独占挂载预览跳过"
                );
            }
        }
        Err(error) => tracing::error!(
            action = "skill_group.exclusive.preview",
            profile_id = %profile_id,
            group_count = group_count,
            error = %error,
            "预览 skill 分组独占挂载失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn toggle_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
) -> RuntimeAppResult<AssetMount> {
    let result = AppService::from_runtime(&state.runtime)
        .toggle_asset_mount(&asset_id, &profile_id)
        .await;

    if let Err(error) = &result {
        tracing::error!(
            action = "skill.mount.toggle",
            asset_id = %asset_id,
            profile_id = %profile_id,
            error = %error,
            "切换 skill 挂载失败"
        );
    }
    result
}

#[tauri::command]
pub(crate) async fn unmount_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
) -> RuntimeAppResult<AssetMountUpdateResult> {
    let result = AppService::from_runtime(&state.runtime)
        .unmount_asset_by_id(&asset_id, &profile_id)
        .await;

    if let Err(error) = &result {
        tracing::error!(
            action = "skill.unmount.command",
            asset_id = %asset_id,
            profile_id = %profile_id,
            error = %error,
            "卸载 skill 命令失败"
        );
    }
    result
}

#[tauri::command]
pub(crate) async fn mount_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
) -> RuntimeAppResult<AssetMountUpdateResult> {
    let result = AppService::from_runtime(&state.runtime)
        .mount_asset_by_id(&asset_id, &profile_id)
        .await;

    if let Err(error) = &result {
        tracing::error!(
            action = "skill.mount.command",
            asset_id = %asset_id,
            profile_id = %profile_id,
            error = %error,
            "挂载 skill 命令失败"
        );
    }
    result
}

#[tauri::command]
pub(crate) async fn set_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
    enabled: bool,
    strategy: Option<DeploymentStrategy>,
) -> RuntimeAppResult<AssetMount> {
    let result = AppService::from_runtime(&state.runtime)
        .set_asset_mount(&asset_id, &profile_id, enabled, strategy)
        .await;

    if let Err(error) = &result {
        tracing::error!(
            action = "skill.mount.set",
            asset_id = %asset_id,
            profile_id = %profile_id,
            enabled = enabled,
            error = %error,
            "设置 skill 挂载关系失败"
        );
    }
    result
}

pub(crate) const SOURCE_SCAN_TASK_UPDATED_EVENT: &str = "source-scan-task-updated";

#[tauri::command]
pub(crate) fn start_source_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: Option<AssetKind>,
    scope: Option<SourceScanScope>,
) -> RuntimeAppResult<SourceScanTaskSnapshot> {
    let scope = scope.unwrap_or(SourceScanScope::All);
    let scan_kind = if scope == SourceScanScope::Skills {
        Some(AssetKind::Skill)
    } else {
        kind
    };
    let tenant_id = state.runtime.context().tenant.id.clone();
    let (snapshot, started) = state
        .background_tasks
        .begin_source_scan(&tenant_id, scope, scan_kind)?;
    if !started {
        return state
            .background_tasks
            .source_scan_snapshot_for_tenant(&tenant_id, &snapshot.id);
    }

    let task_id = snapshot.id.clone();
    let runtime = state.runtime.clone();
    let tasks = state.background_tasks.clone();
    let task_context = runtime.task_runtime().task_context(&task_id)?;
    let params = SourceScanParams {
        kind: scan_kind,
        dry_run: false,
    };
    let skill_sources_only = scope == SourceScanScope::Skills;
    let worker_app = app.clone();
    let worker_task_id = task_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = match tokio::spawn(async move {
            let service = AppService::from_runtime(&runtime);
            crate::backend::application::SourceScanWorkflow::run(
                &service,
                params,
                &task_context,
                skill_sources_only,
            )
            .await
        })
        .await
        {
            Ok(inner_result) => inner_result,
            Err(_) => Err(AppError::Process("source scan worker panicked".to_string())),
        };
        if let Ok(snapshot) = tasks.finish_source_scan(&worker_task_id, result) {
            let _ = worker_app.emit(SOURCE_SCAN_TASK_UPDATED_EVENT, &snapshot);
        }
    });
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_source_scan_task(
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<SourceScanTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .source_scan_snapshot_for_tenant(&tenant_id, &task_id)
}

#[tauri::command]
pub(crate) fn list_source_scan_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<SourceScanTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .source_scan_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_source_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<SourceScanTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let snapshot = state
        .background_tasks
        .cancel_source_scan_for_tenant(&tenant_id, &task_id)?;
    let _ = app.emit(SOURCE_SCAN_TASK_UPDATED_EVENT, &snapshot);
    Ok(snapshot)
}

pub(crate) const BATCH_MOUNT_TASK_UPDATED_EVENT: &str = "batch-mount-task-updated";

#[tauri::command]
pub(crate) fn start_batch_mount(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: String,
    group_id: Option<String>,
    profile_id: String,
    enabled: Option<bool>,
    group_ids: Option<Vec<String>>,
    asset_ids: Option<Vec<String>>,
) -> RuntimeAppResult<BatchMountTaskSnapshot> {
    let mode = mode.trim().to_ascii_lowercase();
    if !matches!(mode.as_str(), "explicit" | "group" | "exclusive") {
        return Err(AppError::Validation(format!(
            "unsupported batch mount mode: {mode}"
        )));
    }
    if profile_id.trim().is_empty() {
        return Err(AppError::Validation("profile_id is required".to_string()));
    }
    let group_id = group_id.map(|value| value.trim().to_string());
    let group_ids = group_ids
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let asset_ids = asset_ids
        .unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if mode == "explicit" && asset_ids.is_empty() {
        return Err(AppError::Validation(
            "asset_ids are required for explicit batch mount".to_string(),
        ));
    }
    if mode == "group" && group_id.as_deref().is_none_or(str::is_empty) {
        return Err(AppError::Validation(
            "group_id is required for group batch mount".to_string(),
        ));
    }
    if mode == "exclusive" && group_ids.is_empty() {
        return Err(AppError::Validation(
            "group_ids are required for exclusive batch mount".to_string(),
        ));
    }
    let workflow_input = match mode.as_str() {
        "explicit" => crate::backend::application::BatchMountWorkflowInput::Explicit {
            asset_ids: asset_ids.clone(),
            profile_id: profile_id.clone(),
            enabled: enabled.unwrap_or(true),
        },
        "group" => crate::backend::application::BatchMountWorkflowInput::Group {
            group_id: group_id.clone().unwrap_or_default(),
            profile_id: profile_id.clone(),
            enabled: enabled.unwrap_or(true),
        },
        "exclusive" => crate::backend::application::BatchMountWorkflowInput::Exclusive {
            group_ids: group_ids.clone(),
            profile_id: profile_id.clone(),
        },
        _ => unreachable!("batch mode was validated"),
    };

    let tenant_id = state.runtime.context().tenant.id.clone();
    let dedup_suffix = if mode == "explicit" {
        asset_ids.join(",")
    } else if mode == "group" {
        format!(
            "{}:{}",
            group_id.as_deref().unwrap_or_default(),
            enabled.unwrap_or(true)
        )
    } else {
        group_ids.join(",")
    };
    let (snapshot, started) =
        state
            .background_tasks
            .begin_batch_mount(&tenant_id, &mode, &profile_id, &dedup_suffix)?;
    if !started {
        return state
            .background_tasks
            .batch_mount_snapshot_for_tenant(&tenant_id, &snapshot.id);
    }

    let task_id = snapshot.id.clone();
    let runtime = state.runtime.clone();
    let tasks = state.background_tasks.clone();
    let task_context = runtime.task_runtime().task_context(&task_id)?;
    let worker_app = app.clone();
    let worker_input = workflow_input;
    let worker_task_id = task_id.clone();
    let spawn_result = std::thread::Builder::new()
        .name(format!("aiw-batch-mount-{}", &task_id[..8]))
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let service = AppService::from_runtime(&runtime);
                if task_context.is_cancelled() {
                    return Err(AppError::Cancelled(
                        "batch mount cancelled before execution".to_string(),
                    ));
                }
                tauri::async_runtime::block_on(async {
                    service
                        .run_batch_mount_workflow_with_progress(
                            worker_input,
                            |completed, total, current_id| {
                                if task_context.is_cancelled() {
                                    return Err(AppError::Cancelled(
                                        "batch mount cancelled".to_string(),
                                    ));
                                }
                                tasks
                                    .update_batch_mount_progress(
                                        &worker_task_id,
                                        completed as u64,
                                        Some(total as u64),
                                        Some(current_id),
                                    )
                                    .map(|_| ())
                            },
                        )
                        .await
                })
                .and_then(|value| {
                    serde_json::to_value(value)
                        .map_err(|error| AppError::External(error.to_string()))
                })
            }))
            .unwrap_or_else(|_| {
                Err(AppError::External(
                    "batch mount worker panicked".to_string(),
                ))
            });
            let mut result = result;
            if let Ok(value) = result.as_mut() {
                let partial = value
                    .get("errorCount")
                    .or_else(|| value.get("error_count"))
                    .and_then(Value::as_u64)
                    .is_some_and(|count| count > 0)
                    || value
                        .get("errors")
                        .and_then(Value::as_array)
                        .is_some_and(|errors| !errors.is_empty());
                if let Some(object) = value.as_object_mut() {
                    object.insert(
                        "status".to_string(),
                        Value::String(
                            if partial {
                                "partial_failure"
                            } else {
                                "succeeded"
                            }
                            .to_string(),
                        ),
                    );
                }
            }
            let (completed, total) = result
                .as_ref()
                .ok()
                .and_then(|value| {
                    let total = value
                        .get("requestedCount")
                        .or_else(|| value.get("requested_count"))
                        .and_then(Value::as_u64)
                        .or_else(|| {
                            let preview = value.get("preview")?;
                            let mount = preview.get("mountCount")?.as_u64()?;
                            let unmount = preview.get("unmountCount")?.as_u64()?;
                            Some(mount + unmount)
                        });
                    total.map(|total| (total, Some(total)))
                })
                .unwrap_or((0, None));
            let _ = tasks.update_batch_mount_progress(&worker_task_id, completed, total, None);
            let finish = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tasks.finish_batch_mount(&worker_task_id, result)
            }))
            .unwrap_or_else(|_| {
                tasks.finish_batch_mount(
                    &worker_task_id,
                    Err(AppError::Process("batch mount worker panicked".to_string())),
                )
            });
            if let Ok(snapshot) = finish {
                let _ = worker_app.emit(BATCH_MOUNT_TASK_UPDATED_EVENT, &snapshot);
            }
        });
    if let Err(error) = spawn_result {
        let failure = state.background_tasks.finish_batch_mount(
            &task_id,
            Err(AppError::Process(format!(
                "启动 batch mount worker 失败: {error}"
            ))),
        )?;
        let _ = app.emit(BATCH_MOUNT_TASK_UPDATED_EVENT, &failure);
        return Ok(failure);
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_batch_mount_task(
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<BatchMountTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .batch_mount_snapshot_for_tenant(&tenant_id, &task_id)
}

#[tauri::command]
pub(crate) fn list_batch_mount_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<BatchMountTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .batch_mount_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_batch_mount(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<BatchMountTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let snapshot = state
        .background_tasks
        .cancel_batch_mount_for_tenant(&tenant_id, &task_id)?;
    let _ = app.emit(BATCH_MOUNT_TASK_UPDATED_EVENT, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn list_conversation_adapters(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<ConversationAdapter>> {
    AppService::from_runtime(&state.runtime).list_conversation_adapters()
}

#[tauri::command]
pub(crate) fn scaffold_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterScaffoldParams,
) -> RuntimeAppResult<crate::backend::conversations::ExternalAdapterScaffoldResult> {
    AppService::from_runtime(&state.runtime).scaffold_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn validate_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterValidateParams,
) -> RuntimeAppResult<crate::backend::conversations::ExternalAdapterValidationResult> {
    AppService::from_runtime(&state.runtime).validate_conversation_adapter(params)
}

#[tauri::command]
pub(crate) async fn list_conversation_adapter_runtime_statuses(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<crate::backend::conversations::ConversationAdapterRuntimeStatus>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_adapter_runtime_statuses()
        .await
}

#[tauri::command]
pub(crate) async fn list_agent_catalog(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<AgentCatalogEntry>> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).list_agent_catalog()
    })
    .await
    .map_err(|error| AppError::External(error.to_string()))?
}

#[tauri::command]
pub(crate) async fn check_agent_connection(
    state: State<'_, AppState>,
    params: AgentConnectionCheckRequest,
) -> RuntimeAppResult<AgentConnectionResult> {
    AppService::from_runtime(&state.runtime)
        .check_agent_connection(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_agent_models(
    state: State<'_, AppState>,
    params: AgentModelsRequest,
) -> RuntimeAppResult<AgentModelsResult> {
    AppService::from_runtime(&state.runtime)
        .list_agent_models(params)
        .await
}

#[tauri::command]
pub(crate) async fn check_opencode_translation_availability(
    state: State<'_, AppState>,
) -> RuntimeAppResult<OpencodeTranslationAvailability> {
    AppService::from_runtime(&state.runtime).check_opencode_translation_availability()
}

#[tauri::command]
pub(crate) async fn check_prompt_optimization_availability(
    state: State<'_, AppState>,
) -> RuntimeAppResult<crate::backend::card_translation::ActionAvailability> {
    AppService::from_runtime(&state.runtime).check_prompt_optimization_availability()
}

#[tauri::command]
pub(crate) async fn translate_conversation_card_with_opencode(
    state: State<'_, AppState>,
    params: OpencodeTranslationRequest,
) -> RuntimeAppResult<OpencodeTranslationResult> {
    AppService::from_runtime(&state.runtime)
        .translate_conversation_card_with_opencode(params)
        .await
}

#[tauri::command]
pub(crate) async fn translate_conversation_card(
    state: State<'_, AppState>,
    params: ConversationTranslationRequest,
) -> RuntimeAppResult<OpencodeTranslationResult> {
    AppService::from_runtime(&state.runtime)
        .translate_conversation_card(params)
        .await
}

#[tauri::command]
pub(crate) async fn optimize_prompt(
    state: State<'_, AppState>,
    params: PromptOptimizationRequest,
) -> RuntimeAppResult<PromptOptimizationResult> {
    AppService::from_runtime(&state.runtime)
        .optimize_prompt(params)
        .await
}

#[tauri::command]
pub(crate) async fn test_conversation_translation_connection(
    state: State<'_, AppState>,
    params: ConversationTranslationConnectionRequest,
) -> RuntimeAppResult<OpencodeTranslationAvailability> {
    AppService::from_runtime(&state.runtime)
        .test_conversation_translation_connection(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_translation_models(
    state: State<'_, AppState>,
    params: ConversationTranslationModelsRequest,
) -> RuntimeAppResult<ConversationTranslationModelsResult> {
    AppService::from_runtime(&state.runtime).list_conversation_translation_models(params)
}

trait AiExecutionTaskEmitter: Send + Sync {
    fn emit(&self, snapshot: &AiExecutionTaskSnapshot);
}

struct TauriAiExecutionTaskEmitter {
    app: AppHandle,
}

impl AiExecutionTaskEmitter for TauriAiExecutionTaskEmitter {
    fn emit(&self, snapshot: &AiExecutionTaskSnapshot) {
        if let Err(error) = self.app.emit(AI_EXECUTION_TASK_UPDATED_EVENT, snapshot) {
            tracing::error!(
                action = "ai_execution.task",
                task_id = %snapshot.id,
                error = %error,
                "推送 AI 执行任务状态失败"
            );
        }
    }
}

struct RegistryAiExecutionProgressSink {
    tasks: Arc<BackgroundTaskRegistry>,
    task_id: String,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
    last_execution_phase: Mutex<Option<AiExecutionPhase>>,
}

impl AiExecutionProgressSink for RegistryAiExecutionProgressSink {
    fn set_phase(&self, phase: AiExecutionPhase) {
        if !matches!(
            phase,
            AiExecutionPhase::Cancelling | AiExecutionPhase::Closing | AiExecutionPhase::CleaningUp
        ) {
            if let Ok(mut last_phase) = self.last_execution_phase.lock() {
                *last_phase = Some(phase);
            }
        }
        match self.tasks.update_ai_execution_phase(&self.task_id, phase) {
            Ok(snapshot) => self.emitter.emit(&snapshot),
            Err(error) => tracing::error!(
                action = "ai_execution.task",
                task_id = %self.task_id,
                error = %error,
                "更新 AI 执行任务阶段失败"
            ),
        }
    }

    fn failure_phase(&self) -> Option<AiExecutionPhase> {
        self.last_execution_phase
            .lock()
            .ok()
            .and_then(|last_phase| *last_phase)
    }

    fn set_cleanup_report(&self, report: AiExecutionCleanupReport) {
        match self
            .tasks
            .update_ai_execution_cleanup(&self.task_id, report)
        {
            Ok(snapshot) => self.emitter.emit(&snapshot),
            Err(error) => tracing::error!(
                action = "ai_execution.task",
                task_id = %self.task_id,
                error = %error,
                "更新 AI 执行清理报告失败"
            ),
        }
    }
}

#[cfg(test)]
fn prepare_ai_execution_task(
    tasks: Arc<BackgroundTaskRegistry>,
    params: ConversationTranslationRequest,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
) -> RuntimeAppResult<(AiExecutionTaskSnapshot, AiExecutionRequest)> {
    prepare_ai_execution_task_for_tenant("default", tasks, params, emitter)
}

fn prepare_ai_execution_task_for_tenant(
    tenant_id: &str,
    tasks: Arc<BackgroundTaskRegistry>,
    params: ConversationTranslationRequest,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
) -> RuntimeAppResult<(AiExecutionTaskSnapshot, AiExecutionRequest)> {
    let (agent_id, prompt, model) = prepare_opencode_agent_translation(params)?;
    let (snapshot, cancellation) = tasks.begin_ai_execution_for_tenant(
        tenant_id,
        AiExecutionPurpose::Translation,
        &agent_id,
    )?;
    let progress = Arc::new(RegistryAiExecutionProgressSink {
        tasks,
        task_id: snapshot.id.clone(),
        emitter: emitter.clone(),
        last_execution_phase: Mutex::new(None),
    });
    let request = AiExecutionRequest {
        execution_id: snapshot.id.clone(),
        agent_id,
        purpose: AiExecutionPurpose::Translation,
        session_mode: crate::backend::ai_execution::AgentSessionMode::OneShot,
        prompt,
        model,
        limits: AiExecutionLimits::default(),
        cancellation,
        progress: Some(progress),
        tenant_id: None,
        execution_context_key: None,
        binding: None,
        replay: false,
        restore_only: false,
        team_tools: None,
        recall_tools: None,
    };
    emitter.emit(&snapshot);
    Ok((snapshot, request))
}

async fn run_ai_execution_task(
    tasks: Arc<BackgroundTaskRegistry>,
    runtime: Arc<dyn AgentExecutionRuntime>,
    task_id: String,
    request: AiExecutionRequest,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
) {
    let progress = request.progress.clone();
    let execution = tokio::spawn(async move { runtime.execute(request).await });
    let result = match execution.await {
        Ok(result) => result,
        Err(_) => Err(AiExecutionError::Protocol {
            operation: "execution_task_panicked",
        }),
    };
    let failure_phase = progress
        .as_ref()
        .and_then(|progress| progress.failure_phase());
    match tasks.finish_ai_execution_with_phase(&task_id, result, failure_phase) {
        Ok(snapshot) => emitter.emit(&snapshot),
        Err(error) => tracing::error!(
            action = "ai_execution.task",
            task_id = %task_id,
            error = %error,
            "收敛 AI 执行任务状态失败"
        ),
    }
}

#[tauri::command]
pub(crate) async fn start_conversation_card_translation(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationTranslationRequest,
) -> RuntimeAppResult<AiExecutionTaskSnapshot> {
    let emitter: Arc<dyn AiExecutionTaskEmitter> = Arc::new(TauriAiExecutionTaskEmitter { app });
    let tasks = state.background_tasks.clone();
    let tenant_id = state.runtime.context().tenant.id.clone();
    let (snapshot, request) =
        prepare_ai_execution_task_for_tenant(&tenant_id, tasks.clone(), params, emitter.clone())?;
    let runtime = state.agent_runtime.clone();
    let task_id = snapshot.id.clone();
    tauri::async_runtime::spawn(run_ai_execution_task(
        tasks, runtime, task_id, request, emitter,
    ));
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_ai_execution_task(
    state: State<'_, AppState>,
    params: AiExecutionTaskGetParams,
) -> RuntimeAppResult<Option<AiExecutionTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .ai_execution_snapshot_for_tenant(&tenant_id, &params.task_id)
}

#[tauri::command]
pub(crate) fn list_ai_execution_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<AiExecutionTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .ai_execution_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_ai_execution_task(
    app: AppHandle,
    state: State<'_, AppState>,
    params: AiExecutionTaskGetParams,
) -> RuntimeAppResult<AiExecutionTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let snapshot = state
        .background_tasks
        .cancel_ai_execution_for_tenant(&tenant_id, &params.task_id)?;
    TauriAiExecutionTaskEmitter { app }.emit(&snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn register_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterRegisterParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .register_conversation_adapter(params)
        .await
}

#[tauri::command]
pub(crate) async fn unregister_conversation_adapter(
    state: State<'_, AppState>,
    params: ConversationAdapterUnregisterParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .unregister_conversation_adapter(params)
        .await
}

#[tauri::command]
pub(crate) async fn try_run_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterTryRunParams,
) -> RuntimeAppResult<crate::backend::conversations::ExternalAdapterRunResult> {
    AppService::from_runtime(&state.runtime)
        .try_run_conversation_adapter(params)
        .await
}

#[tauri::command]
pub(crate) async fn project_conversation_command_parts(
    state: State<'_, AppState>,
    params: ConversationCommandProjectionParams,
) -> RuntimeAppResult<Vec<ConversationCommandProjection>> {
    AppService::from_runtime(&state.runtime)
        .project_conversation_command_parts(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_sources(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<ConversationSource>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_sources()
        .await
}

#[tauri::command]
pub(crate) async fn upsert_conversation_source(
    state: State<'_, AppState>,
    params: ConversationSourceUpsertParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .upsert_conversation_source(params)
        .await
}

#[tauri::command]
pub(crate) async fn disable_conversation_source(
    state: State<'_, AppState>,
    params: ConversationSourceDisableParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .disable_conversation_source(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_script_catalog(
    state: State<'_, AppState>,
    params: ConversationScriptCatalogParams,
) -> RuntimeAppResult<Vec<crate::backend::application::ConversationScriptCatalogEntry>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_script_catalog(params)
        .await
}

#[tauri::command]
pub(crate) async fn register_conversation_adapter_local(
    state: State<'_, AppState>,
    params: ConversationAdapterLocalRegisterParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .register_conversation_adapter_local(params)
        .await
}

#[tauri::command]
pub(crate) async fn inspect_conversation_adapter_package(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageInspectParams,
) -> RuntimeAppResult<crate::backend::application::ConversationAdapterPackageInspection> {
    AppService::from_runtime(&state.runtime)
        .inspect_conversation_adapter_package(params)
        .await
}

#[tauri::command]
pub(crate) async fn prepare_conversation_adapter_package_change(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageChangeParams,
) -> RuntimeAppResult<crate::backend::application::ConversationAdapterPackageChangePreflight> {
    let mut preflight = AppService::from_runtime(&state.runtime)
        .prepare_conversation_adapter_package_change(params)
        .await?;
    if state
        .background_tasks
        .conversation_script_install_snapshot()?
        .is_some_and(|task| task.status == BackgroundTaskStatus::Running)
    {
        preflight.task_conflicts.push("package_change".to_string());
    }
    Ok(preflight)
}

#[tauri::command]
pub(crate) async fn list_conversation_adapter_packages(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageCatalogParams,
) -> RuntimeAppResult<Vec<crate::backend::application::ConversationAdapterPackageCatalogEntry>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_adapter_packages(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_adapter_package_releases(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageReleaseListParams,
) -> RuntimeAppResult<Vec<crate::backend::models::ConversationAdapterCatalogRelease>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_adapter_package_releases(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_installed_conversation_adapter_package_versions(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> RuntimeAppResult<Vec<crate::backend::models::ConversationAdapterPackageVersion>> {
    AppService::from_runtime(&state.runtime)
        .list_installed_conversation_adapter_package_versions(params)
        .await
}

#[tauri::command]
pub(crate) async fn switch_conversation_adapter_package_version(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .switch_conversation_adapter_package_version(params)
        .await
}

#[tauri::command]
pub(crate) async fn rollback_conversation_adapter_package_version(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .rollback_conversation_adapter_package_version(params)
        .await
}

#[tauri::command]
pub(crate) async fn delete_conversation_adapter_package_version(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .delete_conversation_adapter_package_version(params)
        .await
}

#[tauri::command]
pub(crate) async fn refresh_conversation_adapter_catalogs(
    state: State<'_, AppState>,
    params: ConversationAdapterCatalogRefreshParams,
) -> RuntimeAppResult<Vec<crate::backend::models::ConversationAdapterCatalogRelease>> {
    AppService::from_runtime(&state.runtime)
        .refresh_conversation_adapter_catalogs(params)
        .await
}

#[tauri::command]
pub(crate) async fn check_conversation_adapter_package_updates(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageUpdateCheckParams,
) -> RuntimeAppResult<Vec<crate::backend::application::ConversationAdapterPackageUpdateStatus>> {
    AppService::from_runtime(&state.runtime)
        .check_conversation_adapter_package_updates(params)
        .await
}

#[tauri::command]
pub(crate) async fn set_conversation_adapter_package_update_policy(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageUpdatePolicyParams,
) -> RuntimeAppResult<crate::backend::models::ConversationAdapterPackage> {
    AppService::from_runtime(&state.runtime)
        .set_conversation_adapter_package_update_policy(params)
        .await
}

#[tauri::command]
pub(crate) fn install_conversation_adapter_package(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationAdapterPackageInstallParams,
) -> RuntimeAppResult<ConversationScriptInstallTaskSnapshot> {
    let (snapshot, should_start) = state
        .background_tasks
        .begin_conversation_adapter_package_install(&params)?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let task_id = snapshot.id.clone();
    spawn_conversation_lifecycle_task(
        app,
        state.background_tasks.clone(),
        task_id,
        "conversation.adapter_package.install",
        move || {
            tauri::async_runtime::block_on(async {
                AppService::from_runtime(&runtime)
                    .install_conversation_adapter_package(params)
                    .await
            })
        },
    )?;

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn update_conversation_adapter_package(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationAdapterPackageInstallParams,
) -> RuntimeAppResult<ConversationScriptInstallTaskSnapshot> {
    let (snapshot, should_start) = state
        .background_tasks
        .begin_conversation_adapter_package_update(&params)?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let task_id = snapshot.id.clone();
    spawn_conversation_lifecycle_task(
        app,
        state.background_tasks.clone(),
        task_id,
        "conversation.adapter_package.update",
        move || {
            tauri::async_runtime::block_on(async {
                AppService::from_runtime(&runtime)
                    .update_conversation_adapter_package(params)
                    .await
            })
        },
    )?;

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn uninstall_conversation_adapter_package(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationAdapterPackageUninstallParams,
) -> RuntimeAppResult<ConversationScriptInstallTaskSnapshot> {
    let (snapshot, should_start) = state
        .background_tasks
        .begin_conversation_adapter_package_uninstall(&params)?;
    if !should_start {
        return Ok(snapshot);
    }
    let runtime = state.runtime.clone();
    let task_id = snapshot.id.clone();
    spawn_conversation_lifecycle_task(
        app,
        state.background_tasks.clone(),
        task_id,
        "conversation.adapter_package.uninstall",
        move || {
            tauri::async_runtime::block_on(async {
                AppService::from_runtime(&runtime)
                    .uninstall_conversation_adapter_package(params)
                    .await
            })
        },
    )?;
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_adapter_package_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<ConversationScriptInstallTaskSnapshot>> {
    state
        .background_tasks
        .conversation_script_install_snapshot()
}

#[tauri::command]
pub(crate) fn install_conversation_script(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationScriptInstallParams,
) -> RuntimeAppResult<ConversationScriptInstallTaskSnapshot> {
    let (snapshot, should_start) = state
        .background_tasks
        .begin_conversation_script_install(&params)?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let task_id = snapshot.id.clone();
    spawn_conversation_lifecycle_task(
        app,
        state.background_tasks.clone(),
        task_id,
        "conversation.script.install",
        move || {
            tauri::async_runtime::block_on(async {
                AppService::from_runtime(&runtime)
                    .install_conversation_script(params)
                    .await
            })
        },
    )?;

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_script_install_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<ConversationScriptInstallTaskSnapshot>> {
    state
        .background_tasks
        .conversation_script_install_snapshot()
}

#[tauri::command]
pub(crate) fn sync_conversations(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationSyncParams,
) -> RuntimeAppResult<ConversationSyncTaskSnapshot> {
    start_conversation_sync_background(
        app,
        state.runtime.clone(),
        state.background_tasks.clone(),
        params,
    )
}

pub(crate) fn start_conversation_sync_background(
    app: AppHandle,
    runtime: std::sync::Arc<crate::backend::runtime::AppRuntime>,
    background_tasks: std::sync::Arc<
        crate::adapters::tauri::background_tasks::BackgroundTaskRegistry,
    >,
    params: ConversationSyncParams,
) -> RuntimeAppResult<ConversationSyncTaskSnapshot> {
    let tenant_id = runtime.context().tenant.id.clone();
    let (snapshot, should_start) =
        background_tasks.begin_conversation_sync_for_tenant(&tenant_id, &params)?;
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
    let outcome = task_runtime.start_external_with(
        &task_id,
        task_detail,
        Box::new(move |context| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let progress_app = task_app.clone();
                let progress_tasks = task_background_tasks.clone();
                let progress_task_id = task_id_for_runtime.clone();
                let mut on_progress =
                    move |completed_source_count: usize,
                          total_source_count: usize,
                          current_source_name: Option<String>| {
                        match progress_tasks.update_conversation_sync_progress(
                            &progress_task_id,
                            completed_source_count,
                            total_source_count,
                            current_source_name,
                        ) {
                            Ok(snapshot) => {
                                if let Err(error) =
                                    progress_app.emit("conversation-sync-task-updated", &snapshot)
                                {
                                    tracing::error!(
                                        action = "conversation.sync",
                                        task_id = %progress_task_id,
                                        error = %error,
                                        "推送后台同步进度失败"
                                    );
                                }
                            }
                            Err(error) => tracing::error!(
                                action = "conversation.sync",
                                task_id = %progress_task_id,
                                error = %error,
                                "更新后台同步进度失败"
                            ),
                        }
                    };
                let cancellation = context.cancellation();
                if context.is_cancelled() {
                    return Err(AppError::Cancelled(
                        "conversation sync cancelled".to_string(),
                    ));
                }
                tauri::async_runtime::block_on(async {
                    AppService::from_runtime(&runtime)
                        .sync_conversations_with_progress_and_cancellation(
                            params,
                            Some(&cancellation),
                            &mut on_progress,
                        )
                        .await
                })
            }))
            .unwrap_or_else(|_| {
                Err(AppError::Process(
                    "conversation sync task panicked".to_string(),
                ))
            });
            match &result {
                Ok(value) => tracing::info!(
                    action = "conversation.sync",
                    task_id = %task_id_for_runtime,
                    result = %value,
                    "后台同步对话记录成功"
                ),
                Err(error) => tracing::error!(
                    action = "conversation.sync",
                    task_id = %task_id_for_runtime,
                    error = %error,
                    "后台同步对话记录失败"
                ),
            }
            match task_background_tasks.finish_conversation_sync(&task_id_for_runtime, result) {
                Ok(snapshot) => {
                    if let Err(error) = task_app.emit("conversation-sync-task-updated", &snapshot) {
                        tracing::error!(
                            action = "conversation.sync",
                            task_id = %task_id_for_runtime,
                            error = %error,
                            "推送后台同步任务状态失败"
                        );
                    }
                    Ok(Value::Null)
                }
                Err(error) => Err(error),
            }
        }),
    );
    if let Err(error) = outcome {
        return Err(error);
    }

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_sync_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<ConversationSyncTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .conversation_sync_snapshot_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn list_conversation_sync_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<ConversationSyncTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .conversation_sync_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_conversation_sync(
    state: State<'_, AppState>,
    params: BackgroundTaskGetParams,
) -> RuntimeAppResult<ConversationSyncTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .cancel_conversation_sync_for_tenant(&tenant_id, &params.task_id)
}

#[tauri::command]
pub(crate) fn audit_conversation_data(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationDataAuditParams,
) -> RuntimeAppResult<ConversationDataMaintenanceTaskSnapshot> {
    start_conversation_data_maintenance_background(
        app,
        state.runtime.clone(),
        state.background_tasks.clone(),
        "audit",
        params,
        None,
    )
}

#[tauri::command]
pub(crate) fn repair_conversation_data(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationDataRepairParams,
) -> RuntimeAppResult<ConversationDataMaintenanceTaskSnapshot> {
    let audit_params = ConversationDataAuditParams {
        source_id: params.source_id.clone(),
        record_kind: params.record_kind.clone(),
        include_resolved: false,
    };
    start_conversation_data_maintenance_background(
        app,
        state.runtime.clone(),
        state.background_tasks.clone(),
        "repair",
        audit_params,
        Some(params),
    )
}

fn start_conversation_data_maintenance_background(
    app: AppHandle,
    runtime: std::sync::Arc<crate::backend::runtime::AppRuntime>,
    background_tasks: std::sync::Arc<BackgroundTaskRegistry>,
    operation: &'static str,
    audit_params: ConversationDataAuditParams,
    repair_params: Option<ConversationDataRepairParams>,
) -> RuntimeAppResult<ConversationDataMaintenanceTaskSnapshot> {
    let tenant_id = runtime.context().tenant.id.clone();
    let dry_run = repair_params
        .as_ref()
        .map(|params| params.dry_run)
        .unwrap_or(true);
    let (snapshot, should_start) = background_tasks
        .begin_conversation_data_maintenance_for_tenant(
            &tenant_id,
            operation,
            audit_params.source_id.clone(),
            audit_params.record_kind.clone(),
            dry_run,
        )?;
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
    let outcome = task_runtime.start_external_with(
        &task_id,
        task_detail,
        Box::new(move |context| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let service = AppService::from_runtime(&runtime);
                let progress_tasks = task_background_tasks.clone();
                let progress_app = task_app.clone();
                let progress_task_id = task_id_for_runtime.clone();
                let mut on_progress =
                    move |completed_stage: usize, total_stage: usize, note: Option<String>| {
                        if let Ok(snapshot) = progress_tasks
                            .update_conversation_data_maintenance_progress(
                                &progress_task_id,
                                completed_stage,
                                total_stage,
                                note.clone(),
                            )
                        {
                            if let Err(error) = progress_app
                                .emit("conversation-data-maintenance-task-updated", &snapshot)
                            {
                                tracing::error!(
                                    action = "conversation.data.maintenance",
                                    task_id = %progress_task_id,
                                    error = %error,
                                    "推送对话数据维护进度失败"
                                );
                            }
                        }
                    };
                let cancellation = context.cancellation();
                if context.is_cancelled() {
                    return Err(AppError::Cancelled(
                        "conversation data maintenance cancelled".to_string(),
                    ));
                }
                tauri::async_runtime::block_on(async {
                    if let Some(params) = repair_params {
                        service
                            .repair_conversation_data_with_progress_and_cancellation(
                                params,
                                Some(&cancellation),
                                &mut on_progress,
                            )
                            .await
                    } else {
                        service
                            .audit_conversation_data_with_progress_and_cancellation(
                                audit_params,
                                Some(&cancellation),
                                &mut on_progress,
                            )
                            .await
                    }
                })
            }))
            .unwrap_or_else(|_| {
                Err(AppError::Process(
                    "conversation data maintenance task panicked".to_string(),
                ))
            });
            match task_background_tasks
                .finish_conversation_data_maintenance(&task_id_for_runtime, result)
            {
                Ok(snapshot) => {
                    let _ = task_app.emit("conversation-data-maintenance-task-updated", &snapshot);
                    Ok(Value::Null)
                }
                Err(error) => Err(error),
            }
        }),
    );
    if let Err(error) = outcome {
        return Err(error);
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_data_maintenance_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<ConversationDataMaintenanceTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .conversation_data_maintenance_snapshot_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn list_conversation_data_maintenance_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<ConversationDataMaintenanceTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .conversation_data_maintenance_snapshots_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) fn cancel_conversation_data_maintenance(
    state: State<'_, AppState>,
    params: BackgroundTaskGetParams,
) -> RuntimeAppResult<ConversationDataMaintenanceTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .cancel_conversation_data_maintenance_for_tenant(&tenant_id, &params.task_id)
}

#[tauri::command]
pub(crate) async fn rollback_conversation_data(
    state: State<'_, AppState>,
    params: ConversationDataRollbackParams,
) -> RuntimeAppResult<Value> {
    AppService::from_runtime(&state.runtime)
        .rollback_conversation_data(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_sessions(
    state: State<'_, AppState>,
    params: ConversationSessionListParams,
) -> RuntimeAppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_sessions(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> RuntimeAppResult<crate::backend::dto::ConversationSessionDetail> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_session(params)
        .await
}

#[tauri::command]
pub(crate) async fn export_conversation_session(
    state: State<'_, AppState>,
    params: ConversationSessionExportParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .export_conversation_session(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_web_record_sessions(
    state: State<'_, AppState>,
    params: ConversationSessionListParams,
) -> RuntimeAppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
    AppService::from_runtime(&state.runtime)
        .list_web_record_sessions(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_web_record_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> RuntimeAppResult<crate::backend::dto::ConversationSessionDetail> {
    AppService::from_runtime(&state.runtime)
        .get_web_record_session(params)
        .await
}

#[tauri::command]
pub(crate) async fn search_conversation_records(
    state: State<'_, AppState>,
    params: ConversationSearchParams,
) -> RuntimeAppResult<ConversationSearchResult> {
    AppService::from_runtime(&state.runtime)
        .search_conversation_records(params)
        .await
}

/// 检索最近增量同步变动的会话卡片记录
#[tauri::command]
pub(crate) async fn search_recent_incremental_conversation_records(
    state: State<'_, AppState>,
    params: crate::backend::application::ConversationIncrementalSearchParams,
) -> RuntimeAppResult<ConversationSearchResult> {
    AppService::from_runtime(&state.runtime)
        .search_recent_incremental_conversation_records(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_search_index_status(
    state: State<'_, AppState>,
) -> RuntimeAppResult<ConversationSearchIndexStatus> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_search_index_status()
        .await
}

#[tauri::command]
pub(crate) fn start_conversation_search_index_rebuild(
    app: AppHandle,
    state: State<'_, AppState>,
) -> RuntimeAppResult<ConversationSearchIndexTaskSnapshot> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    let (snapshot, should_start) = state
        .background_tasks
        .begin_conversation_search_index_rebuild_for_tenant(&tenant_id)?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let background_tasks = state.background_tasks.clone();
    let task_id = snapshot.id.clone();
    let task_runtime = background_tasks
        .task_runtime()
        .ok_or_else(|| AppError::Conflict("TaskRuntime 未初始化".to_string()))?;
    let task_detail = task_runtime
        .get(&task_id)
        .map(|task| task.detail)
        .ok_or_else(|| AppError::NotFound(format!("background task not found: {task_id}")))?;
    let app = app.clone();
    let task_id_for_runtime = task_id.clone();
    let background_tasks_for_runtime = background_tasks.clone();
    let outcome = task_runtime.start_external_with(
        &task_id,
        task_detail,
        Box::new(move |_context| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tauri::async_runtime::block_on(async {
                    AppService::from_runtime(&runtime)
                        .rebuild_conversation_search_index()
                        .await
                        .and_then(|report| {
                            serde_json::to_value(report).map_err(|error| {
                                AppError::External(format!(
                                    "serialize conversation search index report: {error}"
                                ))
                            })
                        })
                })
            }))
            .unwrap_or_else(|_| {
                Err(AppError::Process(
                    "conversation search index rebuild panicked".to_string(),
                ))
            });
            let projection_result = match &result {
                Ok(value) => Ok(value.clone()),
                Err(error) => Err(AppError::from(error.view())),
            };
            match background_tasks_for_runtime
                .finish_conversation_search_index_rebuild(&task_id_for_runtime, projection_result)
            {
                Ok(snapshot) => {
                    if let Err(error) =
                        app.emit("conversation-search-index-task-updated", &snapshot)
                    {
                        tracing::error!(
                            action = "conversation.search.index.rebuild",
                            task_id = %task_id_for_runtime,
                            error = %error,
                            "推送对话搜索索引任务状态失败"
                        );
                    }
                }
                Err(error) => tracing::error!(
                    action = "conversation.search.index.rebuild",
                    task_id = %task_id_for_runtime,
                    error = %error,
                    "更新对话搜索索引任务状态失败"
                ),
            }
            result
        }),
    );
    if let Err(error) = outcome {
        let projection_error = AppError::from(error.view());
        let _ = background_tasks
            .finish_conversation_search_index_rebuild(&task_id, Err(projection_error));
        return Err(error);
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_search_index_task(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Option<ConversationSearchIndexTaskSnapshot>> {
    let tenant_id = state.runtime.context().tenant.id.clone();
    state
        .background_tasks
        .conversation_search_index_snapshot_for_tenant(&tenant_id)
}

#[tauri::command]
pub(crate) async fn export_web_record_session(
    state: State<'_, AppState>,
    params: ConversationSessionExportParams,
) -> RuntimeAppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime)
        .export_web_record_session(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_questions(
    state: State<'_, AppState>,
    params: ConversationQuestionListParams,
) -> RuntimeAppResult<Vec<crate::backend::dto::ConversationQuestionDetail>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_questions(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionGetParams,
) -> RuntimeAppResult<crate::backend::dto::ConversationQuestionDetail> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_question(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_blocks(
    state: State<'_, AppState>,
    params: ConversationBlockListParams,
) -> RuntimeAppResult<Vec<crate::backend::dto::ConversationBlockLocator>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_blocks(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_block(
    state: State<'_, AppState>,
    params: ConversationBlockGetParams,
) -> RuntimeAppResult<crate::backend::dto::ConversationBlockDetail> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_block(params)
        .await
}

#[tauri::command]
pub(crate) async fn merge_conversation_questions(
    state: State<'_, AppState>,
    params: ConversationQuestionMergeParams,
) -> RuntimeAppResult<crate::backend::dto::ConversationMutationResult> {
    AppService::from_runtime(&state.runtime)
        .merge_conversation_questions(params)
        .await
}

#[tauri::command]
pub(crate) async fn split_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionSplitParams,
) -> RuntimeAppResult<crate::backend::dto::ConversationMutationResult> {
    AppService::from_runtime(&state.runtime)
        .split_conversation_question(params)
        .await
}

#[tauri::command]
pub(crate) async fn update_conversation_part_translation(
    state: State<'_, AppState>,
    params: ConversationPartTranslationUpdateParams,
) -> RuntimeAppResult<()> {
    AppService::from_runtime(&state.runtime)
        .update_conversation_part_translation(params)
        .await
}

#[tauri::command]
pub(crate) async fn create_plan(
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> RuntimeAppResult<DeploymentPlan> {
    let result = AppService::from_runtime(&state.runtime)
        .create_plan(profile_id.as_deref())
        .await;

    match &result {
        Ok(plan) => {
            tracing::info!(
                action = "deployment_plan.create",
                profile_id = ?profile_id,
                plan_id = %plan.id,
                action_count = plan.actions.len(),
                create_count = plan.summary.create_count,
                skip_count = plan.summary.skip_count,
                conflict_count = plan.summary.conflict_count,
                "创建部署计划成功"
            );
        }
        Err(error) => tracing::error!(
            action = "deployment_plan.create",
            profile_id = ?profile_id,
            error = %error,
            "创建部署计划失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn execute_plan(
    state: State<'_, AppState>,
    plan: DeploymentPlan,
    action_ids: Option<Vec<String>>,
) -> RuntimeAppResult<ExecutionResult> {
    let plan_id = plan.id.clone();
    let action_count = plan.actions.len();
    let requested_action_count = action_ids.as_ref().map(Vec::len).unwrap_or(0);
    let result = AppService::from_runtime(&state.runtime)
        .execute_plan(plan, action_ids)
        .await;

    match &result {
        Ok(result) => {
            if result.conflict_count > 0 || !result.errors.is_empty() {
                tracing::warn!(
                    action = "deployment_plan.execute",
                    plan_id = %plan_id,
                    action_count = action_count,
                    requested_action_count = requested_action_count,
                    executed_count = result.executed_count,
                    skipped_count = result.skipped_count,
                    conflict_count = result.conflict_count,
                    error_count = result.errors.len(),
                    "执行部署计划完成但存在冲突或失败"
                );
            } else {
                tracing::info!(
                    action = "deployment_plan.execute",
                    plan_id = %plan_id,
                    action_count = action_count,
                    requested_action_count = requested_action_count,
                    executed_count = result.executed_count,
                    skipped_count = result.skipped_count,
                    conflict_count = result.conflict_count,
                    error_count = result.errors.len(),
                    "执行部署计划成功"
                );
            }
        }
        Err(error) => tracing::error!(
            action = "deployment_plan.execute",
            plan_id = %plan_id,
            action_count = action_count,
            requested_action_count = requested_action_count,
            error = %error,
            "执行部署计划失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn reveal_path(path: String) -> RuntimeAppResult<()> {
    let result = crate::adapters::platform::reveal_path(path.clone());
    match &result {
        Ok(()) => tracing::info!(
            action = "path.reveal",
            resource = "filesystem_path",
            "打开路径成功"
        ),
        Err(error) => tracing::error!(
            action = "path.reveal",
            resource = "filesystem_path",
            error_code = %error.code(),
            "打开路径失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn get_cli_tools_status(
    app: AppHandle,
) -> RuntimeAppResult<crate::adapters::cli_tools::CliToolsStatus> {
    crate::adapters::cli_tools::status(&app)
}

#[tauri::command]
pub(crate) fn install_cli_tools(
    app: AppHandle,
) -> RuntimeAppResult<crate::adapters::cli_tools::CliToolsStatus> {
    let result = crate::adapters::cli_tools::install(&app);
    match &result {
        Ok(status) => tracing::info!(
            action = "cli.install",
            install_dir = %status.install_dir,
            path_configured = status.path_configured,
            "安装命令行工具成功"
        ),
        Err(error) => tracing::error!(
            action = "cli.install",
            error = %error,
            "安装命令行工具失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn logs_get_snapshot(
    file_name: Option<String>,
    line_limit: Option<usize>,
) -> RuntimeAppResult<crate::backend::logs::LogSnapshot> {
    crate::backend::logs::logs_get_snapshot(file_name, line_limit).map_err(AppError::External)
}

#[tauri::command]
pub(crate) fn logs_open_log_directory() -> RuntimeAppResult<()> {
    crate::backend::logs::logs_open_log_directory().map_err(AppError::External)
}

#[tauri::command]
pub(crate) fn logs_write_operation(
    level: String,
    operation: String,
    message: String,
    fields: Option<BTreeMap<String, String>>,
) -> RuntimeAppResult<()> {
    crate::backend::logs::logs_write_operation(level, operation, message, fields)
        .map_err(AppError::External)
}

#[tauri::command]
pub(crate) fn copy_prompt_card_to_clipboard(params: PromptClipboardParams) -> RuntimeAppResult<()> {
    copy_prompt_card_to_clipboard_impl(params)
}

// Keep the generated Tauri command shims in this module so the existing
// command handler remains a single, locally resolvable macro surface. The
// implementation stays in the dedicated Agent Market adapter.
#[tauri::command]
pub(crate) async fn list_agent_market(
    state: State<'_, AppState>,
    params: crate::backend::agent_market::types::AgentMarketListRequest,
) -> crate::backend::runtime::AppResult<Vec<crate::backend::application::AgentMarketItemView>> {
    crate::adapters::tauri::agent_market::list_agent_market(state, params).await
}

#[tauri::command]
pub(crate) async fn inspect_agent_market_item(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::runtime::AppResult<crate::backend::application::AgentMarketItemView> {
    crate::adapters::tauri::agent_market::inspect_agent_market_item(state, agent_id).await
}

#[tauri::command]
pub(crate) fn refresh_agent_market(
    app: AppHandle,
    state: State<'_, AppState>,
) -> crate::backend::runtime::AppResult<
    crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::refresh_agent_market(app, state)
}

#[tauri::command]
pub(crate) fn get_agent_market_refresh_task(
    state: State<'_, AppState>,
    task_id: String,
) -> crate::backend::runtime::AppResult<
    crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::get_agent_market_refresh_task(state, task_id)
}

#[tauri::command]
pub(crate) fn list_agent_market_refresh_tasks(
    state: State<'_, AppState>,
) -> crate::backend::runtime::AppResult<
    Vec<crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot>,
> {
    crate::adapters::tauri::agent_market::list_agent_market_refresh_tasks(state)
}

#[tauri::command]
pub(crate) async fn preview_agent_installation(
    state: State<'_, AppState>,
    params: crate::backend::agent_market::types::AgentInstallPreviewRequest,
) -> crate::backend::runtime::AppResult<crate::backend::application::AgentInstallPreview> {
    crate::adapters::tauri::agent_market::preview_agent_installation(state, params).await
}

#[tauri::command]
pub(crate) async fn preview_agent_uninstall(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::runtime::AppResult<crate::backend::application::AgentUninstallPreview> {
    crate::adapters::tauri::agent_market::preview_agent_uninstall(state, agent_id).await
}

#[tauri::command]
pub(crate) async fn list_installed_agents(
    state: State<'_, AppState>,
) -> crate::backend::runtime::AppResult<
    Vec<crate::backend::agent_market::types::AgentInstallationView>,
> {
    crate::adapters::tauri::agent_market::list_installed_agents(state).await
}

#[tauri::command]
pub(crate) async fn get_installed_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::runtime::AppResult<crate::backend::agent_market::types::AgentInstallationView>
{
    crate::adapters::tauri::agent_market::get_installed_agent(state, agent_id).await
}

#[tauri::command]
pub(crate) async fn check_agent_runtime(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::runtime::AppResult<crate::backend::agent_market::types::AgentInstallationView>
{
    crate::adapters::tauri::agent_market::check_agent_runtime(state, agent_id).await
}

#[tauri::command]
pub(crate) fn get_agent_lifecycle_task(
    state: State<'_, AppState>,
    task_id: String,
) -> crate::backend::runtime::AppResult<
    crate::backend::agent_market::types::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::get_agent_lifecycle_task(state, task_id)
}

#[tauri::command]
pub(crate) fn list_agent_lifecycle_tasks(
    state: State<'_, AppState>,
) -> crate::backend::runtime::AppResult<
    Vec<crate::backend::agent_market::types::AgentLifecycleTaskSnapshot>,
> {
    crate::adapters::tauri::agent_market::list_agent_lifecycle_tasks(state)
}

#[tauri::command]
pub(crate) fn cancel_agent_lifecycle_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> crate::backend::runtime::AppResult<
    crate::backend::agent_market::types::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::cancel_agent_lifecycle_task(app, state, task_id)
}

#[tauri::command]
pub(crate) fn start_agent_installation(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::agent_market::types::AgentInstallStartRequest,
) -> crate::backend::runtime::AppResult<
    crate::backend::agent_market::types::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_installation(app, state, params)
}

#[tauri::command]
pub(crate) fn start_agent_update(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::agent_market::types::AgentInstallStartRequest,
) -> crate::backend::runtime::AppResult<
    crate::backend::agent_market::types::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_update(app, state, params)
}

#[tauri::command]
pub(crate) fn start_agent_reinstallation(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::agent_market::types::AgentInstallStartRequest,
) -> crate::backend::runtime::AppResult<
    crate::backend::agent_market::types::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_reinstallation(app, state, params)
}

#[tauri::command]
pub(crate) fn start_agent_uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::agent_market::types::AgentUninstallStartRequest,
) -> crate::backend::runtime::AppResult<
    crate::backend::agent_market::types::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_uninstall(app, state, params)
}

#[tauri::command]
pub(crate) async fn enable_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::runtime::AppResult<crate::backend::agent_market::types::AgentInstallationView>
{
    crate::adapters::tauri::agent_market::enable_agent(state, agent_id).await
}

#[tauri::command]
pub(crate) async fn disable_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::runtime::AppResult<crate::backend::agent_market::types::AgentInstallationView>
{
    crate::adapters::tauri::agent_market::disable_agent(state, agent_id).await
}

#[tauri::command]
pub(crate) async fn create_team(
    state: State<'_, AppState>,
    input: crate::backend::models::CreateTeamInput,
) -> RuntimeAppResult<crate::backend::models::TeamDetail> {
    AppService::from_runtime(&state.runtime)
        .create_team(input)
        .await
}

#[tauri::command]
pub(crate) async fn get_team(
    state: State<'_, AppState>,
    team_id: String,
) -> RuntimeAppResult<Option<crate::backend::models::TeamDetail>> {
    AppService::from_runtime(&state.runtime)
        .get_team(&team_id)
        .await
}

#[tauri::command]
pub(crate) async fn list_teams(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<crate::backend::models::TeamDetail>> {
    AppService::from_runtime(&state.runtime).list_teams().await
}

#[tauri::command]
pub(crate) async fn update_team(
    state: State<'_, AppState>,
    input: crate::backend::models::UpdateTeamInput,
) -> RuntimeAppResult<crate::backend::models::TeamDetail> {
    AppService::from_runtime(&state.runtime)
        .update_team(input)
        .await
}

#[tauri::command]
pub(crate) async fn delete_team(
    state: State<'_, AppState>,
    team_id: String,
) -> RuntimeAppResult<()> {
    AppService::from_runtime(&state.runtime)
        .delete_team(&team_id)
        .await
}

#[tauri::command]
pub(crate) async fn team_member_turn_start(
    state: State<'_, AppState>,
    input: crate::backend::models::TeamMemberTurnInput,
) -> RuntimeAppResult<crate::backend::application::TeamMemberStreamSnapshot> {
    AppService::from_runtime(&state.runtime)
        .start_team_member_turn(input)
        .await
}

#[tauri::command]
pub(crate) async fn team_member_replay_start(
    state: State<'_, AppState>,
    team_id: String,
    member_id: String,
) -> RuntimeAppResult<crate::backend::application::TeamMemberStreamSnapshot> {
    AppService::from_runtime(&state.runtime)
        .start_member_replay(&team_id, &member_id)
        .await
}

#[tauri::command]
pub(crate) async fn team_member_stream_snapshot(
    state: State<'_, AppState>,
    team_id: String,
    member_id: String,
    execution_id: String,
) -> RuntimeAppResult<Option<crate::backend::application::TeamMemberStreamSnapshot>> {
    AppService::from_runtime(&state.runtime)
        .get_member_stream(&team_id, &member_id, &execution_id)
        .await
}

#[tauri::command]
pub(crate) fn team_member_task_get(
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<Option<crate::backend::runtime::tasks::TaskSnapshot>> {
    AppService::from_runtime(&state.runtime).get_member_turn_task(&task_id)
}

#[tauri::command]
pub(crate) fn team_member_tasks_list(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<crate::backend::runtime::tasks::TaskSnapshot>> {
    AppService::from_runtime(&state.runtime).list_member_turn_tasks()
}

#[tauri::command]
pub(crate) async fn team_member_turn_cancel(
    state: State<'_, AppState>,
    team_id: String,
    member_id: String,
    execution_id: String,
) -> RuntimeAppResult<crate::backend::application::TeamMemberStreamSnapshot> {
    AppService::from_runtime(&state.runtime)
        .cancel_member_turn(&team_id, &member_id, &execution_id)
        .await
}

#[tauri::command]
pub(crate) async fn team_leader_chat(
    state: State<'_, AppState>,
    input: crate::backend::models::TeamLeaderChatInput,
) -> RuntimeAppResult<crate::backend::models::TeamLeaderChatResult> {
    AppService::from_runtime(&state.runtime)
        .leader_chat(input)
        .await
}

#[tauri::command]
pub(crate) async fn team_run_draft(
    state: State<'_, AppState>,
    input: crate::backend::models::TeamDraftInput,
) -> RuntimeAppResult<crate::backend::models::TeamRunSnapshot> {
    AppService::from_runtime(&state.runtime)
        .draft_team(input)
        .await
}

#[tauri::command]
pub(crate) async fn team_run_get(
    state: State<'_, AppState>,
    run_id: String,
) -> RuntimeAppResult<Option<crate::backend::models::TeamRunSnapshot>> {
    AppService::from_runtime(&state.runtime)
        .get_team_run(&run_id)
        .await
}

#[tauri::command]
pub(crate) async fn team_run_latest(
    state: State<'_, AppState>,
    team_id: String,
) -> RuntimeAppResult<Option<crate::backend::models::TeamRunSnapshot>> {
    AppService::from_runtime(&state.runtime)
        .latest_team_run(&team_id)
        .await
}

#[tauri::command]
pub(crate) async fn team_run_restore(
    state: State<'_, AppState>,
    run_id: String,
) -> RuntimeAppResult<crate::backend::runtime::tasks::TaskSnapshot> {
    AppService::from_runtime(&state.runtime)
        .restore_team_run(&run_id)
        .await
}

#[tauri::command]
pub(crate) fn team_run_cancel(
    state: State<'_, AppState>,
    run_id: String,
) -> RuntimeAppResult<crate::backend::runtime::tasks::TaskSnapshot> {
    AppService::from_runtime(&state.runtime).cancel_team_run_task(&run_id)
}

#[tauri::command]
pub(crate) fn team_run_task(
    state: State<'_, AppState>,
    task_id: String,
) -> RuntimeAppResult<Option<crate::backend::runtime::tasks::TaskSnapshot>> {
    let service = AppService::from_runtime(&state.runtime);
    Ok(state
        .runtime
        .task_runtime()
        .get_for_tenant(service.tenant_id(), &task_id)
        .filter(|snapshot| snapshot.kind == TaskKind::TeamRun))
}

#[tauri::command]
pub(crate) fn list_team_run_tasks(
    state: State<'_, AppState>,
) -> RuntimeAppResult<Vec<crate::backend::runtime::tasks::TaskSnapshot>> {
    let service = AppService::from_runtime(&state.runtime);
    Ok(state.runtime.task_runtime().list_for_tenant(
        service.tenant_id(),
        TaskFilter {
            kind: Some(TaskKind::TeamRun),
            active_only: false,
        },
    ))
}

#[tauri::command]
pub(crate) async fn team_run_review(
    state: State<'_, AppState>,
    input: crate::backend::models::TeamReviewInput,
) -> RuntimeAppResult<crate::backend::models::TeamRunSnapshot> {
    AppService::from_runtime(&state.runtime)
        .review_team_run(input)
        .await
}

#[tauri::command]
pub(crate) async fn team_run_confirm(
    state: State<'_, AppState>,
    input: crate::backend::models::TeamConfirmInput,
) -> RuntimeAppResult<crate::backend::models::TeamRunSnapshot> {
    AppService::from_runtime(&state.runtime)
        .confirm_team_run(input)
        .await
}

#[tauri::command]
pub(crate) async fn team_task_update(
    state: State<'_, AppState>,
    input: crate::backend::models::TeamTaskUpdateInput,
) -> RuntimeAppResult<crate::backend::models::TeamTask> {
    AppService::from_runtime(&state.runtime)
        .update_team_task(input)
        .await
}

#[tauri::command]
pub(crate) async fn team_mailbox_send(
    state: State<'_, AppState>,
    input: crate::backend::models::TeamMailboxSendInput,
) -> RuntimeAppResult<crate::backend::models::TeamMailboxMessage> {
    AppService::from_runtime(&state.runtime)
        .send_team_mailbox(input)
        .await
}

#[tauri::command]
pub(crate) async fn team_mailbox_read(
    state: State<'_, AppState>,
    input: crate::backend::models::TeamMailboxReadInput,
) -> RuntimeAppResult<Vec<crate::backend::models::TeamMailboxMessage>> {
    AppService::from_runtime(&state.runtime)
        .read_team_mailbox(input)
        .await
}

#[tauri::command]
pub(crate) async fn team_tool_credential_issue(
    state: State<'_, AppState>,
    input: crate::backend::models::TeamToolCredentialInput,
) -> RuntimeAppResult<crate::backend::models::TeamToolCredential> {
    AppService::from_runtime(&state.runtime)
        .issue_team_tool_credential(input)
        .await
}

#[tauri::command]
pub(crate) async fn team_tool_tasks(
    state: State<'_, AppState>,
    credential: String,
    input: crate::backend::models::TeamToolTaskListInput,
    member_id: String,
) -> RuntimeAppResult<Vec<crate::backend::models::TeamTask>> {
    AppService::from_runtime(&state.runtime)
        .team_tool_list_tasks(&credential, input, &member_id)
        .await
}

#[tauri::command]
pub(crate) async fn team_tool_task_update(
    state: State<'_, AppState>,
    credential: String,
    input: crate::backend::models::TeamTaskUpdateInput,
) -> RuntimeAppResult<crate::backend::models::TeamTask> {
    AppService::from_runtime(&state.runtime)
        .team_tool_update_task(&credential, input)
        .await
}

#[tauri::command]
pub(crate) async fn team_tool_mailbox_send(
    state: State<'_, AppState>,
    credential: String,
    input: crate::backend::models::TeamMailboxSendInput,
) -> RuntimeAppResult<crate::backend::models::TeamMailboxMessage> {
    AppService::from_runtime(&state.runtime)
        .team_tool_send_mailbox(&credential, input)
        .await
}

#[tauri::command]
pub(crate) async fn team_tool_mailbox_read(
    state: State<'_, AppState>,
    credential: String,
    input: crate::backend::models::TeamMailboxReadInput,
) -> RuntimeAppResult<Vec<crate::backend::models::TeamMailboxMessage>> {
    AppService::from_runtime(&state.runtime)
        .team_tool_read_mailbox(&credential, input)
        .await
}

pub(crate) fn command_handler(
) -> impl Fn(::tauri::ipc::Invoke<::tauri::Wry>) -> bool + Send + Sync + 'static {
    ::tauri::generate_handler![
        get_app_overview,
        set_app_window_icon,
        list_tenants,
        get_active_tenant,
        create_tenant,
        switch_tenant,
        get_app_settings,
        save_app_settings,
        initialize_app_locale_if_unset,
        cancel_app_close_prompt,
        complete_app_close,
        list_assets,
        list_source_assets,
        list_memory_recent,
        get_memory_recent_event_target,
        resolve_memory_context,
        get_memory_project,
        rebuild_memory_scope,
        list_memory_public_tasks,
        get_memory_public_task,
        cancel_memory_public_task,
        retry_memory_public_task,
        search_memory_recall,
        create_memory_recall_session,
        get_memory_recall_session,
        send_memory_recall_turn,
        cancel_memory_recall_turn,
        get_skill_backup_settings,
        update_skill_backup_settings,
        backup_skill,
        backup_skills,
        get_skill_backup_task,
        search_skills,
        start_skill_acquire,
        acquire_skill,
        get_skill_acquire_task,
        list_skill_acquire_tasks,
        cancel_skill_acquire_task,
        list_skill_remote_sources,
        check_skill_remote_sources,
        list_sources,
        list_skill_sources,
        create_source,
        update_source,
        delete_source,
        update_asset_description,
        delete_asset,
        list_profiles,
        list_target_profile_descriptors,
        refresh_target_profile_descriptors,
        create_profile,
        update_profile,
        delete_profile,
        get_navigation_model,
        update_navigation_model,
        list_app_shortcuts,
        list_app_shortcut_settings,
        update_app_shortcuts,
        list_asset_mounts,
        list_asset_mount_statuses,
        refresh_asset_mount_statuses,
        list_skill_groups,
        create_skill_group,
        update_skill_group,
        delete_skill_group,
        set_skill_group_manual_members,
        preview_skill_group_exclusive_mount,
        toggle_asset_mount,
        mount_asset_mount,
        unmount_asset_mount,
        set_asset_mount,
        start_source_scan,
        get_source_scan_task,
        list_source_scan_tasks,
        cancel_source_scan,
        start_batch_mount,
        get_batch_mount_task,
        list_batch_mount_tasks,
        cancel_batch_mount,
        list_conversation_adapters,
        scaffold_conversation_adapter,
        validate_conversation_adapter,
        list_conversation_adapter_runtime_statuses,
        list_agent_catalog,
        list_agent_market,
        inspect_agent_market_item,
        refresh_agent_market,
        get_agent_market_refresh_task,
        list_agent_market_refresh_tasks,
        preview_agent_installation,
        preview_agent_uninstall,
        list_installed_agents,
        get_installed_agent,
        check_agent_runtime,
        get_agent_lifecycle_task,
        list_agent_lifecycle_tasks,
        cancel_agent_lifecycle_task,
        start_agent_installation,
        start_agent_update,
        start_agent_reinstallation,
        start_agent_uninstall,
        enable_agent,
        disable_agent,
        check_agent_connection,
        list_agent_models,
        check_opencode_translation_availability,
        check_prompt_optimization_availability,
        translate_conversation_card_with_opencode,
        translate_conversation_card,
        optimize_prompt,
        test_conversation_translation_connection,
        list_conversation_translation_models,
        start_conversation_card_translation,
        get_ai_execution_task,
        list_ai_execution_tasks,
        cancel_ai_execution_task,
        register_conversation_adapter,
        unregister_conversation_adapter,
        try_run_conversation_adapter,
        project_conversation_command_parts,
        list_conversation_sources,
        upsert_conversation_source,
        disable_conversation_source,
        list_conversation_script_catalog,
        register_conversation_adapter_local,
        inspect_conversation_adapter_package,
        prepare_conversation_adapter_package_change,
        list_conversation_adapter_packages,
        list_conversation_adapter_package_releases,
        list_installed_conversation_adapter_package_versions,
        switch_conversation_adapter_package_version,
        rollback_conversation_adapter_package_version,
        delete_conversation_adapter_package_version,
        refresh_conversation_adapter_catalogs,
        check_conversation_adapter_package_updates,
        set_conversation_adapter_package_update_policy,
        install_conversation_adapter_package,
        update_conversation_adapter_package,
        uninstall_conversation_adapter_package,
        get_conversation_adapter_package_task,
        install_conversation_script,
        get_conversation_script_install_task,
        sync_conversations,
        get_conversation_sync_task,
        list_conversation_sync_tasks,
        cancel_conversation_sync,
        audit_conversation_data,
        repair_conversation_data,
        get_conversation_data_maintenance_task,
        list_conversation_data_maintenance_tasks,
        cancel_conversation_data_maintenance,
        rollback_conversation_data,
        list_conversation_sessions,
        get_conversation_session,
        export_conversation_session,
        list_web_record_sessions,
        get_web_record_session,
        search_conversation_records,
        search_recent_incremental_conversation_records,
        get_conversation_search_index_status,
        start_conversation_search_index_rebuild,
        get_conversation_search_index_task,
        export_web_record_session,
        list_conversation_questions,
        get_conversation_question,
        list_conversation_blocks,
        get_conversation_block,
        merge_conversation_questions,
        split_conversation_question,
        update_conversation_part_translation,
        create_plan,
        execute_plan,
        get_cli_tools_status,
        install_cli_tools,
        logs_get_snapshot,
        logs_open_log_directory,
        logs_write_operation,
        copy_prompt_card_to_clipboard,
        reveal_path,
        create_team,
        get_team,
        list_teams,
        update_team,
        delete_team,
        team_member_turn_start,
        team_member_replay_start,
        team_member_stream_snapshot,
        team_member_task_get,
        team_member_tasks_list,
        team_member_turn_cancel,
        team_leader_chat,
        team_run_draft,
        team_run_get,
        team_run_latest,
        team_run_restore,
        team_run_cancel,
        team_run_task,
        list_team_run_tasks,
        team_run_review,
        team_run_confirm,
        team_task_update,
        team_mailbox_send,
        team_mailbox_read,
        team_tool_credential_issue,
        team_tool_tasks,
        team_tool_task_update,
        team_tool_mailbox_send,
        team_tool_mailbox_read
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::ai_execution::{executor::BackendFuture, AiExecutionResult};
    use crate::backend::card_translation::{
        ConversationTranslationCli, ConversationTranslationProvider,
    };
    use crate::backend::dto::{PhysicalMountStateDto, SkillBackupState};
    use crate::backend::models::{
        AppKind, AssetFormat, AssetGroup, AssetGroupRules, AssetKind, DeploymentStrategy,
        ProfileSafety, RuleSet, SourceKind, SourceOrigin, SourceScannerKind,
    };
    use std::{
        path::{Path, PathBuf},
        process::Command,
        sync::{
            atomic::{AtomicBool, Ordering},
            Mutex,
        },
        time::{Duration, Instant},
    };
    use uuid::Uuid;

    #[derive(Default)]
    struct RecordingAiTaskEmitter {
        snapshots: Mutex<Vec<AiExecutionTaskSnapshot>>,
    }

    impl AiExecutionTaskEmitter for RecordingAiTaskEmitter {
        fn emit(&self, snapshot: &AiExecutionTaskSnapshot) {
            self.snapshots.lock().unwrap().push(snapshot.clone());
        }
    }

    struct AdapterFakeRuntime {
        cleaned: Arc<AtomicBool>,
    }

    impl AgentExecutionRuntime for AdapterFakeRuntime {
        fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
            Box::pin(async move {
                request.report_phase(AiExecutionPhase::Resolving);
                request.report_phase(AiExecutionPhase::Spawning);
                request.report_phase(AiExecutionPhase::Prompting);
                self.cleaned.store(true, Ordering::SeqCst);
                Ok(AiExecutionResult {
                    text: "adapter result".to_string(),
                    agent_id: request.agent_id,
                    protocol: crate::backend::agents::types::AgentProtocol::Acp,
                    requested_model: request.model,
                    elapsed_ms: 1,
                    persistent_binding: None,
                    replay_text: None,
                })
            })
        }
    }

    struct FailingAdapterRuntime;

    impl AgentExecutionRuntime for FailingAdapterRuntime {
        fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
            Box::pin(async move {
                request.report_phase(AiExecutionPhase::Prompting);
                request.report_phase(AiExecutionPhase::Closing);
                request.report_phase(AiExecutionPhase::CleaningUp);
                request.report_cleanup(AiExecutionCleanupReport {
                    process_reaped: false,
                    workspace_removed: true,
                    failure_count: 1,
                    session_closed: Some(false),
                    session_deleted: Some(false),
                    session_delete_method: None,
                });
                Err(AiExecutionError::Protocol {
                    operation: "adapter_failure",
                })
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tauri_01_02_start_preparation_is_fast_and_has_no_global_lock_dependency() {
        let tasks = Arc::new(BackgroundTaskRegistry::default());
        let emitter = Arc::new(RecordingAiTaskEmitter::default());
        let started = Instant::now();

        let (snapshot, request) = prepare_ai_execution_task(
            tasks.clone(),
            opencode_translation_request(),
            emitter.clone(),
        )
        .unwrap();

        assert!(started.elapsed() < Duration::from_millis(100));
        assert_eq!(
            snapshot.state,
            crate::adapters::tauri::background_tasks::AiExecutionTaskState::Queued
        );
        assert_eq!(request.prompt, "translate this");
        assert_eq!(emitter.snapshots.lock().unwrap().as_slice(), [snapshot]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tauri_03_04_phase_and_terminal_events_are_full_snapshots_after_runtime_cleanup() {
        let tasks = Arc::new(BackgroundTaskRegistry::default());
        let emitter = Arc::new(RecordingAiTaskEmitter::default());
        let cleaned = Arc::new(AtomicBool::new(false));
        let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(AdapterFakeRuntime {
            cleaned: cleaned.clone(),
        });
        let (queued, request) = prepare_ai_execution_task(
            tasks.clone(),
            opencode_translation_request(),
            emitter.clone(),
        )
        .unwrap();

        run_ai_execution_task(
            tasks.clone(),
            runtime,
            queued.id.clone(),
            request,
            emitter.clone(),
        )
        .await;

        assert!(cleaned.load(Ordering::SeqCst));
        let events = emitter.snapshots.lock().unwrap();
        assert_eq!(events.first().unwrap().phase, AiExecutionPhase::Queued);
        assert!(events
            .iter()
            .any(|snapshot| snapshot.phase == AiExecutionPhase::Resolving));
        assert!(events
            .iter()
            .any(|snapshot| snapshot.phase == AiExecutionPhase::Prompting));
        let terminal = events.last().unwrap();
        assert_eq!(
            terminal.state,
            crate::adapters::tauri::background_tasks::AiExecutionTaskState::Succeeded
        );
        assert_eq!(terminal.result.as_ref().unwrap().text, "adapter result");
        let serialized = serde_json::to_value(terminal).unwrap();
        for field in [
            "id",
            "purpose",
            "agent_id",
            "state",
            "phase",
            "created_at",
            "updated_at",
            "finished_at",
            "result",
            "error",
        ] {
            assert!(serialized.get(field).is_some(), "missing field {field}");
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tauri_03_04_failure_keeps_execution_phase_separate_from_cleanup_phase() {
        let tasks = Arc::new(BackgroundTaskRegistry::default());
        let emitter = Arc::new(RecordingAiTaskEmitter::default());
        let runtime: Arc<dyn AgentExecutionRuntime> = Arc::new(FailingAdapterRuntime);
        let (queued, request) = prepare_ai_execution_task(
            tasks.clone(),
            opencode_translation_request(),
            emitter.clone(),
        )
        .unwrap();

        run_ai_execution_task(tasks.clone(), runtime, queued.id.clone(), request, emitter).await;

        let failed = tasks.ai_execution_snapshot(&queued.id).unwrap().unwrap();
        assert_eq!(failed.phase, AiExecutionPhase::CleaningUp);
        assert_eq!(
            failed.error.as_ref().and_then(|error| error.phase),
            Some(AiExecutionPhase::Prompting)
        );
        assert_eq!(
            failed.cleanup,
            Some(AiExecutionCleanupReport {
                process_reaped: false,
                workspace_removed: true,
                failure_count: 1,
                session_closed: Some(false),
                session_deleted: Some(false),
                session_delete_method: None,
            })
        );
    }

    #[test]
    fn tauri_05_06_get_list_and_cancel_use_the_central_registry_token() {
        let tasks = Arc::new(BackgroundTaskRegistry::default());
        let emitter = Arc::new(RecordingAiTaskEmitter::default());
        let (queued, request) =
            prepare_ai_execution_task(tasks.clone(), opencode_translation_request(), emitter)
                .unwrap();
        let cancellation = request.cancellation.clone();

        let cancelling = tasks.cancel_ai_execution(&queued.id).unwrap();

        assert!(cancellation.is_cancelled());
        assert_eq!(cancelling.phase, AiExecutionPhase::Cancelling);
        assert_eq!(
            tasks.ai_execution_snapshot(&queued.id).unwrap(),
            Some(cancelling.clone())
        );
        assert_eq!(tasks.ai_execution_snapshots().unwrap(), [cancelling]);
    }

    fn opencode_translation_request() -> ConversationTranslationRequest {
        ConversationTranslationRequest {
            agent_id: None,
            provider: ConversationTranslationProvider::Cli,
            cli: ConversationTranslationCli::Opencode,
            model: "model/a".to_string(),
            prompt: "translate this".to_string(),
        }
    }

    async fn open_test_database_async(db_path: &Path) -> crate::backend::store::Database {
        crate::backend::store::Database::open_initialized_async(db_path)
            .await
            .expect("open initialized db")
    }

    async fn upsert_test_source_async(db: &crate::backend::store::Database, source: &Source) {
        crate::backend::store::upsert_source_sqlx(db.pool(), "default", source)
            .await
            .expect("insert source");
    }

    async fn load_test_sources_async(db: &crate::backend::store::Database) -> Vec<Source> {
        crate::backend::store::load_sources_sqlx(db.pool(), "default")
            .await
            .expect("load sources")
    }

    async fn upsert_test_profile_async(
        db: &crate::backend::store::Database,
        profile: &TargetProfile,
    ) {
        crate::backend::store::upsert_profile_sqlx(db.pool(), "default", profile)
            .await
            .expect("insert profile");
    }

    async fn delete_test_profile_async(db: &crate::backend::store::Database, profile_id: &str) {
        crate::backend::store::delete_profile_sqlx(db.pool(), "default", profile_id)
            .await
            .expect("delete profile");
    }

    async fn load_test_profiles_async(db: &crate::backend::store::Database) -> Vec<TargetProfile> {
        crate::backend::store::load_profiles_sqlx(db.pool(), "default")
            .await
            .expect("load profiles")
    }

    async fn replace_test_source_assets_async(
        db: &crate::backend::store::Database,
        source_id: &str,
        assets: &[Asset],
    ) {
        crate::backend::store::replace_source_assets_sqlx(db.pool(), "default", source_id, assets)
            .await
            .expect("insert assets");
    }

    async fn set_test_asset_mount_async(
        db: &crate::backend::store::Database,
        asset_id: &str,
        profile_id: &str,
        enabled: bool,
        strategy: DeploymentStrategy,
    ) -> AssetMount {
        crate::backend::store::set_asset_mount_sqlx(
            db.pool(),
            "default",
            asset_id,
            profile_id,
            enabled,
            strategy,
        )
        .await
        .expect("insert mount")
    }

    async fn load_test_assets_async(db: &crate::backend::store::Database) -> Vec<Asset> {
        crate::backend::store::load_assets_sqlx(db.pool(), "default", None)
            .await
            .expect("load assets")
    }

    async fn load_test_mounts_async(
        db: &crate::backend::store::Database,
        asset_id: Option<&str>,
    ) -> Vec<AssetMount> {
        crate::backend::store::load_asset_mounts_sqlx(db.pool(), "default", asset_id)
            .await
            .expect("load mounts")
    }

    async fn upsert_test_group_async(db: &crate::backend::store::Database, group: &AssetGroup) {
        crate::backend::store::upsert_asset_group_sqlx(db.pool(), "default", group)
            .await
            .expect("insert group");
    }

    async fn replace_test_group_members_async(
        db: &crate::backend::store::Database,
        group_id: &str,
        asset_ids: &[String],
        assets: &[Asset],
    ) {
        crate::backend::store::replace_asset_group_members_sqlx(
            db.pool(),
            "default",
            group_id,
            asset_ids,
            assets,
        )
        .await
        .expect("insert group members");
    }

    async fn load_test_mount_observations_async(
        db: &crate::backend::store::Database,
    ) -> Vec<crate::backend::dto::AssetMountObservation> {
        crate::backend::store::load_asset_mount_observations_sqlx(db.pool(), "default")
            .await
            .expect("load observations")
    }

    async fn is_test_managed_deployment_async(
        db: &crate::backend::store::Database,
        profile_id: &str,
        asset_id: &str,
        target_path: &str,
    ) -> bool {
        crate::backend::store::is_managed_deployment_sqlx(
            db.pool(),
            "default",
            profile_id,
            asset_id,
            target_path,
        )
        .await
        .expect("deployment state")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn refresh_recorded_assets_prunes_missing_sources() {
        let db_path = unique_temp_path("assetiweave-refresh-recorded");
        let database = open_test_database_async(&db_path).await;
        let source = test_missing_source("missing-recorded-source");
        upsert_test_source_async(&database, &source).await;

        refresh_recorded_assets(database.pool(), "default")
            .await
            .expect("refresh recorded assets");

        assert!(!load_test_sources_async(&database)
            .await
            .iter()
            .any(|candidate| candidate.id == source.id));
        std::fs::remove_file(db_path).ok();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn source_scan_prunes_missing_sources_without_error_row() {
        let db_path = unique_temp_path("assetiweave-scan-missing-source");
        let database = open_test_database_async(&db_path).await;
        let source = test_missing_source("missing-scan-source");
        upsert_test_source_async(&database, &source).await;

        scan_selected_sources(
            database.pool(),
            "default",
            vec![source.clone()],
            crate::backend::capabilities::scan_source,
        )
        .await
        .expect("scan selected sources");

        assert!(!load_test_sources_async(&database)
            .await
            .iter()
            .any(|candidate| candidate.id == source.id));
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn target_profile_input_uses_skill_mount_defaults() {
        let profile = target_profile_from_input(TargetProfileInput {
            id: None,
            name: "  Team App  ".to_string(),
            app_kind: None,
            target_provider_id: None,
            target_paths: Some(vec!["  ~/team-app/skills  ".to_string()]),
            supported_kinds: None,
            deployment_strategy: None,
            enabled: None,
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("build profile");

        assert_eq!(profile.id, "team-app");
        assert_eq!(profile.name, "Team App");
        assert_eq!(profile.app_kind, Some(AppKind::Custom));
        assert_eq!(profile.target_paths, vec!["~/team-app/skills"]);
        assert_eq!(profile.supported_kinds, vec![AssetKind::Skill]);
        assert_eq!(profile.include.kinds, vec![AssetKind::Skill]);
        assert_eq!(profile.exclude.kinds, vec![AssetKind::Unclassified]);
        assert!(!profile.safety.allow_remove);
        assert!(!profile.safety.allow_overwrite);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn target_profile_can_be_persisted_updated_and_deleted() {
        let db_path = unique_temp_path("assetiweave-profile-crud-db");
        let database = open_test_database_async(&db_path).await;
        let mut profile = target_profile_from_input(TargetProfileInput {
            id: Some("team-app".to_string()),
            name: "Team App".to_string(),
            app_kind: Some(AppKind::Custom),
            target_provider_id: None,
            target_paths: Some(vec!["~/team-app/skills".to_string()]),
            supported_kinds: None,
            deployment_strategy: None,
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("build profile");

        upsert_test_profile_async(&database, &profile).await;
        profile.name = "Team App Edited".to_string();
        upsert_test_profile_async(&database, &profile).await;

        assert!(load_test_profiles_async(&database)
            .await
            .iter()
            .any(|candidate| candidate.id == profile.id && candidate.name == "Team App Edited"));

        ensure_profile_can_be_deleted_sqlx(database.pool(), "default", &profile.id)
            .await
            .expect("profile delete guard");
        delete_test_profile_async(&database, &profile.id).await;
        assert!(!load_test_profiles_async(&database)
            .await
            .iter()
            .any(|candidate| candidate.id == profile.id));
        std::fs::remove_file(db_path).ok();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn default_app_profile_delete_is_blocked() {
        let db_path = unique_temp_path("assetiweave-default-profile-delete-db");
        let database = open_test_database_async(&db_path).await;

        let error = ensure_profile_can_be_deleted_sqlx(database.pool(), "default", "codex")
            .await
            .expect_err("delete blocked");

        assert!(error.to_string().contains("default app cannot be deleted"));
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn target_profile_delete_is_blocked_when_mount_exists() {
        let db_path = unique_temp_path("assetiweave-profile-delete-block-db");
        let source_root = unique_temp_path("assetiweave-profile-delete-block-source");
        let target_root = unique_temp_path("assetiweave-profile-delete-block-target");
        let asset_path = source_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("profile-delete-source", source_root.clone());
        let profile = test_profile("team-app", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
        upsert_test_profile_async(&database, &profile).await;
        mount_asset_mount_record(database.pool(), "default", &asset.id, &profile.id)
            .await
            .expect("mount asset");

        let error = ensure_profile_can_be_deleted_sqlx(database.pool(), "default", &profile.id)
            .await
            .expect_err("delete blocked");

        assert!(
            error.to_string().contains("managed deployments")
                || error.to_string().contains("mounted assets")
        );
        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn refresh_recorded_assets_removes_mounts_for_deleted_assets() {
        let db_path = unique_temp_path("assetiweave-refresh-deleted-mount");
        let source_root = unique_temp_path("assetiweave-existing-source");
        std::fs::create_dir_all(&source_root).expect("create source root");
        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-deleted-asset", source_root.clone());
        let asset = test_asset(&source, "deleted-asset", source_root.join("deleted-asset"));
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
        set_test_asset_mount_async(
            &database,
            &asset.id,
            "codex",
            true,
            DeploymentStrategy::SymlinkToSource,
        )
        .await;

        refresh_recorded_assets(database.pool(), "default")
            .await
            .expect("refresh recorded assets");

        assert!(load_test_assets_async(&database)
            .await
            .iter()
            .all(|candidate| candidate.id != asset.id));
        assert!(load_test_mounts_async(&database, Some(&asset.id))
            .await
            .is_empty());
        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn mount_asset_mount_creates_symlink_and_enables_mount() {
        let db_path = unique_temp_path("assetiweave-mount-db");
        let source_root = unique_temp_path("assetiweave-mount-source");
        let target_root = unique_temp_path("assetiweave-mount-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-unmounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path.clone());
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
        upsert_test_profile_async(&database, &profile).await;

        let result = mount_asset_mount_record(database.pool(), "default", &asset.id, &profile.id)
            .await
            .expect("mount");

        let metadata = std::fs::symlink_metadata(&target_path).expect("target metadata");
        assert!(metadata.file_type().is_symlink());
        assert_eq!(
            std::fs::read_link(&target_path).expect("read symlink"),
            asset_path.canonicalize().expect("canonical asset path")
        );
        assert!(result.mount.enabled);
        assert_eq!(result.status.state, PhysicalMountStateDto::Mounted);
        assert!(
            is_test_managed_deployment_async(
                &database,
                &profile.id,
                &asset.id,
                &target_path.to_string_lossy()
            )
            .await
        );

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn mount_asset_mount_links_to_real_source_directory() {
        let db_path = unique_temp_path("assetiweave-mount-real-source-db");
        let real_root = unique_temp_path("assetiweave-mount-real-source-real");
        let alias_root = unique_temp_path("assetiweave-mount-real-source-alias");
        let target_root = unique_temp_path("assetiweave-mount-real-source-target");
        let real_asset_path = real_root.join("skill-a");
        let alias_asset_path = alias_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&real_asset_path).expect("create real asset dir");
        std::fs::create_dir_all(&alias_root).expect("create alias root");
        std::fs::create_dir_all(&target_root).expect("create target dir");
        std::os::unix::fs::symlink(&real_asset_path, &alias_asset_path)
            .expect("create alias asset symlink");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-aliased-asset", alias_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", alias_asset_path.clone());
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
        upsert_test_profile_async(&database, &profile).await;

        let result = mount_asset_mount_record(database.pool(), "default", &asset.id, &profile.id)
            .await
            .expect("mount");

        assert_eq!(
            std::fs::read_link(&target_path).expect("read target symlink"),
            real_asset_path
                .canonicalize()
                .expect("canonical real asset")
        );
        let expected_source = real_asset_path
            .canonicalize()
            .expect("canonical real asset")
            .to_string_lossy()
            .to_string();
        assert_eq!(
            result.status.linked_source.as_deref(),
            Some(expected_source.as_str())
        );
        assert_eq!(result.status.state, PhysicalMountStateDto::Mounted);

        std::fs::remove_dir_all(real_root).ok();
        std::fs::remove_dir_all(alias_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn set_asset_mount_creates_symlink_before_enabling_mount() {
        let db_path = unique_temp_path("assetiweave-set-mount-db");
        let source_root = unique_temp_path("assetiweave-set-mount-source");
        let target_root = unique_temp_path("assetiweave-set-mount-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-set-mounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
        upsert_test_profile_async(&database, &profile).await;
        let mount = set_asset_mount_record(
            database.pool(),
            "default",
            &asset.id,
            &profile.id,
            true,
            None,
        )
        .await
        .expect("set mount enabled");

        assert!(mount.enabled);
        assert!(std::fs::symlink_metadata(&target_path)
            .expect("target metadata")
            .file_type()
            .is_symlink());

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn apply_skill_group_mount_only_mounts_group_members() {
        let db_path = unique_temp_path("assetiweave-group-mount-db");
        let source_root = unique_temp_path("assetiweave-group-mount-source");
        let target_root = unique_temp_path("assetiweave-group-mount-target");
        let asset_path_a = source_root.join("skill-a");
        let asset_path_b = source_root.join("skill-b");
        let target_path_a = target_root.join("skill-a");
        let target_path_b = target_root.join("skill-b");
        std::fs::create_dir_all(&asset_path_a).expect("create asset dir a");
        std::fs::create_dir_all(&asset_path_b).expect("create asset dir b");
        std::fs::create_dir_all(&target_root).expect("create target dir");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-group-assets", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset_a = test_asset(&source, "skill-a", asset_path_a.clone());
        let asset_b = test_asset(&source, "skill-b", asset_path_b);
        let assets = vec![asset_a.clone(), asset_b.clone()];
        let group = test_group("frontend");
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, &assets).await;
        upsert_test_profile_async(&database, &profile).await;
        upsert_test_group_async(&database, &group).await;
        replace_test_group_members_async(&database, &group.id, &[asset_a.id.clone()], &assets)
            .await;

        let result = apply_skill_group_mount_record(
            database.pool(),
            "default",
            &group.id,
            &profile.id,
            true,
        )
        .await
        .expect("apply group");

        assert_eq!(result.requested_count, 1);
        assert_eq!(result.updated_count, 1);
        assert_eq!(result.error_count, 0);
        assert!(std::fs::symlink_metadata(&target_path_a)
            .expect("target a metadata")
            .file_type()
            .is_symlink());
        assert_eq!(
            std::fs::read_link(&target_path_a).expect("read symlink"),
            asset_path_a.canonicalize().expect("canonical asset path a")
        );
        assert!(!target_path_b.exists());
        assert!(load_test_mounts_async(&database, Some(&asset_b.id))
            .await
            .is_empty());

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn preview_exclusive_group_mount_uses_enabled_group_union_without_mutation() {
        let db_path = unique_temp_path("assetiweave-exclusive-preview-db");
        let source_root = unique_temp_path("assetiweave-exclusive-preview-source");
        let codex_target = unique_temp_path("assetiweave-exclusive-preview-codex");
        let cursor_target = unique_temp_path("assetiweave-exclusive-preview-cursor");
        let asset_path_a = source_root.join("skill-a");
        let asset_path_b = source_root.join("skill-b");
        let asset_path_c = source_root.join("skill-c");
        std::fs::create_dir_all(&asset_path_a).expect("create asset dir a");
        std::fs::create_dir_all(&asset_path_b).expect("create asset dir b");
        std::fs::create_dir_all(&asset_path_c).expect("create asset dir c");
        std::fs::create_dir_all(&codex_target).expect("create codex target");
        std::fs::create_dir_all(&cursor_target).expect("create cursor target");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-exclusive-preview-assets", source_root.clone());
        let codex = test_profile("codex", codex_target.clone());
        let cursor = test_profile("cursor", cursor_target.clone());
        let asset_a = test_asset(&source, "skill-a", asset_path_a);
        let asset_b = test_asset(&source, "skill-b", asset_path_b);
        let asset_c = test_asset(&source, "skill-c", asset_path_c);
        let skill_assets = vec![asset_a.clone(), asset_b.clone(), asset_c.clone()];
        let group_a = test_group("frontend");
        let group_b = test_group("automation");
        let mut disabled_group = test_group("disabled");
        disabled_group.enabled = false;
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, &skill_assets).await;
        upsert_test_profile_async(&database, &codex).await;
        upsert_test_profile_async(&database, &cursor).await;
        for group in [&group_a, &group_b, &disabled_group] {
            upsert_test_group_async(&database, group).await;
        }
        replace_test_group_members_async(
            &database,
            &group_a.id,
            &[asset_a.id.clone(), asset_b.id.clone()],
            &skill_assets,
        )
        .await;
        replace_test_group_members_async(
            &database,
            &group_b.id,
            &[asset_b.id.clone()],
            &skill_assets,
        )
        .await;
        replace_test_group_members_async(
            &database,
            &disabled_group.id,
            &[asset_c.id.clone()],
            &skill_assets,
        )
        .await;
        mount_asset_mount_record(database.pool(), "default", &asset_a.id, &codex.id)
            .await
            .expect("mount skill a");
        mount_asset_mount_record(database.pool(), "default", &asset_c.id, &codex.id)
            .await
            .expect("mount skill c");
        mount_asset_mount_record(database.pool(), "default", &asset_c.id, &cursor.id)
            .await
            .expect("mount skill c cursor");

        let preview = build_skill_group_exclusive_mount_preview_sqlx(
            database.pool(),
            "default",
            &SkillGroupExclusiveMountInput {
                group_ids: vec![
                    group_a.id.clone(),
                    group_b.id.clone(),
                    disabled_group.id.clone(),
                    group_a.id.clone(),
                ],
                profile_id: codex.id.clone(),
                mount_selected: true,
                dry_run: true,
            },
        )
        .await
        .expect("preview exclusive mount");

        assert_eq!(
            preview.group_ids,
            vec![group_a.id.clone(), group_b.id.clone()]
        );
        assert_eq!(
            preview.selected_skill_ids,
            vec![asset_a.id.clone(), asset_b.id.clone()]
        );
        assert_eq!(preview.keep, vec![exclusive_item(&asset_a)]);
        assert_eq!(preview.mount, vec![exclusive_item(&asset_b)]);
        assert_eq!(preview.unmount, vec![exclusive_item(&asset_c)]);
        assert_eq!(preview.skipped_count, 0);
        assert!(codex_target.join("skill-c").exists());
        assert!(cursor_target.join("skill-c").exists());
        assert!(load_test_mounts_async(&database, Some(&asset_c.id))
            .await
            .iter()
            .any(|mount| mount.profile_id == codex.id && mount.enabled));

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(codex_target).ok();
        std::fs::remove_dir_all(cursor_target).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn apply_exclusive_group_mount_only_changes_target_profile_skill_mounts() {
        let db_path = unique_temp_path("assetiweave-exclusive-apply-db");
        let source_root = unique_temp_path("assetiweave-exclusive-apply-source");
        let codex_target = unique_temp_path("assetiweave-exclusive-apply-codex");
        let cursor_target = unique_temp_path("assetiweave-exclusive-apply-cursor");
        let asset_path_a = source_root.join("skill-a");
        let asset_path_b = source_root.join("skill-b");
        let asset_path_c = source_root.join("skill-c");
        let prompt_path = source_root.join("prompt-a");
        let prompt_target = codex_target.join("prompt-a");
        std::fs::create_dir_all(&asset_path_a).expect("create asset dir a");
        std::fs::create_dir_all(&asset_path_b).expect("create asset dir b");
        std::fs::create_dir_all(&asset_path_c).expect("create asset dir c");
        std::fs::create_dir_all(&prompt_path).expect("create prompt dir");
        std::fs::create_dir_all(&codex_target).expect("create codex target");
        std::fs::create_dir_all(&cursor_target).expect("create cursor target");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-exclusive-apply-assets", source_root.clone());
        let codex = test_profile("codex", codex_target.clone());
        let cursor = test_profile("cursor", cursor_target.clone());
        let asset_a = test_asset(&source, "skill-a", asset_path_a);
        let asset_b = test_asset(&source, "skill-b", asset_path_b);
        let asset_c = test_asset(&source, "skill-c", asset_path_c);
        let prompt =
            test_asset_with_kind(&source, "prompt-a", prompt_path.clone(), AssetKind::Prompt);
        let all_assets = vec![
            asset_a.clone(),
            asset_b.clone(),
            asset_c.clone(),
            prompt.clone(),
        ];
        let skill_assets = vec![asset_a.clone(), asset_b.clone(), asset_c.clone()];
        let group_a = test_group("frontend");
        let group_b = test_group("automation");
        let mut disabled_group = test_group("disabled");
        disabled_group.enabled = false;
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, &all_assets).await;
        upsert_test_profile_async(&database, &codex).await;
        upsert_test_profile_async(&database, &cursor).await;
        for group in [&group_a, &group_b, &disabled_group] {
            upsert_test_group_async(&database, group).await;
        }
        replace_test_group_members_async(
            &database,
            &group_a.id,
            &[asset_a.id.clone(), asset_b.id.clone()],
            &skill_assets,
        )
        .await;
        replace_test_group_members_async(
            &database,
            &group_b.id,
            &[asset_b.id.clone()],
            &skill_assets,
        )
        .await;
        replace_test_group_members_async(
            &database,
            &disabled_group.id,
            &[asset_c.id.clone()],
            &skill_assets,
        )
        .await;
        mount_asset_mount_record(database.pool(), "default", &asset_a.id, &codex.id)
            .await
            .expect("mount skill a");
        mount_asset_mount_record(database.pool(), "default", &asset_c.id, &codex.id)
            .await
            .expect("mount skill c");
        mount_asset_mount_record(database.pool(), "default", &asset_c.id, &cursor.id)
            .await
            .expect("mount skill c cursor");
        std::os::unix::fs::symlink(&prompt_path, &prompt_target).expect("create prompt symlink");
        set_test_asset_mount_async(
            &database,
            &prompt.id,
            &codex.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        )
        .await;

        let result = apply_skill_group_exclusive_mount_record(
            database.pool(),
            "default",
            &SkillGroupExclusiveMountInput {
                group_ids: vec![
                    group_a.id.clone(),
                    group_b.id.clone(),
                    disabled_group.id.clone(),
                ],
                profile_id: codex.id.clone(),
                mount_selected: true,
                dry_run: false,
            },
        )
        .await
        .expect("apply exclusive mount");

        assert_eq!(result.preview.keep_count, 1);
        assert_eq!(result.preview.mount_count, 1);
        assert_eq!(result.preview.unmount_count, 1);
        assert_eq!(result.preview.skipped_count, 0);
        assert!(result.errors.is_empty());
        assert!(codex_target.join("skill-a").exists());
        assert!(codex_target.join("skill-b").exists());
        assert!(!codex_target.join("skill-c").exists());
        assert!(cursor_target.join("skill-c").exists());
        assert!(prompt_target.exists());
        let skill_c_mounts = load_test_mounts_async(&database, Some(&asset_c.id)).await;
        assert!(skill_c_mounts
            .iter()
            .any(|mount| mount.profile_id == codex.id && !mount.enabled));
        assert!(skill_c_mounts
            .iter()
            .any(|mount| mount.profile_id == cursor.id && mount.enabled));
        assert!(load_test_mounts_async(&database, Some(&prompt.id))
            .await
            .iter()
            .any(|mount| mount.profile_id == codex.id && mount.enabled));

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(codex_target).ok();
        std::fs::remove_dir_all(cursor_target).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn preview_exclusive_group_mount_reports_risks_without_forcing_repairs() {
        let db_path = unique_temp_path("assetiweave-exclusive-risk-db");
        let external_root = unique_temp_path("assetiweave-exclusive-risk-external");
        let app_local_root = unique_temp_path("assetiweave-exclusive-risk-local");
        let target_root = unique_temp_path("assetiweave-exclusive-risk-target");
        let external_asset_path = external_root.join("external-skill");
        let app_local_asset_path = app_local_root.join("app-local-skill");
        let external_target = target_root.join("external-skill");
        std::fs::create_dir_all(&external_asset_path).expect("create external asset dir");
        std::fs::create_dir_all(&app_local_asset_path).expect("create app local asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");
        std::os::unix::fs::symlink(&external_asset_path, &external_target)
            .expect("create unmanaged external symlink");

        let database = open_test_database_async(&db_path).await;
        let external_source = test_source("external-source", external_root.clone());
        let app_local_source = test_source_with_origin(
            "app-local-source",
            app_local_root.clone(),
            SourceOrigin::AppLocal,
        );
        let profile = test_profile("codex", target_root.clone());
        let external_asset = test_asset(&external_source, "external-skill", external_asset_path);
        let app_local_asset =
            test_asset(&app_local_source, "app-local-skill", app_local_asset_path);
        let assets = vec![external_asset.clone(), app_local_asset.clone()];
        let group = test_group("selected-app-local");
        upsert_test_source_async(&database, &external_source).await;
        upsert_test_source_async(&database, &app_local_source).await;
        replace_test_source_assets_async(&database, &external_source.id, &[external_asset.clone()])
            .await;
        replace_test_source_assets_async(
            &database,
            &app_local_source.id,
            &[app_local_asset.clone()],
        )
        .await;
        upsert_test_profile_async(&database, &profile).await;
        upsert_test_group_async(&database, &group).await;
        replace_test_group_members_async(
            &database,
            &group.id,
            &[app_local_asset.id.clone()],
            &assets,
        )
        .await;

        let result = apply_skill_group_exclusive_mount_record(
            database.pool(),
            "default",
            &SkillGroupExclusiveMountInput {
                group_ids: vec![group.id.clone()],
                profile_id: profile.id.clone(),
                mount_selected: true,
                dry_run: false,
            },
        )
        .await
        .expect("apply exclusive mount");

        assert_eq!(result.preview.mount_count, 0);
        assert_eq!(result.preview.unmount_count, 0);
        assert_eq!(result.preview.skipped_count, 2);
        assert!(result
            .preview
            .skipped
            .iter()
            .any(|item| item.asset_id == app_local_asset.id
                && item.reason.contains("must be backed up")));
        assert!(result
            .preview
            .skipped
            .iter()
            .any(|item| item.asset_id == external_asset.id
                && item.reason.contains("not managed by AssetIWeave")));
        assert!(result.errors.is_empty());
        assert!(external_target.exists());

        std::fs::remove_dir_all(external_root).ok();
        std::fs::remove_dir_all(app_local_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn scan_asset_mount_statuses_does_not_mutate_snapshot() {
        let db_path = unique_temp_path("assetiweave-status-scan-db");
        let source_root = unique_temp_path("assetiweave-status-scan-source");
        let target_root = unique_temp_path("assetiweave-status-scan-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");
        std::os::unix::fs::symlink(&asset_path, &target_path).expect("create physical symlink");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-scanned-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
        upsert_test_profile_async(&database, &profile).await;
        set_test_asset_mount_async(
            &database,
            &asset.id,
            &profile.id,
            false,
            DeploymentStrategy::SymlinkToSource,
        )
        .await;

        let statuses = scan_asset_mount_statuses_sqlx(database.pool(), "default", None)
            .await
            .expect("scan statuses");

        assert!(statuses.iter().any(|status| {
            status.asset_id == asset.id
                && status.profile_id == profile.id
                && status.state == PhysicalMountStateDto::Mounted
        }));
        assert!(load_test_mounts_async(&database, Some(&asset.id))
            .await
            .iter()
            .all(|mount| !mount.enabled));
        assert!(
            !is_test_managed_deployment_async(
                &database,
                &profile.id,
                &asset.id,
                &target_path.to_string_lossy()
            )
            .await
        );

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn sync_asset_mount_observations_records_physical_mount_snapshot() {
        let db_path = unique_temp_path("assetiweave-observation-db");
        let source_root = unique_temp_path("assetiweave-observation-source");
        let target_root = unique_temp_path("assetiweave-observation-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");
        std::os::unix::fs::symlink(&asset_path, &target_path).expect("create physical symlink");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-observed-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
        upsert_test_profile_async(&database, &profile).await;
        let original_mount = set_test_asset_mount_async(
            &database,
            &asset.id,
            &profile.id,
            false,
            DeploymentStrategy::SymlinkToSource,
        )
        .await;

        sync_asset_mount_observations(database.pool(), "default", None)
            .await
            .expect("sync observations");

        let observations = load_test_mount_observations_async(&database).await;
        let observation = observations
            .iter()
            .find(|candidate| candidate.asset_id == asset.id && candidate.profile_id == profile.id)
            .expect("asset/profile observation");
        assert_eq!(observation.state, PhysicalMountStateDto::Mounted);
        assert!(!observation.observed_at.is_empty());
        let mounts = load_test_mounts_async(&database, Some(&asset.id)).await;
        let synced_mount = mounts
            .iter()
            .find(|mount| mount.profile_id == profile.id)
            .expect("synced mount");
        assert!(synced_mount.enabled);
        assert_eq!(synced_mount.created_at, original_mount.created_at);
        assert!(
            is_test_managed_deployment_async(
                &database,
                &profile.id,
                &asset.id,
                &target_path.to_string_lossy()
            )
            .await
        );

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn sync_asset_mount_observations_repairs_ghost_alias_symlink() {
        let db_path = unique_temp_path("assetiweave-observation-ghost-db");
        let real_root = unique_temp_path("assetiweave-observation-ghost-real");
        let alias_root = unique_temp_path("assetiweave-observation-ghost-alias");
        let target_root = unique_temp_path("assetiweave-observation-ghost-target");
        let real_asset_path = real_root.join("skill-a");
        let alias_asset_path = alias_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&real_asset_path).expect("create real asset dir");
        std::fs::create_dir_all(&alias_root).expect("create alias root");
        std::fs::create_dir_all(&target_root).expect("create target dir");
        std::os::unix::fs::symlink(&real_asset_path, &alias_asset_path)
            .expect("create alias asset symlink");
        std::os::unix::fs::symlink(&alias_asset_path, &target_path)
            .expect("create ghost target symlink");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-ghost-asset", alias_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", alias_asset_path);
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
        upsert_test_profile_async(&database, &profile).await;

        sync_asset_mount_observations(database.pool(), "default", None)
            .await
            .expect("sync observations");

        assert_eq!(
            std::fs::read_link(&target_path).expect("read repaired target symlink"),
            real_asset_path
                .canonicalize()
                .expect("canonical real asset")
        );
        let observations = load_test_mount_observations_async(&database).await;
        let observation = observations
            .iter()
            .find(|candidate| candidate.asset_id == asset.id && candidate.profile_id == profile.id)
            .expect("asset/profile observation");
        assert_eq!(observation.state, PhysicalMountStateDto::Mounted);
        let expected_source = real_asset_path
            .canonicalize()
            .expect("canonical real asset")
            .to_string_lossy()
            .to_string();
        assert_eq!(
            observation.linked_source.as_deref(),
            Some(expected_source.as_str())
        );

        std::fs::remove_dir_all(real_root).ok();
        std::fs::remove_dir_all(alias_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn sync_asset_mount_observations_clears_snapshot_when_link_is_missing() {
        let db_path = unique_temp_path("assetiweave-observation-missing-db");
        let source_root = unique_temp_path("assetiweave-observation-missing-source");
        let target_root = unique_temp_path("assetiweave-observation-missing-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-missing-observed-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
        upsert_test_profile_async(&database, &profile).await;
        set_test_asset_mount_async(
            &database,
            &asset.id,
            &profile.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        )
        .await;

        sync_asset_mount_observations(database.pool(), "default", None)
            .await
            .expect("sync observations");

        assert!(load_test_mounts_async(&database, Some(&asset.id))
            .await
            .iter()
            .all(|mount| !mount.enabled));
        assert!(
            !is_test_managed_deployment_async(
                &database,
                &profile.id,
                &asset.id,
                &target_path.to_string_lossy()
            )
            .await
        );

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn unmount_asset_mount_removes_matching_symlink_and_disables_mount() {
        let db_path = unique_temp_path("assetiweave-unmount-db");
        let source_root = unique_temp_path("assetiweave-unmount-source");
        let target_root = unique_temp_path("assetiweave-unmount-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");
        std::os::unix::fs::symlink(&asset_path, &target_path).expect("create mounted symlink");

        let database = open_test_database_async(&db_path).await;
        let source = test_source("source-with-mounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source_async(&database, &source).await;
        replace_test_source_assets_async(&database, &source.id, std::slice::from_ref(&asset)).await;
        upsert_test_profile_async(&database, &profile).await;
        set_test_asset_mount_async(
            &database,
            &asset.id,
            &profile.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        )
        .await;

        let result = unmount_asset_mount_record(database.pool(), "default", &asset.id, &profile.id)
            .await
            .expect("unmount");

        assert!(!target_path.exists());
        assert!(!std::fs::symlink_metadata(&target_path).is_ok());
        assert!(!result.mount.enabled);
        assert_eq!(result.status.state, PhysicalMountStateDto::NotMounted);
        assert!(load_test_mounts_async(&database, Some(&asset.id))
            .await
            .iter()
            .all(|mount| !mount.enabled));

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn catalog_assets_fold_backed_up_copy_to_original_source() {
        let original_source = test_source("source-a", PathBuf::from("/tmp/source-a"));
        let backup_source =
            assetiweave_library_source_with_root("/tmp/assetiweave-backup".to_string());
        let mut original = test_asset(
            &original_source,
            "skill-a",
            PathBuf::from("/tmp/source-a/skill-a"),
        );
        original.content_hash = Some("same-content".to_string());
        let mut backup = test_asset(
            &backup_source,
            "backup-skill-a",
            PathBuf::from("/tmp/assetiweave-backup/backed-up/source-a/skill-a"),
        );
        backup.name = "skill-a".to_string();
        backup.relative_path = "backed-up/source-a/skill-a".to_string();
        backup.content_hash = Some("same-content".to_string());

        let catalog = build_catalog_assets(
            vec![backup.clone(), original.clone()],
            &[backup_source, original_source],
        );

        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].asset.id, original.id);
        let status = catalog[0].backup_status.as_ref().expect("backup status");
        assert_eq!(status.state, SkillBackupState::BackedUp);
        assert_eq!(
            status.backup_path.as_deref(),
            Some(backup.absolute_path.as_str())
        );
        assert_eq!(status.hidden_asset_ids, vec![backup.id]);
    }

    #[test]
    fn catalog_assets_use_backup_copy_for_app_target_duplicate() {
        let app_source = test_source_with_origin(
            "codex-skills",
            PathBuf::from("/tmp/codex"),
            SourceOrigin::AppTarget,
        );
        let backup_source =
            assetiweave_library_source_with_root("/tmp/assetiweave-backup".to_string());
        let mut app_asset = test_asset(&app_source, "skill-a", PathBuf::from("/tmp/codex/skill-a"));
        app_asset.content_hash = Some("same-content".to_string());
        let mut backup = test_asset(
            &backup_source,
            "backup-skill-a",
            PathBuf::from("/tmp/assetiweave-backup/backed-up/codex/skill-a"),
        );
        backup.name = "skill-a".to_string();
        backup.relative_path = "backed-up/codex/skill-a".to_string();
        backup.content_hash = Some("same-content".to_string());

        let catalog = build_catalog_assets(
            vec![app_asset.clone(), backup.clone()],
            &[app_source, backup_source],
        );

        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].asset.id, backup.id);
        assert_eq!(
            catalog[0].backup_status.as_ref().map(|status| status.state),
            Some(SkillBackupState::BackedUp)
        );
        assert_eq!(
            catalog[0]
                .backup_status
                .as_ref()
                .map(|status| status.hidden_asset_ids.clone()),
            Some(vec![app_asset.id])
        );
    }

    #[test]
    fn catalog_assets_keep_downloaded_unique_skill() {
        let backup_source =
            assetiweave_library_source_with_root("/tmp/assetiweave-backup".to_string());
        let mut downloaded = test_asset(
            &backup_source,
            "downloaded-skill",
            PathBuf::from("/tmp/assetiweave-backup/downloaded/downloaded-skill"),
        );
        downloaded.relative_path = "downloaded/downloaded-skill".to_string();
        downloaded.content_hash = Some("downloaded-content".to_string());

        let catalog = build_catalog_assets(vec![downloaded.clone()], &[backup_source]);

        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].asset.id, downloaded.id);
        assert_eq!(
            catalog[0].backup_status.as_ref().map(|status| status.state),
            Some(SkillBackupState::Downloaded)
        );
    }

    #[test]
    fn catalog_assets_do_not_fold_skills_without_hash() {
        let original_source = test_source("source-a", PathBuf::from("/tmp/source-a"));
        let backup_source =
            assetiweave_library_source_with_root("/tmp/assetiweave-backup".to_string());
        let original = test_asset(
            &original_source,
            "skill-a",
            PathBuf::from("/tmp/source-a/skill-a"),
        );
        let mut backup = test_asset(
            &backup_source,
            "backup-skill-a",
            PathBuf::from("/tmp/assetiweave-backup/backed-up/source-a/skill-a"),
        );
        backup.name = "skill-a".to_string();
        backup.relative_path = "backed-up/source-a/skill-a".to_string();

        let catalog =
            build_catalog_assets(vec![backup, original], &[backup_source, original_source]);

        assert_eq!(catalog.len(), 2);
    }

    #[test]
    fn catalog_assets_attach_each_nested_repository_remote() {
        let collection_root = unique_temp_path("assetiweave-catalog-nested-repositories");
        let first_repo = collection_root.join("first-repo");
        let second_repo = collection_root.join("second-repo");
        let first_skill = first_repo.join("skills").join("first-skill");
        let second_skill = second_repo.join("skills").join("second-skill");
        std::fs::create_dir_all(&first_skill).expect("create first skill");
        std::fs::create_dir_all(&second_skill).expect("create second skill");
        init_git_repo(&first_repo, "https://example.com/first.git");
        init_git_repo(&second_repo, "git@example.com:second.git");

        let source = test_source("repository-collection", collection_root.clone());
        let first_asset = test_asset(&source, "first-skill", first_skill);
        let second_asset = test_asset(&source, "second-skill", second_skill);
        let catalog = build_catalog_assets(
            vec![first_asset.clone(), second_asset.clone()],
            std::slice::from_ref(&source),
        );

        let first_repository = catalog
            .iter()
            .find(|candidate| candidate.asset.id == first_asset.id)
            .and_then(|candidate| candidate.repository.as_ref())
            .expect("first repository");
        let second_repository = catalog
            .iter()
            .find(|candidate| candidate.asset.id == second_asset.id)
            .and_then(|candidate| candidate.repository.as_ref())
            .expect("second repository");
        assert_eq!(
            first_repository.remote_url.as_deref(),
            Some("https://example.com/first.git")
        );
        assert_eq!(
            second_repository.remote_url.as_deref(),
            Some("git@example.com:second.git")
        );
        assert_eq!(PathBuf::from(&first_repository.root_path), first_repo);
        assert_eq!(PathBuf::from(&second_repository.root_path), second_repo);

        std::fs::remove_dir_all(collection_root).ok();
    }

    #[test]
    fn catalog_assets_attach_repository_browser_url_to_asset_directory() {
        let repo = unique_temp_path("assetiweave-catalog-repository-browser-url");
        let skill = repo.join("skills").join("zh-cn").join("office-utils");
        std::fs::create_dir_all(&skill).expect("create skill");
        init_git_repo(&repo, "https://github.com/util6/util6-agents.git");

        let source = test_source("repository-root", repo.clone());
        let asset = test_asset(&source, "office-utils", skill);
        let catalog = build_catalog_assets(vec![asset.clone()], std::slice::from_ref(&source));
        let repository = catalog[0].repository.as_ref().expect("repository");

        assert_eq!(
            repository.web_url.as_deref(),
            Some("https://github.com/util6/util6-agents/tree/main/skills/zh-cn/office-utils")
        );

        std::fs::remove_dir_all(repo).ok();
    }

    #[test]
    fn catalog_assets_convert_github_ssh_remote_to_browser_url() {
        let collection_root = unique_temp_path("assetiweave-catalog-ssh-browser-url");
        let repo = collection_root.join("kicad-happy");
        let skill = repo.join("skills").join("pcbway");
        std::fs::create_dir_all(&skill).expect("create skill");
        init_git_repo(&repo, "git@github.com:aklofas/kicad-happy.git");

        let source = test_source("repository-collection", collection_root.clone());
        let asset = test_asset(&source, "pcbway", skill);
        let catalog = build_catalog_assets(vec![asset.clone()], std::slice::from_ref(&source));
        let repository = catalog[0].repository.as_ref().expect("repository");

        assert_eq!(
            repository.web_url.as_deref(),
            Some("https://github.com/aklofas/kicad-happy/tree/main/skills/pcbway")
        );

        std::fs::remove_dir_all(collection_root).ok();
    }

    fn test_missing_source(id: &str) -> Source {
        let root_path = unique_temp_path(id);
        test_source(id, root_path)
    }

    fn test_source(id: &str, root_path: PathBuf) -> Source {
        test_source_with_origin(id, root_path, SourceOrigin::GitRepo)
    }

    fn test_source_with_origin(
        id: &str,
        root_path: PathBuf,
        source_origin: SourceOrigin,
    ) -> Source {
        Source {
            id: id.to_string(),
            name: id.to_string(),
            kind: SourceKind::Local,
            root_path: root_path.to_string_lossy().to_string(),
            scanner_kind: SourceScannerKind::Skill,
            source_origin,
            repo_root: None,
            scan_root: String::new(),
            origin_app_kind: None,
            origin_provider_id: None,
            include_globs: vec!["**/SKILL.md".to_string()],
            exclude_globs: vec![],
            default_kind: Some(AssetKind::Skill),
            enabled: true,
            priority: 0,
            last_scanned_at: None,
            last_scan_status: None,
        }
    }

    fn test_profile(id: &str, target_root: PathBuf) -> TargetProfile {
        TargetProfile {
            id: id.to_string(),
            name: id.to_string(),
            app_kind: Some(AppKind::Custom),
            target_provider_id: "custom".to_string(),
            target_paths: vec![target_root.to_string_lossy().to_string()],
            supported_kinds: vec![AssetKind::Skill],
            deployment_strategy: DeploymentStrategy::SymlinkToSource,
            enabled: true,
            include: RuleSet {
                kinds: vec![AssetKind::Skill],
                tags: vec![],
                groups: vec![],
                sources: vec![],
                path_patterns: vec![],
            },
            exclude: RuleSet {
                kinds: vec![],
                tags: vec![],
                groups: vec![],
                sources: vec![],
                path_patterns: vec![],
            },
            safety: ProfileSafety {
                allow_remove: false,
                allow_overwrite: false,
            },
        }
    }

    fn test_asset(source: &Source, id: &str, absolute_path: PathBuf) -> Asset {
        test_asset_with_kind(source, id, absolute_path, AssetKind::Skill)
    }

    fn test_asset_with_kind(
        source: &Source,
        id: &str,
        absolute_path: PathBuf,
        kind: AssetKind,
    ) -> Asset {
        Asset {
            id: id.to_string(),
            source_id: source.id.clone(),
            name: id.to_string(),
            kind,
            detector_id: "legacy.classifier".to_string(),
            detector_version: 1,
            format: AssetFormat::Directory,
            relative_path: id.to_string(),
            absolute_path: absolute_path.to_string_lossy().to_string(),
            entry_file: None,
            description: None,
            content_hash: None,
            discovered_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn test_group(id: &str) -> AssetGroup {
        AssetGroup {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            color: "#10b981".to_string(),
            asset_kind: AssetKind::Skill,
            display_icon: None,
            icon_svg: None,
            enabled: true,
            sort_order: 0,
            rules: AssetGroupRules {
                source_ids: vec![],
                relative_path_globs: vec![],
                name_contains: None,
            },
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn unique_temp_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
    }

    fn init_git_repo(path: &Path, remote_url: &str) {
        std::fs::create_dir_all(path).expect("create repository directory");
        let init = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(path)
            .status()
            .expect("run git init");
        assert!(init.success());
        let remote = Command::new("git")
            .args(["remote", "add", "origin", remote_url])
            .current_dir(path)
            .status()
            .expect("add git remote");
        assert!(remote.success());
        let branch = Command::new("git")
            .args(["checkout", "-b", "main", "--quiet"])
            .current_dir(path)
            .status()
            .expect("create main branch");
        assert!(branch.success());
    }
}
