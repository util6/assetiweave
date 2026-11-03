use crate::backend::application::mounting::types::TargetProfileInput;
use crate::backend::application::AppService;
use crate::backend::domain::catalog::{
    Asset, AssetFormat, AssetKind, Source, SourceKind, SourceOrigin, SourceScannerKind,
};
use crate::backend::domain::mounting::{
    AppKind, AssetGroup, AssetGroupRules, DeploymentStrategy, TargetProfile,
};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) fn fixture_asset(id: &str, source_id: &str, dir: &Path) -> Asset {
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
        stored_path: dir.to_string_lossy().to_string(),
        entry_file: Some(dir.join("SKILL.md").to_string_lossy().to_string()),
        description: None,
        content_hash: None,
        discovered_at: now.clone(),
        updated_at: now,
    }
}

pub(super) struct TestMountContext {
    pub(super) root: PathBuf,
    pub(super) target_dir: PathBuf,
    pub(super) service: AppService,
    pub(super) asset_a: Asset,
    pub(super) asset_b: Asset,
    pub(super) profile: TargetProfile,
}

impl Drop for TestMountContext {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl TestMountContext {
    pub(super) async fn setup(prefix: &str) -> Self {
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

    pub(super) fn target_path(&self, asset: &Asset) -> PathBuf {
        self.target_dir.join(&asset.id)
    }

    pub(super) async fn assert_mount_enabled(&self, asset_id: &str, expected: bool) {
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

    pub(super) async fn create_exclusive_group(
        &self,
        name: &str,
        member_asset_ids: &[&str],
    ) -> AssetGroup {
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
