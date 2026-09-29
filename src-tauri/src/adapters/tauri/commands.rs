//! Tauri Command 暴露层与 IPC 调用适配模块
//!
//! 该模块包含了所有前端通过 `invoke('plugin:assetiweave|...')` 调用的 Tauri Command 函数实现。
//! 包含应用配置、数据源管理、资产挂载、会话同步与翻译、Memory 以及 CLI 安装等 IPC 交互逻辑。

pub(crate) mod agents;
pub(crate) mod catalog;
pub(crate) mod conversations;
#[macro_use]
pub(crate) mod memory;
#[macro_use]
pub(crate) mod mounting;
#[macro_use]
pub(crate) mod system;

pub(crate) const BASELINE_COMMAND_COUNT: usize = 191;

pub(crate) use self::memory::*;
pub(crate) use self::mounting::*;
pub(crate) use self::system::*;

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
use crate::backend::application::{
    catalog::{
        catalog_ops::{assetiweave_library_source_with_root, build_catalog_assets},
        source_scanner::{refresh_recorded_assets, scan_selected_sources},
    },
    mounting::{
        groups::{
            apply_skill_group_exclusive_mount_record, apply_skill_group_mount_record,
            build_skill_group_exclusive_mount_preview_sqlx, exclusive_item,
        },
        mount_ops::{
            mount_asset_mount_record, scan_asset_mount_statuses_sqlx, set_asset_mount_record,
            sync_asset_mount_observations, unmount_asset_mount_record,
        },
        profile_ops::{ensure_profile_can_be_deleted_sqlx, target_profile_from_input},
    },
};
use crate::{
    backend::application::agents::{
        AgentCatalogEntry, AgentConnectionCheckRequest, AgentConnectionResult, AgentModelsRequest,
        AgentModelsResult,
    },
    backend::application::conversations::card_translation::{
        ConversationTranslationConnectionRequest, ConversationTranslationModelsRequest,
        ConversationTranslationModelsResult, ConversationTranslationRequest,
        OpencodeTranslationAvailability, OpencodeTranslationRequest, OpencodeTranslationResult,
        PromptOptimizationRequest, PromptOptimizationResult,
    },
    backend::application::{
        conversations::card_translation::PreparedConversationCardTranslation, AppService,
        BackgroundTaskGetParams, ConversationAdapterCatalogRefreshParams,
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
        ConversationSessionListParams, ConversationSessionOutlineParams,
        ConversationSourceDisableParams, ConversationSourceUpsertParams, ConversationSyncParams,
        ListAssetsParams, MemoryContextResolveParams, MemoryProjectGetParams,
        MemoryRecallSearchParams, MemoryRecallSessionCreateParams, MemoryRecallSessionGetParams,
        MemoryRecallTurnCancelParams, MemoryRecallTurnSendParams, MemoryScopeRebuildParams,
        MemoryTaskGetParams, MemoryTaskListParams, MemoryTaskRetryParams, SkillAcquireParams,
        SkillRemoteCheckParams, SkillSearchParams, SkillSearchResult, SourceRemoveParams,
        SourceScanParams, TenantCreateParams, UpdateSkillBackupSettingsParams,
    },
    backend::application::{
        memory::{MemoryContextResult, MemoryProjectView, MemoryRebuildResult, MemoryTaskView},
        mounting::{
            AssetGroupInput, ExecutionResult, SkillGroupExclusiveMountInput, SourceInput,
            TargetProfileInput,
        },
        system::NavigationModel,
    },
    backend::domain::{
        AppErrorView, AppOverview, AppShortcut, Asset, AssetGroup, AssetGroupDetail, AssetKind,
        AssetMount, AssetMountStatus, AssetMountUpdateResult, CatalogAsset, ConversationAdapter,
        ConversationSearchIndexStatus, ConversationSource, DeploymentPlan, DeploymentStrategy,
        PhysicalMountStateDto, SkillBackupSettings, SkillGroupExclusiveMountPreview,
        SkillRemoteSource, Source, TargetProfile, TargetProfileDescriptor, Tenant,
    },
    backend::infrastructure::agent_execution::{
        AiExecutionCancellation, AiExecutionCleanupReport, AiExecutionError, AiExecutionPhase,
        AiExecutionProgressSink, AiExecutionPurpose,
    },
    backend::infrastructure::conversations::{
        ConversationCommandProjection, ConversationCommandProjectionParams,
        ExternalAdapterRegisterParams, ExternalAdapterScaffoldParams, ExternalAdapterTryRunParams,
        ExternalAdapterValidateParams,
    },
    backend::{application::AppError, infrastructure::tasks::TaskContext},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, State};

type RuntimeAppResult<T> = crate::backend::application::AppResult<T>;

pub(crate) const AI_EXECUTION_TASK_UPDATED_EVENT: &str = "ai-execution://task-updated";


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
    let _root_path_input = root_path.clone();
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
        result.map_err(AppErrorView::from)
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
            service
                .scan_sources_with_task_context(params, &task_context, skill_sources_only)
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
) -> RuntimeAppResult<crate::backend::infrastructure::conversations::ExternalAdapterScaffoldResult>
{
    AppService::from_runtime(&state.runtime).scaffold_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn validate_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterValidateParams,
) -> RuntimeAppResult<crate::backend::infrastructure::conversations::ExternalAdapterValidationResult>
{
    AppService::from_runtime(&state.runtime).validate_conversation_adapter(params)
}

#[tauri::command]
pub(crate) async fn list_conversation_adapter_runtime_statuses(
    state: State<'_, AppState>,
) -> RuntimeAppResult<
    Vec<crate::backend::infrastructure::conversations::ConversationAdapterRuntimeStatus>,
> {
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
pub(crate) async fn cancel_agent_model_probe(
    state: State<'_, AppState>,
    agent_id: String,
) -> RuntimeAppResult<()> {
    AppService::from_runtime(&state.runtime)
        .cancel_agent_model_probe(agent_id)
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
) -> RuntimeAppResult<
    crate::backend::application::conversations::card_translation::ActionAvailability,
> {
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

fn prepare_ai_execution_task_for_tenant(
    tenant_id: &str,
    tasks: Arc<BackgroundTaskRegistry>,
    prepared: PreparedConversationCardTranslation,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
) -> RuntimeAppResult<(
    AiExecutionTaskSnapshot,
    PreparedConversationCardTranslation,
    AiExecutionCancellation,
    Arc<dyn AiExecutionProgressSink>,
)> {
    let (snapshot, cancellation) = tasks.begin_ai_execution_for_tenant(
        tenant_id,
        AiExecutionPurpose::Translation,
        &prepared.0,
    )?;
    let progress: Arc<dyn AiExecutionProgressSink> = Arc::new(RegistryAiExecutionProgressSink {
        tasks,
        task_id: snapshot.id.clone(),
        emitter: emitter.clone(),
        last_execution_phase: Mutex::new(None),
    });
    emitter.emit(&snapshot);
    Ok((snapshot, prepared, cancellation, progress))
}

async fn run_ai_execution_task(
    tasks: Arc<BackgroundTaskRegistry>,
    service: AppService,
    task_id: String,
    prepared: PreparedConversationCardTranslation,
    cancellation: AiExecutionCancellation,
    progress: Arc<dyn AiExecutionProgressSink>,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
) {
    let execution_id = task_id.clone();
    let execution_progress = progress.clone();
    let execution = tokio::spawn(async move {
        service
            .execute_prepared_conversation_card_translation(
                prepared,
                execution_id,
                cancellation,
                Some(execution_progress),
            )
            .await
    });
    let result = match execution.await {
        Ok(result) => result,
        Err(_) => Err(AiExecutionError::Protocol {
            operation: "execution_task_panicked",
        }),
    };
    let failure_phase = progress.failure_phase();
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
    let prepared = AppService::prepare_conversation_card_translation(params)?;
    let (snapshot, prepared, cancellation, progress) =
        prepare_ai_execution_task_for_tenant(&tenant_id, tasks.clone(), prepared, emitter.clone())?;
    let service = AppService::from_runtime(&state.runtime);
    let task_id = snapshot.id.clone();
    tauri::async_runtime::spawn(run_ai_execution_task(
        tasks,
        service,
        task_id,
        prepared,
        cancellation,
        progress,
        emitter,
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
) -> RuntimeAppResult<crate::backend::infrastructure::conversations::ExternalAdapterRunResult> {
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
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationAdapterCatalogRelease>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_adapter_package_releases(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_installed_conversation_adapter_package_versions(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationAdapterPackageVersion>> {
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
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationAdapterCatalogRelease>> {
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
) -> RuntimeAppResult<crate::backend::domain::ConversationAdapterPackage> {
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
    runtime: std::sync::Arc<crate::backend::infrastructure::runtime::AppRuntime>,
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
                          current_source_name: Option<String>,
                          completed_adapter_ids: &[String]| {
                        match progress_tasks.update_conversation_sync_progress(
                            &progress_task_id,
                            completed_source_count,
                            total_source_count,
                            current_source_name,
                            completed_adapter_ids.to_vec(),
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
                        .sync_conversations_with_control(
                            params,
                            Some(&cancellation),
                            Some(&task_id_for_runtime),
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
                Err(error) => Err(error.into()),
            }
        }),
    );
    if let Err(error) = outcome {
        return Err(error.into());
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
pub(crate) async fn get_conversation_usage_dashboard(
    state: State<'_, AppState>,
    filter: crate::backend::application::conversations::UsageDashboardFilter,
) -> RuntimeAppResult<crate::backend::application::conversations::UsageDashboardDto> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_usage_dashboard(filter)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_usage_scan_status(
    state: State<'_, AppState>,
) -> RuntimeAppResult<crate::backend::application::conversations::UsageScanStatusDto> {
    let mut status = AppService::from_runtime(&state.runtime)
        .get_conversation_usage_scan_status()
        .await?;
    let context = state.runtime.context();
    let tenant_id = context.tenant.id.as_str();
    if let Some(active_id) = state
        .background_tasks
        .active_conversation_usage_scan_id(tenant_id)
    {
        status.active_scan_task_id = Some(active_id);
    }
    Ok(status)
}

#[tauri::command]
pub(crate) fn scan_conversation_usage(
    app: AppHandle,
    state: State<'_, AppState>,
    options: crate::backend::application::conversations::UsageScanOptions,
) -> RuntimeAppResult<crate::adapters::tauri::background_tasks::ConversationUsageScanTaskSnapshot> {
    start_conversation_usage_scan_background(
        app,
        state.runtime.clone(),
        state.background_tasks.clone(),
        options,
    )
}

pub(crate) fn start_conversation_usage_scan_background(
    app: AppHandle,
    runtime: std::sync::Arc<crate::backend::infrastructure::runtime::AppRuntime>,
    background_tasks: std::sync::Arc<
        crate::adapters::tauri::background_tasks::BackgroundTaskRegistry,
    >,
    options: crate::backend::application::conversations::UsageScanOptions,
) -> RuntimeAppResult<crate::adapters::tauri::background_tasks::ConversationUsageScanTaskSnapshot> {
    let tenant_id = runtime.context().tenant.id.clone();
    let mode = options
        .mode
        .clone()
        .unwrap_or_else(|| "incremental".to_string());
    let (snapshot, should_start) = background_tasks.begin_conversation_usage_scan_for_tenant(
        &tenant_id,
        options.source_id.clone(),
        &mode,
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
    let outcome =
        task_runtime.start_external_with_async(&task_id, task_detail, move |context| async move {
            let progress_app = task_app.clone();
            let progress_tasks = task_background_tasks.clone();
            let progress_task_id = task_id_for_runtime.clone();
            let mut on_progress = move |completed_source_count: usize,
                                        total_source_count: usize,
                                        current_source_name: Option<String>,
                                        total_events: usize| {
                match progress_tasks.update_conversation_usage_scan_progress(
                    &progress_task_id,
                    completed_source_count,
                    total_source_count,
                    current_source_name,
                    total_events,
                ) {
                    Ok(snapshot) => {
                        if let Err(error) =
                            progress_app.emit("conversation-usage-scan-task-updated", &snapshot)
                        {
                            tracing::error!(
                                action = "conversation.usage.scan",
                                task_id = %progress_task_id,
                                error = %error,
                                "推送后台用量扫描进度失败"
                            );
                        }
                    }
                    Err(error) => tracing::error!(
                        action = "conversation.usage.scan",
                        task_id = %progress_task_id,
                        error = %error,
                        "更新后台用量扫描进度失败"
                    ),
                }
            };
            let cancellation = context.cancellation();
            if context.is_cancelled() {
                return Err(AppError::Cancelled(
                    "conversation usage scan cancelled".to_string(),
                ));
            }
            let result = AppService::from_runtime(&runtime)
                .scan_conversation_usage_with_control(
                    options,
                    Some(&cancellation),
                    Some(&task_id_for_runtime),
                    &mut on_progress,
                )
                .await;

            match &result {
                Ok(value) => tracing::info!(
                    action = "conversation.usage.scan",
                    task_id = %task_id_for_runtime,
                    result = %value,
                    "后台扫描对话用量成功"
                ),
                Err(error) => tracing::error!(
                    action = "conversation.usage.scan",
                    task_id = %task_id_for_runtime,
                    error = %error,
                    "后台扫描对话用量失败"
                ),
            }
            match task_background_tasks.finish_conversation_usage_scan(&task_id_for_runtime, result)
            {
                Ok(snapshot) => {
                    if let Err(error) =
                        task_app.emit("conversation-usage-scan-task-updated", &snapshot)
                    {
                        tracing::error!(
                            action = "conversation.usage.scan",
                            task_id = %task_id_for_runtime,
                            error = %error,
                            "推送后台用量扫描任务状态失败"
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
    runtime: std::sync::Arc<crate::backend::infrastructure::runtime::AppRuntime>,
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
                Err(error) => Err(error.into()),
            }
        }),
    );
    if let Err(error) = outcome {
        return Err(error.into());
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
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationSessionListItem>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_sessions(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationSessionDetail> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_session(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_session_outline(
    state: State<'_, AppState>,
    params: ConversationSessionOutlineParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationSessionOutline> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_session_outline(params)
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
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationSessionListItem>> {
    AppService::from_runtime(&state.runtime)
        .list_web_record_sessions(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_web_record_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationSessionDetail> {
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
            result.map_err(AppErrorView::from)
        }),
    );
    if let Err(error) = outcome {
        let projection_error = AppError::from(error.view());
        let _ = background_tasks
            .finish_conversation_search_index_rebuild(&task_id, Err(projection_error));
        return Err(error.into());
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
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationQuestionDetail>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_questions(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionGetParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationQuestionDetail> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_question(params)
        .await
}

#[tauri::command]
pub(crate) async fn list_conversation_blocks(
    state: State<'_, AppState>,
    params: ConversationBlockListParams,
) -> RuntimeAppResult<Vec<crate::backend::domain::ConversationBlockLocator>> {
    AppService::from_runtime(&state.runtime)
        .list_conversation_blocks(params)
        .await
}

#[tauri::command]
pub(crate) async fn get_conversation_block(
    state: State<'_, AppState>,
    params: ConversationBlockGetParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationBlockDetail> {
    AppService::from_runtime(&state.runtime)
        .get_conversation_block(params)
        .await
}

#[tauri::command]
pub(crate) async fn merge_conversation_questions(
    state: State<'_, AppState>,
    params: ConversationQuestionMergeParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationMutationResult> {
    AppService::from_runtime(&state.runtime)
        .merge_conversation_questions(params)
        .await
}

#[tauri::command]
pub(crate) async fn split_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionSplitParams,
) -> RuntimeAppResult<crate::backend::domain::ConversationMutationResult> {
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


// Keep the generated Tauri command shims in this module so the existing
// command handler remains a single, locally resolvable macro surface. The
// implementation stays in the dedicated Agent Market adapter.
#[tauri::command]
pub(crate) async fn list_agent_market(
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentMarketListRequest,
) -> crate::backend::application::AppResult<Vec<crate::backend::application::AgentMarketItemView>> {
    crate::adapters::tauri::agent_market::list_agent_market(state, params).await
}

#[tauri::command]
pub(crate) async fn inspect_agent_market_item(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<crate::backend::application::AgentMarketItemView> {
    crate::adapters::tauri::agent_market::inspect_agent_market_item(state, agent_id).await
}

#[tauri::command]
pub(crate) fn refresh_agent_market(
    app: AppHandle,
    state: State<'_, AppState>,
) -> crate::backend::application::AppResult<
    crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::refresh_agent_market(app, state)
}

#[tauri::command]
pub(crate) fn get_agent_market_refresh_task(
    state: State<'_, AppState>,
    task_id: String,
) -> crate::backend::application::AppResult<
    crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::get_agent_market_refresh_task(state, task_id)
}

#[tauri::command]
pub(crate) fn list_agent_market_refresh_tasks(
    state: State<'_, AppState>,
) -> crate::backend::application::AppResult<
    Vec<crate::adapters::tauri::background_tasks::AgentMarketRefreshTaskSnapshot>,
> {
    crate::adapters::tauri::agent_market::list_agent_market_refresh_tasks(state)
}

#[tauri::command]
pub(crate) async fn preview_agent_installation(
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentInstallPreviewRequest,
) -> crate::backend::application::AppResult<crate::backend::application::AgentInstallPreview> {
    crate::adapters::tauri::agent_market::preview_agent_installation(state, params).await
}

#[tauri::command]
pub(crate) async fn preview_agent_uninstall(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<crate::backend::application::AgentUninstallPreview> {
    crate::adapters::tauri::agent_market::preview_agent_uninstall(state, agent_id).await
}

#[tauri::command]
pub(crate) async fn list_installed_agents(
    state: State<'_, AppState>,
) -> crate::backend::application::AppResult<
    Vec<crate::backend::infrastructure::agent_market::AgentInstallationView>,
> {
    crate::adapters::tauri::agent_market::list_installed_agents(state).await
}

#[tauri::command]
pub(crate) async fn get_installed_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentInstallationView,
> {
    crate::adapters::tauri::agent_market::get_installed_agent(state, agent_id).await
}

#[tauri::command]
pub(crate) async fn check_agent_runtime(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentInstallationView,
> {
    crate::adapters::tauri::agent_market::check_agent_runtime(state, agent_id).await
}

#[tauri::command]
pub(crate) fn get_agent_lifecycle_task(
    state: State<'_, AppState>,
    task_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::get_agent_lifecycle_task(state, task_id)
}

#[tauri::command]
pub(crate) fn list_agent_lifecycle_tasks(
    state: State<'_, AppState>,
) -> crate::backend::application::AppResult<
    Vec<crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot>,
> {
    crate::adapters::tauri::agent_market::list_agent_lifecycle_tasks(state)
}

#[tauri::command]
pub(crate) fn cancel_agent_lifecycle_task(
    app: AppHandle,
    state: State<'_, AppState>,
    task_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::cancel_agent_lifecycle_task(app, state, task_id)
}

#[tauri::command]
pub(crate) fn start_agent_installation(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentInstallStartRequest,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_installation(app, state, params)
}

#[tauri::command]
pub(crate) fn start_agent_update(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentInstallStartRequest,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_update(app, state, params)
}

#[tauri::command]
pub(crate) fn start_agent_reinstallation(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentInstallStartRequest,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_reinstallation(app, state, params)
}

#[tauri::command]
pub(crate) fn start_agent_uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    params: crate::backend::infrastructure::agent_market::AgentUninstallStartRequest,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentLifecycleTaskSnapshot,
> {
    crate::adapters::tauri::agent_market::start_agent_uninstall(app, state, params)
}

#[tauri::command]
pub(crate) async fn enable_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentInstallationView,
> {
    crate::adapters::tauri::agent_market::enable_agent(state, agent_id).await
}

#[tauri::command]
pub(crate) async fn disable_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> crate::backend::application::AppResult<
    crate::backend::infrastructure::agent_market::AgentInstallationView,
> {
    crate::adapters::tauri::agent_market::disable_agent(state, agent_id).await
}


#[tauri::command]
pub(crate) fn agent_session_get(
    state: State<'_, AppState>,
    params: crate::backend::application::agents::AgentSessionGetParams,
) -> RuntimeAppResult<crate::backend::application::agents::AgentSessionGetResult> {
    let service = AppService::from_runtime(&state.runtime);
    service.get_agent_session(params)
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
        get_memory_recent_snapshot,
        duplicate_memory_generation_skill,
        reset_memory_generation_skill_to_default,
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
        cancel_agent_model_probe,
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
        get_conversation_usage_dashboard,
        get_conversation_usage_scan_status,
        scan_conversation_usage,
        audit_conversation_data,
        repair_conversation_data,
        get_conversation_data_maintenance_task,
        list_conversation_data_maintenance_tasks,
        cancel_conversation_data_maintenance,
        rollback_conversation_data,
        list_conversation_sessions,
        get_conversation_session,
        get_conversation_session_outline,
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
        list_public_tasks,
        get_public_task,
        cancel_public_task,
        retry_public_task,
        clear_terminal_tasks,
        agent_session_get
    ]
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "commands/baseline_tests.rs"]
mod baseline_tests;
