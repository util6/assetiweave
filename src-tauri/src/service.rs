use crate::{
    commands, executor, logs, path_utils, planner, platform, scanner, store, targeting,
    types::{
        AppOverview, AppResult, AppShortcut, ApplyAssetGroupMountResult,
        ApplySkillGroupExclusiveMountResult, AssetGroupInput, AssetMountStatus,
        AssetMountUpdateResult, ExecutionResult, NavigationModel, PhysicalMountStateDto,
        SkillGroupExclusiveMountInput, SkillGroupExclusiveMountPreview, SourceInput,
        TargetProfileInput,
    },
};
use assetiweave_core::{
    Asset, AssetGroup, AssetGroupDetail, AssetKind, AssetMount, DeploymentPlan, DeploymentStrategy,
    Source, SourceOrigin, SourceScannerKind, TargetProfile,
};
use chrono::Utc;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub(crate) struct AppService {
    conn: Connection,
    db_path: PathBuf,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListAssetsParams {
    pub(crate) kind: Option<AssetKind>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AssetIdParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RequiredAssetIdParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProfileIdParams {
    #[serde(alias = "profileId")]
    pub(crate) profile_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct IdParams {
    pub(crate) id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AssetProfileParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetAssetMountParams {
    #[serde(alias = "assetId")]
    pub(crate) asset_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
    pub(crate) enabled: bool,
    pub(crate) strategy: Option<DeploymentStrategy>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ApplySkillGroupMountParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateSourceParams {
    pub(crate) source: SourceInput,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateSourceParams {
    pub(crate) source: Source,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateProfileParams {
    pub(crate) input: TargetProfileInput,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateProfileParams {
    pub(crate) profile: TargetProfile,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateNavigationModelParams {
    pub(crate) model: NavigationModel,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateAppShortcutsParams {
    pub(crate) shortcuts: Vec<AppShortcut>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateSkillGroupParams {
    pub(crate) input: AssetGroupInput,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateSkillGroupParams {
    pub(crate) group: AssetGroup,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GroupIdParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SetSkillGroupManualMembersParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
    #[serde(alias = "assetIds")]
    pub(crate) asset_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SkillGroupExclusiveMountParams {
    pub(crate) input: SkillGroupExclusiveMountInput,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExecutePlanParams {
    pub(crate) plan: DeploymentPlan,
    #[serde(alias = "actionIds")]
    pub(crate) action_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LogsGetSnapshotParams {
    #[serde(alias = "fileName")]
    pub(crate) file_name: Option<String>,
    #[serde(alias = "lineLimit")]
    pub(crate) line_limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LogsWriteOperationParams {
    pub(crate) level: String,
    pub(crate) operation: String,
    pub(crate) message: String,
    pub(crate) fields: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RevealPathParams {
    pub(crate) path: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SourceAddParams {
    #[serde(flatten)]
    pub(crate) source: SourceInput,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AssetRefParams {
    #[serde(alias = "assetRef")]
    pub(crate) asset_ref: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
    #[serde(default)]
    pub(crate) unmount: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ImportSkillParams {
    pub(crate) from: String,
    pub(crate) name: Option<String>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SourceRemoveParams {
    pub(crate) id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SourceScanParams {
    pub(crate) kind: Option<AssetKind>,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SkillGroupMountParams {
    #[serde(alias = "groupId")]
    pub(crate) group_id: String,
    #[serde(alias = "profileId")]
    pub(crate) profile_id: String,
    #[serde(default, alias = "dryRun")]
    pub(crate) dry_run: bool,
    #[serde(default)]
    pub(crate) yes: bool,
}

impl AppService {
    pub(crate) fn open_for_engine() -> AppResult<Self> {
        Self::open_with_db_path(engine_db_path()?)
    }

    pub(crate) fn open_with_db_path(db_path: PathBuf) -> AppResult<Self> {
        let conn = store::open_initialized(&db_path)?;
        Ok(Self { conn, db_path })
    }

    pub(crate) fn overview(&self) -> AppResult<AppOverview> {
        Ok(AppOverview {
            source_count: store::count_rows(&self.conn, "sources")?,
            asset_count: store::count_rows(&self.conn, "assets")?,
            profile_count: store::count_rows(&self.conn, "profiles")?,
            last_scan_status: store::latest_scan_status(&self.conn)?,
        })
    }

    pub(crate) fn list_sources(&self) -> AppResult<Vec<Source>> {
        store::load_sources(&self.conn)
    }

    pub(crate) fn list_skill_sources(&self) -> AppResult<Vec<Source>> {
        store::load_skill_sources(&self.conn)
    }

    pub(crate) fn add_source(&self, source: SourceInput) -> AppResult<Source> {
        let source = source_from_input(source);
        store::upsert_source(&self.conn, &source)?;
        Ok(source)
    }

    pub(crate) fn update_source(&self, source: Source) -> AppResult<Source> {
        let source = store::normalize_source(&source);
        if !store::load_sources(&self.conn)?
            .iter()
            .any(|candidate| candidate.id == source.id)
        {
            return Err(format!("source not found: {}", source.id));
        }
        store::upsert_source(&self.conn, &source)?;
        Ok(source)
    }

    pub(crate) fn delete_source(&self, id: String) -> AppResult<()> {
        self.remove_source(SourceRemoveParams {
            id,
            dry_run: false,
            yes: true,
        })
        .map(|_| ())
    }

    pub(crate) fn add_source_with_options(&self, params: SourceAddParams) -> AppResult<Value> {
        let source = source_from_input(params.source);
        if params.dry_run {
            return Ok(json!({ "dry_run": true, "source": source }));
        }
        store::upsert_source(&self.conn, &source)?;
        Ok(json!({ "dry_run": false, "source": source }))
    }

    pub(crate) fn remove_source(&self, params: SourceRemoveParams) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err("source.remove requires --yes".to_string());
        }
        let sources = store::load_sources(&self.conn)?;
        let source = sources
            .into_iter()
            .find(|source| source.id == params.id)
            .ok_or_else(|| format!("source not found: {}", params.id))?;
        if is_protected_source(&source) {
            return Err(
                "default Skill source is managed by AssetIWeave and cannot be deleted".to_string(),
            );
        }
        if params.dry_run {
            return Ok(json!({ "removed": false, "dry_run": true, "source": source }));
        }
        store::delete_source(&self.conn, &source.id)?;
        commands::cleanup_orphan_asset_records(&self.conn)?;
        Ok(json!({ "removed": true, "source_id": source.id }))
    }

    pub(crate) fn scan_sources(&self, params: SourceScanParams) -> AppResult<Vec<Asset>> {
        if params.dry_run {
            return store::load_assets_by_kind(&self.conn, params.kind);
        }
        commands::refresh_all_sources(&self.conn)?;
        store::load_assets_by_kind(&self.conn, params.kind)
    }

    pub(crate) fn scan_skill_sources(&self) -> AppResult<Vec<Asset>> {
        let sources = store::load_skill_sources(&self.conn)?;
        commands::scan_selected_sources(&self.conn, sources, scanner::scan_skill_source)?;
        store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))
    }

    pub(crate) fn list_profiles(&self) -> AppResult<Vec<TargetProfile>> {
        store::load_profiles(&self.conn)
    }

    pub(crate) fn create_profile(&self, input: TargetProfileInput) -> AppResult<TargetProfile> {
        let profile = commands::target_profile_from_input(input)?;
        if store::load_profiles(&self.conn)?
            .iter()
            .any(|candidate| candidate.id == profile.id)
        {
            return Err(format!("profile already exists: {}", profile.id));
        }
        store::upsert_profile(&self.conn, &profile)?;
        Ok(profile)
    }

    pub(crate) fn update_profile(&self, profile: TargetProfile) -> AppResult<TargetProfile> {
        commands::validate_target_profile(&profile)?;
        let existing_profile = store::load_profiles(&self.conn)?
            .into_iter()
            .find(|candidate| candidate.id == profile.id);
        let Some(existing_profile) = existing_profile else {
            return Err(format!("profile not found: {}", profile.id));
        };
        commands::ensure_default_profile_update_is_allowed(&existing_profile, &profile)?;
        store::upsert_profile(&self.conn, &profile)?;
        Ok(profile)
    }

    pub(crate) fn delete_profile(&self, id: String) -> AppResult<()> {
        if !store::load_profiles(&self.conn)?
            .iter()
            .any(|profile| profile.id == id)
        {
            return Err(format!("profile not found: {id}"));
        }
        commands::ensure_profile_can_be_deleted(&self.conn, &id)?;
        store::delete_profile(&self.conn, &id)
    }

    pub(crate) fn navigation_model(&self) -> AppResult<crate::types::NavigationModel> {
        store::load_navigation_model(&self.conn)
    }

    pub(crate) fn update_navigation_model(
        &self,
        model: NavigationModel,
    ) -> AppResult<NavigationModel> {
        store::save_navigation_model(&self.conn, &model)?;
        store::load_navigation_model(&self.conn)
    }

    pub(crate) fn list_app_shortcuts(&self) -> AppResult<Vec<crate::types::AppShortcut>> {
        store::load_app_shortcuts(&self.conn)
    }

    pub(crate) fn list_app_shortcut_settings(&self) -> AppResult<Vec<crate::types::AppShortcut>> {
        store::load_app_shortcut_settings(&self.conn)
    }

    pub(crate) fn update_app_shortcuts(
        &self,
        shortcuts: Vec<AppShortcut>,
    ) -> AppResult<Vec<AppShortcut>> {
        store::save_app_shortcuts(&self.conn, &shortcuts)?;
        store::load_app_shortcut_settings(&self.conn)
    }

    pub(crate) fn list_assets(&self, params: ListAssetsParams) -> AppResult<Vec<Asset>> {
        store::load_assets_by_kind(&self.conn, params.kind)
    }

    pub(crate) fn update_asset_description(
        &self,
        asset_id: String,
        description: Option<String>,
    ) -> AppResult<Asset> {
        let mut asset = store::load_assets(&self.conn)?
            .into_iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        if !store::load_sources(&self.conn)?
            .iter()
            .any(|source| source.id == asset.source_id)
        {
            return Err(format!("source not found: {}", asset.source_id));
        }

        let source_path = path_utils::expand_path(&asset.absolute_path)?;
        if !source_path.exists() {
            return Err(format!(
                "asset source path does not exist: {}",
                source_path.display()
            ));
        }

        asset.description = description
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        asset.updated_at = Utc::now().to_rfc3339();
        store::update_asset_description(&self.conn, &asset)?;
        Ok(asset)
    }

    pub(crate) fn delete_asset(&self, asset_id: String, unmount: bool) -> AppResult<Asset> {
        let asset = store::load_assets(&self.conn)?
            .into_iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        if asset.kind != AssetKind::Skill {
            return Err("only skill assets can be deleted from the catalog".to_string());
        }
        self.delete_skill(AssetRefParams {
            asset_ref: asset.id.clone(),
            profile_id: None,
            dry_run: false,
            yes: true,
            unmount,
        })?;
        Ok(asset)
    }

    pub(crate) fn list_asset_mounts(&self, asset_id: Option<&str>) -> AppResult<Vec<AssetMount>> {
        store::load_asset_mounts(&self.conn, asset_id)
    }

    pub(crate) fn list_asset_mount_statuses(
        &self,
        asset_id: Option<&str>,
    ) -> AppResult<Vec<AssetMountStatus>> {
        commands::scan_asset_mount_statuses(&self.conn, asset_id)
    }

    pub(crate) fn refresh_asset_mount_statuses(
        &self,
        asset_id: Option<&str>,
    ) -> AppResult<Vec<AssetMountStatus>> {
        commands::sync_asset_mount_observations(&self.conn, asset_id)
    }

    pub(crate) fn create_plan(&self, profile_id: Option<&str>) -> AppResult<DeploymentPlan> {
        let assets = store::load_assets(&self.conn)?;
        let profiles = store::load_profiles(&self.conn)?;
        let mounts = store::load_enabled_asset_mounts(&self.conn, profile_id)?;
        Ok(planner::build_plan(&assets, &profiles, &mounts, profile_id))
    }

    pub(crate) fn list_skills(&self) -> AppResult<Vec<Asset>> {
        store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))
    }

    pub(crate) fn import_skill(&self, params: ImportSkillParams) -> AppResult<Value> {
        let source_dir = path_utils::expand_path(&params.from)?;
        if !source_dir.is_dir() {
            return Err(format!(
                "skill import source is not a directory: {}",
                source_dir.display()
            ));
        }
        let skill_file = source_dir.join("SKILL.md");
        if !skill_file.is_file() {
            return Err(format!(
                "skill import source must contain SKILL.md: {}",
                skill_file.display()
            ));
        }

        let name = params
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .or_else(|| {
                source_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .ok_or_else(|| "skill import name could not be inferred".to_string())?;
        let target_dir = path_utils::app_library_skill_root()?
            .join("imported")
            .join(&name);
        if target_dir.exists() {
            return Err(format!(
                "imported skill already exists: {}",
                target_dir.display()
            ));
        }

        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "source_path": source_dir,
                "target_path": target_dir,
                "name": name
            }));
        }

        commands::copy_dir(&source_dir, &target_dir)?;
        let library_source = commands::assetiweave_library_source();
        store::upsert_source(&self.conn, &library_source)?;
        let library_assets = scanner::scan_skill_source(&library_source)?;
        store::replace_source_assets(&self.conn, &library_source.id, &library_assets)?;
        let asset = library_assets
            .into_iter()
            .find(|candidate| candidate.absolute_path == target_dir.to_string_lossy())
            .ok_or_else(|| "imported skill was copied but not found during rescan".to_string())?;
        Ok(json!({ "dry_run": false, "asset": asset }))
    }

    pub(crate) fn delete_skill(&self, params: AssetRefParams) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err("skill.delete requires --yes".to_string());
        }
        let asset = self.resolve_skill_asset(&params.asset_ref)?;
        let source = store::load_sources(&self.conn)?
            .into_iter()
            .find(|source| source.id == asset.source_id)
            .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
        if source.source_origin != SourceOrigin::AssetiweaveLibrary {
            return Err(
                "only AssetIWeave library skills can be deleted; remove the source or unmount the skill instead"
                    .to_string(),
            );
        }

        let enabled_mounts = store::load_asset_mounts(&self.conn, Some(&asset.id))?
            .into_iter()
            .filter(|mount| mount.enabled)
            .collect::<Vec<_>>();
        if !enabled_mounts.is_empty() && !params.unmount {
            return Err(
                "skill has enabled mounts; pass --unmount to remove managed mounts first"
                    .to_string(),
            );
        }
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "deleted": false,
                "asset": asset,
                "enabled_mounts": enabled_mounts
            }));
        }

        for mount in enabled_mounts {
            commands::unmount_asset_mount_record(&self.conn, &asset.id, &mount.profile_id)?;
        }
        let asset_path = PathBuf::from(&asset.absolute_path);
        if asset_path.exists() {
            let metadata = fs::symlink_metadata(&asset_path).map_err(|error| error.to_string())?;
            if metadata.is_dir() {
                fs::remove_dir_all(&asset_path).map_err(|error| error.to_string())?;
            } else {
                fs::remove_file(&asset_path).map_err(|error| error.to_string())?;
            }
        }
        commands::refresh_recorded_assets(&self.conn)?;
        Ok(json!({ "deleted": true, "asset_id": asset.id }))
    }

    pub(crate) fn mount_skill(&self, params: AssetRefParams, enabled: bool) -> AppResult<Value> {
        let profile_id = params
            .profile_id
            .as_deref()
            .ok_or_else(|| "profile_id is required".to_string())?;
        let asset = self.resolve_skill_asset(&params.asset_ref)?;
        if params.dry_run {
            let profile = store::load_profiles(&self.conn)?
                .into_iter()
                .find(|profile| profile.id == profile_id)
                .ok_or_else(|| format!("profile not found: {profile_id}"))?;
            let inspection = targeting::inspect_mount(&profile, &asset)?;
            return Ok(json!({
                "dry_run": true,
                "asset": asset,
                "profile_id": profile_id,
                "enabled": enabled,
                "status": AssetMountStatus {
                    asset_id: asset.id,
                    profile_id: profile.id,
                    target_dir: inspection.target_dir,
                    target_path: inspection.target_path,
                    state: PhysicalMountStateDto::from(inspection.state),
                    linked_source: inspection.linked_source,
                }
            }));
        }

        let update = if enabled {
            self.mount_asset_by_id(&asset.id, profile_id)?
        } else {
            self.unmount_asset_by_id(&asset.id, profile_id)?
        };
        Ok(json!(update))
    }

    pub(crate) fn list_skill_groups(&self) -> AppResult<Vec<AssetGroupDetail>> {
        commands::cleanup_orphan_asset_records(&self.conn)?;
        let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
        store::load_skill_group_details(&self.conn, &assets)
    }

    pub(crate) fn create_skill_group(&self, input: AssetGroupInput) -> AppResult<AssetGroupDetail> {
        let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
        let now = Utc::now().to_rfc3339();
        let group = commands::asset_group_from_input(input, now.clone(), now);
        store::upsert_asset_group(&self.conn, &group)?;
        store::load_skill_group_detail(&self.conn, &group.id, &assets)
    }

    pub(crate) fn update_skill_group(&self, group: AssetGroup) -> AppResult<AssetGroupDetail> {
        let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
        let mut group = group;
        group.updated_at = Utc::now().to_rfc3339();
        store::upsert_asset_group(&self.conn, &group)?;
        store::load_skill_group_detail(&self.conn, &group.id, &assets)
    }

    pub(crate) fn delete_skill_group(&self, group_id: String) -> AppResult<()> {
        let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
        store::load_skill_group_detail(&self.conn, &group_id, &assets)?;
        store::delete_asset_group(&self.conn, &group_id)
    }

    pub(crate) fn set_skill_group_manual_members(
        &self,
        group_id: String,
        asset_ids: Vec<String>,
    ) -> AppResult<AssetGroupDetail> {
        let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
        store::replace_asset_group_members(&self.conn, &group_id, &asset_ids, &assets)?;
        store::load_skill_group_detail(&self.conn, &group_id, &assets)
    }

    pub(crate) fn mount_skill_group(
        &self,
        params: SkillGroupMountParams,
        enabled: bool,
    ) -> AppResult<Value> {
        if !enabled && !params.dry_run && !params.yes {
            return Err("skill.group.unmount requires --yes".to_string());
        }
        if params.dry_run {
            let assets = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?;
            let detail = store::load_skill_group_detail(&self.conn, &params.group_id, &assets)?;
            return Ok(json!({
                "dry_run": true,
                "group_id": params.group_id,
                "profile_id": params.profile_id,
                "enabled": enabled,
                "requested_count": detail.members.len()
            }));
        }
        let result = self.apply_skill_group_mount(&params.group_id, &params.profile_id, enabled)?;
        Ok(json!(result))
    }

    pub(crate) fn apply_skill_group_mount(
        &self,
        group_id: &str,
        profile_id: &str,
        enabled: bool,
    ) -> AppResult<ApplyAssetGroupMountResult> {
        commands::apply_skill_group_mount_record(&self.conn, group_id, profile_id, enabled)
    }

    pub(crate) fn preview_skill_group_exclusive_mount(
        &self,
        input: SkillGroupExclusiveMountInput,
    ) -> AppResult<SkillGroupExclusiveMountPreview> {
        commands::build_skill_group_exclusive_mount_preview(&self.conn, &input)
    }

    pub(crate) fn apply_skill_group_exclusive_mount(
        &self,
        input: SkillGroupExclusiveMountInput,
    ) -> AppResult<ApplySkillGroupExclusiveMountResult> {
        commands::apply_skill_group_exclusive_mount_record(&self.conn, &input)
    }

    pub(crate) fn mount_asset_by_id(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMountUpdateResult> {
        commands::mount_asset_mount_record(&self.conn, asset_id, profile_id)
    }

    pub(crate) fn unmount_asset_by_id(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMountUpdateResult> {
        commands::unmount_asset_mount_record(&self.conn, asset_id, profile_id)
    }

    pub(crate) fn toggle_asset_mount(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMount> {
        let (asset, profile) = load_mount_asset_and_profile(&self.conn, asset_id, profile_id)?;
        let inspection = targeting::inspect_mount(&profile, &asset)?;
        commands::set_asset_mount_record(
            &self.conn,
            asset_id,
            profile_id,
            !matches!(inspection.state, targeting::PhysicalMountState::Mounted),
            None,
        )
    }

    pub(crate) fn set_asset_mount(
        &self,
        asset_id: &str,
        profile_id: &str,
        enabled: bool,
        strategy: Option<DeploymentStrategy>,
    ) -> AppResult<AssetMount> {
        commands::set_asset_mount_record(&self.conn, asset_id, profile_id, enabled, strategy)
    }

    pub(crate) fn adopt_app_local_skill(&self, asset_id: String) -> AppResult<Asset> {
        let assets = store::load_assets(&self.conn)?;
        let asset = assets
            .iter()
            .find(|candidate| candidate.id == asset_id)
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        if !matches!(asset.kind, AssetKind::Skill) {
            return Err("only skill assets can be adopted".to_string());
        }

        let source = store::load_sources(&self.conn)?
            .into_iter()
            .find(|candidate| candidate.id == asset.source_id)
            .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
        if !matches!(
            source.source_origin,
            SourceOrigin::AppTarget | SourceOrigin::AppLocal
        ) {
            return Err("only app-local skill assets need adoption".to_string());
        }

        let origin_bucket = source
            .origin_app_kind
            .map(|kind| format!("{kind:?}").to_ascii_lowercase())
            .unwrap_or_else(|| source.id.clone());
        let target_dir = path_utils::app_library_skill_root()?
            .join(origin_bucket)
            .join(&asset.name);
        if target_dir.exists() {
            return Err(format!(
                "adopted skill already exists: {}",
                target_dir.display()
            ));
        }
        commands::copy_dir(Path::new(&asset.absolute_path), &target_dir)?;

        let library_source = commands::assetiweave_library_source();
        store::upsert_source(&self.conn, &library_source)?;
        let library_assets = scanner::scan_skill_source(&library_source)?;
        store::replace_source_assets(&self.conn, &library_source.id, &library_assets)?;
        library_assets
            .into_iter()
            .find(|candidate| candidate.absolute_path == target_dir.to_string_lossy())
            .ok_or_else(|| "adopted skill was copied but not found during rescan".to_string())
    }

    pub(crate) fn execute_plan(
        &self,
        plan: DeploymentPlan,
        action_ids: Option<Vec<String>>,
    ) -> AppResult<ExecutionResult> {
        let profiles = store::load_profiles(&self.conn)?;
        let assets = store::load_assets(&self.conn)?;
        executor::execute_deployment_plan(
            &self.conn,
            &profiles,
            &assets,
            &plan,
            action_ids.as_deref(),
        )
    }

    pub(crate) fn logs_get_snapshot(
        &self,
        file_name: Option<String>,
        line_limit: Option<usize>,
    ) -> AppResult<logs::LogSnapshot> {
        logs::logs_get_snapshot(file_name, line_limit)
    }

    pub(crate) fn logs_open_log_directory(&self) -> AppResult<()> {
        logs::logs_open_log_directory()
    }

    pub(crate) fn logs_write_operation(
        &self,
        level: String,
        operation: String,
        message: String,
        fields: Option<BTreeMap<String, String>>,
    ) -> AppResult<()> {
        logs::logs_write_operation(level, operation, message, fields)
    }

    pub(crate) fn reveal_path(&self, path: String) -> AppResult<()> {
        platform::reveal_path(path)
    }

    pub(crate) fn run_doctor(&self) -> AppResult<Value> {
        let library_root = path_utils::app_library_skill_root()?;
        Ok(json!({
            "checks": [
                { "name": "database", "status": "pass", "message": self.db_path.to_string_lossy() },
                {
                    "name": "library_skills",
                    "status": if library_root.exists() { "pass" } else { "fail" },
                    "message": library_root.to_string_lossy()
                },
                {
                    "name": "sources",
                    "status": "pass",
                    "message": format!("{} sources", store::count_rows(&self.conn, "sources")?)
                }
            ]
        }))
    }

    fn resolve_skill_asset(&self, asset_ref: &str) -> AppResult<Asset> {
        let needle = asset_ref.trim();
        if needle.is_empty() {
            return Err("asset ref is required".to_string());
        }
        let matches = store::load_assets_by_kind(&self.conn, Some(AssetKind::Skill))?
            .into_iter()
            .filter(|asset| asset.id == needle || asset.name == needle)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [asset] => Ok(asset.clone()),
            [] => Err(format!("skill not found: {needle}")),
            many => Err(format!(
                "ambiguous skill ref {needle}: {}",
                many.iter()
                    .map(|asset| format!("{} ({})", asset.name, asset.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }
}

fn is_protected_source(source: &Source) -> bool {
    source.id == "assetiweave-library-skills"
        || matches!(source.source_origin, SourceOrigin::AssetiweaveLibrary)
}

fn engine_db_path() -> AppResult<PathBuf> {
    if let Ok(path) = env::var("ASSETIWEAVE_DB_PATH") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    path_utils::app_db_path()
}

fn load_mount_asset_and_profile(
    conn: &Connection,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, TargetProfile)> {
    let asset = store::load_assets(conn)?
        .into_iter()
        .find(|asset| asset.id == asset_id)
        .ok_or_else(|| format!("asset not found: {asset_id}"))?;
    let profile = store::load_profiles(conn)?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("profile not found: {profile_id}"))?;

    Ok((asset, profile))
}

fn source_from_input(source: SourceInput) -> Source {
    let source = Source {
        id: source.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: source.name,
        kind: source.kind,
        root_path: source.root_path,
        scanner_kind: source.scanner_kind.unwrap_or(SourceScannerKind::Mixed),
        source_origin: source.source_origin.unwrap_or(SourceOrigin::LocalFolder),
        repo_root: source.repo_root,
        scan_root: source.scan_root.unwrap_or_default(),
        origin_app_kind: source.origin_app_kind,
        include_globs: source.include_globs,
        exclude_globs: source.exclude_globs,
        default_kind: source.default_kind,
        enabled: source.enabled,
        priority: source.priority,
        last_scanned_at: None,
        last_scan_status: Some("pending".to_string()),
    };
    store::normalize_source(&source)
}

pub(crate) const CLI_SERVICE_METHODS: &[&str] = &[
    "overview.get",
    "source.list",
    "source.add",
    "source.remove",
    "source.scan",
    "profile.list",
    "asset.list",
    "skill.list",
    "skill.import",
    "skill.delete",
    "skill.mount",
    "skill.unmount",
    "skill.group.list",
    "skill.group.mount",
    "skill.group.unmount",
    "doctor.run",
];

pub(crate) const TAURI_COMMAND_METHODS: &[&str] = &[
    "get_app_overview",
    "list_assets",
    "list_sources",
    "list_skill_sources",
    "create_source",
    "update_source",
    "delete_source",
    "update_asset_description",
    "delete_asset",
    "list_profiles",
    "create_profile",
    "update_profile",
    "delete_profile",
    "get_navigation_model",
    "update_navigation_model",
    "list_app_shortcuts",
    "list_app_shortcut_settings",
    "update_app_shortcuts",
    "list_asset_mounts",
    "list_asset_mount_statuses",
    "refresh_asset_mount_statuses",
    "list_skill_groups",
    "create_skill_group",
    "update_skill_group",
    "delete_skill_group",
    "set_skill_group_manual_members",
    "apply_skill_group_mount",
    "preview_skill_group_exclusive_mount",
    "apply_skill_group_exclusive_mount",
    "toggle_asset_mount",
    "mount_asset_mount",
    "unmount_asset_mount",
    "set_asset_mount",
    "scan_sources",
    "scan_skill_sources",
    "adopt_app_local_skill",
    "create_plan",
    "execute_plan",
    "logs_get_snapshot",
    "logs_open_log_directory",
    "logs_write_operation",
    "reveal_path",
];

pub(crate) fn is_service_method(method: &str) -> bool {
    CLI_SERVICE_METHODS.contains(&method) || TAURI_COMMAND_METHODS.contains(&method)
}

pub(crate) fn schema_index() -> Value {
    let mut methods = Vec::new();
    methods.extend(CLI_SERVICE_METHODS.iter().copied());
    methods.extend(TAURI_COMMAND_METHODS.iter().copied());
    methods.extend(["schema.list", "schema.get"]);
    json!({ "methods": methods })
}

pub(crate) fn schema_get(method: &str) -> Value {
    let mut schemas = BTreeMap::new();
    schemas.insert(
        "overview.get",
        json!({
            "params": {},
            "cli": "assetiweave-cli overview"
        }),
    );
    schemas.insert(
        "source.list",
        json!({
            "params": {},
            "cli": "assetiweave-cli source list"
        }),
    );
    schemas.insert(
        "source.add",
        json!({
            "params": {
                "name": "string",
                "kind": "local|git_checkout|import|custom",
                "root_path": "string",
                "scanner_kind": "skill|mcp|prompt|rule|mixed|custom?",
                "source_origin": "local_folder|git_repo|assetiweave_library?",
                "default_kind": "prompt|rule|memory|skill|mcp|agent|command|workflow|profile|custom|unclassified?",
                "enabled": "bool?",
                "dry_run": "bool?"
            },
            "cli": "assetiweave-cli source add --name <name> --path <path> [--scanner-kind skill] [--dry-run]"
        }),
    );
    schemas.insert(
        "source.remove",
        json!({
            "params": { "id": "string", "yes": "bool", "dry_run": "bool?" },
            "cli": "assetiweave-cli source remove <source-id> --yes [--dry-run]"
        }),
    );
    schemas.insert(
        "source.scan",
        json!({
            "params": { "kind": "asset_kind?", "dry_run": "bool?" },
            "cli": "assetiweave-cli source scan [--kind skill] [--dry-run]"
        }),
    );
    schemas.insert(
        "profile.list",
        json!({
            "params": {},
            "cli": "assetiweave-cli profile list"
        }),
    );
    schemas.insert(
        "asset.list",
        json!({
            "params": { "kind": "prompt|rule|memory|skill|mcp|agent|command|workflow|profile|custom|unclassified?" },
            "cli": "assetiweave-cli asset list [--kind skill]"
        }),
    );
    schemas.insert(
        "skill.list",
        json!({
            "params": {},
            "cli": "assetiweave-cli skill list"
        }),
    );
    schemas.insert(
        "skill.import",
        json!({
            "params": { "from": "string", "name": "string?", "dry_run": "bool?" },
            "cli": "assetiweave-cli skill import --from <dir> [--name <name>] [--dry-run]"
        }),
    );
    schemas.insert(
        "skill.mount",
        json!({
            "params": { "asset_ref": "string", "profile_id": "string", "dry_run": "bool?" },
            "cli": "assetiweave-cli skill mount <asset-ref> --profile <profile-id> [--dry-run]"
        }),
    );
    schemas.insert(
        "skill.unmount",
        json!({
            "params": { "asset_ref": "string", "profile_id": "string", "dry_run": "bool?" },
            "cli": "assetiweave-cli skill unmount <asset-ref> --profile <profile-id> [--dry-run]"
        }),
    );
    schemas.insert("skill.delete", json!({
        "params": { "asset_ref": "string", "yes": "bool", "unmount": "bool?", "dry_run": "bool?" },
        "cli": "assetiweave-cli skill delete <asset-ref> --yes [--unmount] [--dry-run]"
    }));
    schemas.insert(
        "skill.group.list",
        json!({
            "params": {},
            "cli": "assetiweave-cli skill group list"
        }),
    );
    schemas.insert(
        "skill.group.mount",
        json!({
            "params": { "group_id": "string", "profile_id": "string", "dry_run": "bool?" },
            "cli": "assetiweave-cli skill group mount <group-id> --profile <profile-id> [--dry-run]"
        }),
    );
    schemas.insert(
        "skill.group.unmount",
        json!({
            "params": { "group_id": "string", "profile_id": "string", "yes": "bool", "dry_run": "bool?" },
            "cli": "assetiweave-cli skill group unmount <group-id> --profile <profile-id> --yes [--dry-run]"
        }),
    );
    schemas.insert(
        "doctor.run",
        json!({
            "params": {},
            "cli": "assetiweave-cli doctor"
        }),
    );
    schemas.insert(
        "schema.list",
        json!({
            "params": {},
            "cli": "assetiweave-cli schema"
        }),
    );
    schemas.insert(
        "schema.get",
        json!({
            "params": { "method": "string?" },
            "cli": "assetiweave-cli schema get <method>"
        }),
    );
    schemas.remove(method).unwrap_or_else(|| {
        if TAURI_COMMAND_METHODS.contains(&method) {
            json!({
                "method": method,
                "params": "same JSON shape as the Tauri invoke command arguments",
                "cli": format!("assetiweave-cli api call {method} --json '<json-params>'")
            })
        } else {
            json!({ "method": method, "params": {}, "cli": null })
        }
    })
}
