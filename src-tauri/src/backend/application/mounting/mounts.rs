use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};

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
            } => super::groups::apply_skill_group_mount_record_with_progress(
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
            } => super::groups::apply_skill_group_exclusive_mount_record_with_progress(
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
        let total = asset_ids.len();
        if total == 0 {
            return Ok(BatchMountWorkflowResult {
                requested_count: 0,
                updated_count: 0,
                error_count: 0,
                results: Vec::new(),
                errors: Vec::new(),
            });
        }

        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let profile = crate::backend::application::mounting::profile_ops::load_target_profile_sqlx(
            pool, tenant_id, profile_id,
        )
        .await?
        .ok_or_else(|| AppError::NotFound(format!("profile not found: {profile_id}")))?;

        let all_assets = crate::backend::store::load_assets_sqlx(pool, tenant_id, None).await?;
        let asset_by_id = all_assets
            .into_iter()
            .map(|asset| (asset.id.clone(), asset))
            .collect::<std::collections::HashMap<_, _>>();

        let all_sources = crate::backend::store::load_sources_sqlx(pool, tenant_id).await?;
        let source_by_id = all_sources
            .into_iter()
            .map(|source| (source.id.clone(), source))
            .collect::<std::collections::HashMap<_, _>>();

        let mut results = Vec::new();
        let mut errors = Vec::new();
        for (index, asset_id) in asset_ids.iter().enumerate() {
            before_item(index, total, asset_id)?;
            let item_res = match asset_by_id.get(asset_id.as_str()) {
                Some(asset) => {
                    if !enabled {
                        super::mount_ops::unmount_preloaded_asset_mount_record(
                            self.db.pool(),
                            self.tenant_id(),
                            asset,
                            &profile,
                        )
                        .await
                    } else {
                        match source_by_id.get(asset.source_id.as_str()) {
                            Some(source) => {
                                super::mount_ops::mount_preloaded_asset_mount_record(
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
        let profiles =
            crate::backend::application::mounting::profile_ops::load_target_profiles_sqlx(
                pool, tenant_id,
            )
            .await?;
        let assets = crate::backend::store::load_assets_sqlx(pool, tenant_id, None).await?;
        super::deployment_execution::execute_deployment_plan(
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

pub(crate) async fn load_mount_asset_and_profile(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<(Asset, TargetProfile)> {
    let asset = crate::backend::store::load_asset_sqlx(pool, tenant_id, asset_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("asset not found: {asset_id}")))?;
    let profile = crate::backend::application::mounting::profile_ops::load_target_profile_sqlx(
        pool, tenant_id, profile_id,
    )
    .await?
    .ok_or_else(|| AppError::NotFound(format!("profile not found: {profile_id}")))?;
    AppResult::Ok((asset, profile))
}

#[cfg(test)]
#[path = "mounts_tests.rs"]
mod tests;
