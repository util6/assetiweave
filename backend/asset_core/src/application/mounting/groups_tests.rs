use super::*;
use crate::backend::domain::catalog::{
    Asset, AssetFormat, AssetKind, Source, SourceKind, SourceOrigin, SourceScannerKind,
};
use crate::backend::domain::mounting::{
    AssetGroupRules, DeploymentStrategy, ProfileSafety, RuleSet, TargetProfile,
};
use crate::backend::store::Database;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn fixture_asset() -> Asset {
    Asset {
        id: "asset-1".to_string(),
        source_id: "missing-source".to_string(),
        kind: AssetKind::Skill,
        name: "fixture".to_string(),
        detector_id: "fixture".to_string(),
        detector_version: 1,
        format: crate::backend::domain::AssetFormat::Directory,
        relative_path: "fixture".to_string(),
        stored_path: "/fixture".to_string(),
        entry_file: None,
        description: None,
        content_hash: None,
        discovered_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn fixture_profile() -> TargetProfile {
    TargetProfile {
        id: "profile-1".to_string(),
        name: "fixture".to_string(),
        app_kind: None,
        target_provider_id: "fixture".to_string(),
        target_paths: vec![],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        enabled: true,
        include: crate::backend::domain::RuleSet {
            kinds: vec![AssetKind::Skill],
            tags: vec![],
            groups: vec![],
            sources: vec![],
            path_patterns: vec![],
        },
        exclude: crate::backend::domain::RuleSet {
            kinds: vec![],
            tags: vec![],
            groups: vec![],
            sources: vec![],
            path_patterns: vec![],
        },
        safety: crate::backend::domain::ProfileSafety {
            allow_remove: false,
            allow_overwrite: false,
        },
    }
}

#[test]
fn exclusive_mount_candidate_reports_missing_source_as_typed_not_found() {
    let error =
        validate_exclusive_mount_candidate(&fixture_asset(), &fixture_profile(), &HashMap::new())
            .expect_err("missing source must fail");

    assert!(matches!(error, AppError::NotFound(_)));
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
}

fn cleanup_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn exclusive_mount_cleans_unmanaged_physical_symlinks_and_skips_conflicts() {
    let test_root = unique_temp_dir("assetiweave-exclusive-mount-test");
    let sources_dir = test_root.join("sources");
    let target_dir = test_root.join("target");
    let db_path = test_root.join("test.sqlite");

    fs::create_dir_all(&sources_dir).expect("create sources dir");
    fs::create_dir_all(&target_dir).expect("create target dir");

    let skill_a_dir = sources_dir.join("skill-a");
    let skill_b_dir = sources_dir.join("skill-b");
    let skill_c_dir = sources_dir.join("skill-c");

    fs::create_dir_all(&skill_a_dir).expect("create skill a");
    fs::create_dir_all(&skill_b_dir).expect("create skill b");
    fs::create_dir_all(&skill_c_dir).expect("create skill c");

    fs::write(skill_a_dir.join("SKILL.md"), "skill a").expect("write skill a");
    fs::write(skill_b_dir.join("SKILL.md"), "skill b").expect("write skill b");
    fs::write(skill_c_dir.join("SKILL.md"), "skill c").expect("write skill c");

    let database = Database::open_async(&db_path).await.expect("open db");

    let profile = TargetProfile {
        id: "profile-test".to_string(),
        name: "Test Profile".to_string(),
        app_kind: None,
        target_provider_id: "test".to_string(),
        target_paths: vec![target_dir.to_string_lossy().to_string()],
        supported_kinds: vec![AssetKind::Skill],
        deployment_strategy: DeploymentStrategy::SymlinkToSource,
        enabled: true,
        include: RuleSet {
            kinds: vec![AssetKind::Skill],
            tags: vec![],
            groups: vec![],
            sources: vec![],
            path_patterns: vec![],
        },
        exclude: RuleSet {
            kinds: vec![],
            tags: vec![],
            groups: vec![],
            sources: vec![],
            path_patterns: vec![],
        },
        safety: ProfileSafety {
            allow_remove: true,
            allow_overwrite: true,
        },
    };
    crate::backend::store::upsert_profile_sqlx(database.pool(), "default", &profile)
        .await
        .expect("upsert profile");

    let source = Source {
        id: "source-test".to_string(),
        name: "Source Test".to_string(),
        kind: SourceKind::Local,
        root_path: sources_dir.to_string_lossy().to_string(),
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::LocalFolder,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/*".to_string()],
        exclude_globs: vec![],
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: 0,
        last_scanned_at: None,
        last_scan_status: None,
    };
    crate::backend::store::upsert_source_sqlx(database.pool(), "default", &source)
        .await
        .expect("upsert source");

    let make_asset = |id: &str, dir: &Path| Asset {
        id: id.to_string(),
        source_id: source.id.clone(),
        name: id.to_string(),
        kind: AssetKind::Skill,
        detector_id: "test".to_string(),
        detector_version: 1,
        format: AssetFormat::Directory,
        relative_path: id.to_string(),
        stored_path: dir.to_string_lossy().to_string(),
        entry_file: Some(format!("{id}/SKILL.md")),
        description: None,
        content_hash: None,
        discovered_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };

    let asset_a = make_asset("skill-a", &skill_a_dir);
    let asset_b = make_asset("skill-b", &skill_b_dir);
    let asset_c = make_asset("skill-c", &skill_c_dir);

    crate::backend::store::replace_source_assets_sqlx(
        database.pool(),
        "default",
        &source.id,
        &[asset_a.clone(), asset_b.clone(), asset_c.clone()],
    )
    .await
    .expect("replace source assets");

    let group = AssetGroup {
        id: "group-a".to_string(),
        name: "Group A".to_string(),
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
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    crate::backend::store::upsert_asset_group_sqlx(database.pool(), "default", &group)
        .await
        .expect("upsert asset group");

    crate::backend::store::replace_asset_group_members_sqlx(
        database.pool(),
        "default",
        &group.id,
        &[asset_a.id.clone()],
        &[asset_a.clone(), asset_b.clone(), asset_c.clone()],
    )
    .await
    .expect("replace group members");

    // Physical filesystem setup:
    // 1. skill-a: not mounted in target_dir yet
    // 2. skill-b: external manual symlink pointing to real source skill-b (NOT registered in asset_mounts)
    let target_b_path = target_dir.join("skill-b");
    crate::backend::infrastructure::host_filesystem::HostFilesystem::current()
        .create_symlink(&skill_b_dir, &target_b_path)
        .expect("create manual symlink for skill-b");

    // 3. skill-c: conflict ordinary file occupying target path
    let target_c_path = target_dir.join("skill-c");
    fs::write(&target_c_path, "conflicting ordinary file").expect("write conflicting file");

    let input = SkillGroupExclusiveMountInput {
        group_ids: vec![group.id.clone()],
        profile_id: profile.id.clone(),
        mount_selected: true,
        dry_run: false,
    };

    // 1. Preview exclusive mount
    let preview =
        build_skill_group_exclusive_mount_preview_sqlx(database.pool(), "default", &input)
            .await
            .expect("preview exclusive mount");

    assert_eq!(preview.mount.len(), 1);
    assert_eq!(preview.mount[0].asset_id, "skill-a");

    // Unmanaged physical symlink for skill-b MUST be included in unmount list!
    assert_eq!(preview.unmount.len(), 1);
    assert_eq!(preview.unmount[0].asset_id, "skill-b");

    // Conflicting non-symlink target for skill-c MUST be skipped!
    assert_eq!(preview.skipped.len(), 1);
    assert_eq!(preview.skipped[0].asset_id, "skill-c");
    assert!(
        preview.skipped[0]
            .reason
            .contains("target path is occupied"),
        "expected occupied conflict reason, got: {}",
        preview.skipped[0].reason
    );

    // 2. Apply exclusive mount
    let result = apply_skill_group_exclusive_mount_record(database.pool(), "default", &input)
        .await
        .expect("apply exclusive mount");

    assert!(
        result.errors.is_empty(),
        "apply errors: {:?}",
        result.errors
    );

    // Verify filesystem:
    // skill-a: symlink created
    let target_a_path = target_dir.join("skill-a");
    assert!(
        fs::symlink_metadata(&target_a_path)
            .expect("target_a symlink metadata")
            .file_type()
            .is_symlink(),
        "target_a should be a symlink"
    );

    // skill-b: external symlink has been safely unmounted/removed!
    assert!(
        fs::symlink_metadata(&target_b_path).is_err(),
        "target_b symlink must be removed"
    );

    // skill-c: conflict file is untouched
    assert!(
        target_c_path.is_file(),
        "target_c conflicting file must be preserved"
    );

    // Verify database asset_mounts:
    let mounts = crate::backend::store::load_asset_mounts_sqlx(database.pool(), "default", None)
        .await
        .expect("load asset mounts");
    let mount_a = mounts
        .iter()
        .find(|m| m.asset_id == "skill-a")
        .expect("mount a");
    assert!(mount_a.enabled, "skill-a mount should be enabled");
    let mount_b = mounts
        .iter()
        .find(|m| m.asset_id == "skill-b")
        .expect("mount b");
    assert!(!mount_b.enabled, "skill-b mount should be disabled");

    // 3. Second preview: context is now completely clean
    let second_preview =
        build_skill_group_exclusive_mount_preview_sqlx(database.pool(), "default", &input)
            .await
            .expect("second preview");

    assert_eq!(second_preview.keep.len(), 1);
    assert_eq!(second_preview.keep[0].asset_id, "skill-a");
    assert_eq!(second_preview.mount.len(), 0);
    assert_eq!(second_preview.unmount.len(), 0);
    assert_eq!(second_preview.skipped.len(), 1);
    assert_eq!(second_preview.skipped[0].asset_id, "skill-c");

    drop(database);
    cleanup_dir(&test_root);
}
