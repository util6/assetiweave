//! Tauri Command 暴露层与 IPC 调用适配模块
//!
//! 该模块包含了所有前端通过 `invoke('plugin:assetiweave|...')` 调用的 Tauri Command 函数实现。
//! 包含应用配置、数据源管理、资产挂载、会话同步与翻译、内存梦境 (Memory Dream) 以及 CLI 安装等 IPC 交互逻辑。

use crate::adapters::app_state::AppState;
use crate::adapters::prompt_clipboard::{
    copy_prompt_card_to_clipboard as copy_prompt_card_to_clipboard_impl, PromptClipboardParams,
};
use crate::adapters::tauri::app_icon::set_application_icon;
use crate::adapters::tauri::background_tasks::{
    AiExecutionTaskGetParams, AiExecutionTaskSnapshot, BackgroundTaskRegistry,
    BackgroundTaskStatus, ConversationScriptInstallTaskSnapshot,
    ConversationSearchIndexTaskSnapshot, ConversationSyncTaskSnapshot, MemoryTaskSnapshot,
    SkillBackupTaskSnapshot,
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
        AgentExecutionRuntime, AiExecutionError, AiExecutionLimits, AiExecutionPhase,
        AiExecutionProgressSink, AiExecutionPurpose, AiExecutionRequest,
    },
    backend::application::{
        AppService, ConversationAdapterCatalogRefreshParams,
        ConversationAdapterLocalRegisterParams, ConversationAdapterPackageCatalogParams,
        ConversationAdapterPackageChangeParams, ConversationAdapterPackageInspectParams,
        ConversationAdapterPackageInstallParams, ConversationAdapterPackageReleaseListParams,
        ConversationAdapterPackageUninstallParams, ConversationAdapterPackageUpdateCheckParams,
        ConversationAdapterPackageUpdatePolicyParams,
        ConversationAdapterPackageVersionChangeParams, ConversationAdapterUnregisterParams,
        ConversationBlockGetParams, ConversationBlockListParams,
        ConversationPartTranslationUpdateParams, ConversationQuestionGetParams,
        ConversationQuestionListParams, ConversationQuestionMergeParams,
        ConversationQuestionSplitParams, ConversationScriptCatalogParams,
        ConversationScriptInstallParams, ConversationSearchParams, ConversationSearchResult,
        ConversationSessionExportParams, ConversationSessionGetParams,
        ConversationSessionListParams, ConversationSourceDisableParams,
        ConversationSourceUpsertParams, ConversationSyncParams, ListAssetsParams,
        MemoryCandidateAcceptParams, MemoryDreamGetParams, MemoryDreamListParams,
        MemoryDreamPreviewParams, MemoryDreamRunParams, MemoryDreamScopeParams,
        MemoryItemCreateParams, MemoryItemGetParams, MemoryItemListParams, MemoryItemUpdateParams,
        MemoryRecallPreviewParams, MemoryRecallRunParams, MemoryTaskGetParams,
        MemoryTaskStartParams, MemoryVerifyParams, SkillAcquireParams, SkillRemoteCheckParams,
        SkillSearchParams, SkillSearchResult, SourceRemoveParams, SourceScanParams,
        TenantCreateParams, UpdateSkillBackupSettingsParams,
    },
    backend::card_translation::{
        prepare_opencode_agent_translation, ConversationTranslationConnectionRequest,
        ConversationTranslationModelsRequest, ConversationTranslationModelsResult,
        ConversationTranslationRequest, OpencodeTranslationAvailability,
        OpencodeTranslationRequest, OpencodeTranslationResult,
    },
    backend::conversations::{
        ExternalAdapterRegisterParams, ExternalAdapterScaffoldParams, ExternalAdapterTryRunParams,
        ExternalAdapterValidateParams,
    },
    backend::dto::{
        AppOverview, AppResult, AppShortcut, ApplyAssetGroupMountResult,
        ApplySkillGroupExclusiveMountResult, AssetGroupInput, AssetMountStatus,
        AssetMountUpdateResult, CatalogAsset, ConversationSearchIndexStatus, ExecutionResult,
        MemoryItemPage, NavigationModel, SkillBackupSettings, SkillGroupExclusiveMountInput,
        SkillGroupExclusiveMountPreview, SkillRemoteSource, SourceInput, TargetProfileInput,
    },
    backend::models::{
        Asset, AssetGroup, AssetGroupDetail, AssetKind, AssetMount, ConversationAdapter,
        ConversationSource, DeploymentPlan, DeploymentStrategy, MemoryItemDetail, MemoryRunKind,
        Source, TargetProfile, Tenant,
    },
    backend::operation_log::{
        asset_log_fields, log_error, log_info, log_warn, profile_log_fields,
        source_input_log_fields, source_log_fields, status_summary_fields,
    },
    backend::runtime::{
        tasks::{SpawnOutcome, TaskContext, TaskKind, TaskSpec},
        AppError,
    },
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use tauri::{AppHandle, Emitter, State};

pub(crate) const AI_EXECUTION_TASK_UPDATED_EVENT: &str = "ai-execution://task-updated";

#[tauri::command]
pub(crate) async fn set_app_window_icon(app: AppHandle, icon: Vec<u8>) -> AppResult<()> {
    set_application_icon(app, icon).map_err(Into::into)
}

#[tauri::command]
pub(crate) fn get_app_overview(state: State<'_, AppState>) -> AppResult<AppOverview> {
    AppService::from_runtime(&state.runtime).overview()
}

#[tauri::command]
pub(crate) fn list_tenants(state: State<'_, AppState>) -> AppResult<Vec<Tenant>> {
    AppService::from_runtime(&state.runtime).list_tenants()
}

#[tauri::command]
pub(crate) fn get_active_tenant(state: State<'_, AppState>) -> AppResult<Tenant> {
    AppService::from_runtime(&state.runtime).active_tenant()
}

#[tauri::command]
pub(crate) fn create_tenant(
    state: State<'_, AppState>,
    params: TenantCreateParams,
) -> AppResult<Tenant> {
    let fields = vec![("name", params.name.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).create_tenant(params))();
    match &result {
        Ok(tenant) => log_info(
            "tenant.create",
            "创建租户成功",
            &[("tenant_id", tenant.id.clone())],
        ),
        Err(error) => log_error("tenant.create", "创建租户失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn switch_tenant(state: State<'_, AppState>, tenant_id: String) -> AppResult<Tenant> {
    let fields = vec![("tenant_id", tenant_id.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).switch_tenant(tenant_id))();
    match &result {
        Ok(tenant) => log_info(
            "tenant.switch",
            "切换租户成功",
            &[("tenant_id", tenant.id.clone())],
        ),
        Err(error) => log_error("tenant.switch", "切换租户失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn get_app_settings(
    state: State<'_, AppState>,
) -> AppResult<crate::backend::app_settings::AppSettingsFile> {
    AppService::from_runtime(&state.runtime).get_app_settings()
}

#[tauri::command]
pub(crate) fn save_app_settings(
    state: State<'_, AppState>,
    settings: serde_json::Value,
) -> AppResult<crate::backend::app_settings::AppSettingsFile> {
    AppService::from_runtime(&state.runtime).save_app_settings(settings)
}

#[tauri::command]
pub(crate) fn cancel_app_close_prompt(state: State<'_, AppState>) -> AppResult<()> {
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
) -> AppResult<()> {
    let shutdown_sync_done = state.shutdown_sync_done.clone();
    let exit_prompt_open = state.exit_prompt_open.clone();
    let allow_close = state.allow_close.clone();
    let allow_exit = state.allow_exit.clone();
    let db_path = state.db_path.clone();
    let background_tasks = state.background_tasks.clone();
    let runtime = state.runtime.clone();

    crate::converge_ai_executions_before_close(background_tasks).await;

    if !shutdown_sync_done.swap(true, std::sync::atomic::Ordering::SeqCst) {
        tauri::async_runtime::spawn_blocking(move || {
            crate::sync_before_close(&db_path, backup_database);
        })
        .await
        .map_err(|error| error.to_string())?;
    }

    let shutdown_report = tauri::async_runtime::spawn_blocking(move || {
        runtime.shutdown_with_grace(std::time::Duration::from_secs(5))
    })
    .await
    .map_err(|error| error.to_string())?;
    if !shutdown_report.dispatcher_drained
        || !shutdown_report.unfinished_task_ids.is_empty()
        || shutdown_report.dispatcher_timed_out
    {
        log_warn(
            "app.close.runtime",
            "应用运行时在关闭期限内未完全收敛",
            &[
                (
                    "unfinished_tasks",
                    shutdown_report.unfinished_task_ids.len().to_string(),
                ),
                (
                    "dispatcher_remaining_events",
                    shutdown_report.dispatcher_remaining_events.to_string(),
                ),
                (
                    "dispatcher_timed_out",
                    shutdown_report.dispatcher_timed_out.to_string(),
                ),
            ],
        );
    }

    exit_prompt_open.store(false, std::sync::atomic::Ordering::SeqCst);
    allow_close.store(true, std::sync::atomic::Ordering::SeqCst);
    allow_exit.store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub(crate) fn list_assets(
    state: State<'_, AppState>,
    kind: Option<AssetKind>,
) -> AppResult<Vec<CatalogAsset>> {
    AppService::from_runtime(&state.runtime).list_assets(ListAssetsParams { kind })
}

#[tauri::command]
pub(crate) fn list_source_assets(
    state: State<'_, AppState>,
    kind: Option<AssetKind>,
) -> AppResult<Vec<CatalogAsset>> {
    AppService::from_runtime(&state.runtime).list_source_assets(kind)
}

#[tauri::command]
pub(crate) fn list_memory_items(
    state: State<'_, AppState>,
    params: MemoryItemListParams,
) -> AppResult<MemoryItemPage> {
    AppService::from_runtime(&state.runtime).list_memory_items(params)
}

#[tauri::command]
pub(crate) fn get_memory_item(
    state: State<'_, AppState>,
    params: MemoryItemGetParams,
) -> AppResult<MemoryItemDetail> {
    AppService::from_runtime(&state.runtime).get_memory_item(params)
}

#[tauri::command]
pub(crate) fn create_memory_item(
    state: State<'_, AppState>,
    params: MemoryItemCreateParams,
) -> AppResult<MemoryItemDetail> {
    let result = (|| AppService::from_runtime(&state.runtime).create_memory_item(params))();
    match &result {
        Ok(detail) => log_info(
            "memory.item.create",
            "创建 Memory 成功",
            &[("item_id", detail.item.id.clone())],
        ),
        Err(error) => log_error("memory.item.create", "创建 Memory 失败", error, &[]),
    }
    result
}

#[tauri::command]
pub(crate) fn update_memory_item(
    state: State<'_, AppState>,
    params: MemoryItemUpdateParams,
) -> AppResult<MemoryItemDetail> {
    let item_id = params.item_id.clone();
    let fields = [("item_id", item_id.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).update_memory_item(params))();
    match &result {
        Ok(_) => log_info("memory.item.update", "更新 Memory 成功", &fields),
        Err(error) => log_error("memory.item.update", "更新 Memory 失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn archive_memory_item(
    state: State<'_, AppState>,
    params: MemoryItemGetParams,
) -> AppResult<MemoryItemDetail> {
    let fields = [("item_id", params.item_id.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).archive_memory_item(params))();
    match &result {
        Ok(_) => log_info("memory.item.archive", "归档 Memory 成功", &fields),
        Err(error) => log_error("memory.item.archive", "归档 Memory 失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn accept_memory_candidate(
    state: State<'_, AppState>,
    params: MemoryCandidateAcceptParams,
) -> AppResult<MemoryItemDetail> {
    let fields = [("item_id", params.item_id.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).accept_memory_candidate(params))();
    match &result {
        Ok(_) => log_info("memory.candidate.accept", "接受 Memory 候选成功", &fields),
        Err(error) => log_error(
            "memory.candidate.accept",
            "接受 Memory 候选失败",
            error,
            &fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn reject_memory_candidate(
    state: State<'_, AppState>,
    params: MemoryItemGetParams,
) -> AppResult<MemoryItemDetail> {
    let fields = [("item_id", params.item_id.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).reject_memory_candidate(params))();
    match &result {
        Ok(_) => log_info("memory.candidate.reject", "拒绝 Memory 候选成功", &fields),
        Err(error) => log_error(
            "memory.candidate.reject",
            "拒绝 Memory 候选失败",
            error,
            &fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn memory_dream_status(
    state: State<'_, AppState>,
    params: MemoryDreamScopeParams,
) -> AppResult<crate::backend::dto::MemoryDreamPreview> {
    AppService::from_runtime(&state.runtime).memory_dream_status(params)
}

#[tauri::command]
pub(crate) fn memory_overview(
    state: State<'_, AppState>,
    params: MemoryDreamScopeParams,
) -> AppResult<crate::backend::dto::MemoryOverview> {
    AppService::from_runtime(&state.runtime).memory_overview(params)
}

#[tauri::command]
pub(crate) fn list_memory_dream_notes(
    state: State<'_, AppState>,
    params: MemoryDreamListParams,
) -> AppResult<crate::backend::dto::MemoryDreamNotePage> {
    AppService::from_runtime(&state.runtime).list_memory_dream_notes(params)
}

#[tauri::command]
pub(crate) fn get_memory_dream_note(
    state: State<'_, AppState>,
    params: MemoryDreamGetParams,
) -> AppResult<crate::backend::models::MemoryDreamNoteDetail> {
    AppService::from_runtime(&state.runtime).get_memory_dream_note(params)
}

#[tauri::command]
pub(crate) fn archive_memory_dream_note(
    state: State<'_, AppState>,
    params: MemoryDreamGetParams,
) -> AppResult<crate::backend::models::MemoryDreamNoteDetail> {
    AppService::from_runtime(&state.runtime).archive_memory_dream_note(params)
}

#[tauri::command]
pub(crate) fn promote_memory_dream_note(
    state: State<'_, AppState>,
    params: MemoryDreamGetParams,
) -> AppResult<Vec<MemoryItemDetail>> {
    AppService::from_runtime(&state.runtime).promote_memory_dream_note(params)
}

#[tauri::command]
pub(crate) fn preview_memory_dream(
    state: State<'_, AppState>,
    params: MemoryDreamPreviewParams,
) -> AppResult<crate::backend::dto::MemoryDreamPreview> {
    AppService::from_runtime(&state.runtime).preview_memory_dream(params)
}

#[tauri::command]
pub(crate) fn run_memory_dream(
    state: State<'_, AppState>,
    params: MemoryDreamRunParams,
) -> AppResult<crate::backend::dto::MemoryDreamRunResult> {
    AppService::from_runtime(&state.runtime).run_memory_dream(params)
}

#[tauri::command]
pub(crate) fn preview_memory_recall(
    state: State<'_, AppState>,
    params: MemoryRecallPreviewParams,
) -> AppResult<crate::backend::dto::MemoryRecallPreview> {
    AppService::from_runtime(&state.runtime).preview_memory_recall(params)
}

#[tauri::command]
pub(crate) fn run_memory_recall(
    state: State<'_, AppState>,
    params: MemoryRecallRunParams,
) -> AppResult<crate::backend::dto::MemoryRecallRunResult> {
    AppService::from_runtime(&state.runtime).run_memory_recall(params)
}

#[tauri::command]
pub(crate) fn verify_memory(
    state: State<'_, AppState>,
    params: MemoryVerifyParams,
) -> AppResult<crate::backend::dto::MemoryVerifyResult> {
    AppService::from_runtime(&state.runtime).verify_memory(params)
}

#[tauri::command]
pub(crate) fn start_memory_task(
    app: AppHandle,
    state: State<'_, AppState>,
    params: MemoryTaskStartParams,
) -> AppResult<MemoryTaskSnapshot> {
    if params.kind != MemoryRunKind::AutoDream && params.recall.is_none() {
        return Err(
            "deep Recall and full organize background tasks require Recall parameters".to_string(),
        );
    }
    let (snapshot, cancellation, should_start) =
        state.background_tasks.begin_memory_task(&params)?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let background_tasks = state.background_tasks.clone();
    let task_id = snapshot.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let progress_tasks = background_tasks.clone();
        let progress_app = app.clone();
        let progress_task_id = task_id.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let service = AppService::from_runtime(&runtime);
            let report = move |phase: &str,
                               processed_count: usize,
                               total_count: usize,
                               run_id: Option<&str>| {
                if let Ok(snapshot) = progress_tasks.update_memory_task(
                    &progress_task_id,
                    phase,
                    processed_count,
                    total_count,
                    run_id.map(str::to_string),
                ) {
                    emit_memory_task(&progress_app, &snapshot);
                }
            };
            match params.kind {
                MemoryRunKind::AutoDream => service
                    .run_memory_dream_with_control(
                        MemoryDreamRunParams {
                            scope: params.scope,
                            trigger: params.trigger,
                            dry_run: params.dry_run,
                        },
                        Some(cancellation),
                        report,
                    )
                    .and_then(|value| {
                        serde_json::to_value(value).map_err(|error| error.to_string())
                    }),
                MemoryRunKind::DeepRecall | MemoryRunKind::FullOrganize => {
                    let mut recall = params
                        .recall
                        .ok_or_else(|| "Recall task parameters are required".to_string())?;
                    recall.scope = params.scope;
                    recall.mode = if params.kind == MemoryRunKind::FullOrganize {
                        crate::backend::models::MemoryRecallMode::Full
                    } else {
                        crate::backend::models::MemoryRecallMode::Exact
                    };
                    service
                        .run_memory_recall_with_control(
                            MemoryRecallRunParams {
                                preview: recall,
                                synthesize: params.synthesize,
                                dry_run: params.dry_run,
                            },
                            Some(cancellation),
                            report,
                        )
                        .and_then(|value| {
                            serde_json::to_value(value).map_err(|error| error.to_string())
                        })
                }
            }
        }))
        .unwrap_or_else(|_| Err("Memory task panicked".to_string()));
        match background_tasks.finish_memory_task(&task_id, result) {
            Ok(snapshot) => emit_memory_task(&app, &snapshot),
            Err(error) => log_error(
                "memory.task",
                "更新 Memory 后台任务状态失败",
                &error,
                &[("task_id", task_id)],
            ),
        }
    });
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_memory_task(
    state: State<'_, AppState>,
    params: MemoryTaskGetParams,
) -> AppResult<Option<MemoryTaskSnapshot>> {
    state.background_tasks.memory_task_snapshot(&params.task_id)
}

#[tauri::command]
pub(crate) fn list_memory_tasks(state: State<'_, AppState>) -> AppResult<Vec<MemoryTaskSnapshot>> {
    state.background_tasks.memory_task_snapshots()
}

#[tauri::command]
pub(crate) fn cancel_memory_task(
    app: AppHandle,
    state: State<'_, AppState>,
    params: MemoryTaskGetParams,
) -> AppResult<MemoryTaskSnapshot> {
    let snapshot = state.background_tasks.cancel_memory_task(&params.task_id)?;
    emit_memory_task(&app, &snapshot);
    Ok(snapshot)
}

fn emit_memory_task(app: &AppHandle, snapshot: &MemoryTaskSnapshot) {
    if let Err(error) = app.emit("memory-task-updated", snapshot) {
        log_error(
            "memory.task",
            "推送 Memory 后台任务状态失败",
            &error.to_string(),
            &[("task_id", snapshot.id.clone())],
        );
    }
}

#[tauri::command]
pub(crate) fn get_skill_backup_settings(
    state: State<'_, AppState>,
) -> AppResult<SkillBackupSettings> {
    AppService::from_runtime(&state.runtime).get_skill_backup_settings()
}

#[tauri::command]
pub(crate) fn update_skill_backup_settings(
    state: State<'_, AppState>,
    root_path: String,
    migrate: Option<bool>,
) -> AppResult<SkillBackupSettings> {
    let fields = vec![
        ("root_path", root_path.clone()),
        ("migrate", migrate.unwrap_or(true).to_string()),
    ];
    let result = (|| {
        AppService::from_runtime(&state.runtime).update_skill_backup_settings(
            UpdateSkillBackupSettingsParams {
                root_path,
                migrate: migrate.unwrap_or(true),
            },
        )
    })();

    match &result {
        Ok(settings) => log_info(
            "skill.backup.settings.update",
            "更新 Skill 备份目录成功",
            &[
                ("root_path", settings.root_path.clone()),
                ("expanded_root_path", settings.expanded_root_path.clone()),
            ],
        ),
        Err(error) => log_error(
            "skill.backup.settings.update",
            "更新 Skill 备份目录失败",
            error,
            &fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn backup_skill(
    state: State<'_, AppState>,
    asset_id: String,
) -> AppResult<CatalogAsset> {
    let fields = vec![("asset_id", asset_id.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).backup_skill(asset_id))();

    match &result {
        Ok(asset) => log_info(
            "skill.backup",
            "备份 Skill 成功",
            &asset_log_fields(&asset.asset),
        ),
        Err(error) => log_error("skill.backup", "备份 Skill 失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn backup_skills(
    app: AppHandle,
    state: State<'_, AppState>,
    asset_ids: Vec<String>,
) -> AppResult<SkillBackupTaskSnapshot> {
    let (snapshot, should_start) = state.background_tasks.begin_skill_backup(asset_ids)?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let background_tasks = state.background_tasks.clone();
    let task_id = snapshot.id.clone();
    let task_asset_ids = snapshot.asset_ids.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let progress_app = app.clone();
        let progress_tasks = background_tasks.clone();
        let progress_task_id = task_id.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            AppService::from_runtime(&runtime).backup_skills_with_progress(
                task_asset_ids,
                |completed_count, next_asset_id| match progress_tasks.update_skill_backup_progress(
                    &progress_task_id,
                    completed_count,
                    next_asset_id.map(str::to_string),
                ) {
                    Ok(snapshot) => emit_skill_backup_task(&progress_app, &snapshot),
                    Err(error) => log_error(
                        "skill.backup.background",
                        "更新 Skill 后台备份进度失败",
                        &error,
                        &[("task_id", progress_task_id.clone())],
                    ),
                },
            )
        }))
        .unwrap_or_else(|_| Err("skill backup task panicked".to_string()));
        match &result {
            Ok(assets) => log_info(
                "skill.backup.background",
                "后台备份 Skill 成功",
                &[
                    ("task_id", task_id.clone()),
                    ("asset_count", assets.len().to_string()),
                ],
            ),
            Err(error) => log_error(
                "skill.backup.background",
                "后台备份 Skill 失败",
                error,
                &[("task_id", task_id.clone())],
            ),
        }
        match background_tasks.finish_skill_backup(&task_id, result) {
            Ok(snapshot) => emit_skill_backup_task(&app, &snapshot),
            Err(error) => log_error(
                "skill.backup.background",
                "更新 Skill 后台备份任务状态失败",
                &error,
                &[("task_id", task_id)],
            ),
        }
    });

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_skill_backup_task(
    state: State<'_, AppState>,
) -> AppResult<Option<SkillBackupTaskSnapshot>> {
    state.background_tasks.skill_backup_snapshot()
}

fn emit_skill_backup_task(app: &AppHandle, snapshot: &SkillBackupTaskSnapshot) {
    if let Err(error) = app.emit("skill-backup-task-updated", snapshot) {
        log_error(
            "skill.backup.background",
            "推送 Skill 后台备份任务状态失败",
            &error.to_string(),
            &[("task_id", snapshot.id.clone())],
        );
    }
}

fn emit_conversation_script_install_task(
    app: &AppHandle,
    snapshot: &ConversationScriptInstallTaskSnapshot,
) {
    if let Err(error) = app.emit("conversation-script-install-task-updated", snapshot) {
        log_error(
            "conversation.script.install",
            "推送对话脚本后台安装任务状态失败",
            &error.to_string(),
            &[("task_id", snapshot.id.clone())],
        );
    }
}

fn spawn_conversation_lifecycle_task<F>(
    app: AppHandle,
    background_tasks: Arc<BackgroundTaskRegistry>,
    task_id: String,
    operation: &'static str,
    work: F,
) -> AppResult<()>
where
    F: FnOnce() -> Result<Value, String> + Send + 'static,
{
    let task_id_for_runtime = task_id.clone();
    let background_tasks_for_runtime = background_tasks.clone();
    let app_for_runtime = app.clone();
    let operation_for_runtime = operation;
    let task = Box::new(move |context: TaskContext| {
        let result = if context.is_cancelled() {
            Err(format!("{operation_for_runtime} task cancelled"))
        } else {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                .unwrap_or_else(|_| Err(format!("{operation_for_runtime} task panicked")))
        };
        match &result {
            Ok(value) => log_info(
                operation_for_runtime,
                "扩展生命周期任务成功",
                &[
                    ("task_id", task_id_for_runtime.clone()),
                    ("result", value.to_string()),
                ],
            ),
            Err(error) => log_error(
                operation_for_runtime,
                "扩展生命周期任务失败",
                error,
                &[("task_id", task_id_for_runtime.clone())],
            ),
        }
        let projection_result = result.clone();
        match background_tasks_for_runtime
            .finish_conversation_script_install(&task_id_for_runtime, projection_result)
        {
            Ok(snapshot) => emit_conversation_script_install_task(&app_for_runtime, &snapshot),
            Err(error) => log_error(
                operation_for_runtime,
                "更新扩展生命周期任务状态失败",
                &error,
                &[("task_id", task_id_for_runtime.clone())],
            ),
        }
        result.map_err(AppError::Legacy)
    });
    if let Err(error) = background_tasks.spawn_extension_lifecycle(&task_id, task) {
        let _ =
            background_tasks.finish_conversation_script_install(&task_id, Err(error.to_string()));
        return Err(error.to_string());
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn search_skills(
    state: State<'_, AppState>,
    params: SkillSearchParams,
) -> AppResult<SkillSearchResult> {
    let fields = vec![("query", params.query.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).search_skills(params))();

    match &result {
        Ok(result) => log_info(
            "skill.search",
            "搜索 Skill 成功",
            &[
                ("query", result.query.clone()),
                ("candidate_count", result.candidates.len().to_string()),
            ],
        ),
        Err(error) => log_error("skill.search", "搜索 Skill 失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn acquire_skill(
    state: State<'_, AppState>,
    params: SkillAcquireParams,
) -> AppResult<Value> {
    let fields = vec![("url", params.url.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).acquire_skill(params))();

    match &result {
        Ok(value) => log_info(
            "skill.acquire",
            "获取 Skill 成功",
            &[
                (
                    "dry_run",
                    value
                        .get("dry_run")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                        .to_string(),
                ),
                (
                    "name",
                    value
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                ),
            ],
        ),
        Err(error) => log_error("skill.acquire", "获取 Skill 失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn list_skill_remote_sources(
    state: State<'_, AppState>,
) -> AppResult<Vec<SkillRemoteSource>> {
    let result = (|| AppService::from_runtime(&state.runtime).list_skill_remote_sources())();

    match &result {
        Ok(sources) => log_info(
            "skill.remote.list",
            "读取远程 Skill 来源成功",
            &[("source_count", sources.len().to_string())],
        ),
        Err(error) => log_error("skill.remote.list", "读取远程 Skill 来源失败", error, &[]),
    }
    result
}

#[tauri::command]
pub(crate) fn check_skill_remote_sources(
    state: State<'_, AppState>,
    params: SkillRemoteCheckParams,
) -> AppResult<Vec<SkillRemoteSource>> {
    let fields = params
        .asset_id
        .as_ref()
        .map(|asset_id| vec![("asset_id", asset_id.clone())])
        .unwrap_or_default();
    let result = (|| AppService::from_runtime(&state.runtime).check_skill_remote_sources(params))();

    match &result {
        Ok(sources) => log_info(
            "skill.remote.check",
            "检查远程 Skill 来源成功",
            &[
                ("checked_count", sources.len().to_string()),
                (
                    "changed_count",
                    sources
                        .iter()
                        .filter(|source| source.status == "changed")
                        .count()
                        .to_string(),
                ),
            ],
        ),
        Err(error) => log_error(
            "skill.remote.check",
            "检查远程 Skill 来源失败",
            error,
            &fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn update_asset_description(
    state: State<'_, AppState>,
    asset_id: String,
    description: Option<String>,
) -> AppResult<Asset> {
    let fields = vec![("asset_id", asset_id.clone())];
    let result = (|| {
        AppService::from_runtime(&state.runtime).update_asset_description(asset_id, description)
    })();

    match &result {
        Ok(asset) => log_info(
            "asset.update_description",
            "更新资产说明成功",
            &asset_log_fields(asset),
        ),
        Err(error) => log_error(
            "asset.update_description",
            "更新资产说明失败",
            error,
            &fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn delete_asset(
    state: State<'_, AppState>,
    asset_id: String,
    unmount: Option<bool>,
) -> AppResult<Asset> {
    let fields = vec![("asset_id", asset_id.clone())];
    let result = (|| {
        AppService::from_runtime(&state.runtime).delete_asset(asset_id, unmount.unwrap_or(false))
    })();

    match &result {
        Ok(asset) => log_info("asset.delete", "删除资产成功", &asset_log_fields(asset)),
        Err(error) => log_error("asset.delete", "删除资产失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn list_sources(state: State<'_, AppState>) -> AppResult<Vec<Source>> {
    AppService::from_runtime(&state.runtime).list_sources()
}

#[tauri::command]
pub(crate) fn list_skill_sources(state: State<'_, AppState>) -> AppResult<Vec<Source>> {
    AppService::from_runtime(&state.runtime).list_skill_sources()
}

#[tauri::command]
pub(crate) fn create_source(state: State<'_, AppState>, source: SourceInput) -> AppResult<Source> {
    let input_fields = source_input_log_fields(&source);
    let result = (|| AppService::from_runtime(&state.runtime).add_source(source))();

    match &result {
        Ok(source) => log_info(
            "source.create",
            "添加数据来源成功",
            &source_log_fields(source),
        ),
        Err(error) => log_error("source.create", "添加数据来源失败", error, &input_fields),
    }
    result
}

#[tauri::command]
pub(crate) fn update_source(state: State<'_, AppState>, source: Source) -> AppResult<Source> {
    let input_fields = source_log_fields(&source);
    let result = (|| AppService::from_runtime(&state.runtime).update_source(source))();

    match &result {
        Ok(source) => log_info(
            "source.update",
            "更新数据来源成功",
            &source_log_fields(source),
        ),
        Err(error) => log_error("source.update", "更新数据来源失败", error, &input_fields),
    }
    result
}

#[tauri::command]
pub(crate) fn delete_source(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let fields = vec![("source_id", id.clone())];
    let result = (|| {
        AppService::from_runtime(&state.runtime)
            .remove_source(SourceRemoveParams {
                id: id.clone(),
                dry_run: false,
                yes: true,
            })
            .map(|_| ())
    })();

    match &result {
        Ok(()) => log_info("source.delete", "删除数据来源成功", &fields),
        Err(error) => log_error("source.delete", "删除数据来源失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn list_profiles(state: State<'_, AppState>) -> AppResult<Vec<TargetProfile>> {
    AppService::from_runtime(&state.runtime).list_profiles()
}

#[tauri::command]
pub(crate) fn create_profile(
    state: State<'_, AppState>,
    input: TargetProfileInput,
) -> AppResult<TargetProfile> {
    let mut input_fields = vec![("profile_name", input.name.clone())];
    if let Some(target_paths) = &input.target_paths {
        input_fields.push(("target_paths", target_paths.join(",")));
    }
    if let Some(app_kind) = input.app_kind {
        input_fields.push(("app_kind", format!("{app_kind:?}")));
    }
    let result = (|| AppService::from_runtime(&state.runtime).create_profile(input))();

    match &result {
        Ok(profile) => log_info(
            "profile.create",
            "添加目标 APP 配置成功",
            &profile_log_fields(profile),
        ),
        Err(error) => log_error(
            "profile.create",
            "添加目标 APP 配置失败",
            error,
            &input_fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn update_profile(
    state: State<'_, AppState>,
    profile: TargetProfile,
) -> AppResult<TargetProfile> {
    let input_fields = profile_log_fields(&profile);
    let result = (|| AppService::from_runtime(&state.runtime).update_profile(profile))();

    match &result {
        Ok(profile) => log_info(
            "profile.update",
            "更新目标 APP 配置成功",
            &profile_log_fields(profile),
        ),
        Err(error) => log_error(
            "profile.update",
            "更新目标 APP 配置失败",
            error,
            &input_fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn delete_profile(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let fields = vec![("profile_id", id.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).delete_profile(id))();

    match &result {
        Ok(()) => log_info("profile.delete", "删除目标 APP 配置成功", &fields),
        Err(error) => log_error("profile.delete", "删除目标 APP 配置失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn get_navigation_model(state: State<'_, AppState>) -> AppResult<NavigationModel> {
    AppService::from_runtime(&state.runtime).navigation_model()
}

#[tauri::command]
pub(crate) fn update_navigation_model(
    state: State<'_, AppState>,
    model: NavigationModel,
) -> AppResult<NavigationModel> {
    let fields = vec![
        ("active_rail_id", model.active_rail_id.clone()),
        ("active_header_tab_id", model.active_header_tab_id.clone()),
        ("active_sub_nav_id", model.active_sub_nav_id.clone()),
        ("rail_count", model.rail_items.len().to_string()),
    ];
    let result = (|| AppService::from_runtime(&state.runtime).update_navigation_model(model))();

    match &result {
        Ok(_) => log_info("navigation.update", "更新导航配置成功", &fields),
        Err(error) => log_error("navigation.update", "更新导航配置失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn list_app_shortcuts(state: State<'_, AppState>) -> AppResult<Vec<AppShortcut>> {
    AppService::from_runtime(&state.runtime).list_app_shortcuts()
}

#[tauri::command]
pub(crate) fn list_app_shortcut_settings(
    state: State<'_, AppState>,
) -> AppResult<Vec<AppShortcut>> {
    AppService::from_runtime(&state.runtime).list_app_shortcut_settings()
}

#[tauri::command]
pub(crate) fn update_app_shortcuts(
    state: State<'_, AppState>,
    shortcuts: Vec<AppShortcut>,
) -> AppResult<Vec<AppShortcut>> {
    let fields = vec![("shortcut_count", shortcuts.len().to_string())];
    let result = (|| AppService::from_runtime(&state.runtime).update_app_shortcuts(shortcuts))();

    match &result {
        Ok(shortcuts) => log_info(
            "settings.app_shortcuts.update",
            "更新 APP 快捷入口配置成功",
            &[("shortcut_count", shortcuts.len().to_string())],
        ),
        Err(error) => log_error(
            "settings.app_shortcuts.update",
            "更新 APP 快捷入口配置失败",
            error,
            &fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn list_asset_mounts(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> AppResult<Vec<AssetMount>> {
    AppService::from_runtime(&state.runtime).list_asset_mounts(asset_id.as_deref())
}

#[tauri::command]
pub(crate) fn list_asset_mount_statuses(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> AppResult<Vec<AssetMountStatus>> {
    AppService::from_runtime(&state.runtime).list_asset_mount_statuses(asset_id.as_deref())
}

#[tauri::command]
pub(crate) fn refresh_asset_mount_statuses(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> AppResult<Vec<AssetMountStatus>> {
    let fields = asset_id
        .as_ref()
        .map(|asset_id| vec![("asset_id", asset_id.clone())])
        .unwrap_or_default();
    let result = (|| {
        AppService::from_runtime(&state.runtime).refresh_asset_mount_statuses(asset_id.as_deref())
    })();

    match &result {
        Ok(statuses) => {
            let mut fields = fields.clone();
            fields.extend(status_summary_fields(statuses));
            log_info("mount_status.refresh", "刷新挂载状态成功", &fields);
        }
        Err(error) => log_error("mount_status.refresh", "刷新挂载状态失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn list_skill_groups(state: State<'_, AppState>) -> AppResult<Vec<AssetGroupDetail>> {
    AppService::from_runtime(&state.runtime).list_skill_groups()
}

#[tauri::command]
pub(crate) fn create_skill_group(
    state: State<'_, AppState>,
    input: AssetGroupInput,
) -> AppResult<AssetGroupDetail> {
    let input_fields = vec![("group_name", input.name.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).create_skill_group(input))();

    match &result {
        Ok(detail) => log_info(
            "skill_group.create",
            "添加 skill 分组成功",
            &[
                ("group_id", detail.group.id.clone()),
                ("group_name", detail.group.name.clone()),
                ("member_count", detail.members.len().to_string()),
            ],
        ),
        Err(error) => log_error(
            "skill_group.create",
            "添加 skill 分组失败",
            error,
            &input_fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn update_skill_group(
    state: State<'_, AppState>,
    group: AssetGroup,
) -> AppResult<AssetGroupDetail> {
    let input_fields = vec![
        ("group_id", group.id.clone()),
        ("group_name", group.name.clone()),
    ];
    let result = (|| AppService::from_runtime(&state.runtime).update_skill_group(group))();

    match &result {
        Ok(detail) => log_info(
            "skill_group.update",
            "更新 skill 分组成功",
            &[
                ("group_id", detail.group.id.clone()),
                ("group_name", detail.group.name.clone()),
                ("member_count", detail.members.len().to_string()),
            ],
        ),
        Err(error) => log_error(
            "skill_group.update",
            "更新 skill 分组失败",
            error,
            &input_fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn delete_skill_group(state: State<'_, AppState>, group_id: String) -> AppResult<()> {
    let fields = vec![("group_id", group_id.clone())];
    let result = (|| AppService::from_runtime(&state.runtime).delete_skill_group(group_id))();

    match &result {
        Ok(()) => log_info("skill_group.delete", "删除 skill 分组成功", &fields),
        Err(error) => log_error("skill_group.delete", "删除 skill 分组失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn set_skill_group_manual_members(
    state: State<'_, AppState>,
    group_id: String,
    asset_ids: Vec<String>,
) -> AppResult<AssetGroupDetail> {
    let fields = vec![
        ("group_id", group_id.clone()),
        ("asset_count", asset_ids.len().to_string()),
    ];
    let result = (|| {
        AppService::from_runtime(&state.runtime).set_skill_group_manual_members(group_id, asset_ids)
    })();

    match &result {
        Ok(detail) => log_info(
            "skill_group.members.update",
            "更新 skill 分组成员成功",
            &[
                ("group_id", detail.group.id.clone()),
                ("group_name", detail.group.name.clone()),
                ("member_count", detail.members.len().to_string()),
            ],
        ),
        Err(error) => log_error(
            "skill_group.members.update",
            "更新 skill 分组成员失败",
            error,
            &fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn apply_skill_group_mount(
    state: State<'_, AppState>,
    group_id: String,
    profile_id: String,
    enabled: bool,
) -> AppResult<ApplyAssetGroupMountResult> {
    let fields = vec![
        ("group_id", group_id.clone()),
        ("profile_id", profile_id.clone()),
        ("enabled", enabled.to_string()),
    ];
    let result = (|| {
        AppService::from_runtime(&state.runtime).apply_skill_group_mount(
            &group_id,
            &profile_id,
            enabled,
        )
    })();

    match &result {
        Ok(result) => {
            let mut fields = fields.clone();
            fields.extend([
                ("requested_count", result.requested_count.to_string()),
                ("updated_count", result.updated_count.to_string()),
                ("error_count", result.error_count.to_string()),
            ]);
            let level_message = if result.error_count > 0 {
                (
                    "skill_group.mount.apply",
                    "应用 skill 分组挂载完成但存在失败",
                )
            } else {
                ("skill_group.mount.apply", "应用 skill 分组挂载成功")
            };
            if result.error_count > 0 {
                log_warn(level_message.0, level_message.1, &fields);
            } else {
                log_info(level_message.0, level_message.1, &fields);
            }
            for item in &result.errors {
                log_error(
                    "skill_group.mount.error",
                    "skill 分组挂载成员失败",
                    &item.message,
                    &[
                        ("group_id", result.group_id.clone()),
                        ("profile_id", result.profile_id.clone()),
                        ("asset_id", item.asset_id.clone()),
                    ],
                );
            }
        }
        Err(error) => log_error(
            "skill_group.mount.apply",
            "应用 skill 分组挂载失败",
            error,
            &fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn preview_skill_group_exclusive_mount(
    state: State<'_, AppState>,
    input: SkillGroupExclusiveMountInput,
) -> AppResult<SkillGroupExclusiveMountPreview> {
    let fields = vec![
        ("profile_id", input.profile_id.clone()),
        ("group_count", input.group_ids.len().to_string()),
    ];
    let result =
        (|| AppService::from_runtime(&state.runtime).preview_skill_group_exclusive_mount(input))();

    match &result {
        Ok(preview) => {
            log_info(
                "skill_group.exclusive.preview",
                "预览 skill 分组独占挂载成功",
                &[
                    ("profile_id", preview.profile_id.clone()),
                    ("group_count", preview.group_ids.len().to_string()),
                    (
                        "selected_count",
                        preview.selected_skill_ids.len().to_string(),
                    ),
                    ("keep_count", preview.keep_count.to_string()),
                    ("mount_count", preview.mount_count.to_string()),
                    ("unmount_count", preview.unmount_count.to_string()),
                    ("skipped_count", preview.skipped_count.to_string()),
                ],
            );
            for item in &preview.skipped {
                log_warn(
                    "skill_group.exclusive.skipped",
                    "skill 独占挂载预览跳过",
                    &[
                        ("profile_id", preview.profile_id.clone()),
                        ("asset_id", item.asset_id.clone()),
                        ("skill_name", item.name.clone()),
                        ("reason", item.reason.clone()),
                    ],
                );
            }
        }
        Err(error) => log_error(
            "skill_group.exclusive.preview",
            "预览 skill 分组独占挂载失败",
            error,
            &fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn apply_skill_group_exclusive_mount(
    state: State<'_, AppState>,
    input: SkillGroupExclusiveMountInput,
) -> AppResult<ApplySkillGroupExclusiveMountResult> {
    let fields = vec![
        ("profile_id", input.profile_id.clone()),
        ("group_count", input.group_ids.len().to_string()),
    ];
    let result =
        (|| AppService::from_runtime(&state.runtime).apply_skill_group_exclusive_mount(input))();

    match &result {
        Ok(result) => {
            let fields = vec![
                ("profile_id", result.preview.profile_id.clone()),
                ("group_count", result.preview.group_ids.len().to_string()),
                ("keep_count", result.preview.keep_count.to_string()),
                ("mount_count", result.preview.mount_count.to_string()),
                ("unmount_count", result.preview.unmount_count.to_string()),
                ("skipped_count", result.preview.skipped_count.to_string()),
                ("error_count", result.errors.len().to_string()),
            ];
            if result.errors.is_empty() && result.preview.skipped_count == 0 {
                log_info(
                    "skill_group.exclusive.apply",
                    "应用 skill 分组独占挂载成功",
                    &fields,
                );
            } else {
                log_warn(
                    "skill_group.exclusive.apply",
                    "应用 skill 分组独占挂载完成但存在跳过或失败",
                    &fields,
                );
            }
            for item in &result.preview.skipped {
                log_warn(
                    "skill_group.exclusive.skipped",
                    "skill 独占挂载应用跳过",
                    &[
                        ("profile_id", result.preview.profile_id.clone()),
                        ("asset_id", item.asset_id.clone()),
                        ("skill_name", item.name.clone()),
                        ("reason", item.reason.clone()),
                    ],
                );
            }
            for item in &result.errors {
                log_error(
                    "skill_group.exclusive.error",
                    "skill 独占挂载应用失败",
                    &item.message,
                    &[
                        ("profile_id", result.preview.profile_id.clone()),
                        ("asset_id", item.asset_id.clone()),
                        ("skill_name", item.name.clone()),
                    ],
                );
            }
        }
        Err(error) => log_error(
            "skill_group.exclusive.apply",
            "应用 skill 分组独占挂载失败",
            error,
            &fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn toggle_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
) -> AppResult<AssetMount> {
    let result =
        (|| AppService::from_runtime(&state.runtime).toggle_asset_mount(&asset_id, &profile_id))();

    if let Err(error) = &result {
        log_error(
            "skill.mount.toggle",
            "切换 skill 挂载失败",
            error,
            &[("asset_id", asset_id), ("profile_id", profile_id)],
        );
    }
    result
}

#[tauri::command]
pub(crate) fn unmount_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
) -> AppResult<AssetMountUpdateResult> {
    let result =
        (|| AppService::from_runtime(&state.runtime).unmount_asset_by_id(&asset_id, &profile_id))();

    if let Err(error) = &result {
        log_error(
            "skill.unmount.command",
            "卸载 skill 命令失败",
            error,
            &[("asset_id", asset_id), ("profile_id", profile_id)],
        );
    }
    result
}

#[tauri::command]
pub(crate) fn mount_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
) -> AppResult<AssetMountUpdateResult> {
    let result =
        (|| AppService::from_runtime(&state.runtime).mount_asset_by_id(&asset_id, &profile_id))();

    if let Err(error) = &result {
        log_error(
            "skill.mount.command",
            "挂载 skill 命令失败",
            error,
            &[("asset_id", asset_id), ("profile_id", profile_id)],
        );
    }
    result
}

#[tauri::command]
pub(crate) fn set_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
    enabled: bool,
    strategy: Option<DeploymentStrategy>,
) -> AppResult<AssetMount> {
    let result = (|| {
        AppService::from_runtime(&state.runtime).set_asset_mount(
            &asset_id,
            &profile_id,
            enabled,
            strategy,
        )
    })();

    if let Err(error) = &result {
        log_error(
            "skill.mount.set",
            "设置 skill 挂载关系失败",
            error,
            &[
                ("asset_id", asset_id),
                ("profile_id", profile_id),
                ("enabled", enabled.to_string()),
            ],
        );
    }
    result
}

#[tauri::command]
pub(crate) fn scan_sources(
    state: State<'_, AppState>,
    kind: Option<AssetKind>,
) -> AppResult<Vec<CatalogAsset>> {
    let fields = kind
        .map(|kind| vec![("asset_kind", format!("{kind:?}"))])
        .unwrap_or_default();
    let result = (|| {
        AppService::from_runtime(&state.runtime).scan_sources(SourceScanParams {
            kind,
            dry_run: false,
        })
    })();

    match &result {
        Ok(assets) => {
            let mut fields = fields.clone();
            fields.push(("asset_count", assets.len().to_string()));
            log_info("source.scan.all", "扫描全部来源成功", &fields);
        }
        Err(error) => log_error("source.scan.all", "扫描全部来源失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn scan_skill_sources(state: State<'_, AppState>) -> AppResult<Vec<CatalogAsset>> {
    let result = (|| AppService::from_runtime(&state.runtime).scan_skill_sources())();

    match &result {
        Ok(assets) => log_info(
            "source.scan.skills",
            "扫描 skill 来源成功",
            &[("skill_count", assets.len().to_string())],
        ),
        Err(error) => log_error("source.scan.skills", "扫描 skill 来源失败", error, &[]),
    }
    result
}

#[tauri::command]
pub(crate) fn list_conversation_adapters(
    state: State<'_, AppState>,
) -> AppResult<Vec<ConversationAdapter>> {
    AppService::from_runtime(&state.runtime).list_conversation_adapters()
}

#[tauri::command]
pub(crate) fn scaffold_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterScaffoldParams,
) -> AppResult<crate::backend::conversations::ExternalAdapterScaffoldResult> {
    AppService::from_runtime(&state.runtime).scaffold_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn validate_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterValidateParams,
) -> AppResult<crate::backend::conversations::ExternalAdapterValidationResult> {
    AppService::from_runtime(&state.runtime).validate_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn list_conversation_adapter_runtime_statuses(
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::backend::conversations::ConversationAdapterRuntimeStatus>> {
    AppService::from_runtime(&state.runtime).list_conversation_adapter_runtime_statuses()
}

#[tauri::command]
pub(crate) async fn list_agent_catalog(
    state: State<'_, AppState>,
) -> AppResult<Vec<AgentCatalogEntry>> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).list_agent_catalog()
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn check_agent_connection(
    state: State<'_, AppState>,
    params: AgentConnectionCheckRequest,
) -> AppResult<AgentConnectionResult> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).check_agent_connection(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn list_agent_models(
    state: State<'_, AppState>,
    params: AgentModelsRequest,
) -> AppResult<AgentModelsResult> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).list_agent_models(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn check_opencode_translation_availability(
    state: State<'_, AppState>,
) -> AppResult<OpencodeTranslationAvailability> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).check_opencode_translation_availability()
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(String::from)
}

#[tauri::command]
pub(crate) async fn translate_conversation_card_with_opencode(
    state: State<'_, AppState>,
    params: OpencodeTranslationRequest,
) -> AppResult<OpencodeTranslationResult> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).translate_conversation_card_with_opencode(params)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(String::from)
}

#[tauri::command]
pub(crate) async fn translate_conversation_card(
    state: State<'_, AppState>,
    params: ConversationTranslationRequest,
) -> AppResult<OpencodeTranslationResult> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).translate_conversation_card(params)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(String::from)
}

#[tauri::command]
pub(crate) async fn test_conversation_translation_connection(
    state: State<'_, AppState>,
    params: ConversationTranslationConnectionRequest,
) -> AppResult<OpencodeTranslationAvailability> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).test_conversation_translation_connection(params)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(String::from)
}

#[tauri::command]
pub(crate) async fn list_conversation_translation_models(
    state: State<'_, AppState>,
    params: ConversationTranslationModelsRequest,
) -> AppResult<ConversationTranslationModelsResult> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).list_conversation_translation_models(params)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(String::from)
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
            log_error(
                "ai_execution.task",
                "推送 AI 执行任务状态失败",
                &error.to_string(),
                &[("task_id", snapshot.id.clone())],
            );
        }
    }
}

struct RegistryAiExecutionProgressSink {
    tasks: Arc<BackgroundTaskRegistry>,
    task_id: String,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
}

impl AiExecutionProgressSink for RegistryAiExecutionProgressSink {
    fn set_phase(&self, phase: AiExecutionPhase) {
        match self.tasks.update_ai_execution_phase(&self.task_id, phase) {
            Ok(snapshot) => self.emitter.emit(&snapshot),
            Err(error) => log_error(
                "ai_execution.task",
                "更新 AI 执行任务阶段失败",
                &error,
                &[("task_id", self.task_id.clone())],
            ),
        }
    }
}

fn prepare_ai_execution_task(
    tasks: Arc<BackgroundTaskRegistry>,
    params: ConversationTranslationRequest,
    emitter: Arc<dyn AiExecutionTaskEmitter>,
) -> AppResult<(AiExecutionTaskSnapshot, AiExecutionRequest)> {
    let (agent_id, prompt, model) = prepare_opencode_agent_translation(params)?;
    let (snapshot, cancellation) =
        tasks.begin_ai_execution(AiExecutionPurpose::Translation, &agent_id)?;
    let progress = Arc::new(RegistryAiExecutionProgressSink {
        tasks,
        task_id: snapshot.id.clone(),
        emitter: emitter.clone(),
    });
    let request = AiExecutionRequest {
        execution_id: snapshot.id.clone(),
        agent_id,
        purpose: AiExecutionPurpose::Translation,
        prompt,
        model,
        limits: AiExecutionLimits::default(),
        cancellation,
        progress: Some(progress),
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
    let execution = tokio::spawn(async move { runtime.execute(request).await });
    let result = match execution.await {
        Ok(result) => result,
        Err(_) => Err(AiExecutionError::Protocol {
            operation: "execution_task_panicked",
        }),
    };
    match tasks.finish_ai_execution(&task_id, result) {
        Ok(snapshot) => emitter.emit(&snapshot),
        Err(error) => log_error(
            "ai_execution.task",
            "收敛 AI 执行任务状态失败",
            &error,
            &[("task_id", task_id)],
        ),
    }
}

#[tauri::command]
pub(crate) async fn start_conversation_card_translation(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationTranslationRequest,
) -> AppResult<AiExecutionTaskSnapshot> {
    let emitter: Arc<dyn AiExecutionTaskEmitter> = Arc::new(TauriAiExecutionTaskEmitter { app });
    let tasks = state.background_tasks.clone();
    let (snapshot, request) = prepare_ai_execution_task(tasks.clone(), params, emitter.clone())?;
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
) -> AppResult<Option<AiExecutionTaskSnapshot>> {
    state
        .background_tasks
        .ai_execution_snapshot(&params.task_id)
}

#[tauri::command]
pub(crate) fn list_ai_execution_tasks(
    state: State<'_, AppState>,
) -> AppResult<Vec<AiExecutionTaskSnapshot>> {
    state.background_tasks.ai_execution_snapshots()
}

#[tauri::command]
pub(crate) fn cancel_ai_execution_task(
    app: AppHandle,
    state: State<'_, AppState>,
    params: AiExecutionTaskGetParams,
) -> AppResult<AiExecutionTaskSnapshot> {
    let snapshot = state
        .background_tasks
        .cancel_ai_execution(&params.task_id)?;
    TauriAiExecutionTaskEmitter { app }.emit(&snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn register_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterRegisterParams,
) -> AppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime).register_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn unregister_conversation_adapter(
    state: State<'_, AppState>,
    params: ConversationAdapterUnregisterParams,
) -> AppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime).unregister_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn try_run_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterTryRunParams,
) -> AppResult<crate::backend::conversations::ExternalAdapterRunResult> {
    AppService::from_runtime(&state.runtime).try_run_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn list_conversation_sources(
    state: State<'_, AppState>,
) -> AppResult<Vec<ConversationSource>> {
    AppService::from_runtime(&state.runtime).list_conversation_sources()
}

#[tauri::command]
pub(crate) fn upsert_conversation_source(
    state: State<'_, AppState>,
    params: ConversationSourceUpsertParams,
) -> AppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime).upsert_conversation_source(params)
}

#[tauri::command]
pub(crate) fn disable_conversation_source(
    state: State<'_, AppState>,
    params: ConversationSourceDisableParams,
) -> AppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime).disable_conversation_source(params)
}

#[tauri::command]
pub(crate) fn list_conversation_script_catalog(
    state: State<'_, AppState>,
    params: ConversationScriptCatalogParams,
) -> AppResult<Vec<crate::backend::application::ConversationScriptCatalogEntry>> {
    AppService::from_runtime(&state.runtime).list_conversation_script_catalog(params)
}

#[tauri::command]
pub(crate) fn register_conversation_adapter_local(
    state: State<'_, AppState>,
    params: ConversationAdapterLocalRegisterParams,
) -> AppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime).register_conversation_adapter_local(params)
}

#[tauri::command]
pub(crate) async fn inspect_conversation_adapter_package(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageInspectParams,
) -> AppResult<crate::backend::application::ConversationAdapterPackageInspection> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).inspect_conversation_adapter_package(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn prepare_conversation_adapter_package_change(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageChangeParams,
) -> AppResult<crate::backend::application::ConversationAdapterPackageChangePreflight> {
    let runtime = state.runtime.clone();
    let mut preflight = tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).prepare_conversation_adapter_package_change(params)
    })
    .await
    .map_err(|error| error.to_string())??;
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
) -> AppResult<Vec<crate::backend::application::ConversationAdapterPackageCatalogEntry>> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).list_conversation_adapter_packages(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn list_conversation_adapter_package_releases(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageReleaseListParams,
) -> AppResult<Vec<crate::backend::models::ConversationAdapterCatalogRelease>> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).list_conversation_adapter_package_releases(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn list_installed_conversation_adapter_package_versions(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> AppResult<Vec<crate::backend::models::ConversationAdapterPackageVersion>> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime)
            .list_installed_conversation_adapter_package_versions(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn switch_conversation_adapter_package_version(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> AppResult<serde_json::Value> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).switch_conversation_adapter_package_version(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn rollback_conversation_adapter_package_version(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> AppResult<serde_json::Value> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).rollback_conversation_adapter_package_version(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn delete_conversation_adapter_package_version(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageVersionChangeParams,
) -> AppResult<serde_json::Value> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).delete_conversation_adapter_package_version(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn refresh_conversation_adapter_catalogs(
    state: State<'_, AppState>,
    params: ConversationAdapterCatalogRefreshParams,
) -> AppResult<Vec<crate::backend::models::ConversationAdapterCatalogRelease>> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).refresh_conversation_adapter_catalogs(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn check_conversation_adapter_package_updates(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageUpdateCheckParams,
) -> AppResult<Vec<crate::backend::application::ConversationAdapterPackageUpdateStatus>> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).check_conversation_adapter_package_updates(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn set_conversation_adapter_package_update_policy(
    state: State<'_, AppState>,
    params: ConversationAdapterPackageUpdatePolicyParams,
) -> AppResult<crate::backend::models::ConversationAdapterPackage> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).set_conversation_adapter_package_update_policy(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) fn install_conversation_adapter_package(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationAdapterPackageInstallParams,
) -> AppResult<ConversationScriptInstallTaskSnapshot> {
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
        move || AppService::from_runtime(&runtime).install_conversation_adapter_package(params),
    )?;

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn update_conversation_adapter_package(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationAdapterPackageInstallParams,
) -> AppResult<ConversationScriptInstallTaskSnapshot> {
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
        move || AppService::from_runtime(&runtime).update_conversation_adapter_package(params),
    )?;

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn uninstall_conversation_adapter_package(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationAdapterPackageUninstallParams,
) -> AppResult<ConversationScriptInstallTaskSnapshot> {
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
        move || AppService::from_runtime(&runtime).uninstall_conversation_adapter_package(params),
    )?;
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_adapter_package_task(
    state: State<'_, AppState>,
) -> AppResult<Option<ConversationScriptInstallTaskSnapshot>> {
    state
        .background_tasks
        .conversation_script_install_snapshot()
}

#[tauri::command]
pub(crate) fn install_conversation_script(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationScriptInstallParams,
) -> AppResult<ConversationScriptInstallTaskSnapshot> {
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
        move || AppService::from_runtime(&runtime).install_conversation_script(params),
    )?;

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_script_install_task(
    state: State<'_, AppState>,
) -> AppResult<Option<ConversationScriptInstallTaskSnapshot>> {
    state
        .background_tasks
        .conversation_script_install_snapshot()
}

#[tauri::command]
pub(crate) fn sync_conversations(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationSyncParams,
) -> AppResult<ConversationSyncTaskSnapshot> {
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
) -> AppResult<ConversationSyncTaskSnapshot> {
    let (snapshot, should_start) = background_tasks.begin_conversation_sync(&params)?;
    if !should_start {
        return Ok(snapshot);
    }

    let task_id = snapshot.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let progress_app = app.clone();
        let progress_tasks = background_tasks.clone();
        let progress_task_id = task_id.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            AppService::from_runtime(&runtime).sync_conversations_with_progress(
                params,
                |completed_source_count, total_source_count, current_source_name| {
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
                                log_error(
                                    "conversation.sync",
                                    "推送后台同步进度失败",
                                    &error.to_string(),
                                    &[("task_id", progress_task_id.clone())],
                                );
                            }
                        }
                        Err(error) => log_error(
                            "conversation.sync",
                            "更新后台同步进度失败",
                            &error,
                            &[("task_id", progress_task_id.clone())],
                        ),
                    }
                },
            )
        }))
        .unwrap_or_else(|_| Err("conversation sync task panicked".to_string()));
        match &result {
            Ok(value) => log_info(
                "conversation.sync",
                "后台同步对话记录成功",
                &[("task_id", task_id.clone()), ("result", value.to_string())],
            ),
            Err(error) => log_error(
                "conversation.sync",
                "后台同步对话记录失败",
                error,
                &[("task_id", task_id.clone())],
            ),
        }
        match background_tasks.finish_conversation_sync(&task_id, result) {
            Ok(snapshot) => {
                if let Err(error) = app.emit("conversation-sync-task-updated", &snapshot) {
                    log_error(
                        "conversation.sync",
                        "推送后台同步任务状态失败",
                        &error.to_string(),
                        &[("task_id", task_id)],
                    );
                }
            }
            Err(error) => {
                log_error(
                    "conversation.sync",
                    "更新后台同步任务状态失败",
                    &error,
                    &[("task_id", task_id)],
                );
            }
        }
    });

    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_sync_task(
    state: State<'_, AppState>,
) -> AppResult<Option<ConversationSyncTaskSnapshot>> {
    state.background_tasks.conversation_sync_snapshot()
}

#[tauri::command]
pub(crate) fn list_conversation_sync_tasks(
    state: State<'_, AppState>,
) -> AppResult<Vec<ConversationSyncTaskSnapshot>> {
    state.background_tasks.conversation_sync_snapshots()
}

#[tauri::command]
pub(crate) async fn list_conversation_sessions(
    state: State<'_, AppState>,
    params: ConversationSessionListParams,
) -> AppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).list_conversation_sessions(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn get_conversation_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> AppResult<crate::backend::dto::ConversationSessionDetail> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).get_conversation_session(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) fn export_conversation_session(
    state: State<'_, AppState>,
    params: ConversationSessionExportParams,
) -> AppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime).export_conversation_session(params)
}

#[tauri::command]
pub(crate) async fn list_web_record_sessions(
    state: State<'_, AppState>,
    params: ConversationSessionListParams,
) -> AppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).list_web_record_sessions(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn get_web_record_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> AppResult<crate::backend::dto::ConversationSessionDetail> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).get_web_record_session(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn search_conversation_records(
    state: State<'_, AppState>,
    params: ConversationSearchParams,
) -> AppResult<ConversationSearchResult> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).search_conversation_records(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 检索最近增量同步变动的会话卡片记录
#[tauri::command]
pub(crate) async fn search_recent_incremental_conversation_records(
    state: State<'_, AppState>,
    params: crate::backend::application::ConversationIncrementalSearchParams,
) -> AppResult<ConversationSearchResult> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).search_recent_incremental_conversation_records(params)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn get_conversation_search_index_status(
    state: State<'_, AppState>,
) -> AppResult<ConversationSearchIndexStatus> {
    let runtime = state.runtime.clone();
    tauri::async_runtime::spawn_blocking(move || {
        AppService::from_runtime(&runtime).get_conversation_search_index_status()
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) fn start_conversation_search_index_rebuild(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<ConversationSearchIndexTaskSnapshot> {
    let (snapshot, should_start) = state
        .background_tasks
        .begin_conversation_search_index_rebuild()?;
    if !should_start {
        return Ok(snapshot);
    }

    let runtime = state.runtime.clone();
    let background_tasks = state.background_tasks.clone();
    let task_id = snapshot.id.clone();
    if let Some(task_runtime) = background_tasks.task_runtime() {
        let app = app.clone();
        let task_id_for_runtime = task_id.clone();
        let background_tasks_for_runtime = background_tasks.clone();
        let outcome = task_runtime.spawn(
            TaskSpec::new(
                TaskKind::SearchIndexRebuild,
                Some("conversation-search-index".to_string()),
            ),
            Box::new(move |_context| {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    AppService::from_runtime(&runtime)
                        .rebuild_conversation_search_index()
                        .and_then(|report| {
                            serde_json::to_value(report).map_err(|error| error.to_string())
                        })
                }))
                .unwrap_or_else(|_| Err("conversation search index rebuild panicked".to_string()));
                let projection_result = result.clone();
                match background_tasks_for_runtime.finish_conversation_search_index_rebuild(
                    &task_id_for_runtime,
                    projection_result,
                ) {
                    Ok(snapshot) => {
                        if let Err(error) =
                            app.emit("conversation-search-index-task-updated", &snapshot)
                        {
                            log_error(
                                "conversation.search.index.rebuild",
                                "推送对话搜索索引任务状态失败",
                                &error.to_string(),
                                &[("task_id", task_id_for_runtime.clone())],
                            );
                        }
                    }
                    Err(error) => log_error(
                        "conversation.search.index.rebuild",
                        "更新对话搜索索引任务状态失败",
                        &error,
                        &[("task_id", task_id_for_runtime.clone())],
                    ),
                }
                result.map_err(AppError::Legacy)
            }),
        );
        match outcome {
            Ok(SpawnOutcome::Started(_)) => {}
            Ok(SpawnOutcome::Existing(_)) => {
                return Err(
                    "conversation search index task is already running in TaskRuntime".to_string(),
                );
            }
            Err(error) => {
                let _ = background_tasks
                    .finish_conversation_search_index_rebuild(&task_id, Err(error.to_string()));
                return Err(error.into());
            }
        }
    } else {
        tauri::async_runtime::spawn_blocking(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                AppService::from_runtime(&runtime)
                    .rebuild_conversation_search_index()
                    .and_then(|report| {
                        serde_json::to_value(report).map_err(|error| error.to_string())
                    })
            }))
            .unwrap_or_else(|_| Err("conversation search index rebuild panicked".to_string()));
            match background_tasks.finish_conversation_search_index_rebuild(&task_id, result) {
                Ok(snapshot) => {
                    if let Err(error) =
                        app.emit("conversation-search-index-task-updated", &snapshot)
                    {
                        log_error(
                            "conversation.search.index.rebuild",
                            "推送对话搜索索引任务状态失败",
                            &error.to_string(),
                            &[("task_id", task_id)],
                        );
                    }
                }
                Err(error) => log_error(
                    "conversation.search.index.rebuild",
                    "更新对话搜索索引任务状态失败",
                    &error,
                    &[("task_id", task_id)],
                ),
            }
        });
    }
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn get_conversation_search_index_task(
    state: State<'_, AppState>,
) -> AppResult<Option<ConversationSearchIndexTaskSnapshot>> {
    state.background_tasks.conversation_search_index_snapshot()
}

#[tauri::command]
pub(crate) fn export_web_record_session(
    state: State<'_, AppState>,
    params: ConversationSessionExportParams,
) -> AppResult<serde_json::Value> {
    AppService::from_runtime(&state.runtime).export_web_record_session(params)
}

#[tauri::command]
pub(crate) fn list_conversation_questions(
    state: State<'_, AppState>,
    params: ConversationQuestionListParams,
) -> AppResult<Vec<crate::backend::dto::ConversationQuestionDetail>> {
    AppService::from_runtime(&state.runtime).list_conversation_questions(params)
}

#[tauri::command]
pub(crate) fn get_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionGetParams,
) -> AppResult<crate::backend::dto::ConversationQuestionDetail> {
    AppService::from_runtime(&state.runtime).get_conversation_question(params)
}

#[tauri::command]
pub(crate) fn list_conversation_blocks(
    state: State<'_, AppState>,
    params: ConversationBlockListParams,
) -> AppResult<Vec<crate::backend::dto::ConversationBlockLocator>> {
    AppService::from_runtime(&state.runtime).list_conversation_blocks(params)
}

#[tauri::command]
pub(crate) fn get_conversation_block(
    state: State<'_, AppState>,
    params: ConversationBlockGetParams,
) -> AppResult<crate::backend::dto::ConversationBlockDetail> {
    AppService::from_runtime(&state.runtime).get_conversation_block(params)
}

#[tauri::command]
pub(crate) fn merge_conversation_questions(
    state: State<'_, AppState>,
    params: ConversationQuestionMergeParams,
) -> AppResult<crate::backend::dto::ConversationMutationResult> {
    AppService::from_runtime(&state.runtime).merge_conversation_questions(params)
}

#[tauri::command]
pub(crate) fn split_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionSplitParams,
) -> AppResult<crate::backend::dto::ConversationMutationResult> {
    AppService::from_runtime(&state.runtime).split_conversation_question(params)
}

#[tauri::command]
pub(crate) fn update_conversation_part_translation(
    state: State<'_, AppState>,
    params: ConversationPartTranslationUpdateParams,
) -> AppResult<()> {
    AppService::from_runtime(&state.runtime).update_conversation_part_translation(params)
}

#[tauri::command]
pub(crate) fn create_plan(
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> AppResult<DeploymentPlan> {
    let fields = profile_id
        .as_ref()
        .map(|profile_id| vec![("profile_id", profile_id.clone())])
        .unwrap_or_default();
    let result = (|| AppService::from_runtime(&state.runtime).create_plan(profile_id.as_deref()))();

    match &result {
        Ok(plan) => {
            let mut fields = fields.clone();
            fields.extend([
                ("plan_id", plan.id.clone()),
                ("action_count", plan.actions.len().to_string()),
                ("create_count", plan.summary.create_count.to_string()),
                ("skip_count", plan.summary.skip_count.to_string()),
                ("conflict_count", plan.summary.conflict_count.to_string()),
            ]);
            log_info("deployment_plan.create", "创建部署计划成功", &fields);
        }
        Err(error) => log_error("deployment_plan.create", "创建部署计划失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn execute_plan(
    state: State<'_, AppState>,
    plan: DeploymentPlan,
    action_ids: Option<Vec<String>>,
) -> AppResult<ExecutionResult> {
    let fields = vec![
        ("plan_id", plan.id.clone()),
        ("action_count", plan.actions.len().to_string()),
        (
            "requested_action_count",
            action_ids.as_ref().map(Vec::len).unwrap_or(0).to_string(),
        ),
    ];
    let result = (|| AppService::from_runtime(&state.runtime).execute_plan(plan, action_ids))();

    match &result {
        Ok(result) => {
            let mut fields = fields.clone();
            fields.extend([
                ("executed_count", result.executed_count.to_string()),
                ("skipped_count", result.skipped_count.to_string()),
                ("conflict_count", result.conflict_count.to_string()),
                ("error_count", result.errors.len().to_string()),
            ]);
            if result.conflict_count > 0 || !result.errors.is_empty() {
                log_warn(
                    "deployment_plan.execute",
                    "执行部署计划完成但存在冲突或失败",
                    &fields,
                );
            } else {
                log_info("deployment_plan.execute", "执行部署计划成功", &fields);
            }
        }
        Err(error) => log_error(
            "deployment_plan.execute",
            "执行部署计划失败",
            error,
            &fields,
        ),
    }
    result
}

#[tauri::command]
pub(crate) fn reveal_path(path: String) -> AppResult<()> {
    let fields = vec![("path", path.clone())];
    let result = crate::adapters::platform::reveal_path(path);
    match &result {
        Ok(()) => log_info("path.reveal", "打开路径成功", &fields),
        Err(error) => log_error("path.reveal", "打开路径失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn get_cli_tools_status(
    app: AppHandle,
) -> AppResult<crate::adapters::cli_tools::CliToolsStatus> {
    crate::adapters::cli_tools::status(&app)
}

#[tauri::command]
pub(crate) fn install_cli_tools(
    app: AppHandle,
) -> AppResult<crate::adapters::cli_tools::CliToolsStatus> {
    let result = crate::adapters::cli_tools::install(&app);
    match &result {
        Ok(status) => log_info(
            "cli.install",
            "安装命令行工具成功",
            &[
                ("install_dir", status.install_dir.clone()),
                ("path_configured", status.path_configured.to_string()),
            ],
        ),
        Err(error) => log_error("cli.install", "安装命令行工具失败", error, &[]),
    }
    result
}

#[tauri::command]
pub(crate) fn logs_get_snapshot(
    file_name: Option<String>,
    line_limit: Option<usize>,
) -> AppResult<crate::backend::logs::LogSnapshot> {
    crate::backend::logs::logs_get_snapshot(file_name, line_limit)
}

#[tauri::command]
pub(crate) fn logs_open_log_directory() -> AppResult<()> {
    crate::backend::logs::logs_open_log_directory()
}

#[tauri::command]
pub(crate) fn logs_write_operation(
    level: String,
    operation: String,
    message: String,
    fields: Option<BTreeMap<String, String>>,
) -> AppResult<()> {
    crate::backend::logs::logs_write_operation(level, operation, message, fields)
}

#[tauri::command]
pub(crate) fn copy_prompt_card_to_clipboard(params: PromptClipboardParams) -> AppResult<()> {
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
        cancel_app_close_prompt,
        complete_app_close,
        list_assets,
        list_source_assets,
        list_memory_items,
        get_memory_item,
        create_memory_item,
        update_memory_item,
        archive_memory_item,
        accept_memory_candidate,
        reject_memory_candidate,
        memory_dream_status,
        memory_overview,
        list_memory_dream_notes,
        get_memory_dream_note,
        archive_memory_dream_note,
        promote_memory_dream_note,
        preview_memory_dream,
        run_memory_dream,
        preview_memory_recall,
        run_memory_recall,
        verify_memory,
        start_memory_task,
        get_memory_task,
        list_memory_tasks,
        cancel_memory_task,
        get_skill_backup_settings,
        update_skill_backup_settings,
        backup_skill,
        backup_skills,
        get_skill_backup_task,
        search_skills,
        acquire_skill,
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
        apply_skill_group_mount,
        preview_skill_group_exclusive_mount,
        apply_skill_group_exclusive_mount,
        toggle_asset_mount,
        mount_asset_mount,
        unmount_asset_mount,
        set_asset_mount,
        scan_sources,
        scan_skill_sources,
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
        translate_conversation_card_with_opencode,
        translate_conversation_card,
        test_conversation_translation_connection,
        list_conversation_translation_models,
        start_conversation_card_translation,
        get_ai_execution_task,
        list_ai_execution_tasks,
        cancel_ai_execution_task,
        register_conversation_adapter,
        unregister_conversation_adapter,
        try_run_conversation_adapter,
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
        reveal_path
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

    fn open_test_database(db_path: &Path) -> crate::backend::store::Database {
        crate::backend::store::Database::open_initialized(db_path).expect("open initialized db")
    }

    fn upsert_test_source(db: &crate::backend::store::Database, source: &Source) {
        db.block_on(async move {
            crate::backend::store::upsert_source_sqlx(db.pool(), "default", source).await
        })
        .expect("insert source");
    }

    fn upsert_test_profile(db: &crate::backend::store::Database, profile: &TargetProfile) {
        db.block_on(async move {
            crate::backend::store::upsert_profile_sqlx(db.pool(), "default", profile).await
        })
        .expect("insert profile");
    }

    fn delete_test_profile(db: &crate::backend::store::Database, profile_id: &str) {
        db.block_on(async move {
            crate::backend::store::delete_profile_sqlx(db.pool(), "default", profile_id).await
        })
        .expect("delete profile");
    }

    fn replace_test_source_assets(
        db: &crate::backend::store::Database,
        source_id: &str,
        assets: &[Asset],
    ) {
        db.block_on(async move {
            crate::backend::store::replace_source_assets_sqlx(
                db.pool(),
                "default",
                source_id,
                assets,
            )
            .await
        })
        .expect("insert assets");
    }

    fn set_test_asset_mount(
        db: &crate::backend::store::Database,
        asset_id: &str,
        profile_id: &str,
        enabled: bool,
        strategy: DeploymentStrategy,
    ) -> AssetMount {
        db.block_on(async move {
            crate::backend::store::set_asset_mount_sqlx(
                db.pool(),
                "default",
                asset_id,
                profile_id,
                enabled,
                strategy,
            )
            .await
        })
        .expect("insert mount")
    }

    fn upsert_test_group(db: &crate::backend::store::Database, group: &AssetGroup) {
        db.block_on(async move {
            crate::backend::store::upsert_asset_group_sqlx(db.pool(), "default", group).await
        })
        .expect("insert group");
    }

    fn replace_test_group_members(
        db: &crate::backend::store::Database,
        group_id: &str,
        asset_ids: &[String],
        assets: &[Asset],
    ) {
        db.block_on(async move {
            crate::backend::store::replace_asset_group_members_sqlx(
                db.pool(),
                "default",
                group_id,
                asset_ids,
                assets,
            )
            .await
        })
        .expect("insert group members");
    }

    fn load_test_sources(db: &crate::backend::store::Database) -> Vec<Source> {
        db.block_on(
            async move { crate::backend::store::load_sources_sqlx(db.pool(), "default").await },
        )
        .expect("load sources")
    }

    fn load_test_profiles(db: &crate::backend::store::Database) -> Vec<TargetProfile> {
        db.block_on(
            async move { crate::backend::store::load_profiles_sqlx(db.pool(), "default").await },
        )
        .expect("load profiles")
    }

    fn load_test_assets(db: &crate::backend::store::Database) -> Vec<Asset> {
        db.block_on(async move {
            crate::backend::store::load_assets_sqlx(db.pool(), "default", None).await
        })
        .expect("load assets")
    }

    fn load_test_mounts(
        db: &crate::backend::store::Database,
        asset_id: Option<&str>,
    ) -> Vec<AssetMount> {
        db.block_on(async move {
            crate::backend::store::load_asset_mounts_sqlx(db.pool(), "default", asset_id).await
        })
        .expect("load mounts")
    }

    fn load_test_mount_observations(
        db: &crate::backend::store::Database,
    ) -> Vec<crate::backend::dto::AssetMountObservation> {
        db.block_on(async move {
            crate::backend::store::load_asset_mount_observations_sqlx(db.pool(), "default").await
        })
        .expect("load observations")
    }

    fn is_test_managed_deployment(
        db: &crate::backend::store::Database,
        profile_id: &str,
        asset_id: &str,
        target_path: &str,
    ) -> bool {
        db.block_on(async move {
            crate::backend::store::is_managed_deployment_sqlx(
                db.pool(),
                "default",
                profile_id,
                asset_id,
                target_path,
            )
            .await
        })
        .expect("deployment state")
    }

    #[test]
    fn refresh_recorded_assets_prunes_missing_sources() {
        let db_path = unique_temp_path("assetiweave-refresh-recorded");
        let database = open_test_database(&db_path);
        let source = test_missing_source("missing-recorded-source");
        upsert_test_source(&database, &source);

        refresh_recorded_assets(&database, "default").expect("refresh recorded assets");

        assert!(!load_test_sources(&database)
            .iter()
            .any(|candidate| candidate.id == source.id));
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn source_scan_prunes_missing_sources_without_error_row() {
        let db_path = unique_temp_path("assetiweave-scan-missing-source");
        let database = open_test_database(&db_path);
        let source = test_missing_source("missing-scan-source");
        upsert_test_source(&database, &source);

        scan_selected_sources(
            &database,
            "default",
            vec![source.clone()],
            crate::backend::scanner::scan_source,
        )
        .expect("scan selected sources");

        assert!(!load_test_sources(&database)
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

    #[test]
    fn target_profile_can_be_persisted_updated_and_deleted() {
        let db_path = unique_temp_path("assetiweave-profile-crud-db");
        let database = open_test_database(&db_path);
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

        upsert_test_profile(&database, &profile);
        profile.name = "Team App Edited".to_string();
        upsert_test_profile(&database, &profile);

        assert!(load_test_profiles(&database)
            .iter()
            .any(|candidate| candidate.id == profile.id && candidate.name == "Team App Edited"));

        ensure_profile_can_be_deleted_sqlx(&database, "default", &profile.id)
            .expect("profile delete guard");
        delete_test_profile(&database, &profile.id);
        assert!(!load_test_profiles(&database)
            .iter()
            .any(|candidate| candidate.id == profile.id));
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn default_app_profile_delete_is_blocked() {
        let db_path = unique_temp_path("assetiweave-default-profile-delete-db");
        let database = open_test_database(&db_path);

        let error = ensure_profile_can_be_deleted_sqlx(&database, "default", "codex")
            .expect_err("delete blocked");

        assert!(error.contains("default app cannot be deleted"));
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn target_profile_delete_is_blocked_when_mount_exists() {
        let db_path = unique_temp_path("assetiweave-profile-delete-block-db");
        let source_root = unique_temp_path("assetiweave-profile-delete-block-source");
        let target_root = unique_temp_path("assetiweave-profile-delete-block-target");
        let asset_path = source_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");

        let database = open_test_database(&db_path);
        let source = test_source("profile-delete-source", source_root.clone());
        let profile = test_profile("team-app", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, std::slice::from_ref(&asset));
        upsert_test_profile(&database, &profile);
        mount_asset_mount_record(&database, "default", &asset.id, &profile.id)
            .expect("mount asset");

        let error = ensure_profile_can_be_deleted_sqlx(&database, "default", &profile.id)
            .expect_err("delete blocked");

        assert!(error.contains("managed deployments") || error.contains("mounted assets"));
        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn refresh_recorded_assets_removes_mounts_for_deleted_assets() {
        let db_path = unique_temp_path("assetiweave-refresh-deleted-mount");
        let source_root = unique_temp_path("assetiweave-existing-source");
        std::fs::create_dir_all(&source_root).expect("create source root");
        let database = open_test_database(&db_path);
        let source = test_source("source-with-deleted-asset", source_root.clone());
        let asset = test_asset(&source, "deleted-asset", source_root.join("deleted-asset"));
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, std::slice::from_ref(&asset));
        set_test_asset_mount(
            &database,
            &asset.id,
            "codex",
            true,
            DeploymentStrategy::SymlinkToSource,
        );

        refresh_recorded_assets(&database, "default").expect("refresh recorded assets");

        assert!(load_test_assets(&database)
            .iter()
            .all(|candidate| candidate.id != asset.id));
        assert!(load_test_mounts(&database, Some(&asset.id)).is_empty());
        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn mount_asset_mount_creates_symlink_and_enables_mount() {
        let db_path = unique_temp_path("assetiweave-mount-db");
        let source_root = unique_temp_path("assetiweave-mount-source");
        let target_root = unique_temp_path("assetiweave-mount-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");

        let database = open_test_database(&db_path);
        let source = test_source("source-with-unmounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path.clone());
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, std::slice::from_ref(&asset));
        upsert_test_profile(&database, &profile);

        let result =
            mount_asset_mount_record(&database, "default", &asset.id, &profile.id).expect("mount");

        let metadata = std::fs::symlink_metadata(&target_path).expect("target metadata");
        assert!(metadata.file_type().is_symlink());
        assert_eq!(
            std::fs::read_link(&target_path).expect("read symlink"),
            asset_path.canonicalize().expect("canonical asset path")
        );
        assert!(result.mount.enabled);
        assert_eq!(result.status.state, PhysicalMountStateDto::Mounted);
        assert!(is_test_managed_deployment(
            &database,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        ));

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn mount_asset_mount_links_to_real_source_directory() {
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

        let database = open_test_database(&db_path);
        let source = test_source("source-with-aliased-asset", alias_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", alias_asset_path.clone());
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, std::slice::from_ref(&asset));
        upsert_test_profile(&database, &profile);

        let result =
            mount_asset_mount_record(&database, "default", &asset.id, &profile.id).expect("mount");

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
    #[test]
    fn set_asset_mount_creates_symlink_before_enabling_mount() {
        let db_path = unique_temp_path("assetiweave-set-mount-db");
        let source_root = unique_temp_path("assetiweave-set-mount-source");
        let target_root = unique_temp_path("assetiweave-set-mount-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");

        let database = open_test_database(&db_path);
        let source = test_source("source-with-set-mounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, std::slice::from_ref(&asset));
        upsert_test_profile(&database, &profile);
        let mount =
            set_asset_mount_record(&database, "default", &asset.id, &profile.id, true, None)
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
    #[test]
    fn apply_skill_group_mount_only_mounts_group_members() {
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

        let database = open_test_database(&db_path);
        let source = test_source("source-with-group-assets", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset_a = test_asset(&source, "skill-a", asset_path_a.clone());
        let asset_b = test_asset(&source, "skill-b", asset_path_b);
        let assets = vec![asset_a.clone(), asset_b.clone()];
        let group = test_group("frontend");
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, &assets);
        upsert_test_profile(&database, &profile);
        upsert_test_group(&database, &group);
        replace_test_group_members(&database, &group.id, &[asset_a.id.clone()], &assets);

        let result =
            apply_skill_group_mount_record(&database, "default", &group.id, &profile.id, true)
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
        assert!(load_test_mounts(&database, Some(&asset_b.id)).is_empty());

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn preview_exclusive_group_mount_uses_enabled_group_union_without_mutation() {
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

        let database = open_test_database(&db_path);
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
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, &skill_assets);
        upsert_test_profile(&database, &codex);
        upsert_test_profile(&database, &cursor);
        for group in [&group_a, &group_b, &disabled_group] {
            upsert_test_group(&database, group);
        }
        replace_test_group_members(
            &database,
            &group_a.id,
            &[asset_a.id.clone(), asset_b.id.clone()],
            &skill_assets,
        );
        replace_test_group_members(&database, &group_b.id, &[asset_b.id.clone()], &skill_assets);
        replace_test_group_members(
            &database,
            &disabled_group.id,
            &[asset_c.id.clone()],
            &skill_assets,
        );
        mount_asset_mount_record(&database, "default", &asset_a.id, &codex.id)
            .expect("mount skill a");
        mount_asset_mount_record(&database, "default", &asset_c.id, &codex.id)
            .expect("mount skill c");
        mount_asset_mount_record(&database, "default", &asset_c.id, &cursor.id)
            .expect("mount skill c cursor");

        let preview = build_skill_group_exclusive_mount_preview_sqlx(
            &database,
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
        assert!(load_test_mounts(&database, Some(&asset_c.id))
            .iter()
            .any(|mount| mount.profile_id == codex.id && mount.enabled));

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(codex_target).ok();
        std::fs::remove_dir_all(cursor_target).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn apply_exclusive_group_mount_only_changes_target_profile_skill_mounts() {
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

        let database = open_test_database(&db_path);
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
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, &all_assets);
        upsert_test_profile(&database, &codex);
        upsert_test_profile(&database, &cursor);
        for group in [&group_a, &group_b, &disabled_group] {
            upsert_test_group(&database, group);
        }
        replace_test_group_members(
            &database,
            &group_a.id,
            &[asset_a.id.clone(), asset_b.id.clone()],
            &skill_assets,
        );
        replace_test_group_members(&database, &group_b.id, &[asset_b.id.clone()], &skill_assets);
        replace_test_group_members(
            &database,
            &disabled_group.id,
            &[asset_c.id.clone()],
            &skill_assets,
        );
        mount_asset_mount_record(&database, "default", &asset_a.id, &codex.id)
            .expect("mount skill a");
        mount_asset_mount_record(&database, "default", &asset_c.id, &codex.id)
            .expect("mount skill c");
        mount_asset_mount_record(&database, "default", &asset_c.id, &cursor.id)
            .expect("mount skill c cursor");
        std::os::unix::fs::symlink(&prompt_path, &prompt_target).expect("create prompt symlink");
        set_test_asset_mount(
            &database,
            &prompt.id,
            &codex.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        );

        let result = apply_skill_group_exclusive_mount_record(
            &database,
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
        let skill_c_mounts = load_test_mounts(&database, Some(&asset_c.id));
        assert!(skill_c_mounts
            .iter()
            .any(|mount| mount.profile_id == codex.id && !mount.enabled));
        assert!(skill_c_mounts
            .iter()
            .any(|mount| mount.profile_id == cursor.id && mount.enabled));
        assert!(load_test_mounts(&database, Some(&prompt.id))
            .iter()
            .any(|mount| mount.profile_id == codex.id && mount.enabled));

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(codex_target).ok();
        std::fs::remove_dir_all(cursor_target).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn preview_exclusive_group_mount_reports_risks_without_forcing_repairs() {
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

        let database = open_test_database(&db_path);
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
        upsert_test_source(&database, &external_source);
        upsert_test_source(&database, &app_local_source);
        replace_test_source_assets(&database, &external_source.id, &[external_asset.clone()]);
        replace_test_source_assets(&database, &app_local_source.id, &[app_local_asset.clone()]);
        upsert_test_profile(&database, &profile);
        upsert_test_group(&database, &group);
        replace_test_group_members(&database, &group.id, &[app_local_asset.id.clone()], &assets);

        let result = apply_skill_group_exclusive_mount_record(
            &database,
            "default",
            &SkillGroupExclusiveMountInput {
                group_ids: vec![group.id.clone()],
                profile_id: profile.id.clone(),
                mount_selected: true,
                dry_run: false,
            },
        )
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
    #[test]
    fn scan_asset_mount_statuses_does_not_mutate_snapshot() {
        let db_path = unique_temp_path("assetiweave-status-scan-db");
        let source_root = unique_temp_path("assetiweave-status-scan-source");
        let target_root = unique_temp_path("assetiweave-status-scan-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");
        std::os::unix::fs::symlink(&asset_path, &target_path).expect("create physical symlink");

        let database = open_test_database(&db_path);
        let source = test_source("source-with-scanned-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, std::slice::from_ref(&asset));
        upsert_test_profile(&database, &profile);
        set_test_asset_mount(
            &database,
            &asset.id,
            &profile.id,
            false,
            DeploymentStrategy::SymlinkToSource,
        );

        let statuses =
            scan_asset_mount_statuses_sqlx(&database, "default", None).expect("scan statuses");

        assert!(statuses.iter().any(|status| {
            status.asset_id == asset.id
                && status.profile_id == profile.id
                && status.state == PhysicalMountStateDto::Mounted
        }));
        assert!(load_test_mounts(&database, Some(&asset.id))
            .iter()
            .all(|mount| !mount.enabled));
        assert!(!is_test_managed_deployment(
            &database,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        ));

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn sync_asset_mount_observations_records_physical_mount_snapshot() {
        let db_path = unique_temp_path("assetiweave-observation-db");
        let source_root = unique_temp_path("assetiweave-observation-source");
        let target_root = unique_temp_path("assetiweave-observation-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");
        std::os::unix::fs::symlink(&asset_path, &target_path).expect("create physical symlink");

        let database = open_test_database(&db_path);
        let source = test_source("source-with-observed-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, std::slice::from_ref(&asset));
        upsert_test_profile(&database, &profile);
        let original_mount = set_test_asset_mount(
            &database,
            &asset.id,
            &profile.id,
            false,
            DeploymentStrategy::SymlinkToSource,
        );

        sync_asset_mount_observations(&database, "default", None).expect("sync observations");

        let observations = load_test_mount_observations(&database);
        let observation = observations
            .iter()
            .find(|candidate| candidate.asset_id == asset.id && candidate.profile_id == profile.id)
            .expect("asset/profile observation");
        assert_eq!(observation.state, PhysicalMountStateDto::Mounted);
        assert!(!observation.observed_at.is_empty());
        let mounts = load_test_mounts(&database, Some(&asset.id));
        let synced_mount = mounts
            .iter()
            .find(|mount| mount.profile_id == profile.id)
            .expect("synced mount");
        assert!(synced_mount.enabled);
        assert_eq!(synced_mount.created_at, original_mount.created_at);
        assert!(is_test_managed_deployment(
            &database,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        ));

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn sync_asset_mount_observations_repairs_ghost_alias_symlink() {
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

        let database = open_test_database(&db_path);
        let source = test_source("source-with-ghost-asset", alias_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", alias_asset_path);
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, std::slice::from_ref(&asset));
        upsert_test_profile(&database, &profile);

        sync_asset_mount_observations(&database, "default", None).expect("sync observations");

        assert_eq!(
            std::fs::read_link(&target_path).expect("read repaired target symlink"),
            real_asset_path
                .canonicalize()
                .expect("canonical real asset")
        );
        let observations = load_test_mount_observations(&database);
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
    #[test]
    fn sync_asset_mount_observations_clears_snapshot_when_link_is_missing() {
        let db_path = unique_temp_path("assetiweave-observation-missing-db");
        let source_root = unique_temp_path("assetiweave-observation-missing-source");
        let target_root = unique_temp_path("assetiweave-observation-missing-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");

        let database = open_test_database(&db_path);
        let source = test_source("source-with-missing-observed-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, std::slice::from_ref(&asset));
        upsert_test_profile(&database, &profile);
        set_test_asset_mount(
            &database,
            &asset.id,
            &profile.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        );

        sync_asset_mount_observations(&database, "default", None).expect("sync observations");

        assert!(load_test_mounts(&database, Some(&asset.id))
            .iter()
            .all(|mount| !mount.enabled));
        assert!(!is_test_managed_deployment(
            &database,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        ));

        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_dir_all(target_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    #[cfg(unix)]
    #[test]
    fn unmount_asset_mount_removes_matching_symlink_and_disables_mount() {
        let db_path = unique_temp_path("assetiweave-unmount-db");
        let source_root = unique_temp_path("assetiweave-unmount-source");
        let target_root = unique_temp_path("assetiweave-unmount-target");
        let asset_path = source_root.join("skill-a");
        let target_path = target_root.join("skill-a");
        std::fs::create_dir_all(&asset_path).expect("create asset dir");
        std::fs::create_dir_all(&target_root).expect("create target dir");
        std::os::unix::fs::symlink(&asset_path, &target_path).expect("create mounted symlink");

        let database = open_test_database(&db_path);
        let source = test_source("source-with-mounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        upsert_test_source(&database, &source);
        replace_test_source_assets(&database, &source.id, std::slice::from_ref(&asset));
        upsert_test_profile(&database, &profile);
        set_test_asset_mount(
            &database,
            &asset.id,
            &profile.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        );

        let result = unmount_asset_mount_record(&database, "default", &asset.id, &profile.id)
            .expect("unmount");

        assert!(!target_path.exists());
        assert!(!std::fs::symlink_metadata(&target_path).is_ok());
        assert!(!result.mount.enabled);
        assert_eq!(result.status.state, PhysicalMountStateDto::NotMounted);
        assert!(load_test_mounts(&database, Some(&asset.id))
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
