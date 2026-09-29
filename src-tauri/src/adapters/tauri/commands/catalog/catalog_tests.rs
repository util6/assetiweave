use super::*;

#[test]
fn test_catalog_module_exports_expected_commands() {
    let _ = list_assets;
    let _ = list_source_assets;
    let _ = update_asset_description;
    let _ = delete_asset;
    let _ = list_sources;
    let _ = list_skill_sources;
    let _ = create_source;
    let _ = update_source;
    let _ = delete_source;
    let _ = start_source_scan;
    let _ = get_source_scan_task;
    let _ = list_source_scan_tasks;
    let _ = cancel_source_scan;
    let _ = get_skill_backup_settings;
    let _ = update_skill_backup_settings;
    let _ = backup_skill;
    let _ = backup_skills;
    let _ = get_skill_backup_task;
    let _ = search_skills;
    let _ = start_skill_acquire;
    let _ = acquire_skill;
    let _ = get_skill_acquire_task;
    let _ = list_skill_acquire_tasks;
    let _ = cancel_skill_acquire_task;
    let _ = list_skill_remote_sources;
    let _ = check_skill_remote_sources;
}
