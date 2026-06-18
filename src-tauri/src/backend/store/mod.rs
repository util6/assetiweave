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
    load_assets, load_assets_by_kind, load_assets_sqlx, replace_source_assets,
    replace_source_assets_sqlx, update_asset_description_sqlx,
};
pub(crate) use conversation_repo::{
    delete_conversation_adapter, disable_conversation_source, import_conversation_sessions,
    list_conversation_adapters, list_conversation_question_details, list_conversation_sessions,
    list_conversation_sources, load_conversation_adapter, load_conversation_question_detail,
    load_conversation_session_detail, load_conversation_source, merge_conversation_questions,
    render_session_markdown_for_questions_with_filter, render_session_markdown_with_filter,
    search_conversation_cards, seed_builtin_conversation_adapters, split_conversation_question,
    upsert_conversation_adapter, upsert_conversation_source,
};
pub(crate) use database::{
    count_rows as count_rows_sqlx, latest_scan_status as latest_scan_status_sqlx, Database,
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
    count_deployment_state_by_profile, delete_profile_sqlx, load_profiles, load_profiles_sqlx,
    upsert_profile_sqlx,
};
#[cfg(test)]
pub(crate) use profile_repo::{delete_profile, upsert_profile};
pub(crate) use schema::open_initialized;
pub(crate) use shortcut_repo::{
    load_app_shortcut_settings, load_app_shortcuts, save_app_shortcuts,
};
pub(crate) use skill_remote_repo::{
    delete_orphan_skill_remote_sources, list_skill_remote_sources_sqlx,
    load_skill_remote_source_sqlx, update_skill_remote_check_result_sqlx,
    upsert_skill_remote_source_sqlx,
};
pub(crate) use source_repo::{
    delete_source, delete_source_sqlx, load_skill_sources, load_skill_sources_sqlx, load_sources,
    load_sources_sqlx, normalize_source, upsert_source, upsert_source_sqlx,
};
pub(crate) use web_record_repo::{
    import_web_record_sessions, list_web_record_sessions, load_web_record_session_detail,
    render_web_record_markdown_with_filter,
};
