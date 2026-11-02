use super::*;
use crate::backend::application::{AppError, AppService};
use crate::backend::domain::catalog::{
    Asset, AssetFormat, AssetKind, Source, SourceKind, SourceOrigin, SourceScannerKind,
};
use crate::backend::domain::mounting::{
    AppKind, AssetGroup, AssetGroupRules, DeploymentStrategy, PhysicalMountStateDto, TargetProfile,
};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn fixture_asset(id: &str, source_id: &str, dir: &Path) -> Asset {
    let now = Utc::now().to_rfc3339();
    Asset {
        id: id.to_string(),
        source_id: source_id.to_string(),
        name: id.to_string(),
        kind: AssetKind::Skill,
        detector_id: "test".to_string(),
        detector_version: 1,
        format: AssetFormat::Directory,
        relative_path: id.to_string(),
        absolute_path: dir.to_string_lossy().to_string(),
        entry_file: Some(dir.join("SKILL.md").to_string_lossy().to_string()),
        description: None,
        content_hash: None,
        discovered_at: now.clone(),
        updated_at: now,
    }
}

struct TestMountContext {
    root: PathBuf,
    target_dir: PathBuf,
    service: AppService,
    asset_a: Asset,
    asset_b: Asset,
    profile: TargetProfile,
}

impl Drop for TestMountContext {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl TestMountContext {
    async fn setup(prefix: &str) -> Self {
        let root = std::env::temp_dir().join(format!("assetiweave-{prefix}-{}", Uuid::new_v4()));
        let source_dir = root.join("sources");
        let target_dir = root.join("target");
        let skill_a_dir = source_dir.join("skill-a");
        let skill_b_dir = source_dir.join("skill-b");

        fs::create_dir_all(&skill_a_dir).expect("create skill a source dir");
        fs::create_dir_all(&skill_b_dir).expect("create skill b source dir");
        fs::create_dir_all(&target_dir).expect("create target dir");

        fs::write(skill_a_dir.join("SKILL.md"), "---\nname: skill-a\n---\n").unwrap();
        fs::write(skill_b_dir.join("SKILL.md"), "---\nname: skill-b\n---\n").unwrap();

        let service = AppService::open_with_db_path(root.join("app.db"))
            .await
            .unwrap();

        let profile = service
            .create_profile(TargetProfileInput {
                id: Some(format!("custom-profile-{}", Uuid::new_v4().simple())),
                name: "Custom Profile".to_string(),
                app_kind: Some(AppKind::Custom),
                target_provider_id: None,
                target_paths: Some(vec![target_dir.to_string_lossy().to_string()]),
                supported_kinds: Some(vec![AssetKind::Skill]),
                deployment_strategy: Some(DeploymentStrategy::SymlinkToSource),
                enabled: Some(true),
                include: None,
                exclude: None,
                safety: None,
            })
            .await
            .unwrap();

        let source = Source {
            id: format!("source-{}", Uuid::new_v4().simple()),
            name: "Test Source".to_string(),
            kind: SourceKind::Local,
            root_path: source_dir.to_string_lossy().to_string(),
            scanner_kind: SourceScannerKind::Skill,
            source_origin: SourceOrigin::LocalFolder,
            repo_root: None,
            scan_root: String::new(),
            origin_app_kind: None,
            origin_provider_id: None,
            include_globs: vec!["**/SKILL.md".to_string()],
            exclude_globs: Vec::new(),
            default_kind: Some(AssetKind::Skill),
            enabled: true,
            priority: 0,
            last_scanned_at: None,
            last_scan_status: None,
        };
        crate::backend::store::upsert_source_sqlx(service.db.pool(), service.tenant_id(), &source)
            .await
            .unwrap();

        let asset_a = fixture_asset("skill-a", &source.id, &skill_a_dir);
        let asset_b = fixture_asset("skill-b", &source.id, &skill_b_dir);
        crate::backend::store::replace_source_assets_sqlx(
            service.db.pool(),
            service.tenant_id(),
            &source.id,
            &[asset_a.clone(), asset_b.clone()],
        )
        .await
        .unwrap();

        Self {
            root,
            target_dir,
            service,
            asset_a,
            asset_b,
            profile,
        }
    }

    fn target_path(&self, asset: &Asset) -> PathBuf {
        self.target_dir.join(&asset.id)
    }

    async fn assert_mount_enabled(&self, asset_id: &str, expected: bool) {
        let mounts = self
            .service
            .list_asset_mounts(Some(asset_id))
            .await
            .unwrap();
        let mount = mounts
            .iter()
            .find(|m| m.profile_id == self.profile.id)
            .expect("mount record");
        assert_eq!(mount.enabled, expected);
    }

    async fn create_exclusive_group(&self, name: &str, member_asset_ids: &[&str]) -> AssetGroup {
        let now = Utc::now().to_rfc3339();
        let group = AssetGroup {
            id: format!("group-{}", Uuid::new_v4().simple()),
            name: name.to_string(),
            description: None,
            color: "#10b981".to_string(),
            asset_kind: AssetKind::Skill,
            display_icon: None,
            icon_svg: None,
            enabled: true,
            sort_order: 0,
            rules: AssetGroupRules {
                source_ids: vec![],
                relative_path_globs: vec![],
                name_contains: None,
            },
            created_at: now.clone(),
            updated_at: now,
        };
        let pool = self.service.db.pool();
        let tenant = self.service.tenant_id();
        crate::backend::store::upsert_asset_group_sqlx(pool, tenant, &group)
            .await
            .unwrap();
        let member_ids = member_asset_ids
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        crate::backend::store::replace_asset_group_members_sqlx(
            pool,
            tenant,
            &group.id,
            &member_ids,
            &[self.asset_a.clone(), self.asset_b.clone()],
        )
        .await
        .unwrap();
        group
    }
}

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
