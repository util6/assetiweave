use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;

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
