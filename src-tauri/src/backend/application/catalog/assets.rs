use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};

impl AppService {
    pub(crate) async fn list_assets(
        &self,
        params: ListAssetsParams,
    ) -> AppResult<Vec<CatalogAsset>> {
        super::catalog_ops::catalog_assets_sqlx(self.db.pool(), self.tenant_id(), params.kind).await
    }

    pub(crate) async fn update_asset_description(
        &self,
        asset_id: String,
        description: Option<String>,
    ) -> AppResult<Asset> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let mut asset = crate::backend::store::load_assets_sqlx(pool, tenant_id, None)
            .await?
            .into_iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(|| AppError::NotFound(format!("asset not found: {asset_id}")))?;
        if !self
            .list_sources()
            .await?
            .iter()
            .any(|source| source.id == asset.source_id)
        {
            return Err(AppError::NotFound(format!(
                "source not found: {}",
                asset.source_id
            )));
        }

        let source_path =
            crate::backend::infrastructure::path_utils::expand_path(&asset.absolute_path)?;
        if !source_path.exists() {
            return Err(AppError::NotFound(format!(
                "asset source path does not exist: {}",
                source_path.display()
            )));
        }

        asset.description = description
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        asset.updated_at = Utc::now().to_rfc3339();
        crate::backend::store::update_asset_description_sqlx(pool, tenant_id, &asset).await?;
        Ok(asset)
    }

    pub(crate) async fn delete_asset(&self, asset_id: String, unmount: bool) -> AppResult<Asset> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let asset = crate::backend::store::load_assets_sqlx(pool, tenant_id, None)
            .await?
            .into_iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(|| AppError::NotFound(format!("asset not found: {asset_id}")))?;
        if asset.kind != AssetKind::Skill {
            return Err(AppError::Validation(
                "only skill assets can be deleted from the catalog".to_string(),
            ));
        }
        self.delete_skill(AssetRefParams {
            asset_ref: asset.id.clone(),
            profile_id: None,
            dry_run: false,
            yes: true,
            unmount,
        })
        .await?;
        Ok(asset)
    }

    pub(crate) async fn list_asset_mounts(
        &self,
        asset_id: Option<&str>,
    ) -> AppResult<Vec<AssetMount>> {
        Ok(crate::backend::store::load_asset_mounts_sqlx(
            self.db.pool(),
            self.tenant_id(),
            asset_id,
        )
        .await?)
    }

    pub(crate) async fn list_asset_mount_statuses(
        &self,
        asset_id: Option<&str>,
    ) -> AppResult<Vec<AssetMountStatus>> {
        crate::backend::application::mounting::mount_ops::scan_asset_mount_statuses_sqlx(
            self.db.pool(),
            self.tenant_id(),
            asset_id,
        )
        .await
    }

    pub(crate) async fn refresh_asset_mount_statuses(
        &self,
        asset_id: Option<&str>,
    ) -> AppResult<Vec<AssetMountStatus>> {
        crate::backend::application::mounting::mount_ops::sync_asset_mount_observations(
            self.db.pool(),
            self.tenant_id(),
            asset_id,
        )
        .await
    }

    pub(crate) async fn create_plan(&self, profile_id: Option<&str>) -> AppResult<DeploymentPlan> {
        let assets =
            super::catalog_ops::catalog_visible_assets_sqlx(self.db.pool(), self.tenant_id(), None)
                .await?;
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let profiles =
            crate::backend::application::mounting::profile_ops::load_target_profiles_sqlx(
                pool, tenant_id,
            )
            .await?;
        let mounts =
            crate::backend::store::load_enabled_asset_mounts_sqlx(pool, tenant_id, profile_id)
                .await?;
        let catalog = self.runtime.target_catalog();
        for profile in profiles
            .iter()
            .filter(|profile| profile_id.is_none_or(|requested| requested == profile.id))
        {
            catalog
                .require_descriptor(&profile.target_provider_id)
                .map_err(AppError::external)?;
        }

        let mut candidates = Vec::new();
        for mount in &mounts {
            if profile_id.is_some_and(|requested| requested != mount.profile_id) {
                continue;
            }
            let Some(profile) = profiles
                .iter()
                .find(|profile| profile.id == mount.profile_id)
            else {
                continue;
            };
            let Some(asset) = assets.iter().find(|asset| asset.id == mount.asset_id) else {
                continue;
            };
            let Ok(inspection) =
                crate::backend::application::mounting::targeting::inspect_mount_with_catalog(
                    profile,
                    asset,
                    catalog.as_ref(),
                )
            else {
                continue;
            };

            candidates.push(crate::backend::domain::mounting::DeploymentPlanCandidate {
                asset_id: asset.id.clone(),
                asset_kind: asset.kind,
                profile_id: profile.id.clone(),
                profile_name: profile.name.clone(),
                source_path: asset.absolute_path.clone(),
                display_source_path:
                    crate::backend::infrastructure::path_utils::display_path_or_original(
                        &asset.absolute_path,
                    ),
                display_target_path:
                    crate::backend::infrastructure::path_utils::display_path_or_original(
                        &inspection.target_path,
                    ),
                target_path: inspection.target_path,
                strategy: mount.strategy,
                profile_enabled: profile.enabled,
                supported: profile.supported_kinds.contains(&asset.kind)
                    && profile.include.kinds.contains(&asset.kind),
                state: inspection.state,
            });
        }

        Ok(crate::backend::domain::mounting::build_deployment_plan(
            candidates, profile_id,
        ))
    }

    pub(crate) async fn mount_asset_by_id(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMountUpdateResult> {
        crate::backend::application::mounting::mount_ops::mount_asset_mount_record(
            self.db.pool(),
            self.tenant_id(),
            asset_id,
            profile_id,
        )
        .await
    }

    pub(crate) async fn unmount_asset_by_id(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMountUpdateResult> {
        crate::backend::application::mounting::mount_ops::unmount_asset_mount_record(
            self.db.pool(),
            self.tenant_id(),
            asset_id,
            profile_id,
        )
        .await
    }

    pub(crate) async fn toggle_asset_mount(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMount> {
        let (asset, profile) =
            crate::backend::application::mounting::mounts::load_mount_asset_and_profile(
                self.db.pool(),
                self.tenant_id(),
                asset_id,
                profile_id,
            )
            .await?;
        let inspection =
            crate::backend::application::mounting::targeting::inspect_mount(&profile, &asset)?;
        crate::backend::application::mounting::mount_ops::set_asset_mount_record(
            self.db.pool(),
            self.tenant_id(),
            asset_id,
            profile_id,
            !matches!(
                inspection.state,
                crate::backend::domain::PhysicalMountState::Mounted
            ),
            None,
        )
        .await
    }

    pub(crate) async fn set_asset_mount(
        &self,
        asset_id: &str,
        profile_id: &str,
        enabled: bool,
        strategy: Option<DeploymentStrategy>,
    ) -> AppResult<AssetMount> {
        crate::backend::application::mounting::mount_ops::set_asset_mount_record(
            self.db.pool(),
            self.tenant_id(),
            asset_id,
            profile_id,
            enabled,
            strategy,
        )
        .await
    }
}
