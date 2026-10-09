use tauri::{AppHandle, Emitter, State};

use crate::adapters::app_state::AppState;
use crate::adapters::tauri::background_tasks::{
    BackgroundTaskStatus, RemoteSkillAcquireTaskSnapshot, SkillBackupTaskSnapshot,
};
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::domain::{CatalogAsset, SkillBackupSettings, SkillRemoteSource};

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
