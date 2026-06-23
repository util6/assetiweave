use super::prelude::*;

impl AppService {
    pub(crate) fn list_assets(&self, params: ListAssetsParams) -> AppResult<Vec<CatalogAsset>> {
        capabilities::catalog_assets_sqlx(&self.db, params.kind)
    }

    pub(crate) fn update_asset_description(
        &self,
        asset_id: String,
        description: Option<String>,
    ) -> AppResult<Asset> {
        let pool = self.db.pool().clone();
        let mut asset = self
            .db
            .block_on(async move { crate::backend::store::load_assets_sqlx(&pool, None).await })?
            .into_iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        if !self
            .list_sources()?
            .iter()
            .any(|source| source.id == asset.source_id)
        {
            return Err(format!("source not found: {}", asset.source_id));
        }

        let source_path = crate::backend::path_utils::expand_path(&asset.absolute_path)?;
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
        let pool = self.db.pool().clone();
        let asset_to_save = asset.clone();
        self.db.block_on(async move {
            crate::backend::store::update_asset_description_sqlx(&pool, &asset_to_save).await
        })?;
        Ok(asset)
    }

    pub(crate) fn delete_asset(&self, asset_id: String, unmount: bool) -> AppResult<Asset> {
        let pool = self.db.pool().clone();
        let asset = self
            .db
            .block_on(async move { crate::backend::store::load_assets_sqlx(&pool, None).await })?
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
        let pool = self.db.pool().clone();
        let asset_id = asset_id.map(str::to_string);
        self.db.block_on(async move {
            crate::backend::store::load_asset_mounts_sqlx(&pool, asset_id.as_deref()).await
        })
    }

    pub(crate) fn list_asset_mount_statuses(
        &self,
        asset_id: Option<&str>,
    ) -> AppResult<Vec<AssetMountStatus>> {
        capabilities::scan_asset_mount_statuses_sqlx(&self.db, asset_id)
    }

    pub(crate) fn refresh_asset_mount_statuses(
        &self,
        asset_id: Option<&str>,
    ) -> AppResult<Vec<AssetMountStatus>> {
        capabilities::sync_asset_mount_observations(&self.db, asset_id)
    }

    pub(crate) fn create_plan(&self, profile_id: Option<&str>) -> AppResult<DeploymentPlan> {
        let assets = capabilities::catalog_visible_assets_sqlx(&self.db, None)?;
        let pool = self.db.pool().clone();
        let profile_filter = profile_id.map(str::to_string);
        let profile_filter_for_query = profile_filter.clone();
        let (profiles, mounts) = self.db.block_on(async move {
            let profiles = crate::backend::store::load_profiles_sqlx(&pool).await?;
            let mounts = crate::backend::store::load_enabled_asset_mounts_sqlx(
                &pool,
                profile_filter_for_query.as_deref(),
            )
            .await?;
            AppResult::Ok((profiles, mounts))
        })?;
        Ok(crate::backend::planner::build_plan(
            &assets,
            &profiles,
            &mounts,
            profile_filter.as_deref(),
        ))
    }

    pub(crate) fn mount_asset_by_id(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMountUpdateResult> {
        capabilities::mount_asset_mount_record(&self.db, asset_id, profile_id)
    }

    pub(crate) fn unmount_asset_by_id(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMountUpdateResult> {
        capabilities::unmount_asset_mount_record(&self.db, asset_id, profile_id)
    }

    pub(crate) fn toggle_asset_mount(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMount> {
        let (asset, profile) = load_mount_asset_and_profile(&self.db, asset_id, profile_id)?;
        let inspection = crate::backend::targeting::inspect_mount(&profile, &asset)?;
        capabilities::set_asset_mount_record(
            &self.db,
            asset_id,
            profile_id,
            !matches!(
                inspection.state,
                crate::backend::targeting::PhysicalMountState::Mounted
            ),
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
        capabilities::set_asset_mount_record(&self.db, asset_id, profile_id, enabled, strategy)
    }

    pub(crate) fn execute_plan(
        &self,
        plan: DeploymentPlan,
        action_ids: Option<Vec<String>>,
    ) -> AppResult<ExecutionResult> {
        let pool = self.db.pool().clone();
        self.db.block_on(async move {
            let profiles = crate::backend::store::load_profiles_sqlx(&pool).await?;
            let assets = crate::backend::store::load_assets_sqlx(&pool, None).await?;
            crate::backend::executor::execute_deployment_plan(
                &pool,
                &profiles,
                &assets,
                &plan,
                action_ids.as_deref(),
            )
            .await
        })
    }
}

fn load_mount_asset_and_profile(
    db: &crate::backend::store::Database,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, TargetProfile)> {
    let pool = db.pool().clone();
    let asset_id = asset_id.to_string();
    let profile_id = profile_id.to_string();
    db.block_on(async move {
        let asset = crate::backend::store::load_asset_sqlx(&pool, &asset_id)
            .await?
            .ok_or_else(|| format!("asset not found: {asset_id}"))?;
        let profile = crate::backend::store::load_profile_sqlx(&pool, &profile_id)
            .await?
            .ok_or_else(|| format!("profile not found: {profile_id}"))?;
        AppResult::Ok((asset, profile))
    })
}
