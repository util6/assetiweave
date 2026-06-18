use crate::adapters::app_state::AppState;
use crate::adapters::tauri::background_tasks::{
    ConversationSyncTaskSnapshot, SkillBackupTaskSnapshot,
};
#[cfg(test)]
use crate::backend::capabilities::{
    apply_skill_group_exclusive_mount_record, apply_skill_group_mount_record,
    assetiweave_library_source_with_root, build_catalog_assets,
    build_skill_group_exclusive_mount_preview, ensure_profile_can_be_deleted, exclusive_item,
    mount_asset_mount_record, refresh_recorded_assets, scan_asset_mount_statuses,
    scan_selected_sources, set_asset_mount_record, sync_asset_mount_observations,
    target_profile_from_input, unmount_asset_mount_record,
};
use crate::{
    backend::application::{
        AppService, ConversationAdapterUnregisterParams, ConversationQuestionGetParams,
        ConversationQuestionListParams, ConversationQuestionMergeParams,
        ConversationQuestionSplitParams, ConversationSearchParams, ConversationSearchResult,
        ConversationSessionExportParams, ConversationSessionGetParams,
        ConversationSessionListParams, ConversationSourceDisableParams,
        ConversationSourceUpsertParams, ConversationSyncParams, ListAssetsParams,
        SkillAcquireParams, SkillRemoteCheckParams, SkillSearchParams, SkillSearchResult,
        SourceRemoveParams, SourceScanParams, UpdateSkillBackupSettingsParams,
    },
    backend::conversations::{
        ExternalAdapterRegisterParams, ExternalAdapterScaffoldParams, ExternalAdapterTryRunParams,
        ExternalAdapterValidateParams,
    },
    backend::dto::{
        AppOverview, AppResult, AppShortcut, ApplyAssetGroupMountResult,
        ApplySkillGroupExclusiveMountResult, AssetGroupInput, AssetMountStatus,
        AssetMountUpdateResult, CatalogAsset, ExecutionResult, NavigationModel,
        SkillBackupSettings, SkillGroupExclusiveMountInput, SkillGroupExclusiveMountPreview,
        SkillRemoteSource, SourceInput, TargetProfileInput,
    },
    backend::models::{
        Asset, AssetGroup, AssetGroupDetail, AssetKind, AssetMount, ConversationAdapter,
        ConversationSource, DeploymentPlan, DeploymentStrategy, Source, TargetProfile,
    },
    backend::operation_log::{
        asset_log_fields, log_error, log_info, log_warn, profile_log_fields,
        source_input_log_fields, source_log_fields, status_summary_fields,
    },
};
use serde_json::Value;
use std::collections::BTreeMap;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub(crate) fn get_app_overview(state: State<'_, AppState>) -> AppResult<AppOverview> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.overview()
}

#[tauri::command]
pub(crate) fn get_app_settings(
    state: State<'_, AppState>,
) -> AppResult<crate::backend::app_settings::AppSettingsFile> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.get_app_settings()
}

#[tauri::command]
pub(crate) fn save_app_settings(
    state: State<'_, AppState>,
    settings: serde_json::Value,
) -> AppResult<crate::backend::app_settings::AppSettingsFile> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.save_app_settings(settings)
}

#[tauri::command]
pub(crate) fn list_assets(
    state: State<'_, AppState>,
    kind: Option<AssetKind>,
) -> AppResult<Vec<CatalogAsset>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_assets(ListAssetsParams { kind })
}

#[tauri::command]
pub(crate) fn get_skill_backup_settings(
    state: State<'_, AppState>,
) -> AppResult<SkillBackupSettings> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.get_skill_backup_settings()
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
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.update_skill_backup_settings(
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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.backup_skill(asset_id)
    })();

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

    let db_path = state.db_path.clone();
    let background_tasks = state.background_tasks.clone();
    let task_id = snapshot.id.clone();
    let task_asset_ids = snapshot.asset_ids.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let progress_app = app.clone();
        let progress_tasks = background_tasks.clone();
        let progress_task_id = task_id.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            AppService::open_with_db_path(db_path).and_then(|service| {
                service.backup_skills_with_progress(
                    task_asset_ids,
                    |completed_count, next_asset_id| match progress_tasks
                        .update_skill_backup_progress(
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
            })
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

#[tauri::command]
pub(crate) fn search_skills(
    state: State<'_, AppState>,
    params: SkillSearchParams,
) -> AppResult<SkillSearchResult> {
    let fields = vec![("query", params.query.clone())];
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.search_skills(params)
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.acquire_skill(params)
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.list_skill_remote_sources()
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.check_skill_remote_sources(params)
    })();

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
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?
            .update_asset_description(asset_id, description)
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
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?
            .delete_asset(asset_id, unmount.unwrap_or(false))
    })();

    match &result {
        Ok(asset) => log_info("asset.delete", "删除资产成功", &asset_log_fields(asset)),
        Err(error) => log_error("asset.delete", "删除资产失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn list_sources(state: State<'_, AppState>) -> AppResult<Vec<Source>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_sources()
}

#[tauri::command]
pub(crate) fn list_skill_sources(state: State<'_, AppState>) -> AppResult<Vec<Source>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_skill_sources()
}

#[tauri::command]
pub(crate) fn create_source(state: State<'_, AppState>, source: SourceInput) -> AppResult<Source> {
    let input_fields = source_input_log_fields(&source);
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.add_source(source)
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.update_source(source)
    })();

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
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?
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
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_profiles()
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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.create_profile(input)
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.update_profile(profile)
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.delete_profile(id)
    })();

    match &result {
        Ok(()) => log_info("profile.delete", "删除目标 APP 配置成功", &fields),
        Err(error) => log_error("profile.delete", "删除目标 APP 配置失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn get_navigation_model(state: State<'_, AppState>) -> AppResult<NavigationModel> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.navigation_model()
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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.update_navigation_model(model)
    })();

    match &result {
        Ok(_) => log_info("navigation.update", "更新导航配置成功", &fields),
        Err(error) => log_error("navigation.update", "更新导航配置失败", error, &fields),
    }
    result
}

#[tauri::command]
pub(crate) fn list_app_shortcuts(state: State<'_, AppState>) -> AppResult<Vec<AppShortcut>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_app_shortcuts()
}

#[tauri::command]
pub(crate) fn list_app_shortcut_settings(
    state: State<'_, AppState>,
) -> AppResult<Vec<AppShortcut>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_app_shortcut_settings()
}

#[tauri::command]
pub(crate) fn update_app_shortcuts(
    state: State<'_, AppState>,
    shortcuts: Vec<AppShortcut>,
) -> AppResult<Vec<AppShortcut>> {
    let fields = vec![("shortcut_count", shortcuts.len().to_string())];
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.update_app_shortcuts(shortcuts)
    })();

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
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_asset_mounts(asset_id.as_deref())
}

#[tauri::command]
pub(crate) fn list_asset_mount_statuses(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> AppResult<Vec<AssetMountStatus>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?
        .list_asset_mount_statuses(asset_id.as_deref())
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
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?
            .refresh_asset_mount_statuses(asset_id.as_deref())
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
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_skill_groups()
}

#[tauri::command]
pub(crate) fn create_skill_group(
    state: State<'_, AppState>,
    input: AssetGroupInput,
) -> AppResult<AssetGroupDetail> {
    let input_fields = vec![("group_name", input.name.clone())];
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.create_skill_group(input)
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.update_skill_group(group)
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.delete_skill_group(group_id)
    })();

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
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?
            .set_skill_group_manual_members(group_id, asset_ids)
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
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.apply_skill_group_mount(
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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?
            .preview_skill_group_exclusive_mount(input)
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?
            .apply_skill_group_exclusive_mount(input)
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?
            .toggle_asset_mount(&asset_id, &profile_id)
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?
            .unmount_asset_by_id(&asset_id, &profile_id)
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?
            .mount_asset_by_id(&asset_id, &profile_id)
    })();

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
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.set_asset_mount(
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
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.scan_sources(SourceScanParams {
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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.scan_skill_sources()
    })();

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
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_conversation_adapters()
}

#[tauri::command]
pub(crate) fn scaffold_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterScaffoldParams,
) -> AppResult<crate::backend::conversations::ExternalAdapterScaffoldResult> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.scaffold_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn validate_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterValidateParams,
) -> AppResult<crate::backend::conversations::ExternalAdapterValidationResult> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.validate_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn register_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterRegisterParams,
) -> AppResult<serde_json::Value> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.register_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn unregister_conversation_adapter(
    state: State<'_, AppState>,
    params: ConversationAdapterUnregisterParams,
) -> AppResult<serde_json::Value> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.unregister_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn try_run_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterTryRunParams,
) -> AppResult<crate::backend::conversations::ExternalAdapterRunResult> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.try_run_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn list_conversation_sources(
    state: State<'_, AppState>,
) -> AppResult<Vec<ConversationSource>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_conversation_sources()
}

#[tauri::command]
pub(crate) fn upsert_conversation_source(
    state: State<'_, AppState>,
    params: ConversationSourceUpsertParams,
) -> AppResult<serde_json::Value> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.upsert_conversation_source(params)
}

#[tauri::command]
pub(crate) fn disable_conversation_source(
    state: State<'_, AppState>,
    params: ConversationSourceDisableParams,
) -> AppResult<serde_json::Value> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.disable_conversation_source(params)
}

#[tauri::command]
pub(crate) fn sync_conversations(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ConversationSyncParams,
) -> AppResult<ConversationSyncTaskSnapshot> {
    let (snapshot, should_start) = state.background_tasks.begin_conversation_sync(&params)?;
    if !should_start {
        return Ok(snapshot);
    }

    let db_path = state.db_path.clone();
    let background_tasks = state.background_tasks.clone();
    let task_id = snapshot.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            AppService::open_with_db_path(db_path)
                .and_then(|service| service.sync_conversations(params))
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
pub(crate) fn list_conversation_sessions(
    state: State<'_, AppState>,
    params: ConversationSessionListParams,
) -> AppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_conversation_sessions(params)
}

#[tauri::command]
pub(crate) fn get_conversation_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> AppResult<crate::backend::dto::ConversationSessionDetail> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.get_conversation_session(params)
}

#[tauri::command]
pub(crate) fn export_conversation_session(
    state: State<'_, AppState>,
    params: ConversationSessionExportParams,
) -> AppResult<serde_json::Value> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.export_conversation_session(params)
}

#[tauri::command]
pub(crate) fn list_web_record_sessions(
    state: State<'_, AppState>,
    params: ConversationSessionListParams,
) -> AppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_web_record_sessions(params)
}

#[tauri::command]
pub(crate) fn get_web_record_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> AppResult<crate::backend::dto::ConversationSessionDetail> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.get_web_record_session(params)
}

#[tauri::command]
pub(crate) fn search_conversation_records(
    state: State<'_, AppState>,
    params: ConversationSearchParams,
) -> AppResult<ConversationSearchResult> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.search_conversation_records(params)
}

#[tauri::command]
pub(crate) fn export_web_record_session(
    state: State<'_, AppState>,
    params: ConversationSessionExportParams,
) -> AppResult<serde_json::Value> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.export_web_record_session(params)
}

#[tauri::command]
pub(crate) fn list_conversation_questions(
    state: State<'_, AppState>,
    params: ConversationQuestionListParams,
) -> AppResult<Vec<crate::backend::dto::ConversationQuestionDetail>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_conversation_questions(params)
}

#[tauri::command]
pub(crate) fn get_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionGetParams,
) -> AppResult<crate::backend::dto::ConversationQuestionDetail> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.get_conversation_question(params)
}

#[tauri::command]
pub(crate) fn merge_conversation_questions(
    state: State<'_, AppState>,
    params: ConversationQuestionMergeParams,
) -> AppResult<crate::backend::dto::ConversationMutationResult> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.merge_conversation_questions(params)
}

#[tauri::command]
pub(crate) fn split_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionSplitParams,
) -> AppResult<crate::backend::dto::ConversationMutationResult> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.split_conversation_question(params)
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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.create_plan(profile_id.as_deref())
    })();

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
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(state.db_path.clone())?.execute_plan(plan, action_ids)
    })();

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

pub(crate) fn command_handler(
) -> impl Fn(::tauri::ipc::Invoke<::tauri::Wry>) -> bool + Send + Sync + 'static {
    ::tauri::generate_handler![
        get_app_overview,
        get_app_settings,
        save_app_settings,
        list_assets,
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
        register_conversation_adapter,
        unregister_conversation_adapter,
        try_run_conversation_adapter,
        list_conversation_sources,
        upsert_conversation_source,
        disable_conversation_source,
        sync_conversations,
        get_conversation_sync_task,
        list_conversation_sessions,
        get_conversation_session,
        export_conversation_session,
        list_web_record_sessions,
        get_web_record_session,
        search_conversation_records,
        export_web_record_session,
        list_conversation_questions,
        get_conversation_question,
        merge_conversation_questions,
        split_conversation_question,
        create_plan,
        execute_plan,
        logs_get_snapshot,
        logs_open_log_directory,
        logs_write_operation,
        reveal_path
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::dto::{PhysicalMountStateDto, SkillBackupState};
    use crate::backend::models::{
        AppKind, AssetFormat, AssetGroup, AssetGroupRules, AssetKind, DeploymentStrategy,
        ProfileSafety, RuleSet, SourceKind, SourceOrigin, SourceScannerKind,
    };
    use std::{
        path::{Path, PathBuf},
        process::Command,
    };
    use uuid::Uuid;

    #[test]
    fn refresh_recorded_assets_prunes_missing_sources() {
        let db_path = unique_temp_path("assetiweave-refresh-recorded");
        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_missing_source("missing-recorded-source");
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        let database =
            crate::backend::store::Database::open(&db_path).expect("open migrated database");

        refresh_recorded_assets(&conn, &database).expect("refresh recorded assets");

        assert!(!crate::backend::store::load_sources(&conn)
            .expect("load sources")
            .iter()
            .any(|candidate| candidate.id == source.id));
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn source_scan_prunes_missing_sources_without_error_row() {
        let db_path = unique_temp_path("assetiweave-scan-missing-source");
        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_missing_source("missing-scan-source");
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        let database =
            crate::backend::store::Database::open(&db_path).expect("open migrated database");

        scan_selected_sources(
            &conn,
            &database,
            vec![source.clone()],
            crate::backend::scanner::scan_source,
        )
        .expect("scan selected sources");

        assert!(!crate::backend::store::load_sources(&conn)
            .expect("load sources")
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
        assert_eq!(profile.app_kind, AppKind::Custom);
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
        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let mut profile = target_profile_from_input(TargetProfileInput {
            id: Some("team-app".to_string()),
            name: "Team App".to_string(),
            app_kind: Some(AppKind::Custom),
            target_paths: Some(vec!["~/team-app/skills".to_string()]),
            supported_kinds: None,
            deployment_strategy: None,
            enabled: Some(true),
            include: None,
            exclude: None,
            safety: None,
        })
        .expect("build profile");

        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");
        profile.name = "Team App Edited".to_string();
        crate::backend::store::upsert_profile(&conn, &profile).expect("update profile");

        assert!(crate::backend::store::load_profiles(&conn)
            .expect("load profiles")
            .iter()
            .any(|candidate| candidate.id == profile.id && candidate.name == "Team App Edited"));

        ensure_profile_can_be_deleted(&conn, &profile.id).expect("profile delete guard");
        crate::backend::store::delete_profile(&conn, &profile.id).expect("delete profile");
        assert!(!crate::backend::store::load_profiles(&conn)
            .expect("load profiles")
            .iter()
            .any(|candidate| candidate.id == profile.id));
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn default_app_profile_delete_is_blocked() {
        let db_path = unique_temp_path("assetiweave-default-profile-delete-db");
        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");

        let error = ensure_profile_can_be_deleted(&conn, "codex").expect_err("delete blocked");

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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("profile-delete-source", source_root.clone());
        let profile = test_profile("team-app", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(
            &conn,
            &source.id,
            std::slice::from_ref(&asset),
        )
        .expect("insert asset");
        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");
        mount_asset_mount_record(&conn, &asset.id, &profile.id).expect("mount asset");

        let error = ensure_profile_can_be_deleted(&conn, &profile.id).expect_err("delete blocked");

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
        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-deleted-asset", source_root.clone());
        let asset = test_asset(&source, "deleted-asset", source_root.join("deleted-asset"));
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(
            &conn,
            &source.id,
            std::slice::from_ref(&asset),
        )
        .expect("insert asset");
        crate::backend::store::set_asset_mount(
            &conn,
            &asset.id,
            "codex",
            true,
            DeploymentStrategy::SymlinkToSource,
        )
        .expect("insert mount");
        let database =
            crate::backend::store::Database::open(&db_path).expect("open migrated database");

        refresh_recorded_assets(&conn, &database).expect("refresh recorded assets");

        assert!(crate::backend::store::load_assets(&conn)
            .expect("load assets")
            .iter()
            .all(|candidate| candidate.id != asset.id));
        assert!(
            crate::backend::store::load_asset_mounts(&conn, Some(&asset.id))
                .expect("load mounts")
                .is_empty()
        );
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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-unmounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path.clone());
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(
            &conn,
            &source.id,
            std::slice::from_ref(&asset),
        )
        .expect("insert asset");
        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");

        let result = mount_asset_mount_record(&conn, &asset.id, &profile.id).expect("mount");

        let metadata = std::fs::symlink_metadata(&target_path).expect("target metadata");
        assert!(metadata.file_type().is_symlink());
        assert_eq!(
            std::fs::read_link(&target_path).expect("read symlink"),
            asset_path.canonicalize().expect("canonical asset path")
        );
        assert!(result.mount.enabled);
        assert_eq!(result.status.state, PhysicalMountStateDto::Mounted);
        assert!(crate::backend::store::is_managed_deployment(
            &conn,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        )
        .expect("deployment state"));

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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-aliased-asset", alias_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", alias_asset_path.clone());
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(
            &conn,
            &source.id,
            std::slice::from_ref(&asset),
        )
        .expect("insert asset");
        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");

        let result = mount_asset_mount_record(&conn, &asset.id, &profile.id).expect("mount");

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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-set-mounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(
            &conn,
            &source.id,
            std::slice::from_ref(&asset),
        )
        .expect("insert asset");
        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");

        let database =
            crate::backend::store::Database::open(&db_path).expect("open migrated database");
        let mount = set_asset_mount_record(&conn, &database, &asset.id, &profile.id, true, None)
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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-group-assets", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset_a = test_asset(&source, "skill-a", asset_path_a.clone());
        let asset_b = test_asset(&source, "skill-b", asset_path_b);
        let assets = vec![asset_a.clone(), asset_b.clone()];
        let group = test_group("frontend");
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(&conn, &source.id, &assets)
            .expect("insert assets");
        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");
        crate::backend::store::upsert_asset_group(&conn, &group).expect("insert group");
        crate::backend::store::replace_asset_group_members(
            &conn,
            &group.id,
            &[asset_a.id.clone()],
            &assets,
        )
        .expect("insert group members");

        let result = apply_skill_group_mount_record(&conn, &group.id, &profile.id, true)
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
        assert!(
            crate::backend::store::load_asset_mounts(&conn, Some(&asset_b.id))
                .expect("load unrelated mounts")
                .is_empty()
        );

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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
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
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(&conn, &source.id, &skill_assets)
            .expect("insert assets");
        crate::backend::store::upsert_profile(&conn, &codex).expect("insert codex profile");
        crate::backend::store::upsert_profile(&conn, &cursor).expect("insert cursor profile");
        for group in [&group_a, &group_b, &disabled_group] {
            crate::backend::store::upsert_asset_group(&conn, group).expect("insert group");
        }
        crate::backend::store::replace_asset_group_members(
            &conn,
            &group_a.id,
            &[asset_a.id.clone(), asset_b.id.clone()],
            &skill_assets,
        )
        .expect("insert group a members");
        crate::backend::store::replace_asset_group_members(
            &conn,
            &group_b.id,
            &[asset_b.id.clone()],
            &skill_assets,
        )
        .expect("insert group b members");
        crate::backend::store::replace_asset_group_members(
            &conn,
            &disabled_group.id,
            &[asset_c.id.clone()],
            &skill_assets,
        )
        .expect("insert disabled group members");
        mount_asset_mount_record(&conn, &asset_a.id, &codex.id).expect("mount skill a");
        mount_asset_mount_record(&conn, &asset_c.id, &codex.id).expect("mount skill c");
        mount_asset_mount_record(&conn, &asset_c.id, &cursor.id).expect("mount skill c cursor");

        let preview = build_skill_group_exclusive_mount_preview(
            &conn,
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
        assert!(
            crate::backend::store::load_asset_mounts(&conn, Some(&asset_c.id))
                .expect("load skill c mounts")
                .iter()
                .any(|mount| mount.profile_id == codex.id && mount.enabled)
        );

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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
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
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(&conn, &source.id, &all_assets)
            .expect("insert assets");
        crate::backend::store::upsert_profile(&conn, &codex).expect("insert codex profile");
        crate::backend::store::upsert_profile(&conn, &cursor).expect("insert cursor profile");
        for group in [&group_a, &group_b, &disabled_group] {
            crate::backend::store::upsert_asset_group(&conn, group).expect("insert group");
        }
        crate::backend::store::replace_asset_group_members(
            &conn,
            &group_a.id,
            &[asset_a.id.clone(), asset_b.id.clone()],
            &skill_assets,
        )
        .expect("insert group a members");
        crate::backend::store::replace_asset_group_members(
            &conn,
            &group_b.id,
            &[asset_b.id.clone()],
            &skill_assets,
        )
        .expect("insert group b members");
        crate::backend::store::replace_asset_group_members(
            &conn,
            &disabled_group.id,
            &[asset_c.id.clone()],
            &skill_assets,
        )
        .expect("insert disabled group members");
        mount_asset_mount_record(&conn, &asset_a.id, &codex.id).expect("mount skill a");
        mount_asset_mount_record(&conn, &asset_c.id, &codex.id).expect("mount skill c");
        mount_asset_mount_record(&conn, &asset_c.id, &cursor.id).expect("mount skill c cursor");
        std::os::unix::fs::symlink(&prompt_path, &prompt_target).expect("create prompt symlink");
        crate::backend::store::set_asset_mount(
            &conn,
            &prompt.id,
            &codex.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        )
        .expect("store prompt mount");

        let result = apply_skill_group_exclusive_mount_record(
            &conn,
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
        let skill_c_mounts = crate::backend::store::load_asset_mounts(&conn, Some(&asset_c.id))
            .expect("load skill c mounts");
        assert!(skill_c_mounts
            .iter()
            .any(|mount| mount.profile_id == codex.id && !mount.enabled));
        assert!(skill_c_mounts
            .iter()
            .any(|mount| mount.profile_id == cursor.id && mount.enabled));
        assert!(
            crate::backend::store::load_asset_mounts(&conn, Some(&prompt.id))
                .expect("load prompt mounts")
                .iter()
                .any(|mount| mount.profile_id == codex.id && mount.enabled)
        );

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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
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
        crate::backend::store::upsert_source(&conn, &external_source)
            .expect("insert external source");
        crate::backend::store::upsert_source(&conn, &app_local_source)
            .expect("insert app local source");
        crate::backend::store::replace_source_assets(
            &conn,
            &external_source.id,
            &[external_asset.clone()],
        )
        .expect("insert external asset");
        crate::backend::store::replace_source_assets(
            &conn,
            &app_local_source.id,
            &[app_local_asset.clone()],
        )
        .expect("insert app local asset");
        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");
        crate::backend::store::upsert_asset_group(&conn, &group).expect("insert group");
        crate::backend::store::replace_asset_group_members(
            &conn,
            &group.id,
            &[app_local_asset.id.clone()],
            &assets,
        )
        .expect("insert group members");

        let result = apply_skill_group_exclusive_mount_record(
            &conn,
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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-scanned-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(
            &conn,
            &source.id,
            std::slice::from_ref(&asset),
        )
        .expect("insert asset");
        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");
        crate::backend::store::set_asset_mount(
            &conn,
            &asset.id,
            &profile.id,
            false,
            DeploymentStrategy::SymlinkToSource,
        )
        .expect("store disabled snapshot");

        let statuses = scan_asset_mount_statuses(&conn, None).expect("scan statuses");

        assert!(statuses.iter().any(|status| {
            status.asset_id == asset.id
                && status.profile_id == profile.id
                && status.state == PhysicalMountStateDto::Mounted
        }));
        assert!(
            crate::backend::store::load_asset_mounts(&conn, Some(&asset.id))
                .expect("load mounts")
                .iter()
                .all(|mount| !mount.enabled)
        );
        assert!(!crate::backend::store::is_managed_deployment(
            &conn,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        )
        .expect("deployment state"));

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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-observed-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(
            &conn,
            &source.id,
            std::slice::from_ref(&asset),
        )
        .expect("insert asset");
        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");
        crate::backend::store::set_asset_mount(
            &conn,
            &asset.id,
            &profile.id,
            false,
            DeploymentStrategy::SymlinkToSource,
        )
        .expect("store disabled intent");

        sync_asset_mount_observations(&conn, None).expect("sync observations");

        let observations =
            crate::backend::store::load_asset_mount_observations(&conn).expect("load observations");
        let observation = observations
            .iter()
            .find(|candidate| candidate.asset_id == asset.id && candidate.profile_id == profile.id)
            .expect("asset/profile observation");
        assert_eq!(observation.state, PhysicalMountStateDto::Mounted);
        assert!(!observation.observed_at.is_empty());
        let mounts =
            crate::backend::store::load_asset_mounts(&conn, Some(&asset.id)).expect("load mounts");
        assert!(mounts
            .iter()
            .any(|mount| mount.profile_id == profile.id && mount.enabled));
        assert!(crate::backend::store::is_managed_deployment(
            &conn,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        )
        .expect("deployment state"));

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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-ghost-asset", alias_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", alias_asset_path);
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(
            &conn,
            &source.id,
            std::slice::from_ref(&asset),
        )
        .expect("insert asset");
        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");

        sync_asset_mount_observations(&conn, None).expect("sync observations");

        assert_eq!(
            std::fs::read_link(&target_path).expect("read repaired target symlink"),
            real_asset_path
                .canonicalize()
                .expect("canonical real asset")
        );
        let observations =
            crate::backend::store::load_asset_mount_observations(&conn).expect("load observations");
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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-missing-observed-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        crate::backend::store::upsert_source(&conn, &source).expect("insert source");
        crate::backend::store::replace_source_assets(
            &conn,
            &source.id,
            std::slice::from_ref(&asset),
        )
        .expect("insert asset");
        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");
        crate::backend::store::set_asset_mount(
            &conn,
            &asset.id,
            &profile.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        )
        .expect("store stale enabled snapshot");

        sync_asset_mount_observations(&conn, None).expect("sync observations");

        assert!(
            crate::backend::store::load_asset_mounts(&conn, Some(&asset.id))
                .expect("load mounts")
                .iter()
                .all(|mount| !mount.enabled)
        );
        assert!(!crate::backend::store::is_managed_deployment(
            &conn,
            &profile.id,
            &asset.id,
            &target_path.to_string_lossy()
        )
        .expect("deployment state"));

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

        let conn = crate::backend::store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-mounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        crate::backend::store::replace_source_assets(
            &conn,
            &source.id,
            std::slice::from_ref(&asset),
        )
        .expect("insert asset");
        crate::backend::store::upsert_profile(&conn, &profile).expect("insert profile");
        crate::backend::store::set_asset_mount(
            &conn,
            &asset.id,
            &profile.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        )
        .expect("enable mount");

        let result = unmount_asset_mount_record(&conn, &asset.id, &profile.id).expect("unmount");

        assert!(!target_path.exists());
        assert!(!std::fs::symlink_metadata(&target_path).is_ok());
        assert!(!result.mount.enabled);
        assert_eq!(result.status.state, PhysicalMountStateDto::NotMounted);
        assert!(
            crate::backend::store::load_asset_mounts(&conn, Some(&asset.id))
                .expect("load mounts")
                .iter()
                .all(|mount| !mount.enabled)
        );

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
            app_kind: AppKind::Custom,
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
