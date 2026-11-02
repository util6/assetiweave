use super::test_support::*;
use super::*;
use crate::backend::application::mounting::types::SkillGroupExclusiveMountInput;
use crate::backend::application::AppError;
use crate::backend::domain::mounting::PhysicalMountStateDto;
use std::fs;
use std::path::PathBuf;

#[tokio::test]
async fn test_e2e_mount_and_unmount_closed_loop() {
    let ctx = TestMountContext::setup("closed-loop").await;
    let target_a = ctx.target_path(&ctx.asset_a);

    let initial = ctx
        .service
        .list_asset_mounts(Some(&ctx.asset_a.id))
        .await
        .unwrap();
    assert!(initial.is_empty() || !initial[0].enabled);
    assert!(!target_a.exists());

    // 1. 挂载后：物理软链接存在，list_asset_mounts 返回 enabled = true
    let mount_res = ctx
        .service
        .mount_asset_by_id(&ctx.asset_a.id, &ctx.profile.id)
        .await
        .unwrap();
    assert!(mount_res.mount.enabled);
    assert_eq!(mount_res.status.state, PhysicalMountStateDto::Mounted);
    assert!(fs::symlink_metadata(&target_a)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_link(&target_a).unwrap().canonicalize().unwrap(),
        PathBuf::from(&ctx.asset_a.absolute_path)
            .canonicalize()
            .unwrap()
    );
    ctx.assert_mount_enabled(&ctx.asset_a.id, true).await;

    // 2. 卸载后：物理软链接被删除，list_asset_mounts 返回 enabled = false
    let unmount_res = ctx
        .service
        .unmount_asset_by_id(&ctx.asset_a.id, &ctx.profile.id)
        .await
        .unwrap();
    assert!(!unmount_res.mount.enabled);
    assert_eq!(unmount_res.status.state, PhysicalMountStateDto::NotMounted);
    assert!(
        fs::symlink_metadata(&target_a).is_err(),
        "物理软链接应被删除"
    );
    ctx.assert_mount_enabled(&ctx.asset_a.id, false).await;
}

#[tokio::test]
async fn test_e2e_external_manual_symlink_consistency_and_unmount() {
    let ctx = TestMountContext::setup("manual-symlink").await;
    let target_b = ctx.target_path(&ctx.asset_b);

    // 预先通过 symlink 手动创建指向源资产目录的软链接
    #[cfg(unix)]
    std::os::unix::fs::symlink(&ctx.asset_b.absolute_path, &target_b).unwrap();
    assert!(fs::symlink_metadata(&target_b)
        .unwrap()
        .file_type()
        .is_symlink());

    // 挂载状态刷新前：asset_mounts 未记录或 enabled = false
    let initial = ctx
        .service
        .list_asset_mounts(Some(&ctx.asset_b.id))
        .await
        .unwrap();
    assert!(initial.is_empty() || !initial.iter().any(|m| m.enabled));

    // 调用挂载状态刷新：asset_mounts enabled 自动同步为 true
    let statuses = ctx
        .service
        .refresh_asset_mount_statuses(None)
        .await
        .unwrap();
    let status_b = statuses
        .iter()
        .find(|s| s.asset_id == ctx.asset_b.id && s.profile_id == ctx.profile.id)
        .unwrap();
    assert_eq!(status_b.state, PhysicalMountStateDto::Mounted);
    ctx.assert_mount_enabled(&ctx.asset_b.id, true).await;

    // 单项卸载：外部手动软链接被顺利删除，enabled 同步为 false
    let unmount_res = ctx
        .service
        .unmount_asset_by_id(&ctx.asset_b.id, &ctx.profile.id)
        .await
        .unwrap();
    assert!(!unmount_res.mount.enabled);
    assert!(
        fs::symlink_metadata(&target_b).is_err(),
        "外部软链接必须被物理删除"
    );
    ctx.assert_mount_enabled(&ctx.asset_b.id, false).await;

    // 重新创建外部手动软链接
    #[cfg(unix)]
    std::os::unix::fs::symlink(&ctx.asset_b.absolute_path, &target_b).unwrap();
    assert!(fs::symlink_metadata(&target_b).is_ok());

    // 创建仅包含 asset_a 的分组
    let group = ctx
        .create_exclusive_group("Exclusive Group", &[&ctx.asset_a.id])
        .await;

    // 分组互斥挂载预览：外部手动软链接不再被跳过，纳入 unmount 清单
    let input = SkillGroupExclusiveMountInput {
        group_ids: vec![group.id.clone()],
        profile_id: ctx.profile.id.clone(),
        mount_selected: true,
        dry_run: false,
    };
    let preview = ctx
        .service
        .preview_skill_group_exclusive_mount(input.clone())
        .await
        .unwrap();
    assert_eq!(preview.mount.len(), 1);
    assert_eq!(preview.mount[0].asset_id, ctx.asset_a.id);
    assert_eq!(preview.unmount.len(), 1);
    assert_eq!(preview.unmount[0].asset_id, ctx.asset_b.id);
    assert!(preview.skipped.is_empty(), "外部手动软链接不应被跳过");

    // 执行分组互斥挂载：外部手动软链接被顺利卸载并物理删除
    let result = ctx
        .service
        .apply_skill_group_exclusive_mount(input)
        .await
        .unwrap();
    assert!(result.errors.is_empty(), "互斥挂载执行不应报错");

    let target_a = ctx.target_path(&ctx.asset_a);
    assert!(fs::symlink_metadata(&target_a)
        .unwrap()
        .file_type()
        .is_symlink());
    ctx.assert_mount_enabled(&ctx.asset_a.id, true).await;

    assert!(
        fs::symlink_metadata(&target_b).is_err(),
        "外部手动软链接应在互斥挂载中被删除"
    );
    ctx.assert_mount_enabled(&ctx.asset_b.id, false).await;
}

#[tokio::test]
async fn test_e2e_conflict_and_damage_protection_not_degraded() {
    let ctx = TestMountContext::setup("conflict-protection").await;
    let target_a = ctx.target_path(&ctx.asset_a);

    // 1. 目标路径存在普通文件（非软链接）
    fs::write(&target_a, "conflicting ordinary file").unwrap();

    let mount_err = ctx
        .service
        .mount_asset_by_id(&ctx.asset_a.id, &ctx.profile.id)
        .await
        .unwrap_err();
    assert!(matches!(mount_err, AppError::Conflict(_)));
    assert!(target_a.is_file(), "普通文件必须安全保留");
    assert_eq!(
        fs::read_to_string(&target_a).unwrap(),
        "conflicting ordinary file"
    );

    let group = ctx
        .create_exclusive_group("Conflict Group", &[&ctx.asset_a.id])
        .await;

    let input = SkillGroupExclusiveMountInput {
        group_ids: vec![group.id.clone()],
        profile_id: ctx.profile.id.clone(),
        mount_selected: true,
        dry_run: false,
    };
    let preview = ctx
        .service
        .preview_skill_group_exclusive_mount(input.clone())
        .await
        .unwrap();
    assert_eq!(preview.skipped.len(), 1);
    assert_eq!(preview.skipped[0].asset_id, ctx.asset_a.id);
    assert!(preview.skipped[0]
        .reason
        .contains("target path is occupied"));

    let res = ctx
        .service
        .apply_skill_group_exclusive_mount(input)
        .await
        .unwrap();
    assert_eq!(res.preview.skipped.len(), 1);
    assert_eq!(
        fs::read_to_string(&target_a).unwrap(),
        "conflicting ordinary file"
    );

    // 2. 目标路径存在指向其他非源资产路径的软链接（异物软链接）
    fs::remove_file(&target_a).unwrap();
    let alien_dir = ctx.root.join("alien-dir");
    fs::create_dir_all(&alien_dir).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&alien_dir, &target_a).unwrap();

    let mount_err = ctx
        .service
        .mount_asset_by_id(&ctx.asset_a.id, &ctx.profile.id)
        .await
        .unwrap_err();
    assert!(matches!(mount_err, AppError::Conflict(_)));
    assert!(fs::symlink_metadata(&target_a)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_link(&target_a).unwrap(), alien_dir);

    let input = SkillGroupExclusiveMountInput {
        group_ids: vec![group.id.clone()],
        profile_id: ctx.profile.id.clone(),
        mount_selected: true,
        dry_run: false,
    };
    let preview = ctx
        .service
        .preview_skill_group_exclusive_mount(input)
        .await
        .unwrap();
    assert_eq!(preview.skipped.len(), 1);
    assert_eq!(preview.skipped[0].asset_id, ctx.asset_a.id);
    assert_eq!(fs::read_link(&target_a).unwrap(), alien_dir);
}

#[tokio::test]
async fn test_e2e_target_profile_deletion_guard() {
    let ctx = TestMountContext::setup("profile-deletion-guard").await;

    // 1. 挂载 asset_a
    ctx.service
        .mount_asset_by_id(&ctx.asset_a.id, &ctx.profile.id)
        .await
        .unwrap();
    let target_a = ctx.target_path(&ctx.asset_a);
    assert!(fs::symlink_metadata(&target_a)
        .unwrap()
        .file_type()
        .is_symlink());

    // 2. 存在物理处于 Mounted 状态的软链接时，拒绝删除该 Profile
    let delete_err = ctx
        .service
        .delete_profile(ctx.profile.id.clone())
        .await
        .unwrap_err();
    assert!(matches!(delete_err, AppError::Conflict(_)));
    assert!(delete_err
        .to_string()
        .contains("profile has mounted assets"));
    assert!(ctx
        .service
        .list_profiles()
        .await
        .unwrap()
        .iter()
        .any(|p| p.id == ctx.profile.id));

    // 3. 卸载后，允许顺利删除自定义 Profile
    ctx.service
        .unmount_asset_by_id(&ctx.asset_a.id, &ctx.profile.id)
        .await
        .unwrap();
    assert!(fs::symlink_metadata(&target_a).is_err());

    ctx.service
        .delete_profile(ctx.profile.id.clone())
        .await
        .unwrap();
    assert!(!ctx
        .service
        .list_profiles()
        .await
        .unwrap()
        .iter()
        .any(|p| p.id == ctx.profile.id));
}
