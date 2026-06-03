mod asset_repo;
mod codec;
mod deployment_repo;
mod group_repo;
mod menu_repo;
mod mount_observation_repo;
mod mount_repo;
mod profile_repo;
mod schema;
mod shortcut_repo;
mod source_repo;
mod sql;

pub(crate) use asset_repo::{
    load_assets, load_assets_by_kind, replace_source_assets, update_asset_description,
};
pub(crate) use deployment_repo::{
    delete_deployment_state, delete_orphan_deployment_state, is_managed_deployment,
    upsert_deployment_state,
};
pub(crate) use group_repo::{
    delete_asset_group, delete_orphan_asset_group_members, load_skill_group_detail,
    load_skill_group_details, replace_asset_group_members, upsert_asset_group,
};
pub(crate) use menu_repo::{load_navigation_model, save_navigation_model};
#[cfg(test)]
pub(crate) use mount_observation_repo::load_asset_mount_observations;
pub(crate) use mount_observation_repo::{
    delete_orphan_asset_mount_observations, upsert_asset_mount_observations,
};
pub(crate) use mount_repo::{
    delete_orphan_asset_mounts, load_asset_mounts, load_enabled_asset_mounts, set_asset_mount,
};
pub(crate) use profile_repo::{
    count_deployment_state_by_profile, delete_profile, load_profiles, upsert_profile,
};
pub(crate) use schema::{count_rows, latest_scan_status, open_initialized};
pub(crate) use shortcut_repo::{
    load_app_shortcut_settings, load_app_shortcuts, save_app_shortcuts,
};
pub(crate) use source_repo::{
    delete_source, load_skill_sources, load_sources, normalize_source, upsert_source,
};
