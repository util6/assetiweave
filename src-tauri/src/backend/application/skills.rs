use super::prelude::*;

struct SkillBackupCopyTarget {
    asset: Asset,
    target_dir: PathBuf,
    source_is_library: bool,
}

impl AppService {
    pub(crate) fn list_skills(&self) -> AppResult<Vec<CatalogAsset>> {
        capabilities::catalog_assets_sqlx(&self.db, self.tenant_id(), Some(AssetKind::Skill))
    }

    pub(crate) fn get_skill_backup_settings(&self) -> AppResult<SkillBackupSettings> {
        capabilities::skill_backup_settings_sqlx(&self.db, self.tenant_id())
    }

    pub(crate) fn update_skill_backup_settings(
        &self,
        params: UpdateSkillBackupSettingsParams,
    ) -> AppResult<SkillBackupSettings> {
        let raw_root_path = params.root_path.trim();
        if raw_root_path.is_empty() {
            return Err("skill backup root path is required".to_string());
        }
        let root_path = crate::backend::path_utils::normalize_path_for_storage(raw_root_path)?;

        let current = capabilities::skill_backup_settings_sqlx(&self.db, self.tenant_id())?;
        let current_root = PathBuf::from(&current.expanded_root_path);
        let next_root = crate::backend::path_utils::expand_path(&root_path)?;
        if capabilities::same_path_or_text(&current_root, &next_root) {
            let source = capabilities::assetiweave_library_source_with_root(root_path);
            let pool = self.db.pool().clone();
            let tenant_id = self.tenant_id().to_string();
            self.db.block_on(async move {
                crate::backend::store::upsert_source_sqlx(&pool, &tenant_id, &source).await
            })?;
            return capabilities::skill_backup_settings_sqlx(&self.db, self.tenant_id());
        }

        if params.migrate {
            if !current.is_default_root && path_contains(&current_root, &next_root) {
                return Err(
                    "custom backup migration target cannot be inside the old backup directory"
                        .to_string(),
                );
            }
            fs::create_dir_all(&next_root).map_err(|error| error.to_string())?;
            capabilities::copy_dir_without_conflicts(&current_root, &next_root)?;
        } else {
            fs::create_dir_all(&next_root).map_err(|error| error.to_string())?;
        }

        let source = capabilities::assetiweave_library_source_with_root(root_path);
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::upsert_source_sqlx(&pool, &tenant_id, &source).await
        })?;
        capabilities::refresh_all_sources(&self.db, self.tenant_id())?;

        if params.migrate && !current.is_default_root && current_root.exists() {
            fs::remove_dir_all(&current_root).map_err(|error| error.to_string())?;
        }

        capabilities::skill_backup_settings_sqlx(&self.db, self.tenant_id())
    }

    pub(crate) fn backup_skill(&self, asset_id: String) -> AppResult<CatalogAsset> {
        self.backup_skills(vec![asset_id])?
            .into_iter()
            .next()
            .ok_or_else(|| "backed up skill was copied but not found during rescan".to_string())
    }

    pub(crate) fn backup_skills(&self, asset_ids: Vec<String>) -> AppResult<Vec<CatalogAsset>> {
        self.backup_skills_with_progress(asset_ids, |_, _| {})
    }

    pub(crate) fn backup_skills_with_progress<F>(
        &self,
        asset_ids: Vec<String>,
        mut on_progress: F,
    ) -> AppResult<Vec<CatalogAsset>>
    where
        F: FnMut(usize, Option<&str>),
    {
        let asset_ids = dedupe_non_empty_strings(asset_ids);
        if asset_ids.is_empty() {
            return Ok(Vec::new());
        }

        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let (assets, sources) = self.db.block_on(async move {
            let assets = crate::backend::store::load_assets_sqlx(&pool, &tenant_id, None).await?;
            let sources = crate::backend::store::load_sources_sqlx(&pool, &tenant_id).await?;
            AppResult::Ok((assets, sources))
        })?;
        let assets_by_id = assets
            .iter()
            .map(|asset| (asset.id.as_str(), asset))
            .collect::<HashMap<_, _>>();
        let sources_by_id = sources
            .iter()
            .map(|source| (source.id.as_str(), source))
            .collect::<HashMap<_, _>>();
        let backup_root = capabilities::skill_backup_root_sqlx(&self.db, self.tenant_id())?;
        let mut targets = Vec::with_capacity(asset_ids.len());

        for asset_id in asset_ids {
            let asset = assets_by_id
                .get(asset_id.as_str())
                .ok_or_else(|| format!("asset not found: {asset_id}"))?;
            if asset.kind != AssetKind::Skill {
                return Err("only skill assets can be backed up".to_string());
            }

            let source = sources_by_id
                .get(asset.source_id.as_str())
                .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
            let source_is_library = source.source_origin == SourceOrigin::AssetiweaveLibrary;
            let target_dir = if source_is_library {
                PathBuf::from(&asset.absolute_path)
            } else {
                let asset_name = crate::backend::host_filesystem::HostFilesystem::current()
                    .validate_path_segment(&asset.name)?;
                let origin_bucket = source
                    .origin_app_kind
                    .map(|kind| format!("{kind:?}").to_ascii_lowercase())
                    .unwrap_or_else(|| slug_path_segment(&source.id));
                backup_root
                    .join("backed-up")
                    .join(origin_bucket)
                    .join(asset_name)
            };
            targets.push(SkillBackupCopyTarget {
                asset: (*asset).clone(),
                target_dir,
                source_is_library,
            });
        }

        for index in 0..targets.len() {
            let target = &targets[index];
            if !target.source_is_library {
                let source_path = Path::new(&target.asset.absolute_path);
                if target.target_dir.exists() {
                    let source_hash = crate::backend::path_utils::hash_path(source_path)?;
                    let target_hash = crate::backend::path_utils::hash_path(&target.target_dir)?;
                    if source_hash != target_hash {
                        return Err(format!(
                            "backup skill target already exists with different content: {}",
                            target.target_dir.display()
                        ));
                    }
                } else {
                    capabilities::copy_dir(source_path, &target.target_dir)?;
                }
            }
            let next_asset_id = targets
                .get(index + 1)
                .map(|target| target.asset.id.as_str());
            on_progress(index + 1, next_asset_id);
        }

        let library_source = capabilities::assetiweave_library_source_with_root(
            capabilities::skill_backup_settings_sqlx(&self.db, self.tenant_id())?.root_path,
        );
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::upsert_source_sqlx(&pool, &tenant_id, &library_source).await
        })?;
        capabilities::refresh_all_sources(&self.db, self.tenant_id())?;

        let catalog =
            capabilities::catalog_assets_sqlx(&self.db, self.tenant_id(), Some(AssetKind::Skill))?;
        let mut backed_up_assets = Vec::with_capacity(targets.len());
        for target in targets {
            let target_path = target.target_dir.to_string_lossy();
            let backed_up_asset = catalog
                .iter()
                .find(|candidate| {
                    candidate.asset.id == target.asset.id
                        || candidate.asset.absolute_path == target_path
                        || (target.asset.content_hash.is_some()
                            && candidate.asset.content_hash.as_deref()
                                == target.asset.content_hash.as_deref())
                })
                .cloned()
                .ok_or_else(|| {
                    "backed up skill was copied but not found during rescan".to_string()
                })?;
            backed_up_assets.push(backed_up_asset);
        }
        Ok(backed_up_assets)
    }

    pub(crate) fn import_skill(&self, params: ImportSkillParams) -> AppResult<Value> {
        let source_dir = crate::backend::path_utils::expand_path(&params.from)?;
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
        let name = crate::backend::host_filesystem::HostFilesystem::current()
            .validate_path_segment(&name)?;
        let target_dir = capabilities::skill_backup_root_sqlx(&self.db, self.tenant_id())?
            .join("downloaded")
            .join(&name);
        if target_dir.exists() {
            return Err(format!(
                "downloaded skill already exists: {}",
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

        capabilities::copy_dir(&source_dir, &target_dir)?;
        let library_source = capabilities::assetiweave_library_source_with_root(
            capabilities::skill_backup_settings_sqlx(&self.db, self.tenant_id())?.root_path,
        );
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let library_source_to_save = library_source.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_source_sqlx(&pool, &tenant_id, &library_source_to_save)
                .await
        })?;
        let library_assets = crate::backend::scanner::scan_skill_source(&library_source)?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let library_source_id = library_source.id.clone();
        let library_assets_to_save = library_assets.clone();
        self.db.block_on(async move {
            crate::backend::store::replace_source_assets_sqlx(
                &pool,
                &tenant_id,
                &library_source_id,
                &library_assets_to_save,
            )
            .await
        })?;
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
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let source_id = asset.source_id.clone();
        let source = self.db.block_on(async move {
            crate::backend::store::load_source_sqlx(&pool, &tenant_id, &source_id).await
        })?;
        let source = source.ok_or_else(|| format!("source not found: {}", asset.source_id))?;
        if source.source_origin != SourceOrigin::AssetiweaveLibrary {
            return Err(
                "only AssetIWeave backup library skills can be deleted; remove the source or unmount the skill instead"
                    .to_string(),
            );
        }

        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let asset_id = asset.id.clone();
        let enabled_mounts = self
            .db
            .block_on(async move {
                crate::backend::store::load_asset_mounts_sqlx(&pool, &tenant_id, Some(&asset_id))
                    .await
            })?
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
            capabilities::unmount_asset_mount_record(
                &self.db,
                self.tenant_id(),
                &asset.id,
                &mount.profile_id,
            )?;
        }
        let asset_path = PathBuf::from(&asset.absolute_path);
        if asset_path.exists() {
            crate::backend::host_filesystem::HostFilesystem::current().remove_path(&asset_path)?;
        }
        capabilities::refresh_recorded_assets(&self.db, self.tenant_id())?;
        Ok(json!({ "deleted": true, "asset_id": asset.id }))
    }

    pub(crate) fn mount_skill(&self, params: AssetRefParams, enabled: bool) -> AppResult<Value> {
        let profile_id = params
            .profile_id
            .as_deref()
            .ok_or_else(|| "profile_id is required".to_string())?;
        let asset = self.resolve_skill_asset(&params.asset_ref)?;
        if params.dry_run {
            let pool = self.db.pool().clone();
            let tenant_id = self.tenant_id().to_string();
            let profile = self.db.block_on(async move {
                crate::backend::store::load_profile_sqlx(&pool, &tenant_id, profile_id).await
            })?;
            let profile = profile.ok_or_else(|| format!("profile not found: {profile_id}"))?;
            let inspection = crate::backend::targeting::inspect_mount(&profile, &asset)?;
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
        capabilities::cleanup_orphan_asset_records(&self.db, self.tenant_id())?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            let assets =
                crate::backend::store::load_assets_sqlx(&pool, &tenant_id, Some(AssetKind::Skill))
                    .await?;
            crate::backend::store::load_skill_group_details_sqlx(&pool, &tenant_id, &assets).await
        })
    }

    pub(crate) fn get_skill_group(&self, group_id: String) -> AppResult<AssetGroupDetail> {
        capabilities::cleanup_orphan_asset_records(&self.db, self.tenant_id())?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            let assets =
                crate::backend::store::load_assets_sqlx(&pool, &tenant_id, Some(AssetKind::Skill))
                    .await?;
            crate::backend::store::load_skill_group_detail_sqlx(
                &pool, &tenant_id, &group_id, &assets,
            )
            .await
        })
    }

    pub(crate) fn create_skill_group(&self, input: AssetGroupInput) -> AppResult<AssetGroupDetail> {
        let now = Utc::now().to_rfc3339();
        let group = capabilities::asset_group_from_input(input, now.clone(), now);
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            let assets =
                crate::backend::store::load_assets_sqlx(&pool, &tenant_id, Some(AssetKind::Skill))
                    .await?;
            crate::backend::store::upsert_asset_group_sqlx(&pool, &tenant_id, &group).await?;
            crate::backend::store::load_skill_group_detail_sqlx(
                &pool, &tenant_id, &group.id, &assets,
            )
            .await
        })
    }

    pub(crate) fn update_skill_group(&self, group: AssetGroup) -> AppResult<AssetGroupDetail> {
        let mut group = group;
        group.updated_at = Utc::now().to_rfc3339();
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            let assets =
                crate::backend::store::load_assets_sqlx(&pool, &tenant_id, Some(AssetKind::Skill))
                    .await?;
            crate::backend::store::upsert_asset_group_sqlx(&pool, &tenant_id, &group).await?;
            crate::backend::store::load_skill_group_detail_sqlx(
                &pool, &tenant_id, &group.id, &assets,
            )
            .await
        })
    }

    pub(crate) fn delete_skill_group(&self, group_id: String) -> AppResult<()> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            let assets =
                crate::backend::store::load_assets_sqlx(&pool, &tenant_id, Some(AssetKind::Skill))
                    .await?;
            crate::backend::store::load_skill_group_detail_sqlx(
                &pool, &tenant_id, &group_id, &assets,
            )
            .await?;
            crate::backend::store::delete_asset_group_sqlx(&pool, &tenant_id, &group_id).await
        })
    }

    pub(crate) fn set_skill_group_manual_members(
        &self,
        group_id: String,
        asset_ids: Vec<String>,
    ) -> AppResult<AssetGroupDetail> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            let assets =
                crate::backend::store::load_assets_sqlx(&pool, &tenant_id, Some(AssetKind::Skill))
                    .await?;
            crate::backend::store::replace_asset_group_members_sqlx(
                &pool, &tenant_id, &group_id, &asset_ids, &assets,
            )
            .await?;
            crate::backend::store::load_skill_group_detail_sqlx(
                &pool, &tenant_id, &group_id, &assets,
            )
            .await
        })
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
            let pool = self.db.pool().clone();
            let tenant_id = self.tenant_id().to_string();
            let group_id = params.group_id.clone();
            let detail = self.db.block_on(async move {
                let assets = crate::backend::store::load_assets_sqlx(
                    &pool,
                    &tenant_id,
                    Some(AssetKind::Skill),
                )
                .await?;
                crate::backend::store::load_skill_group_detail_sqlx(
                    &pool, &tenant_id, &group_id, &assets,
                )
                .await
            })?;
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
        capabilities::apply_skill_group_mount_record(
            &self.db,
            self.tenant_id(),
            group_id,
            profile_id,
            enabled,
        )
    }

    pub(crate) fn preview_skill_group_exclusive_mount(
        &self,
        input: SkillGroupExclusiveMountInput,
    ) -> AppResult<SkillGroupExclusiveMountPreview> {
        capabilities::build_skill_group_exclusive_mount_preview_sqlx(
            &self.db,
            self.tenant_id(),
            &input,
        )
    }

    pub(crate) fn apply_skill_group_exclusive_mount(
        &self,
        input: SkillGroupExclusiveMountInput,
    ) -> AppResult<ApplySkillGroupExclusiveMountResult> {
        capabilities::apply_skill_group_exclusive_mount_record(&self.db, self.tenant_id(), &input)
    }

    fn resolve_skill_asset(&self, asset_ref: &str) -> AppResult<Asset> {
        let needle = asset_ref.trim().to_string();
        if needle.is_empty() {
            return Err("asset ref is required".to_string());
        }
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let matches = self
            .db
            .block_on(async move {
                crate::backend::store::load_assets_sqlx(&pool, &tenant_id, Some(AssetKind::Skill))
                    .await
            })?
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

fn path_contains(parent: &Path, child: &Path) -> bool {
    crate::backend::host_filesystem::HostFilesystem::current().is_within(child, parent)
}

fn dedupe_non_empty_strings(values: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for value in values {
        let value = value.trim().to_string();
        if !value.is_empty() && seen.insert(value.clone()) {
            deduped.push(value);
        }
    }
    deduped
}
