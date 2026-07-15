mod asset_repo;
mod backup_repo;
mod codec;
mod conversation_repo;
mod database;
mod deployment_repo;
mod group_repo;
mod menu_repo;
mod mount_observation_repo;
mod mount_repo;
mod profile_repo;
mod shortcut_repo;
mod skill_remote_repo;
mod source_repo;
mod sql;
mod tenant_repo;
mod web_record_repo;

pub(crate) use asset_repo::{
    load_asset_sqlx, load_assets_sqlx, replace_source_assets_sqlx, update_asset_description_sqlx,
};
pub(crate) use backup_repo::{checkpoint_database_wal_sqlx, vacuum_database_into_sqlx};
pub(crate) use conversation_repo::{
    activate_conversation_adapter_package_sqlx, delete_conversation_adapter_registration_sqlx,
    disable_conversation_source_sqlx, has_running_conversation_sync_for_adapter_sqlx,
    import_conversation_sessions_sqlx, list_conversation_adapter_packages_sqlx,
    list_conversation_adapters_sqlx, list_conversation_question_details_sqlx,
    list_conversation_sessions_sqlx, list_conversation_sources_sqlx,
    load_conversation_adapter_package_by_adapter_sqlx, load_conversation_adapter_package_sqlx,
    load_conversation_adapter_sqlx, load_conversation_question_detail_sqlx,
    load_conversation_session_detail_sqlx, load_conversation_source_sqlx,
    merge_conversation_questions_sqlx, search_conversation_cards_sqlx,
    split_conversation_question_sqlx, update_conversation_part_translation_sqlx,
    upsert_conversation_adapter_package_sqlx, upsert_conversation_adapter_sqlx,
    upsert_conversation_source_sqlx,
};
pub(crate) use database::{
    count_rows as count_rows_sqlx, latest_scan_status as latest_scan_status_sqlx,
    seed_tenant_defaults_sqlx, Database,
};
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
pub(crate) use menu_repo::{load_navigation_model_sqlx, save_navigation_model_sqlx};
#[cfg(test)]
pub(crate) use mount_observation_repo::load_asset_mount_observations_sqlx;
pub(crate) use mount_observation_repo::persist_asset_mount_snapshot_sqlx;
pub(crate) use mount_repo::{
    delete_orphan_asset_mounts_sqlx, load_asset_mounts_sqlx, load_enabled_asset_mounts_sqlx,
    persist_verified_mount_sqlx, persist_verified_unmount_sqlx, set_asset_mount_sqlx,
};
pub(crate) use profile_repo::{
    delete_profile_sqlx, load_profile_sqlx, load_profiles_sqlx, upsert_profile_sqlx,
};
pub(crate) use shortcut_repo::{
    load_app_shortcut_settings_sqlx, load_app_shortcuts_sqlx, save_app_shortcuts_sqlx,
};
pub(crate) use skill_remote_repo::{
    delete_orphan_skill_remote_sources_sqlx, list_skill_remote_sources_sqlx,
    load_skill_remote_source_sqlx, update_skill_remote_check_result_sqlx,
    upsert_skill_remote_source_sqlx,
};
pub(crate) use source_repo::{
    delete_source_sqlx, load_skill_sources_sqlx, load_source_sqlx, load_sources_sqlx,
    normalize_source, upsert_source_sqlx,
};
pub(crate) use tenant_repo::{
    create_local_tenant_sqlx, list_tenants_for_principal_sqlx, load_local_request_context_sqlx,
    set_active_tenant_sqlx,
};
pub(crate) use web_record_repo::{
    import_web_record_sessions_sqlx, list_web_record_sessions_sqlx,
    load_web_record_session_detail_sqlx, update_web_record_part_translation_sqlx,
};
