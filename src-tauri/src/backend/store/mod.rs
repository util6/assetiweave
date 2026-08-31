mod asset_repo;
mod backup_repo;
mod codec;
mod conversation_repo;
mod database;
mod deployment_repo;
mod group_repo;
mod memory_repo;
mod menu_repo;
mod mount_observation_repo;
mod mount_repo;
mod profile_repo;
mod project_memory_repo;
mod search_index_repo;
mod session_memory_repo;
mod settings_repo;
mod shortcut_repo;
mod skill_remote_repo;
mod source_repo;
mod sql;
mod team_repo;
mod tenant_repo;
mod web_record_repo;

pub(crate) use asset_repo::{
    load_asset_sqlx, load_assets_sqlx, replace_source_assets_sqlx, update_asset_description_sqlx,
};
pub(crate) use backup_repo::{checkpoint_database_wal_sqlx, vacuum_database_into_sqlx};
pub(crate) use conversation_repo::{
    activate_conversation_adapter_package_sqlx, activate_conversation_adapter_workspace_sqlx,
    conversation_payload_policy_reparse_required_sqlx,
    deactivate_conversation_adapter_package_sqlx, delete_conversation_adapter_package_version_sqlx,
    delete_conversation_adapter_registration_sqlx, disable_builtin_conversation_adapter_sqlx,
    disable_conversation_source_sqlx, enable_conversation_sources_by_adapter_sqlx,
    has_running_conversation_sync_for_adapter_sqlx, hydrate_conversation_search_matches_sqlx,
    import_conversation_sessions_sqlx, import_incremental_conversation_sessions_sqlx,
    list_conversation_adapter_catalog_releases_sqlx,
    list_conversation_adapter_package_versions_sqlx, list_conversation_adapter_packages_sqlx,
    list_conversation_adapters_sqlx, list_conversation_block_locators_sqlx,
    list_conversation_question_details_sqlx, list_conversation_sessions_by_id_fragment_sqlx,
    list_conversation_sessions_sqlx, list_conversation_sources_sqlx,
    list_recent_conversation_sessions_sqlx, load_conversation_adapter_package_by_adapter_sqlx,
    load_conversation_adapter_package_sqlx, load_conversation_adapter_sqlx,
    load_conversation_block_detail_sqlx, load_conversation_question_detail_sqlx,
    load_conversation_session_detail_sqlx, load_conversation_session_versions_sqlx,
    load_conversation_source_sqlx, load_recent_conversation_sync_deltas_sqlx,
    mark_conversation_payload_policy_applied_sqlx, merge_conversation_questions_sqlx,
    migrate_legacy_conversation_adapter_hashes_sqlx, normalize_conversation_paths_sqlx,
    persist_conversation_session_observations_sqlx, resolve_conversation_part_id_prefix_sqlx,
    resolve_conversation_question_id_prefix_sqlx, resolve_conversation_session_id_prefix_sqlx,
    resolve_conversation_turn_id_prefix_sqlx, search_conversation_cards_sqlx,
    seed_prepared_builtin_conversation_adapters_sqlx, set_app_conversation_adapter_projection_sqlx,
    split_conversation_question_sqlx, update_conversation_part_translation_sqlx,
    upsert_conversation_adapter_catalog_release_sqlx, upsert_conversation_adapter_package_sqlx,
    upsert_conversation_adapter_sqlx, upsert_conversation_source_sqlx, ConversationImportResult,
    RecentConversationSessionRecord,
};
pub(crate) use database::{
    build_runtime, count_rows as count_rows_sqlx, latest_scan_status as latest_scan_status_sqlx,
    open_migrated_pool, seed_defaults_sqlx_with_catalog, seed_tenant_defaults_sqlx_with_catalog,
    Database,
};
#[cfg(test)]
pub(crate) use database::{seed_defaults_sqlx, seed_tenant_defaults_sqlx};
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
pub(crate) use memory_repo::upsert_memory_evidence_snapshot_sqlx;
pub(crate) use memory_repo::{
    archive_memory_dream_note_sqlx, count_memory_dream_notes_sqlx, count_memory_items_sqlx,
    create_memory_dream_run_sqlx, create_memory_item_sqlx, create_memory_recall_run_sqlx,
    fail_memory_recall_run_sqlx, finish_memory_dream_error_sqlx, has_active_memory_scope_lock_sqlx,
    interrupt_stale_memory_runs_sqlx, list_memory_dream_notes_sqlx, list_memory_items_sqlx,
    list_memory_recall_question_refs_sqlx, load_memory_dream_delta_rows_sqlx,
    load_memory_dream_note_detail_sqlx, load_memory_dream_state_sqlx, load_memory_item_detail_sqlx,
    load_memory_run_evidence_sqlx, load_memory_source_revision_sqlx,
    memory_evidence_stale_reason_sqlx, persist_memory_dream_success_sqlx,
    persist_memory_extraction_sqlx, persist_memory_recall_success_sqlx,
    promote_memory_dream_note_sqlx, set_memory_run_phase_sqlx, update_memory_item_sqlx,
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
pub(crate) use project_memory_repo::*;
pub(crate) use search_index_repo::{
    bump_conversation_search_source_revision_sqlx_tx,
    complete_conversation_search_index_rebuild_with_offset_sqlx,
    fail_conversation_search_index_rebuild_sqlx, load_conversation_search_index_documents_sqlx,
    load_or_create_conversation_search_index_state_sqlx,
    mark_conversation_search_index_unusable_sqlx,
    try_acquire_conversation_search_writer_lease_sqlx, ConversationSearchIndexState,
};
pub(crate) use session_memory_repo::*;
pub(crate) use settings_repo::{load_app_settings_sqlx, save_app_settings_sqlx};
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
    normalize_source_with_catalog, upsert_source_sqlx, upsert_source_sqlx_with_catalog,
};
pub(crate) use team_repo::{
    authenticate_team_tool_sqlx, cancel_team_run_sqlx, claim_team_task_sqlx,
    complete_team_run_draft_sqlx, confirm_team_run_sqlx, create_team_run_shell_sqlx,
    create_team_sqlx, create_team_tool_credential_sqlx, delete_team_sqlx, fail_team_run_sqlx,
    finish_team_task_sqlx, get_latest_team_run_snapshot_sqlx, get_team_detail_sqlx,
    get_team_run_snapshot_sqlx, get_team_task_sqlx, list_recoverable_team_run_ids_sqlx,
    list_teams_sqlx, mark_team_run_terminal_sqlx, mark_team_task_running_sqlx,
    read_team_mailbox_sqlx, review_team_run_sqlx, send_team_mailbox_sqlx, update_team_sqlx,
};
pub(crate) use tenant_repo::{
    create_local_tenant_sqlx, list_tenants_for_principal_sqlx, load_local_request_context_sqlx,
    set_active_tenant_sqlx,
};
pub(crate) use web_record_repo::{
    import_web_record_sessions_sqlx, list_web_record_sessions_sqlx,
    load_web_record_session_detail_sqlx, resolve_web_record_part_id_prefix_sqlx,
    resolve_web_record_session_id_prefix_sqlx, update_web_record_part_translation_sqlx,
};
