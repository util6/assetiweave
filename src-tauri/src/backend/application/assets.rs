use super::prelude::*;
use crate::backend::runtime::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BatchMountWorkflowResult {
    pub(crate) requested_count: usize,
    pub(crate) updated_count: usize,
    pub(crate) error_count: usize,
    pub(crate) results: Vec<AssetMountUpdateResult>,
    pub(crate) errors: Vec<BatchMountItemError>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BatchMountItemError {
    pub(crate) asset_id: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone)]
pub(crate) enum BatchMountWorkflowInput {
    Explicit {
        asset_ids: Vec<String>,
        profile_id: String,
        enabled: bool,
    },
    Group {
        group_id: String,
        profile_id: String,
        enabled: bool,
    },
    Exclusive {
        group_ids: Vec<String>,
        profile_id: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub(crate) enum BatchMountWorkflowOutput {
    Explicit(BatchMountWorkflowResult),
    Group(ApplyAssetGroupMountResult),
    Exclusive(ApplySkillGroupExclusiveMountResult),
}

impl AppService {
    pub(crate) async fn run_batch_mount_workflow_with_progress<BeforeItem>(
        &self,
        input: BatchMountWorkflowInput,
        mut before_item: BeforeItem,
    ) -> AppResult<BatchMountWorkflowOutput>
    where
        BeforeItem: FnMut(usize, usize, &str) -> AppResult<()>,
    {
        match input {
            BatchMountWorkflowInput::Explicit {
                asset_ids,
                profile_id,
                enabled,
            } => self
                .apply_explicit_mount_with_progress(asset_ids, &profile_id, enabled, before_item)
                .await
                .map(BatchMountWorkflowOutput::Explicit),
            BatchMountWorkflowInput::Group {
                group_id,
                profile_id,
                enabled,
            } => capabilities::apply_skill_group_mount_record_with_progress(
                self.db.pool(),
                self.tenant_id(),
                &group_id,
                &profile_id,
                enabled,
                before_item,
            )
            .await
            .map(BatchMountWorkflowOutput::Group),
            BatchMountWorkflowInput::Exclusive {
                group_ids,
                profile_id,
            } => capabilities::apply_skill_group_exclusive_mount_record_with_progress(
                self.db.pool(),
                self.tenant_id(),
                &SkillGroupExclusiveMountInput {
                    group_ids,
                    profile_id,
                    mount_selected: true,
                    dry_run: false,
                },
                |index, total, asset_id| before_item(index, total, asset_id),
            )
            .await
            .map(BatchMountWorkflowOutput::Exclusive),
        }
    }

    pub(crate) async fn list_assets(
        &self,
        params: ListAssetsParams,
    ) -> AppResult<Vec<CatalogAsset>> {
        capabilities::catalog_assets_sqlx(self.db.pool(), self.tenant_id(), params.kind).await
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

        let source_path = crate::backend::path_utils::expand_path(&asset.absolute_path)?;
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
        crate::backend::store::load_asset_mounts_sqlx(self.db.pool(), self.tenant_id(), asset_id)
            .await
    }

    pub(crate) async fn list_asset_mount_statuses(
        &self,
        asset_id: Option<&str>,
    ) -> AppResult<Vec<AssetMountStatus>> {
        capabilities::scan_asset_mount_statuses_sqlx(self.db.pool(), self.tenant_id(), asset_id)
            .await
    }

    pub(crate) async fn refresh_asset_mount_statuses(
        &self,
        asset_id: Option<&str>,
    ) -> AppResult<Vec<AssetMountStatus>> {
        capabilities::sync_asset_mount_observations(self.db.pool(), self.tenant_id(), asset_id)
            .await
    }

    pub(crate) async fn create_plan(&self, profile_id: Option<&str>) -> AppResult<DeploymentPlan> {
        let assets =
            capabilities::catalog_visible_assets_sqlx(self.db.pool(), self.tenant_id(), None)
                .await?;
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let profiles = crate::backend::store::load_profiles_sqlx(pool, tenant_id).await?;
        let mounts =
            crate::backend::store::load_enabled_asset_mounts_sqlx(pool, tenant_id, profile_id)
                .await?;
        Ok(crate::backend::planner::build_plan_with_catalog(
            &assets,
            &profiles,
            &mounts,
            profile_id,
            self.runtime.target_catalog().as_ref(),
        )
        .map_err(AppError::external)?)
    }

    pub(crate) async fn mount_asset_by_id(
        &self,
        asset_id: &str,
        profile_id: &str,
    ) -> AppResult<AssetMountUpdateResult> {
        capabilities::mount_asset_mount_record(
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
        capabilities::unmount_asset_mount_record(
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
            load_mount_asset_and_profile(self.db.pool(), self.tenant_id(), asset_id, profile_id)
                .await?;
        let inspection = crate::backend::targeting::inspect_mount(&profile, &asset)?;
        capabilities::set_asset_mount_record(
            self.db.pool(),
            self.tenant_id(),
            asset_id,
            profile_id,
            !matches!(
                inspection.state,
                crate::backend::targeting::PhysicalMountState::Mounted
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
        capabilities::set_asset_mount_record(
            self.db.pool(),
            self.tenant_id(),
            asset_id,
            profile_id,
            enabled,
            strategy,
        )
        .await
    }

    pub(crate) async fn apply_explicit_mount_with_progress<BeforeItem>(
        &self,
        asset_ids: Vec<String>,
        profile_id: &str,
        enabled: bool,
        mut before_item: BeforeItem,
    ) -> AppResult<BatchMountWorkflowResult>
    where
        BeforeItem: FnMut(usize, usize, &str) -> AppResult<()>,
    {
        let asset_ids = asset_ids
            .into_iter()
            .map(|asset_id| asset_id.trim().to_string())
            .filter(|asset_id| !asset_id.is_empty())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if asset_ids.is_empty() {
            return Err(AppError::Validation(
                "asset_ids are required for explicit batch mount".to_string(),
            ));
        }

        let (assets, sources, profile) = capabilities::load_batch_mount_inputs_sqlx(
            self.db.pool(),
            self.tenant_id(),
            profile_id,
        )
        .await?;
        let asset_by_id = assets
            .iter()
            .map(|asset| (asset.id.as_str(), asset))
            .collect::<HashMap<_, _>>();
        let source_by_id = sources
            .iter()
            .map(|source| (source.id.as_str(), source))
            .collect::<HashMap<_, _>>();

        let total = asset_ids.len();
        let mut results = Vec::new();
        let mut errors = Vec::new();
        for (index, asset_id) in asset_ids.iter().enumerate() {
            before_item(index, total, asset_id)?;
            let item_res = match asset_by_id.get(asset_id.as_str()) {
                Some(asset) => {
                    if !enabled {
                        capabilities::unmount_preloaded_asset_mount_record(
                            self.db.pool(),
                            self.tenant_id(),
                            asset,
                            &profile,
                        )
                        .await
                    } else {
                        match source_by_id.get(asset.source_id.as_str()) {
                            Some(source) => {
                                capabilities::mount_preloaded_asset_mount_record(
                                    self.db.pool(),
                                    self.tenant_id(),
                                    asset,
                                    source,
                                    &profile,
                                )
                                .await
                            }
                            None => Err(AppError::NotFound(format!(
                                "source not found: {}",
                                asset.source_id
                            ))),
                        }
                    }
                }
                None => Err(AppError::NotFound(format!("asset not found: {asset_id}"))),
            };
            match item_res {
                Ok(result) => results.push(result),
                Err(error) => errors.push(BatchMountItemError {
                    asset_id: asset_id.clone(),
                    message: error.to_string(),
                }),
            }
        }

        Ok(BatchMountWorkflowResult {
            requested_count: total,
            updated_count: results.len(),
            error_count: errors.len(),
            results,
            errors,
        })
    }

    pub(crate) async fn execute_plan(
        &self,
        plan: DeploymentPlan,
        action_ids: Option<Vec<String>>,
    ) -> AppResult<ExecutionResult> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let profiles = crate::backend::store::load_profiles_sqlx(pool, tenant_id).await?;
        let assets = crate::backend::store::load_assets_sqlx(pool, tenant_id, None).await?;
        crate::backend::executor::execute_deployment_plan(
            pool,
            tenant_id,
            &profiles,
            &assets,
            &plan,
            action_ids.as_deref(),
            self.runtime.target_catalog().as_ref(),
        )
        .await
        .map_err(AppError::external)
    }
}

#[cfg(test)]
fn run_batch_items<T, BeforeItem, ApplyItem>(
    asset_ids: &[String],
    before_item: &mut BeforeItem,
    mut apply_item: ApplyItem,
) -> AppResult<(Vec<T>, Vec<BatchMountItemError>)>
where
    BeforeItem: FnMut(usize, usize, &str) -> AppResult<()>,
    ApplyItem: FnMut(&str) -> AppResult<T>,
{
    let total = asset_ids.len();
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for (index, asset_id) in asset_ids.iter().enumerate() {
        before_item(index, total, asset_id)?;
        match apply_item(asset_id) {
            Ok(result) => results.push(result),
            Err(error) => errors.push(BatchMountItemError {
                asset_id: asset_id.clone(),
                message: error.to_string(),
            }),
        }
    }
    Ok((results, errors))
}

async fn load_mount_asset_and_profile(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, TargetProfile)> {
    let asset = crate::backend::store::load_asset_sqlx(pool, tenant_id, asset_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("asset not found: {asset_id}")))?;
    let profile = crate::backend::store::load_profile_sqlx(pool, tenant_id, profile_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("profile not found: {profile_id}")))?;
    AppResult::Ok((asset, profile))
}

#[cfg(test)]
mod tests {
    use super::run_batch_items;
    use crate::backend::runtime::AppError;

    #[test]
    fn batch_cancel_is_checked_before_the_next_item() {
        let asset_ids = vec![
            "asset-a".to_string(),
            "asset-b".to_string(),
            "asset-c".to_string(),
        ];
        let mut started = Vec::new();
        let result = run_batch_items(
            &asset_ids,
            &mut |index, _, _| {
                if index == 1 {
                    return Err(AppError::Cancelled("batch cancelled".to_string()));
                }
                Ok(())
            },
            |asset_id| {
                started.push(asset_id.to_string());
                Ok::<_, AppError>(asset_id.to_string())
            },
        );

        assert!(matches!(result, Err(AppError::Cancelled(_))));
        assert_eq!(started, vec!["asset-a"]);
    }
}
