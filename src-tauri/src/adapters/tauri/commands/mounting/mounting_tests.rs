use super::*;

#[test]
fn test_mounting_module_exports_expected_commands() {
    let _ = list_profiles;
    let _ = list_target_profile_descriptors;
    let _ = refresh_target_profile_descriptors;
    let _ = create_profile;
    let _ = update_profile;
    let _ = delete_profile;
    let _ = list_asset_mounts;
    let _ = list_asset_mount_statuses;
    let _ = refresh_asset_mount_statuses;
    let _ = toggle_asset_mount;
    let _ = unmount_asset_mount;
    let _ = mount_asset_mount;
    let _ = set_asset_mount;
    let _ = list_skill_groups;
    let _ = create_skill_group;
    let _ = update_skill_group;
    let _ = delete_skill_group;
    let _ = set_skill_group_manual_members;
    let _ = preview_skill_group_exclusive_mount;
    let _ = start_batch_mount;
    let _ = get_batch_mount_task;
    let _ = list_batch_mount_tasks;
    let _ = cancel_batch_mount;
}
