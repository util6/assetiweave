mod asset_repo;
mod codec;
mod conversation_repo;
mod database;
mod deployment_repo;
mod group_repo;
mod menu_repo;
mod mount_observation_repo;
mod mount_repo;
mod profile_repo;
mod schema;
mod shortcut_repo;
mod skill_remote_repo;
mod source_repo;
mod sql;
mod web_record_repo;

pub(crate) use asset_repo::{
    load_asset_sqlx, load_assets_sqlx, replace_source_assets_sqlx, update_asset_description_sqlx,
};
#[cfg(test)]
pub(crate) use asset_repo::{load_assets, load_assets_by_kind, replace_source_assets};
pub(crate) use conversation_repo::{
    delete_conversation_adapter_sqlx, disable_conversation_source_sqlx,
    import_conversation_sessions_sqlx, list_conversation_adapters_sqlx,
    list_conversation_question_details_sqlx, list_conversation_sessions_sqlx,
    list_conversation_sources_sqlx, load_conversation_adapter_sqlx,
    load_conversation_question_detail_sqlx, load_conversation_session_detail_sqlx,
    load_conversation_source_sqlx, merge_conversation_questions_sqlx,
    render_conversation_detail_markdown_with_filter, search_conversation_cards,
    seed_builtin_conversation_adapters, split_conversation_question_sqlx,
    upsert_conversation_adapter_sqlx, upsert_conversation_source_sqlx,
};
pub(crate) use database::{
    count_rows as count_rows_sqlx, latest_scan_status as latest_scan_status_sqlx, Database,
};
#[cfg(test)]
pub(crate) use deployment_repo::is_managed_deployment;
pub(crate) use deployment_repo::{
    count_deployment_state_by_profile_sqlx, delete_orphan_deployment_state_sqlx,
    is_managed_deployment_sqlx, load_managed_deployment_targets_by_profile_sqlx,
    upsert_deployment_state_sqlx,
};
pub(crate) use group_repo::{
    delete_asset_group_sqlx, delete_orphan_asset_group_members_sqlx, load_skill_group_detail_sqlx,
    load_skill_group_details_by_ids_sqlx, load_skill_group_details_sqlx,
    replace_asset_group_members_sqlx, upsert_asset_group_sqlx,
};
#[cfg(test)]
pub(crate) use group_repo::{replace_asset_group_members, upsert_asset_group};
pub(crate) use menu_repo::{load_navigation_model_sqlx, save_navigation_model_sqlx};
#[cfg(test)]
pub(crate) use mount_observation_repo::load_asset_mount_observations;
pub(crate) use mount_observation_repo::persist_asset_mount_snapshot_sqlx;
#[cfg(test)]
pub(crate) use mount_repo::load_asset_mounts;
#[cfg(test)]
pub(crate) use mount_repo::set_asset_mount;
pub(crate) use mount_repo::{
    delete_orphan_asset_mounts_sqlx, load_asset_mounts_sqlx, load_enabled_asset_mounts_sqlx,
    persist_verified_mount_sqlx, persist_verified_unmount_sqlx, set_asset_mount_sqlx,
};
#[cfg(test)]
pub(crate) use profile_repo::{
    count_deployment_state_by_profile, delete_profile, load_profiles, upsert_profile,
};
pub(crate) use profile_repo::{
    delete_profile_sqlx, load_profile_sqlx, load_profiles_sqlx, upsert_profile_sqlx,
};
pub(crate) use schema::open_initialized;
pub(crate) use shortcut_repo::{
    load_app_shortcut_settings_sqlx, load_app_shortcuts_sqlx, save_app_shortcuts_sqlx,
};
pub(crate) use skill_remote_repo::{
    delete_orphan_skill_remote_sources_sqlx, list_skill_remote_sources_sqlx,
    load_skill_remote_source_sqlx, update_skill_remote_check_result_sqlx,
    upsert_skill_remote_source_sqlx,
};
#[cfg(test)]
pub(crate) use source_repo::load_sources;
#[cfg(test)]
pub(crate) use source_repo::upsert_source;
pub(crate) use source_repo::{
    delete_source_sqlx, load_skill_sources_sqlx, load_source_sqlx, load_sources_sqlx,
    normalize_source, upsert_source_sqlx,
};
pub(crate) use web_record_repo::{
    import_web_record_sessions_sqlx, list_web_record_sessions_sqlx,
    load_web_record_session_detail_sqlx, render_web_record_detail_markdown_with_filter,
};
