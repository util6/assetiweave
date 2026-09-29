//! Catalog 领域 Tauri Command 模块
//!
//! 包含资产列表/删除、数据源增删改查/扫描、Skill 搜索/拉取/备份与远程源等命令。

#[macro_use]
pub(crate) mod assets;
#[macro_use]
pub(crate) mod skills;
#[macro_use]
pub(crate) mod sources;

pub(crate) use assets::{delete_asset, list_assets, list_source_assets, update_asset_description};
pub(crate) use skills::{
    acquire_skill, backup_skill, backup_skills, cancel_skill_acquire_task,
    check_skill_remote_sources, get_skill_acquire_task, get_skill_backup_settings,
    get_skill_backup_task, list_skill_acquire_tasks, list_skill_remote_sources, search_skills,
    start_skill_acquire, update_skill_backup_settings,
};
pub(crate) use sources::{
    cancel_source_scan, create_source, delete_source, get_source_scan_task, list_skill_sources,
    list_source_scan_tasks, list_sources, start_source_scan, update_source,
    SOURCE_SCAN_TASK_UPDATED_EVENT,
};

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
