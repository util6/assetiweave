use crate::models::{
    AppKind, Asset, AssetGroup, AssetGroupDetail, AssetGroupRules, AssetKind, AssetMount,
    ConversationAdapter, ConversationSource, DeploymentPlan, DeploymentState, DeploymentStrategy,
    ProfileSafety, RuleSet, Source, SourceKind, SourceOrigin, SourceScannerKind, TargetProfile,
};
use crate::{
    conversations::{
        ExternalAdapterRegisterParams, ExternalAdapterScaffoldParams, ExternalAdapterTryRunParams,
        ExternalAdapterValidateParams,
    },
    defaults, executor, logs,
    path_utils::{
        default_skill_backup_root, expand_path, find_git_root, git_browser_url,
        git_repository_for_path,
    },
    planner, platform, scanner,
    service::{
        AppService, ConversationAdapterUnregisterParams, ConversationQuestionGetParams,
        ConversationQuestionListParams, ConversationQuestionMergeParams,
        ConversationQuestionSplitParams, ConversationSessionExportParams,
        ConversationSessionGetParams, ConversationSessionListParams,
        ConversationSourceDisableParams, ConversationSourceUpsertParams, ConversationSyncParams,
        ListAssetsParams, SkillAcquireParams, SkillRemoteCheckParams, SkillSearchParams,
        SkillSearchResult, SourceRemoveParams, SourceScanParams, UpdateSkillBackupSettingsParams,
    },
    store, targeting,
    types::{
        AppOverview, AppResult, AppShortcut, AppState, ApplyAssetGroupMountResult,
        ApplySkillGroupExclusiveMountResult, AssetGroupInput, AssetGroupMountError,
        AssetMountObservation, AssetMountStatus, AssetMountUpdateResult, CatalogAsset,
        ExecutionResult, NavigationModel, PhysicalMountStateDto, SkillBackupAssetStatus,
        SkillBackupSettings, SkillBackupState, SkillGroupExclusiveMountError,
        SkillGroupExclusiveMountInput, SkillGroupExclusiveMountItem,
        SkillGroupExclusiveMountPreview, SkillGroupExclusiveMountSkippedItem, SkillRemoteSource,
        SourceInput, TargetProfileInput,
    },
};
use chrono::Utc;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};
use tauri::State;
use uuid::Uuid;
use walkdir::WalkDir;

type LogField = (&'static str, String);

pub(crate) const SKILL_BACKUP_SOURCE_ID: &str = "assetiweave-library-skills";

fn log_info(operation: &str, message: &str, fields: &[LogField]) {
    logs::record_info(operation, message, fields);
}

fn log_warn(operation: &str, message: &str, fields: &[LogField]) {
    logs::record_warn(operation, message, fields);
}

fn log_error<E: std::fmt::Display + ?Sized>(
    operation: &str,
    message: &str,
    error: &E,
    fields: &[LogField],
) {
    let mut fields = fields.to_vec();
    fields.push(("error", error.to_string()));
    logs::record_error(operation, message, &fields);
}

fn source_input_log_fields(source: &SourceInput) -> Vec<LogField> {
    let mut fields = vec![
        ("name", source.name.clone()),
        ("root_path", source.root_path.clone()),
        ("kind", format!("{:?}", source.kind)),
        (
            "scanner_kind",
            source
                .scanner_kind
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|| "Mixed".to_string()),
        ),
        (
            "source_origin",
            source
                .source_origin
                .map(|origin| format!("{origin:?}"))
                .unwrap_or_else(|| "LocalFolder".to_string()),
        ),
        ("enabled", source.enabled.to_string()),
        ("priority", source.priority.to_string()),
    ];
    if let Some(id) = &source.id {
        fields.push(("source_id", id.clone()));
    }
    if let Some(origin_app_kind) = source.origin_app_kind {
        fields.push(("origin_app_kind", format!("{origin_app_kind:?}")));
    }
    fields
}

fn source_log_fields(source: &Source) -> Vec<LogField> {
    let mut fields = vec![
        ("source_id", source.id.clone()),
        ("name", source.name.clone()),
        ("root_path", source.root_path.clone()),
        ("scanner_kind", format!("{:?}", source.scanner_kind)),
        ("source_origin", format!("{:?}", source.source_origin)),
        ("enabled", source.enabled.to_string()),
        ("priority", source.priority.to_string()),
    ];
    if let Some(origin_app_kind) = source.origin_app_kind {
        fields.push(("origin_app_kind", format!("{origin_app_kind:?}")));
    }
    if let Some(status) = &source.last_scan_status {
        fields.push(("last_scan_status", status.clone()));
    }
    fields
}

fn asset_log_fields(asset: &Asset) -> Vec<LogField> {
    vec![
        ("asset_id", asset.id.clone()),
        ("skill_name", asset.name.clone()),
        ("source_id", asset.source_id.clone()),
        ("asset_kind", format!("{:?}", asset.kind)),
        ("relative_path", asset.relative_path.clone()),
        ("absolute_path", asset.absolute_path.clone()),
    ]
}

fn profile_log_fields(profile: &TargetProfile) -> Vec<LogField> {
    vec![
        ("profile_id", profile.id.clone()),
        ("profile_name", profile.name.clone()),
        ("app_kind", format!("{:?}", profile.app_kind)),
        ("enabled", profile.enabled.to_string()),
        (
            "deployment_strategy",
            format!("{:?}", profile.deployment_strategy),
        ),
        ("target_paths", profile.target_paths.join(",")),
    ]
}

fn mount_log_fields(
    conn: &rusqlite::Connection,
    asset_id: &str,
    profile_id: &str,
) -> Vec<LogField> {
    if let Ok((asset, profile)) = load_mount_asset_and_profile(conn, asset_id, profile_id) {
        let mut fields = asset_log_fields(&asset);
        fields.extend(profile_log_fields(&profile));
        return fields;
    }

    vec![
        ("asset_id", asset_id.to_string()),
        ("profile_id", profile_id.to_string()),
    ]
}

fn status_summary_fields(statuses: &[AssetMountStatus]) -> Vec<LogField> {
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

    vec![
        ("count", statuses.len().to_string()),
        ("mounted", mounted.to_string()),
        ("issues", issues.to_string()),
    ]
}

#[tauri::command]
pub(crate) fn get_app_overview(state: State<'_, AppState>) -> AppResult<AppOverview> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.overview()
}

#[tauri::command]
pub(crate) fn get_app_settings(
    state: State<'_, AppState>,
) -> AppResult<crate::app_settings::AppSettingsFile> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.get_app_settings()
}

#[tauri::command]
pub(crate) fn save_app_settings(
    state: State<'_, AppState>,
    settings: serde_json::Value,
) -> AppResult<crate::app_settings::AppSettingsFile> {
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
    let conn = store::open_initialized(&state.db_path)?;
    store::load_skill_sources(&conn)
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
        let conn = store::open_initialized(&state.db_path)?;
        let profiles = store::load_profiles(&conn)?;
        let profile = target_profile_from_input(input)?;
        if profiles.iter().any(|candidate| candidate.id == profile.id) {
            return Err(format!("profile already exists: {}", profile.id));
        }
        store::upsert_profile(&conn, &profile)?;
        Ok(profile)
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
        let conn = store::open_initialized(&state.db_path)?;
        validate_target_profile(&profile)?;
        let existing_profile = store::load_profiles(&conn)?
            .into_iter()
            .find(|candidate| candidate.id == profile.id);
        let Some(existing_profile) = existing_profile else {
            return Err(format!("profile not found: {}", profile.id));
        };
        ensure_default_profile_update_is_allowed(&existing_profile, &profile)?;
        store::upsert_profile(&conn, &profile)?;
        Ok(profile)
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
        let conn = store::open_initialized(&state.db_path)?;
        let profiles = store::load_profiles(&conn)?;
        if !profiles.iter().any(|profile| profile.id == id) {
            return Err(format!("profile not found: {id}"));
        }

        ensure_profile_can_be_deleted(&conn, &id)?;
        store::delete_profile(&conn, &id)
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
    let conn = store::open_initialized(&state.db_path)?;
    store::load_navigation_model(&conn)
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
        let conn = store::open_initialized(&state.db_path)?;
        store::save_navigation_model(&conn, &model)?;
        store::load_navigation_model(&conn)
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
    let conn = store::open_initialized(&state.db_path)?;
    store::load_app_shortcuts(&conn)
}

#[tauri::command]
pub(crate) fn list_app_shortcut_settings(
    state: State<'_, AppState>,
) -> AppResult<Vec<AppShortcut>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::load_app_shortcut_settings(&conn)
}

#[tauri::command]
pub(crate) fn update_app_shortcuts(
    state: State<'_, AppState>,
    shortcuts: Vec<AppShortcut>,
) -> AppResult<Vec<AppShortcut>> {
    let fields = vec![("shortcut_count", shortcuts.len().to_string())];
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        let conn = store::open_initialized(&state.db_path)?;
        store::save_app_shortcuts(&conn, &shortcuts)?;
        store::load_app_shortcut_settings(&conn)
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
    let conn = store::open_initialized(&state.db_path)?;
    store::load_asset_mounts(&conn, asset_id.as_deref())
}

#[tauri::command]
pub(crate) fn list_asset_mount_statuses(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> AppResult<Vec<AssetMountStatus>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    scan_asset_mount_statuses(&conn, asset_id.as_deref())
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
        let conn = store::open_initialized(&state.db_path)?;
        sync_asset_mount_observations(&conn, asset_id.as_deref())
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
        let conn = store::open_initialized(&state.db_path)?;
        let assets = store::load_assets_by_kind(&conn, Some(AssetKind::Skill))?;
        let now = Utc::now().to_rfc3339();
        let group = asset_group_from_input(input, now.clone(), now);
        store::upsert_asset_group(&conn, &group)?;
        store::load_skill_group_detail(&conn, &group.id, &assets)
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
        let conn = store::open_initialized(&state.db_path)?;
        let assets = store::load_assets_by_kind(&conn, Some(AssetKind::Skill))?;
        let mut group = group;
        group.updated_at = Utc::now().to_rfc3339();
        store::upsert_asset_group(&conn, &group)?;
        store::load_skill_group_detail(&conn, &group.id, &assets)
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
        let conn = store::open_initialized(&state.db_path)?;
        let assets = store::load_assets_by_kind(&conn, Some(AssetKind::Skill))?;
        store::replace_asset_group_members(&conn, &group_id, &asset_ids, &assets)?;
        store::load_skill_group_detail(&conn, &group_id, &assets)
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

pub(crate) fn apply_skill_group_mount_record(
    conn: &rusqlite::Connection,
    group_id: &str,
    profile_id: &str,
    enabled: bool,
) -> AppResult<ApplyAssetGroupMountResult> {
    let assets = store::load_assets_by_kind(&conn, Some(AssetKind::Skill))?;
    let detail = store::load_skill_group_detail(conn, group_id, &assets)?;
    if !detail.group.enabled {
        return Err(format!("asset group is disabled: {}", detail.group.name));
    }

    let mut mounts = Vec::new();
    let mut statuses = Vec::new();
    let mut errors = Vec::new();
    for member in &detail.members {
        let result = if enabled {
            mount_asset_mount_record(conn, &member.asset_id, profile_id)
        } else {
            unmount_asset_mount_record(conn, &member.asset_id, profile_id)
        };

        match result {
            Ok(update) => {
                mounts.push(update.mount);
                statuses.push(update.status);
            }
            Err(message) => errors.push(AssetGroupMountError {
                asset_id: member.asset_id.clone(),
                message,
            }),
        }
    }

    Ok(ApplyAssetGroupMountResult {
        group_id: group_id.to_string(),
        profile_id: profile_id.to_string(),
        enabled,
        requested_count: detail.members.len(),
        updated_count: mounts.len(),
        error_count: errors.len(),
        mounts,
        statuses,
        errors,
    })
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
        let conn = store::open_initialized(&state.db_path)?;
        build_skill_group_exclusive_mount_preview(&conn, &input)
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
        let conn = store::open_initialized(&state.db_path)?;
        apply_skill_group_exclusive_mount_record(&conn, &input)
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

pub(crate) fn apply_skill_group_exclusive_mount_record(
    conn: &rusqlite::Connection,
    input: &SkillGroupExclusiveMountInput,
) -> AppResult<ApplySkillGroupExclusiveMountResult> {
    let preview = build_skill_group_exclusive_mount_preview(conn, input)?;
    let assets = store::load_assets(conn)?;
    let asset_by_id = assets
        .iter()
        .map(|asset| (asset.id.as_str(), asset))
        .collect::<HashMap<_, _>>();
    let profiles = store::load_profiles(conn)?;
    let profile = profiles
        .iter()
        .find(|profile| profile.id == preview.profile_id)
        .ok_or_else(|| format!("profile not found: {}", preview.profile_id))?;
    let mut statuses = Vec::new();
    let mut errors = Vec::new();

    for item in &preview.keep {
        if let Some(asset) = asset_by_id.get(item.asset_id.as_str()) {
            let inspection = targeting::inspect_mount(profile, asset)?;
            statuses.push(asset_mount_status(&asset.id, &profile.id, inspection));
        }
    }

    for item in &preview.mount {
        match mount_asset_mount_record(conn, &item.asset_id, &preview.profile_id) {
            Ok(update) => statuses.push(update.status),
            Err(message) => errors.push(SkillGroupExclusiveMountError {
                asset_id: item.asset_id.clone(),
                name: item.name.clone(),
                message,
            }),
        }
    }

    for item in &preview.unmount {
        match unmount_exclusive_skill_mount_record(conn, &item.asset_id, &preview.profile_id) {
            Ok(update) => statuses.push(update.status),
            Err(message) => errors.push(SkillGroupExclusiveMountError {
                asset_id: item.asset_id.clone(),
                name: item.name.clone(),
                message,
            }),
        }
    }

    Ok(ApplySkillGroupExclusiveMountResult {
        preview,
        statuses,
        errors,
    })
}

pub(crate) fn build_skill_group_exclusive_mount_preview(
    conn: &rusqlite::Connection,
    input: &SkillGroupExclusiveMountInput,
) -> AppResult<SkillGroupExclusiveMountPreview> {
    if !input.mount_selected {
        return Err("exclusive skill group mount requires mount_selected=true".to_string());
    }
    let _dry_run_requested = input.dry_run;

    let profiles = store::load_profiles(conn)?;
    let profile = profiles
        .iter()
        .find(|profile| profile.id == input.profile_id)
        .ok_or_else(|| format!("profile not found: {}", input.profile_id))?;
    validate_exclusive_skill_profile(profile)?;

    let assets = store::load_assets(conn)?;
    let skill_assets = assets
        .iter()
        .filter(|asset| asset.kind == AssetKind::Skill)
        .cloned()
        .collect::<Vec<_>>();
    let skill_asset_by_id = skill_assets
        .iter()
        .map(|asset| (asset.id.clone(), asset.clone()))
        .collect::<BTreeMap<_, _>>();
    let source_by_id = store::load_sources(conn)?
        .into_iter()
        .map(|source| (source.id.clone(), source))
        .collect::<HashMap<_, _>>();
    let enabled_mount_asset_ids = store::load_asset_mounts(conn, None)?
        .into_iter()
        .filter(|mount| mount.profile_id == profile.id && mount.enabled)
        .map(|mount| mount.asset_id)
        .collect::<BTreeSet<_>>();

    let mut group_ids = Vec::new();
    let mut selected_skill_ids = BTreeSet::new();
    let mut seen_group_ids = BTreeSet::new();
    for group_id in input
        .group_ids
        .iter()
        .map(|group_id| group_id.trim())
        .filter(|group_id| !group_id.is_empty())
    {
        if !seen_group_ids.insert(group_id.to_string()) {
            continue;
        }

        let detail = store::load_skill_group_detail(conn, group_id, &skill_assets)?;
        if !detail.group.enabled {
            continue;
        }

        group_ids.push(detail.group.id.clone());
        for member in detail.members {
            if skill_asset_by_id.contains_key(&member.asset_id) {
                selected_skill_ids.insert(member.asset_id);
            }
        }
    }

    let mut keep = Vec::new();
    let mut mount = Vec::new();
    let mut unmount = Vec::new();
    let mut skipped = Vec::new();

    for asset_id in &selected_skill_ids {
        let Some(asset) = skill_asset_by_id.get(asset_id) else {
            continue;
        };
        let inspection = targeting::inspect_mount(profile, asset)?;
        match inspection.state {
            targeting::PhysicalMountState::Mounted => keep.push(exclusive_item(asset)),
            targeting::PhysicalMountState::NotMounted => {
                match validate_exclusive_mount_candidate(asset, profile, &source_by_id) {
                    Ok(()) => mount.push(exclusive_item(asset)),
                    Err(reason) => skipped.push(exclusive_skipped_item(asset, reason)),
                }
            }
            targeting::PhysicalMountState::Conflict => skipped.push(exclusive_skipped_item(
                asset,
                format!("target path is occupied: {}", inspection.target_path),
            )),
            targeting::PhysicalMountState::Broken => skipped.push(exclusive_skipped_item(
                asset,
                format!("target symlink is broken: {}", inspection.target_path),
            )),
        }
    }

    for asset in &skill_assets {
        if selected_skill_ids.contains(&asset.id) {
            continue;
        }

        let inspection = targeting::inspect_mount(profile, asset)?;
        match inspection.state {
            targeting::PhysicalMountState::Mounted => {
                if store::is_managed_deployment(
                    conn,
                    &profile.id,
                    &asset.id,
                    &inspection.target_path,
                )? {
                    unmount.push(exclusive_item(asset));
                } else {
                    skipped.push(exclusive_skipped_item(
                        asset,
                        format!(
                            "target is mounted but not managed by AssetIWeave: {}",
                            inspection.target_path
                        ),
                    ));
                }
            }
            targeting::PhysicalMountState::NotMounted => {
                if enabled_mount_asset_ids.contains(&asset.id) {
                    unmount.push(exclusive_item(asset));
                }
            }
            targeting::PhysicalMountState::Conflict => skipped.push(exclusive_skipped_item(
                asset,
                format!("target path is occupied: {}", inspection.target_path),
            )),
            targeting::PhysicalMountState::Broken => skipped.push(exclusive_skipped_item(
                asset,
                format!("target symlink is broken: {}", inspection.target_path),
            )),
        }
    }

    let selected_skill_ids = selected_skill_ids.into_iter().collect::<Vec<_>>();
    let keep_count = keep.len();
    let mount_count = mount.len();
    let unmount_count = unmount.len();
    let skipped_count = skipped.len();

    Ok(SkillGroupExclusiveMountPreview {
        profile_id: profile.id.clone(),
        group_ids,
        selected_skill_ids,
        keep,
        mount,
        unmount,
        skipped,
        keep_count,
        mount_count,
        unmount_count,
        skipped_count,
    })
}

fn validate_exclusive_skill_profile(profile: &TargetProfile) -> AppResult<()> {
    if !profile.enabled {
        return Err(format!("profile is disabled: {}", profile.name));
    }
    if !profile.supported_kinds.contains(&AssetKind::Skill)
        || !profile.include.kinds.contains(&AssetKind::Skill)
    {
        return Err(format!(
            "profile {} does not support skill assets",
            profile.name
        ));
    }
    if !matches!(
        profile.deployment_strategy,
        DeploymentStrategy::SymlinkToSource
    ) {
        return Err(
            "exclusive skill group mount only supports symlink_to_source profiles".to_string(),
        );
    }
    Ok(())
}

fn validate_exclusive_mount_candidate(
    asset: &Asset,
    _profile: &TargetProfile,
    source_by_id: &HashMap<String, Source>,
) -> Result<(), String> {
    let source = source_by_id
        .get(&asset.source_id)
        .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
    if matches!(
        source.source_origin,
        SourceOrigin::AppTarget | SourceOrigin::AppLocal
    ) {
        return Err("app-local skills must be backed up before mounting".to_string());
    }

    let source_path = expand_path(&asset.absolute_path)?;
    if !source_path.exists() {
        return Err(format!(
            "source asset path does not exist: {}",
            source_path.display()
        ));
    }

    Ok(())
}

fn unmount_exclusive_skill_mount_record(
    conn: &rusqlite::Connection,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<AssetMountUpdateResult> {
    let (asset, profile) = load_mount_asset_and_profile(conn, asset_id, profile_id)?;
    let inspection = targeting::inspect_mount(&profile, &asset)?;
    match inspection.state {
        targeting::PhysicalMountState::Mounted => {
            if !store::is_managed_deployment(conn, &profile.id, &asset.id, &inspection.target_path)?
            {
                return Err(format!(
                    "target is mounted but not managed by AssetIWeave: {}",
                    inspection.target_path
                ));
            }
        }
        targeting::PhysicalMountState::NotMounted => {}
        targeting::PhysicalMountState::Conflict | targeting::PhysicalMountState::Broken => {
            return Err(format!(
                "target is not a managed mount for this asset: {}",
                inspection.target_path
            ));
        }
    }

    unmount_asset_mount_record(conn, asset_id, profile_id)
}

fn exclusive_item(asset: &Asset) -> SkillGroupExclusiveMountItem {
    SkillGroupExclusiveMountItem {
        asset_id: asset.id.clone(),
        name: asset.name.clone(),
    }
}

fn exclusive_skipped_item(asset: &Asset, reason: String) -> SkillGroupExclusiveMountSkippedItem {
    SkillGroupExclusiveMountSkippedItem {
        asset_id: asset.id.clone(),
        name: asset.name.clone(),
        reason,
    }
}

pub(crate) fn sync_asset_mount_observations(
    conn: &rusqlite::Connection,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMountStatus>> {
    repair_ghost_mount_symlinks(conn, asset_id)?;
    let statuses = scan_asset_mount_statuses(conn, asset_id)?;
    persist_asset_mount_observation_snapshot(conn, &statuses)?;
    Ok(statuses)
}

pub(crate) fn scan_asset_mount_statuses(
    conn: &rusqlite::Connection,
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMountStatus>> {
    let assets = store::load_assets(conn)?;
    let profiles = store::load_profiles(conn)?;
    inspect_asset_mount_statuses(&assets, &profiles, asset_id)
}

fn persist_asset_mount_observation_snapshot(
    conn: &rusqlite::Connection,
    statuses: &[AssetMountStatus],
) -> AppResult<()> {
    let assets = store::load_assets(conn)?;
    let profiles = store::load_profiles(conn)?;
    let observed_at = Utc::now().to_rfc3339();
    let observations = statuses
        .iter()
        .map(|status| AssetMountObservation {
            asset_id: status.asset_id.clone(),
            profile_id: status.profile_id.clone(),
            target_dir: status.target_dir.clone(),
            target_path: status.target_path.clone(),
            state: status.state,
            linked_source: status.linked_source.clone(),
            observed_at: observed_at.clone(),
        })
        .collect::<Vec<_>>();
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    store::upsert_asset_mount_observations(&tx, &observations)?;
    sync_asset_mount_snapshot_records(&tx, &assets, &profiles, &statuses)?;
    store::delete_orphan_asset_mount_observations(&tx)?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(())
}

fn inspect_asset_mount_statuses(
    assets: &[Asset],
    profiles: &[TargetProfile],
    asset_id: Option<&str>,
) -> AppResult<Vec<AssetMountStatus>> {
    let mut statuses = Vec::new();

    for asset in assets
        .iter()
        .filter(|asset| asset_id.map_or(true, |requested| requested == asset.id))
    {
        for profile in profiles {
            let inspection = targeting::inspect_mount(profile, asset)?;
            statuses.push(AssetMountStatus {
                asset_id: asset.id.clone(),
                profile_id: profile.id.clone(),
                target_dir: inspection.target_dir,
                target_path: inspection.target_path,
                state: PhysicalMountStateDto::from(inspection.state),
                linked_source: inspection.linked_source,
            });
        }
    }

    Ok(statuses)
}

fn sync_asset_mount_snapshot_records(
    conn: &rusqlite::Connection,
    assets: &[Asset],
    profiles: &[TargetProfile],
    statuses: &[AssetMountStatus],
) -> AppResult<()> {
    let asset_by_id = assets
        .iter()
        .map(|asset| (asset.id.as_str(), asset))
        .collect::<HashMap<_, _>>();
    let profile_by_id = profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect::<HashMap<_, _>>();

    for status in statuses {
        let asset = asset_by_id
            .get(status.asset_id.as_str())
            .ok_or_else(|| format!("asset not found: {}", status.asset_id))?;
        let profile = profile_by_id
            .get(status.profile_id.as_str())
            .ok_or_else(|| format!("profile not found: {}", status.profile_id))?;

        if matches!(status.state, PhysicalMountStateDto::Mounted) {
            let state = DeploymentState {
                profile_id: profile.id.clone(),
                asset_id: asset.id.clone(),
                target_path: status.target_path.clone(),
                strategy: profile.deployment_strategy,
                source_hash: asset.content_hash.clone().unwrap_or_default(),
                deployed_at: Utc::now().to_rfc3339(),
                managed_by: "assetiweave".to_string(),
            };
            store::upsert_deployment_state(conn, &state)?;
            store::set_asset_mount(
                conn,
                &asset.id,
                &profile.id,
                true,
                profile.deployment_strategy,
            )?;
        } else {
            store::delete_deployment_state(conn, &profile.id, &asset.id, &status.target_path)?;
            store::set_asset_mount(
                conn,
                &asset.id,
                &profile.id,
                false,
                profile.deployment_strategy,
            )?;
        }
    }

    Ok(())
}

pub(crate) fn asset_group_from_input(
    input: AssetGroupInput,
    created_at: String,
    updated_at: String,
) -> AssetGroup {
    AssetGroup {
        id: input.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: input.name,
        description: input.description,
        color: input.color.unwrap_or_else(|| "#10b981".to_string()),
        asset_kind: AssetKind::Skill,
        enabled: input.enabled.unwrap_or(true),
        sort_order: input.sort_order.unwrap_or(0),
        rules: input.rules.unwrap_or(AssetGroupRules {
            source_ids: vec![],
            relative_path_globs: vec![],
            name_contains: None,
        }),
        created_at,
        updated_at,
    }
}

#[tauri::command]
pub(crate) fn toggle_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
) -> AppResult<AssetMount> {
    let result = (|| {
        let _guard = state.lock.lock().map_err(|error| error.to_string())?;
        let conn = store::open_initialized(&state.db_path)?;
        let (asset, profile) = load_mount_asset_and_profile(&conn, &asset_id, &profile_id)?;
        let inspection = targeting::inspect_mount(&profile, &asset)?;
        set_asset_mount_record(
            &conn,
            &asset_id,
            &profile_id,
            !matches!(inspection.state, targeting::PhysicalMountState::Mounted),
            None,
        )
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
        let conn = store::open_initialized(&state.db_path)?;
        set_asset_mount_record(&conn, &asset_id, &profile_id, enabled, strategy)
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

pub(crate) fn set_asset_mount_record(
    conn: &rusqlite::Connection,
    asset_id: &str,
    profile_id: &str,
    enabled: bool,
    strategy: Option<DeploymentStrategy>,
) -> AppResult<AssetMount> {
    let default_strategy = validate_mount_target(conn, asset_id, profile_id)?;
    if enabled {
        return mount_asset_mount_record(conn, asset_id, profile_id).map(|result| result.mount);
    }

    let (asset, profile) = load_mount_asset_and_profile(conn, asset_id, profile_id)?;
    let inspection = targeting::inspect_mount(&profile, &asset)?;
    if matches!(inspection.state, targeting::PhysicalMountState::Mounted) {
        return unmount_asset_mount_record(conn, asset_id, profile_id).map(|result| result.mount);
    }

    let result = store::set_asset_mount(
        conn,
        asset_id,
        profile_id,
        enabled,
        strategy.unwrap_or(default_strategy),
    );
    match &result {
        Ok(_) => {
            let mut fields = mount_log_fields(conn, asset_id, profile_id);
            fields.push(("enabled", enabled.to_string()));
            log_info("skill.mount.preference", "更新 skill 挂载关系成功", &fields);
        }
        Err(error) => log_error(
            "skill.mount.preference",
            "更新 skill 挂载关系失败",
            error,
            &mount_log_fields(conn, asset_id, profile_id),
        ),
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
        let conn = store::open_initialized(&state.db_path)?;
        let sources = store::load_skill_sources(&conn)?;
        scan_selected_sources(&conn, sources, scanner::scan_skill_source)?;
        catalog_assets(&conn, Some(AssetKind::Skill))
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
) -> AppResult<crate::conversations::ExternalAdapterScaffoldResult> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.scaffold_conversation_adapter(params)
}

#[tauri::command]
pub(crate) fn validate_conversation_adapter(
    state: State<'_, AppState>,
    params: ExternalAdapterValidateParams,
) -> AppResult<crate::conversations::ExternalAdapterValidationResult> {
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
) -> AppResult<crate::conversations::ExternalAdapterRunResult> {
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
pub(crate) async fn sync_conversations(
    state: State<'_, AppState>,
    params: ConversationSyncParams,
) -> AppResult<serde_json::Value> {
    let db_path = state.db_path.clone();
    let lock = state.lock.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock.lock().map_err(|error| error.to_string())?;
        AppService::open_with_db_path(db_path)?.sync_conversations(params)
    })
    .await
    .map_err(|error| error.to_string())?;

    match &result {
        Ok(value) => log_info(
            "conversation.sync",
            "同步对话记录成功",
            &[("result", value.to_string())],
        ),
        Err(error) => log_error("conversation.sync", "同步对话记录失败", error, &[]),
    }
    result
}

#[tauri::command]
pub(crate) fn list_conversation_sessions(
    state: State<'_, AppState>,
    params: ConversationSessionListParams,
) -> AppResult<Vec<store::ConversationSessionListItem>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_conversation_sessions(params)
}

#[tauri::command]
pub(crate) fn get_conversation_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> AppResult<store::ConversationSessionDetail> {
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
) -> AppResult<Vec<store::ConversationSessionListItem>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_web_record_sessions(params)
}

#[tauri::command]
pub(crate) fn get_web_record_session(
    state: State<'_, AppState>,
    params: ConversationSessionGetParams,
) -> AppResult<store::ConversationSessionDetail> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.get_web_record_session(params)
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
) -> AppResult<Vec<store::ConversationQuestionDetail>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.list_conversation_questions(params)
}

#[tauri::command]
pub(crate) fn get_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionGetParams,
) -> AppResult<store::ConversationQuestionDetail> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.get_conversation_question(params)
}

#[tauri::command]
pub(crate) fn merge_conversation_questions(
    state: State<'_, AppState>,
    params: ConversationQuestionMergeParams,
) -> AppResult<store::ConversationMutationResult> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.merge_conversation_questions(params)
}

#[tauri::command]
pub(crate) fn split_conversation_question(
    state: State<'_, AppState>,
    params: ConversationQuestionSplitParams,
) -> AppResult<store::ConversationMutationResult> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    AppService::open_with_db_path(state.db_path.clone())?.split_conversation_question(params)
}

pub(crate) fn scan_selected_sources(
    conn: &rusqlite::Connection,
    sources: Vec<Source>,
    scan: fn(&Source) -> AppResult<Vec<Asset>>,
) -> AppResult<Vec<Asset>> {
    for mut source in prune_missing_sources(conn, sources)? {
        if !source.enabled {
            log_info(
                "source.scan.skip",
                "跳过已禁用来源",
                &source_log_fields(&source),
            );
            continue;
        }

        log_info(
            "source.scan.start",
            "开始扫描来源",
            &source_log_fields(&source),
        );
        let now = Utc::now().to_rfc3339();
        match scan(&source) {
            Ok(assets) => {
                store::replace_source_assets(&conn, &source.id, &assets)?;
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("ok: {} assets", assets.len()));
                store::upsert_source(&conn, &source)?;
                let mut fields = source_log_fields(&source);
                fields.push(("asset_count", assets.len().to_string()));
                log_info("source.scan.success", "扫描来源成功", &fields);
                for asset in &assets {
                    if matches!(asset.kind, AssetKind::Skill) {
                        log_info(
                            "skill.scan.success",
                            "扫描到 skill",
                            &asset_log_fields(asset),
                        );
                    }
                }
            }
            Err(error) => {
                if should_remove_source_on_scan_error(&error) {
                    let mut fields = source_log_fields(&source);
                    fields.push(("error", error.clone()));
                    log_warn("source.scan.removed", "来源路径不存在，已移除", &fields);
                    store::delete_source(conn, &source.id)?;
                    continue;
                }
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("error: {error}"));
                store::upsert_source(&conn, &source)?;
                log_error(
                    "source.scan.error",
                    "扫描来源失败",
                    &error,
                    &source_log_fields(&source),
                );
            }
        }
    }

    cleanup_orphan_asset_records(conn)?;
    store::load_assets(&conn)
}

pub(crate) fn refresh_all_sources(conn: &rusqlite::Connection) -> AppResult<Vec<Asset>> {
    let sources = store::load_sources(conn)?;
    scan_selected_sources(conn, sources, scanner::scan_source)
}

pub(crate) fn refresh_recorded_assets(conn: &rusqlite::Connection) -> AppResult<Vec<Asset>> {
    let sources = prune_missing_sources(conn, store::load_sources(conn)?)?;
    let source_map: HashMap<&str, &Source> = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect();
    let mut assets_by_source: HashMap<String, Vec<Asset>> = sources
        .iter()
        .map(|source| (source.id.clone(), Vec::new()))
        .collect();
    let mut removed_by_source: HashMap<String, usize> = HashMap::new();
    let mut updated_by_source: HashMap<String, usize> = HashMap::new();
    let mut orphan_source_ids = Vec::new();
    let now = Utc::now().to_rfc3339();

    for asset in store::load_assets(conn)? {
        let Some(source) = source_map.get(asset.source_id.as_str()) else {
            orphan_source_ids.push(asset.source_id.clone());
            continue;
        };

        match scanner::refresh_recorded_asset(source, &asset, &now) {
            Ok(Some(refreshed)) => {
                if refreshed.content_hash != asset.content_hash
                    || refreshed.description != asset.description
                {
                    *updated_by_source.entry(source.id.clone()).or_default() += 1;
                }
                assets_by_source
                    .entry(source.id.clone())
                    .or_default()
                    .push(refreshed);
            }
            Ok(None) => {
                *removed_by_source.entry(source.id.clone()).or_default() += 1;
            }
            Err(_) => {
                assets_by_source
                    .entry(source.id.clone())
                    .or_default()
                    .push(asset);
            }
        }
    }

    for source in sources {
        let retained_assets = assets_by_source.remove(&source.id).unwrap_or_default();
        let retained_count = retained_assets.len();
        store::replace_source_assets(conn, &source.id, &retained_assets)?;

        let removed_count = removed_by_source.get(&source.id).copied().unwrap_or(0);
        let updated_count = updated_by_source.get(&source.id).copied().unwrap_or(0);
        let mut source = source;
        source.last_scanned_at = Some(now.clone());
        source.last_scan_status = Some(format!(
            "validated: {retained_count} assets, {removed_count} removed, {updated_count} updated"
        ));
        store::upsert_source(conn, &source)?;
    }

    orphan_source_ids.sort();
    orphan_source_ids.dedup();
    for source_id in orphan_source_ids {
        store::replace_source_assets(conn, &source_id, &[])?;
    }

    cleanup_orphan_asset_records(conn)?;
    store::load_assets(conn)
}

pub(crate) fn cleanup_orphan_asset_records(conn: &rusqlite::Connection) -> AppResult<()> {
    store::delete_orphan_asset_mounts(conn)?;
    store::delete_orphan_asset_group_members(conn)?;
    store::delete_orphan_deployment_state(conn)?;
    store::delete_orphan_skill_remote_sources(conn)?;
    Ok(())
}

fn prune_missing_sources(
    conn: &rusqlite::Connection,
    sources: Vec<Source>,
) -> AppResult<Vec<Source>> {
    let mut retained_sources = Vec::new();
    for source in sources {
        if source_root_is_missing(&source) {
            log_warn(
                "source.prune_missing",
                "来源路径不存在，已从索引移除",
                &source_log_fields(&source),
            );
            store::delete_source(conn, &source.id)?;
        } else {
            retained_sources.push(source);
        }
    }
    Ok(retained_sources)
}

fn source_root_is_missing(source: &Source) -> bool {
    expand_path(&source.root_path)
        .map(|root| !root.exists())
        .unwrap_or(false)
}

fn should_remove_source_on_scan_error(error: &str) -> bool {
    error.starts_with("source path does not exist:")
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
        let conn = store::open_initialized(&state.db_path)?;
        let assets = store::load_assets(&conn)?;
        let profiles = store::load_profiles(&conn)?;
        let mounts = store::load_enabled_asset_mounts(&conn, profile_id.as_deref())?;
        Ok(planner::build_plan(
            &assets,
            &profiles,
            &mounts,
            profile_id.as_deref(),
        ))
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
        let conn = store::open_initialized(&state.db_path)?;
        let profiles = store::load_profiles(&conn)?;
        let assets = store::load_assets(&conn)?;
        executor::execute_deployment_plan(&conn, &profiles, &assets, &plan, action_ids.as_deref())
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
    let result = platform::reveal_path(path);
    match &result {
        Ok(()) => log_info("path.reveal", "打开路径成功", &fields),
        Err(error) => log_error("path.reveal", "打开路径失败", error, &fields),
    }
    result
}

fn validate_mount_target(
    conn: &rusqlite::Connection,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<DeploymentStrategy> {
    let asset = store::load_assets(conn)?
        .iter()
        .find(|asset| asset.id == asset_id)
        .cloned()
        .ok_or_else(|| format!("asset not found: {asset_id}"))?;
    let source = store::load_sources(conn)?
        .into_iter()
        .find(|source| source.id == asset.source_id)
        .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
    if matches!(
        source.source_origin,
        SourceOrigin::AppTarget | SourceOrigin::AppLocal
    ) {
        return Err("app-local skills must be backed up before mounting".to_string());
    }

    let profile = store::load_profiles(conn)?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("profile not found: {profile_id}"))?;

    Ok(profile.deployment_strategy)
}

pub(crate) fn mount_asset_mount_record(
    conn: &rusqlite::Connection,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<AssetMountUpdateResult> {
    let result = (|| {
        let strategy = validate_mount_target(conn, asset_id, profile_id)?;
        if !matches!(strategy, DeploymentStrategy::SymlinkToSource) {
            return Err("immediate mount only supports symlink_to_source profiles".to_string());
        }
        let (asset, profile) = load_mount_asset_and_profile(conn, asset_id, profile_id)?;
        validate_immediate_mount_support(&asset, &profile)?;

        let inspection = targeting::inspect_mount(&profile, &asset)?;
        match inspection.state {
            targeting::PhysicalMountState::Mounted => {
                let inspection =
                    repair_mounted_symlink_to_real_source(&asset, &profile, inspection)?;
                let mount = persist_verified_mount(
                    conn,
                    &asset,
                    &profile,
                    &inspection.target_path,
                    strategy,
                )?;
                return Ok(AssetMountUpdateResult {
                    mount,
                    status: asset_mount_status(&asset.id, &profile.id, inspection),
                });
            }
            targeting::PhysicalMountState::NotMounted => {}
            targeting::PhysicalMountState::Conflict | targeting::PhysicalMountState::Broken => {
                return Err(format!(
                    "target is not available for mounting: {}",
                    inspection.target_path
                ));
            }
        }

        let target_path = PathBuf::from(&inspection.target_path);
        create_mount_symlink(&asset, &profile, &target_path)?;
        let inspection = targeting::inspect_mount(&profile, &asset)?;
        if !matches!(inspection.state, targeting::PhysicalMountState::Mounted) {
            remove_created_mount_symlink(&target_path).ok();
            return Err(format!(
                "mount verification failed for {asset_id} on {profile_id}: {}",
                inspection.target_path
            ));
        }

        let mount =
            match persist_verified_mount(conn, &asset, &profile, &inspection.target_path, strategy)
            {
                Ok(mount) => mount,
                Err(error) => {
                    remove_created_mount_symlink(&target_path).ok();
                    return Err(error);
                }
            };
        Ok(AssetMountUpdateResult {
            mount,
            status: asset_mount_status(&asset.id, &profile.id, inspection),
        })
    })();

    match &result {
        Ok(update) => {
            let mut fields = mount_log_fields(conn, asset_id, profile_id);
            fields.push(("target_path", update.status.target_path.clone()));
            fields.push(("state", format!("{:?}", update.status.state)));
            log_info("skill.mount.success", "skill 挂载成功", &fields);
        }
        Err(error) => log_error(
            "skill.mount.error",
            "skill 挂载失败",
            error,
            &mount_log_fields(conn, asset_id, profile_id),
        ),
    }
    result
}

pub(crate) fn unmount_asset_mount_record(
    conn: &rusqlite::Connection,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<AssetMountUpdateResult> {
    let result = (|| {
        let (asset, profile) = load_mount_asset_and_profile(conn, asset_id, profile_id)?;
        let inspection = targeting::inspect_mount(&profile, &asset)?;
        let target_path = PathBuf::from(&inspection.target_path);
        let removed_link = matches!(inspection.state, targeting::PhysicalMountState::Mounted);

        match inspection.state {
            targeting::PhysicalMountState::Mounted => {
                remove_mounted_symlink(&inspection.target_path)?
            }
            targeting::PhysicalMountState::NotMounted => {}
            targeting::PhysicalMountState::Conflict | targeting::PhysicalMountState::Broken => {
                return Err(format!(
                    "target is not a symlink to this asset: {}",
                    inspection.target_path
                ));
            }
        }

        let inspection = targeting::inspect_mount(&profile, &asset)?;
        if !matches!(inspection.state, targeting::PhysicalMountState::NotMounted) {
            return Err(format!(
                "unmount verification failed for {asset_id} on {profile_id}: {}",
                inspection.target_path
            ));
        }

        match persist_verified_unmount(conn, &asset, &profile, &inspection.target_path) {
            Ok(mount) => Ok(AssetMountUpdateResult {
                mount,
                status: asset_mount_status(&asset.id, &profile.id, inspection),
            }),
            Err(error) => {
                if removed_link {
                    create_mount_symlink(&asset, &profile, &target_path).ok();
                }
                Err(error)
            }
        }
    })();

    match &result {
        Ok(update) => {
            let mut fields = mount_log_fields(conn, asset_id, profile_id);
            fields.push(("target_path", update.status.target_path.clone()));
            fields.push(("state", format!("{:?}", update.status.state)));
            log_info("skill.unmount.success", "skill 卸载成功", &fields);
        }
        Err(error) => log_error(
            "skill.unmount.error",
            "skill 卸载失败",
            error,
            &mount_log_fields(conn, asset_id, profile_id),
        ),
    }
    result
}

fn load_mount_asset_and_profile(
    conn: &rusqlite::Connection,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, TargetProfile)> {
    let asset = store::load_assets(conn)?
        .into_iter()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| format!("asset not found: {asset_id}"))?;
    let profile = store::load_profiles(conn)?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("profile not found: {profile_id}"))?;

    Ok((asset, profile))
}

fn validate_immediate_mount_support(asset: &Asset, profile: &TargetProfile) -> AppResult<()> {
    if !profile.enabled {
        return Err(format!("profile is disabled: {}", profile.name));
    }
    if matches!(asset.kind, AssetKind::Unclassified)
        || !profile.supported_kinds.contains(&asset.kind)
        || !profile.include.kinds.contains(&asset.kind)
    {
        return Err(format!(
            "profile {} does not support {:?}",
            profile.name, asset.kind
        ));
    }

    Ok(())
}

fn create_mount_symlink(
    asset: &Asset,
    profile: &TargetProfile,
    target_path: &Path,
) -> AppResult<()> {
    ensure_target_within_profile(profile, target_path)?;
    let source_path = targeting::canonical_source_path(asset)?;

    let parent = target_path.parent().ok_or_else(|| {
        format!(
            "target path is missing parent directory: {}",
            target_path.display()
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    create_symlink(&source_path, target_path)
}

fn repair_ghost_mount_symlinks(
    conn: &rusqlite::Connection,
    asset_id: Option<&str>,
) -> AppResult<()> {
    let assets = store::load_assets(conn)?;
    let profiles = store::load_profiles(conn)?;
    for asset in assets
        .iter()
        .filter(|asset| asset_id.map(|id| asset.id == id).unwrap_or(true))
    {
        for profile in profiles.iter().filter(|profile| profile.enabled) {
            let inspection = targeting::inspect_mount(profile, asset)?;
            repair_mounted_symlink_to_real_source(asset, profile, inspection)?;
        }
    }
    Ok(())
}

fn repair_mounted_symlink_to_real_source(
    asset: &Asset,
    profile: &TargetProfile,
    inspection: targeting::MountInspection,
) -> AppResult<targeting::MountInspection> {
    if !matches!(inspection.state, targeting::PhysicalMountState::Mounted) {
        return Ok(inspection);
    }

    let target_path = PathBuf::from(&inspection.target_path);
    let expected_source_path = targeting::canonical_source_path(asset)?;
    let linked_source = inspection
        .linked_source
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_default();
    if linked_source == expected_source_path {
        return Ok(inspection);
    }

    let metadata = fs::symlink_metadata(&target_path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_symlink() {
        return Ok(inspection);
    }

    let previous_link = fs::read_link(&target_path).map_err(|error| error.to_string())?;
    fs::remove_file(&target_path).map_err(|error| error.to_string())?;
    if let Err(error) = create_symlink(&expected_source_path, &target_path) {
        create_symlink(&previous_link, &target_path).ok();
        return Err(error);
    }

    let repaired = targeting::inspect_mount(profile, asset)?;
    if !matches!(repaired.state, targeting::PhysicalMountState::Mounted) {
        fs::remove_file(&target_path).ok();
        create_symlink(&previous_link, &target_path).ok();
        return Err(format!(
            "ghost symlink repair verification failed: {}",
            repaired.target_path
        ));
    }
    Ok(repaired)
}

fn persist_verified_mount(
    conn: &rusqlite::Connection,
    asset: &Asset,
    profile: &TargetProfile,
    target_path: &str,
    strategy: DeploymentStrategy,
) -> AppResult<AssetMount> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let state = DeploymentState {
        profile_id: profile.id.clone(),
        asset_id: asset.id.clone(),
        target_path: target_path.to_string(),
        strategy,
        source_hash: asset.content_hash.clone().unwrap_or_default(),
        deployed_at: Utc::now().to_rfc3339(),
        managed_by: "assetiweave".to_string(),
    };
    store::upsert_deployment_state(&tx, &state)?;
    let mount = store::set_asset_mount(&tx, &asset.id, &profile.id, true, strategy)?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(mount)
}

fn persist_verified_unmount(
    conn: &rusqlite::Connection,
    asset: &Asset,
    profile: &TargetProfile,
    target_path: &str,
) -> AppResult<AssetMount> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    store::delete_deployment_state(&tx, &profile.id, &asset.id, target_path)?;
    let mount = store::set_asset_mount(
        &tx,
        &asset.id,
        &profile.id,
        false,
        profile.deployment_strategy,
    )?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(mount)
}

fn ensure_target_within_profile(profile: &TargetProfile, target_path: &Path) -> AppResult<()> {
    let target_dir = targeting::target_dir(profile)?;
    if !target_path.starts_with(&target_dir) {
        return Err(format!(
            "refusing to write outside profile target directory: {}",
            target_path.display()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn create_symlink(source: &Path, target: &Path) -> AppResult<()> {
    std::os::unix::fs::symlink(source, target).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn create_symlink(source: &Path, target: &Path) -> AppResult<()> {
    if source.is_dir() {
        std::os::windows::fs::symlink_dir(source, target)
    } else {
        std::os::windows::fs::symlink_file(source, target)
    }
    .map_err(|error| error.to_string())
}

fn remove_created_mount_symlink(target_path: &Path) -> AppResult<()> {
    let metadata = fs::symlink_metadata(target_path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_symlink() {
        return Ok(());
    }
    fs::remove_file(target_path).map_err(|error| error.to_string())
}

fn remove_mounted_symlink(target_path: &str) -> AppResult<()> {
    let path = Path::new(target_path);
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_symlink() {
        return Err(format!("target is not a symlink: {}", path.display()));
    }
    fs::remove_file(path).map_err(|error| error.to_string())
}

fn asset_mount_status(
    asset_id: &str,
    profile_id: &str,
    inspection: targeting::MountInspection,
) -> AssetMountStatus {
    AssetMountStatus {
        asset_id: asset_id.to_string(),
        profile_id: profile_id.to_string(),
        target_dir: inspection.target_dir,
        target_path: inspection.target_path,
        state: PhysicalMountStateDto::from(inspection.state),
        linked_source: inspection.linked_source,
    }
}

pub(crate) fn assetiweave_library_source() -> Source {
    let root_path = default_skill_backup_root()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| "~/.assetiweave/library/skills".to_string());
    assetiweave_library_source_with_root(root_path)
}

pub(crate) fn assetiweave_library_source_with_root(root_path: String) -> Source {
    Source {
        id: SKILL_BACKUP_SOURCE_ID.to_string(),
        name: "AssetIWeave Skill Backup Library".to_string(),
        kind: SourceKind::Local,
        root_path,
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::AssetiweaveLibrary,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: vec![
            "**/.git/**".to_string(),
            "**/node_modules/**".to_string(),
            "**/target/**".to_string(),
            "**/dist/**".to_string(),
        ],
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: -100,
        last_scanned_at: None,
        last_scan_status: Some("pending".to_string()),
    }
}

pub(crate) fn skill_backup_root(conn: &rusqlite::Connection) -> AppResult<PathBuf> {
    let root_path = store::load_sources(conn)?
        .into_iter()
        .find(|source| source.id == SKILL_BACKUP_SOURCE_ID)
        .map(|source| source.root_path)
        .unwrap_or_else(|| {
            default_skill_backup_root()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|_| "~/.assetiweave/library/skills".to_string())
        });
    let root = expand_path(&root_path)?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

pub(crate) fn skill_backup_settings(conn: &rusqlite::Connection) -> AppResult<SkillBackupSettings> {
    let default_root = default_skill_backup_root()?;
    let source = store::load_sources(conn)?
        .into_iter()
        .find(|source| source.id == SKILL_BACKUP_SOURCE_ID)
        .unwrap_or_else(|| assetiweave_library_source());
    let expanded_root = expand_path(&source.root_path)?;
    Ok(SkillBackupSettings {
        root_path: source.root_path,
        expanded_root_path: expanded_root.to_string_lossy().to_string(),
        default_root_path: default_root.to_string_lossy().to_string(),
        is_default_root: same_path_or_text(&expanded_root, &default_root),
        exists: expanded_root.exists(),
    })
}

pub(crate) fn catalog_assets(
    conn: &rusqlite::Connection,
    kind: Option<AssetKind>,
) -> AppResult<Vec<CatalogAsset>> {
    let assets = store::load_assets_by_kind(conn, kind)?;
    let sources = store::load_sources(conn)?;
    Ok(build_catalog_assets(assets, &sources))
}

pub(crate) fn catalog_asset_for_id(
    conn: &rusqlite::Connection,
    asset_id: &str,
) -> AppResult<CatalogAsset> {
    catalog_assets(conn, None)?
        .into_iter()
        .find(|asset| asset.asset.id == asset_id)
        .ok_or_else(|| format!("asset not found in catalog: {asset_id}"))
}

pub(crate) fn build_catalog_assets(assets: Vec<Asset>, sources: &[Source]) -> Vec<CatalogAsset> {
    let source_by_id = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect::<HashMap<_, _>>();
    let mut content_groups: BTreeMap<String, Vec<Asset>> = BTreeMap::new();
    let mut without_identity = Vec::new();

    for asset in assets {
        if asset.kind == AssetKind::Skill {
            if let Some(content_hash) = asset.content_hash.clone().filter(|hash| !hash.is_empty()) {
                content_groups.entry(content_hash).or_default().push(asset);
                continue;
            }
        }
        without_identity.push(CatalogAsset {
            backup_status: standalone_backup_status(
                &asset,
                source_by_id.get(asset.source_id.as_str()).copied(),
            ),
            repository: None,
            asset,
        });
    }

    let mut catalog_assets = without_identity;
    for mut group in content_groups.into_values() {
        if group.len() == 1 {
            let asset = group.remove(0);
            catalog_assets.push(CatalogAsset {
                backup_status: standalone_backup_status(
                    &asset,
                    source_by_id.get(asset.source_id.as_str()).copied(),
                ),
                repository: None,
                asset,
            });
            continue;
        }

        group.sort_by(|left, right| {
            let left_score =
                canonical_asset_score(left, source_by_id.get(left.source_id.as_str()).copied());
            let right_score =
                canonical_asset_score(right, source_by_id.get(right.source_id.as_str()).copied());
            left_score
                .cmp(&right_score)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.absolute_path.cmp(&right.absolute_path))
        });

        let canonical = group.remove(0);
        let hidden_asset_ids = group
            .iter()
            .map(|asset| asset.id.clone())
            .collect::<Vec<_>>();
        let backup_path = std::iter::once(&canonical)
            .chain(group.iter())
            .find(|asset| {
                backup_entry_state(asset, source_by_id.get(asset.source_id.as_str()).copied())
                    == Some(SkillBackupState::BackedUp)
            })
            .map(|asset| asset.absolute_path.clone());
        let backup_status = if let Some(backup_path) = backup_path {
            Some(SkillBackupAssetStatus {
                state: SkillBackupState::BackedUp,
                backup_path: Some(backup_path),
                hidden_asset_ids,
            })
        } else {
            standalone_backup_status(
                &canonical,
                source_by_id.get(canonical.source_id.as_str()).copied(),
            )
            .map(|mut status| {
                status.hidden_asset_ids = hidden_asset_ids;
                status
            })
        };

        catalog_assets.push(CatalogAsset {
            asset: canonical,
            backup_status,
            repository: None,
        });
    }

    attach_git_repository_info(&mut catalog_assets);
    catalog_assets.sort_by(|left, right| {
        left.asset
            .name
            .cmp(&right.asset.name)
            .then_with(|| left.asset.relative_path.cmp(&right.asset.relative_path))
    });
    catalog_assets
}

fn attach_git_repository_info(catalog_assets: &mut [CatalogAsset]) {
    let mut repository_by_root = HashMap::new();
    for catalog_asset in catalog_assets {
        let asset_path = Path::new(&catalog_asset.asset.absolute_path);
        let Some(repository_root) = find_git_root(asset_path) else {
            continue;
        };
        let repository = repository_by_root
            .entry(repository_root.clone())
            .or_insert_with(|| git_repository_for_path(&repository_root));
        catalog_asset.repository = repository.clone().map(|mut repository| {
            repository.web_url = repository
                .remote_url
                .as_deref()
                .and_then(|remote| git_browser_url(remote, &repository_root, asset_path));
            repository
        });
    }
}

fn standalone_backup_status(
    asset: &Asset,
    source: Option<&Source>,
) -> Option<SkillBackupAssetStatus> {
    backup_entry_state(asset, source).map(|state| SkillBackupAssetStatus {
        state,
        backup_path: Some(asset.absolute_path.clone()),
        hidden_asset_ids: Vec::new(),
    })
}

fn backup_entry_state(asset: &Asset, source: Option<&Source>) -> Option<SkillBackupState> {
    let source = source?;
    if source.id != SKILL_BACKUP_SOURCE_ID
        && !matches!(source.source_origin, SourceOrigin::AssetiweaveLibrary)
    {
        return None;
    }

    if asset.relative_path.starts_with("downloaded/")
        || asset.relative_path.starts_with("imported/")
    {
        return Some(SkillBackupState::Downloaded);
    }
    if asset.relative_path.starts_with("backed-up/") {
        return Some(SkillBackupState::BackedUp);
    }
    None
}

fn canonical_asset_score(asset: &Asset, source: Option<&Source>) -> u8 {
    let Some(source) = source else {
        return 50;
    };
    match source.source_origin {
        SourceOrigin::AppTarget | SourceOrigin::AppLocal => 40,
        SourceOrigin::AssetiweaveLibrary => match backup_entry_state(asset, Some(source)) {
            Some(SkillBackupState::Downloaded) => 20,
            Some(SkillBackupState::BackedUp) => 30,
            None => 25,
        },
        SourceOrigin::GitRepo | SourceOrigin::LocalFolder | SourceOrigin::Custom => 0,
    }
}

pub(crate) fn target_profile_from_input(input: TargetProfileInput) -> AppResult<TargetProfile> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("profile name is required".to_string());
    }

    let id = input
        .id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| slug_profile_id(&name));
    if id.is_empty() {
        return Err("profile id is required".to_string());
    }

    let target_paths = input
        .target_paths
        .unwrap_or_default()
        .into_iter()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    if target_paths.is_empty() {
        return Err("profile target path is required".to_string());
    }

    let profile = TargetProfile {
        id,
        name,
        app_kind: input.app_kind.unwrap_or(AppKind::Custom),
        target_paths,
        supported_kinds: input
            .supported_kinds
            .unwrap_or_else(|| vec![AssetKind::Skill]),
        deployment_strategy: input
            .deployment_strategy
            .unwrap_or(DeploymentStrategy::SymlinkToSource),
        enabled: input.enabled.unwrap_or(true),
        include: input.include.unwrap_or_else(default_profile_include),
        exclude: input.exclude.unwrap_or_else(default_profile_exclude),
        safety: input.safety.unwrap_or(ProfileSafety {
            allow_remove: false,
            allow_overwrite: false,
        }),
    };
    validate_target_profile(&profile)?;
    Ok(profile)
}

pub(crate) fn ensure_profile_can_be_deleted(
    conn: &rusqlite::Connection,
    profile_id: &str,
) -> AppResult<()> {
    if defaults::is_default_app_profile_id(profile_id) {
        return Err(format!("default app cannot be deleted: {profile_id}"));
    }

    if store::count_deployment_state_by_profile(conn, profile_id)? > 0 {
        return Err(format!("profile has managed deployments: {profile_id}"));
    }

    if scan_asset_mount_statuses(conn, None)?.iter().any(|status| {
        status.profile_id == profile_id && status.state == PhysicalMountStateDto::Mounted
    }) {
        return Err(format!("profile has mounted assets: {profile_id}"));
    }

    Ok(())
}

pub(crate) fn ensure_default_profile_update_is_allowed(
    existing: &TargetProfile,
    next: &TargetProfile,
) -> AppResult<()> {
    if defaults::is_default_app_profile_id(&existing.id) && existing.id != next.id {
        return Err(format!(
            "default app profile id cannot be changed: {}",
            existing.id
        ));
    }
    Ok(())
}

pub(crate) fn validate_target_profile(profile: &TargetProfile) -> AppResult<()> {
    if profile.id.trim().is_empty() {
        return Err("profile id is required".to_string());
    }
    if profile.name.trim().is_empty() {
        return Err("profile name is required".to_string());
    }
    if profile
        .target_paths
        .iter()
        .map(|path| path.trim())
        .all(str::is_empty)
    {
        return Err("profile target path is required".to_string());
    }
    Ok(())
}

fn default_profile_include() -> RuleSet {
    RuleSet {
        kinds: vec![AssetKind::Skill],
        tags: vec![],
        groups: vec![],
        sources: vec![],
        path_patterns: vec![],
    }
}

fn default_profile_exclude() -> RuleSet {
    RuleSet {
        kinds: vec![AssetKind::Unclassified],
        tags: vec![],
        groups: vec![],
        sources: vec![],
        path_patterns: vec![],
    }
}

fn slug_profile_id(name: &str) -> String {
    let mut id = String::new();
    let mut last_was_separator = false;
    for character in name.trim().chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            id.push(character);
            last_was_separator = false;
        } else if !last_was_separator && !id.is_empty() {
            id.push('-');
            last_was_separator = true;
        }
    }
    id.trim_matches('-').to_string()
}

pub(crate) fn copy_dir(source: &Path, target: &Path) -> AppResult<()> {
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub(crate) fn copy_dir_without_conflicts(source: &Path, target: &Path) -> AppResult<()> {
    if !source.exists() {
        return Ok(());
    }
    if !source.is_dir() {
        return Err(format!(
            "backup source is not a directory: {}",
            source.display()
        ));
    }

    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }

        if destination.exists() {
            if !destination.is_file() {
                return Err(format!(
                    "backup migration target is not a file: {}",
                    destination.display()
                ));
            }
            let source_bytes = fs::read(entry.path()).map_err(|error| error.to_string())?;
            let destination_bytes = fs::read(&destination).map_err(|error| error.to_string())?;
            if source_bytes != destination_bytes {
                return Err(format!(
                    "backup migration target already has different content: {}",
                    destination.display()
                ));
            }
            continue;
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn same_path_or_text(left: &Path, right: &Path) -> bool {
    let normalized_left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let normalized_right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    normalized_left == normalized_right || left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AppKind, AssetFormat, AssetGroup, AssetGroupRules, AssetKind, DeploymentStrategy,
        ProfileSafety, RuleSet, SourceKind,
    };
    use std::{path::PathBuf, process::Command};

    #[test]
    fn refresh_recorded_assets_prunes_missing_sources() {
        let db_path = unique_temp_path("assetiweave-refresh-recorded");
        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_missing_source("missing-recorded-source");
        store::upsert_source(&conn, &source).expect("insert source");

        refresh_recorded_assets(&conn).expect("refresh recorded assets");

        assert!(!store::load_sources(&conn)
            .expect("load sources")
            .iter()
            .any(|candidate| candidate.id == source.id));
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn source_scan_prunes_missing_sources_without_error_row() {
        let db_path = unique_temp_path("assetiweave-scan-missing-source");
        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_missing_source("missing-scan-source");
        store::upsert_source(&conn, &source).expect("insert source");

        scan_selected_sources(&conn, vec![source.clone()], scanner::scan_source)
            .expect("scan selected sources");

        assert!(!store::load_sources(&conn)
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
        let conn = store::open_initialized(&db_path).expect("open initialized db");
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

        store::upsert_profile(&conn, &profile).expect("insert profile");
        profile.name = "Team App Edited".to_string();
        store::upsert_profile(&conn, &profile).expect("update profile");

        assert!(store::load_profiles(&conn)
            .expect("load profiles")
            .iter()
            .any(|candidate| candidate.id == profile.id && candidate.name == "Team App Edited"));

        ensure_profile_can_be_deleted(&conn, &profile.id).expect("profile delete guard");
        store::delete_profile(&conn, &profile.id).expect("delete profile");
        assert!(!store::load_profiles(&conn)
            .expect("load profiles")
            .iter()
            .any(|candidate| candidate.id == profile.id));
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn default_app_profile_delete_is_blocked() {
        let db_path = unique_temp_path("assetiweave-default-profile-delete-db");
        let conn = store::open_initialized(&db_path).expect("open initialized db");

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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("profile-delete-source", source_root.clone());
        let profile = test_profile("team-app", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, std::slice::from_ref(&asset))
            .expect("insert asset");
        store::upsert_profile(&conn, &profile).expect("insert profile");
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
        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-deleted-asset", source_root.clone());
        let asset = test_asset(&source, "deleted-asset", source_root.join("deleted-asset"));
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, std::slice::from_ref(&asset))
            .expect("insert asset");
        store::set_asset_mount(
            &conn,
            &asset.id,
            "codex",
            true,
            DeploymentStrategy::SymlinkToSource,
        )
        .expect("insert mount");

        refresh_recorded_assets(&conn).expect("refresh recorded assets");

        assert!(store::load_assets(&conn)
            .expect("load assets")
            .iter()
            .all(|candidate| candidate.id != asset.id));
        assert!(store::load_asset_mounts(&conn, Some(&asset.id))
            .expect("load mounts")
            .is_empty());
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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-unmounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path.clone());
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, std::slice::from_ref(&asset))
            .expect("insert asset");
        store::upsert_profile(&conn, &profile).expect("insert profile");

        let result = mount_asset_mount_record(&conn, &asset.id, &profile.id).expect("mount");

        let metadata = std::fs::symlink_metadata(&target_path).expect("target metadata");
        assert!(metadata.file_type().is_symlink());
        assert_eq!(
            std::fs::read_link(&target_path).expect("read symlink"),
            asset_path.canonicalize().expect("canonical asset path")
        );
        assert!(result.mount.enabled);
        assert_eq!(result.status.state, PhysicalMountStateDto::Mounted);
        assert!(store::is_managed_deployment(
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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-aliased-asset", alias_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", alias_asset_path.clone());
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, std::slice::from_ref(&asset))
            .expect("insert asset");
        store::upsert_profile(&conn, &profile).expect("insert profile");

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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-set-mounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, std::slice::from_ref(&asset))
            .expect("insert asset");
        store::upsert_profile(&conn, &profile).expect("insert profile");

        let mount = set_asset_mount_record(&conn, &asset.id, &profile.id, true, None)
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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-group-assets", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset_a = test_asset(&source, "skill-a", asset_path_a.clone());
        let asset_b = test_asset(&source, "skill-b", asset_path_b);
        let assets = vec![asset_a.clone(), asset_b.clone()];
        let group = test_group("frontend");
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, &assets).expect("insert assets");
        store::upsert_profile(&conn, &profile).expect("insert profile");
        store::upsert_asset_group(&conn, &group).expect("insert group");
        store::replace_asset_group_members(&conn, &group.id, &[asset_a.id.clone()], &assets)
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
        assert!(store::load_asset_mounts(&conn, Some(&asset_b.id))
            .expect("load unrelated mounts")
            .is_empty());

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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
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
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, &skill_assets).expect("insert assets");
        store::upsert_profile(&conn, &codex).expect("insert codex profile");
        store::upsert_profile(&conn, &cursor).expect("insert cursor profile");
        for group in [&group_a, &group_b, &disabled_group] {
            store::upsert_asset_group(&conn, group).expect("insert group");
        }
        store::replace_asset_group_members(
            &conn,
            &group_a.id,
            &[asset_a.id.clone(), asset_b.id.clone()],
            &skill_assets,
        )
        .expect("insert group a members");
        store::replace_asset_group_members(
            &conn,
            &group_b.id,
            &[asset_b.id.clone()],
            &skill_assets,
        )
        .expect("insert group b members");
        store::replace_asset_group_members(
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
        assert!(store::load_asset_mounts(&conn, Some(&asset_c.id))
            .expect("load skill c mounts")
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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
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
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, &all_assets).expect("insert assets");
        store::upsert_profile(&conn, &codex).expect("insert codex profile");
        store::upsert_profile(&conn, &cursor).expect("insert cursor profile");
        for group in [&group_a, &group_b, &disabled_group] {
            store::upsert_asset_group(&conn, group).expect("insert group");
        }
        store::replace_asset_group_members(
            &conn,
            &group_a.id,
            &[asset_a.id.clone(), asset_b.id.clone()],
            &skill_assets,
        )
        .expect("insert group a members");
        store::replace_asset_group_members(
            &conn,
            &group_b.id,
            &[asset_b.id.clone()],
            &skill_assets,
        )
        .expect("insert group b members");
        store::replace_asset_group_members(
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
        store::set_asset_mount(
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
        let skill_c_mounts =
            store::load_asset_mounts(&conn, Some(&asset_c.id)).expect("load skill c mounts");
        assert!(skill_c_mounts
            .iter()
            .any(|mount| mount.profile_id == codex.id && !mount.enabled));
        assert!(skill_c_mounts
            .iter()
            .any(|mount| mount.profile_id == cursor.id && mount.enabled));
        assert!(store::load_asset_mounts(&conn, Some(&prompt.id))
            .expect("load prompt mounts")
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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
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
        store::upsert_source(&conn, &external_source).expect("insert external source");
        store::upsert_source(&conn, &app_local_source).expect("insert app local source");
        store::replace_source_assets(&conn, &external_source.id, &[external_asset.clone()])
            .expect("insert external asset");
        store::replace_source_assets(&conn, &app_local_source.id, &[app_local_asset.clone()])
            .expect("insert app local asset");
        store::upsert_profile(&conn, &profile).expect("insert profile");
        store::upsert_asset_group(&conn, &group).expect("insert group");
        store::replace_asset_group_members(
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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-scanned-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, std::slice::from_ref(&asset))
            .expect("insert asset");
        store::upsert_profile(&conn, &profile).expect("insert profile");
        store::set_asset_mount(
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
        assert!(store::load_asset_mounts(&conn, Some(&asset.id))
            .expect("load mounts")
            .iter()
            .all(|mount| !mount.enabled));
        assert!(!store::is_managed_deployment(
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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-observed-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, std::slice::from_ref(&asset))
            .expect("insert asset");
        store::upsert_profile(&conn, &profile).expect("insert profile");
        store::set_asset_mount(
            &conn,
            &asset.id,
            &profile.id,
            false,
            DeploymentStrategy::SymlinkToSource,
        )
        .expect("store disabled intent");

        sync_asset_mount_observations(&conn, None).expect("sync observations");

        let observations = store::load_asset_mount_observations(&conn).expect("load observations");
        let observation = observations
            .iter()
            .find(|candidate| candidate.asset_id == asset.id && candidate.profile_id == profile.id)
            .expect("asset/profile observation");
        assert_eq!(observation.state, PhysicalMountStateDto::Mounted);
        assert!(!observation.observed_at.is_empty());
        let mounts = store::load_asset_mounts(&conn, Some(&asset.id)).expect("load mounts");
        assert!(mounts
            .iter()
            .any(|mount| mount.profile_id == profile.id && mount.enabled));
        assert!(store::is_managed_deployment(
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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-ghost-asset", alias_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", alias_asset_path);
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, std::slice::from_ref(&asset))
            .expect("insert asset");
        store::upsert_profile(&conn, &profile).expect("insert profile");

        sync_asset_mount_observations(&conn, None).expect("sync observations");

        assert_eq!(
            std::fs::read_link(&target_path).expect("read repaired target symlink"),
            real_asset_path
                .canonicalize()
                .expect("canonical real asset")
        );
        let observations = store::load_asset_mount_observations(&conn).expect("load observations");
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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-missing-observed-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, std::slice::from_ref(&asset))
            .expect("insert asset");
        store::upsert_profile(&conn, &profile).expect("insert profile");
        store::set_asset_mount(
            &conn,
            &asset.id,
            &profile.id,
            true,
            DeploymentStrategy::SymlinkToSource,
        )
        .expect("store stale enabled snapshot");

        sync_asset_mount_observations(&conn, None).expect("sync observations");

        assert!(store::load_asset_mounts(&conn, Some(&asset.id))
            .expect("load mounts")
            .iter()
            .all(|mount| !mount.enabled));
        assert!(!store::is_managed_deployment(
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

        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-mounted-asset", source_root.clone());
        let profile = test_profile("codex", target_root.clone());
        let asset = test_asset(&source, "skill-a", asset_path);
        store::replace_source_assets(&conn, &source.id, std::slice::from_ref(&asset))
            .expect("insert asset");
        store::upsert_profile(&conn, &profile).expect("insert profile");
        store::set_asset_mount(
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
        assert!(store::load_asset_mounts(&conn, Some(&asset.id))
            .expect("load mounts")
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
