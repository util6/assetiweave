//! Mounting 领域 Tauri Command 模块
//!
//! 包含 Profile 目标、资产挂载状态、单项/批量挂载、Skill 分组与部署计划等命令。

#[macro_use]
pub(crate) mod batch;
#[macro_use]
pub(crate) mod groups;
#[macro_use]
pub(crate) mod mounts;
#[macro_use]
pub(crate) mod profiles;

pub(crate) use batch::{cancel_batch_mount, get_batch_mount_task, list_batch_mount_tasks, start_batch_mount, BATCH_MOUNT_TASK_UPDATED_EVENT};
pub(crate) use groups::{
    create_skill_group, delete_skill_group, list_skill_groups, preview_skill_group_exclusive_mount,
    set_skill_group_manual_members, update_skill_group,
};
pub(crate) use mounts::{
    list_asset_mount_statuses, list_asset_mounts, mount_asset_mount, refresh_asset_mount_statuses,
    set_asset_mount, toggle_asset_mount, unmount_asset_mount,
};
pub(crate) use profiles::{
    create_profile, delete_profile, list_profiles, list_target_profile_descriptors,
    refresh_target_profile_descriptors, update_profile,
};

#[cfg(test)]
#[path = "mounting_tests.rs"]
mod tests;
