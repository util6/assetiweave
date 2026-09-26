use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};

impl AppService {
    pub(crate) async fn list_skill_groups(&self) -> AppResult<Vec<AssetGroupDetail>> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        super::source_scanner::cleanup_orphan_asset_records(pool, tenant_id).await?;
        let assets =
            crate::backend::store::load_assets_sqlx(pool, tenant_id, Some(AssetKind::Skill))
                .await?;
        Ok(crate::backend::store::load_skill_group_details_sqlx(pool, tenant_id, &assets).await?)
    }

    pub(crate) async fn get_skill_group(&self, group_id: String) -> AppResult<AssetGroupDetail> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        super::source_scanner::cleanup_orphan_asset_records(pool, tenant_id).await?;
        let assets =
            crate::backend::store::load_assets_sqlx(pool, tenant_id, Some(AssetKind::Skill))
                .await?;
        Ok(
            crate::backend::store::load_skill_group_detail_sqlx(
                pool, tenant_id, &group_id, &assets,
            )
            .await?,
        )
    }

    pub(crate) async fn create_skill_group(
        &self,
        input: AssetGroupInput,
    ) -> AppResult<AssetGroupDetail> {
        let now = Utc::now().to_rfc3339();
        let group = crate::backend::application::mounting::groups::asset_group_from_input(
            input,
            now.clone(),
            now,
        );
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let assets =
            crate::backend::store::load_assets_sqlx(pool, tenant_id, Some(AssetKind::Skill))
                .await?;
        crate::backend::store::upsert_asset_group_sqlx(pool, tenant_id, &group).await?;
        Ok(
            crate::backend::store::load_skill_group_detail_sqlx(
                pool, tenant_id, &group.id, &assets,
            )
            .await?,
        )
    }

    pub(crate) async fn update_skill_group(
        &self,
        group: AssetGroup,
    ) -> AppResult<AssetGroupDetail> {
        let mut group = group;
        group.updated_at = Utc::now().to_rfc3339();
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let assets =
            crate::backend::store::load_assets_sqlx(pool, tenant_id, Some(AssetKind::Skill))
                .await?;
        crate::backend::store::upsert_asset_group_sqlx(pool, tenant_id, &group).await?;
        Ok(
            crate::backend::store::load_skill_group_detail_sqlx(
                pool, tenant_id, &group.id, &assets,
            )
            .await?,
        )
    }

    pub(crate) async fn delete_skill_group(&self, group_id: String) -> AppResult<()> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let assets =
            crate::backend::store::load_assets_sqlx(pool, tenant_id, Some(AssetKind::Skill))
                .await?;
        crate::backend::store::load_skill_group_detail_sqlx(pool, tenant_id, &group_id, &assets)
            .await?;
        Ok(crate::backend::store::delete_asset_group_sqlx(pool, tenant_id, &group_id).await?)
    }

    pub(crate) async fn set_skill_group_manual_members(
        &self,
        group_id: String,
        asset_ids: Vec<String>,
    ) -> AppResult<AssetGroupDetail> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let assets =
            crate::backend::store::load_assets_sqlx(pool, tenant_id, Some(AssetKind::Skill))
                .await?;
        crate::backend::store::replace_asset_group_members_sqlx(
            pool, tenant_id, &group_id, &asset_ids, &assets,
        )
        .await?;
        Ok(
            crate::backend::store::load_skill_group_detail_sqlx(
                pool, tenant_id, &group_id, &assets,
            )
            .await?,
        )
    }

    pub(crate) async fn mount_skill_group(
        &self,
        params: SkillGroupMountParams,
        enabled: bool,
    ) -> AppResult<Value> {
        if !enabled && !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "skill.group.unmount requires --yes".to_string(),
            ));
        }
        if params.dry_run {
            let pool = self.db.pool();
            let tenant_id = self.tenant_id();
            let assets =
                crate::backend::store::load_assets_sqlx(pool, tenant_id, Some(AssetKind::Skill))
                    .await?;
            let detail = crate::backend::store::load_skill_group_detail_sqlx(
                pool,
                tenant_id,
                &params.group_id,
                &assets,
            )
            .await?;
            return Ok(json!({
                "dry_run": true,
                "group_id": params.group_id,
                "profile_id": params.profile_id,
                "enabled": enabled,
                "requested_count": detail.members.len()
            }));
        }
        let result: ApplyAssetGroupMountResult = self
            .apply_skill_group_mount(&params.group_id, &params.profile_id, enabled)
            .await?;
        Ok(json!(result))
    }

    pub(crate) async fn apply_skill_group_mount(
        &self,
        group_id: &str,
        profile_id: &str,
        enabled: bool,
    ) -> AppResult<ApplyAssetGroupMountResult> {
        self.apply_skill_group_mount_with_progress(group_id, profile_id, enabled, |_, _, _| Ok(()))
            .await
    }

    pub(crate) async fn apply_skill_group_mount_with_progress<BeforeItem>(
        &self,
        group_id: &str,
        profile_id: &str,
        enabled: bool,
        mut before_item: BeforeItem,
    ) -> AppResult<ApplyAssetGroupMountResult>
    where
        BeforeItem: FnMut(usize, usize, &str) -> AppResult<()>,
    {
        match self
            .run_batch_mount_workflow_with_progress(
                crate::backend::application::mounting::mounts::BatchMountWorkflowInput::Group {
                    group_id: group_id.to_string(),
                    profile_id: profile_id.to_string(),
                    enabled,
                },
                |index, total, asset_id| before_item(index, total, asset_id),
            )
            .await?
        {
            crate::backend::application::mounting::mounts::BatchMountWorkflowOutput::Group(
                result,
            ) => Ok(result),
            _ => unreachable!("group workflow returns a group result"),
        }
    }

    pub(crate) async fn preview_skill_group_exclusive_mount(
        &self,
        input: SkillGroupExclusiveMountInput,
    ) -> AppResult<SkillGroupExclusiveMountPreview> {
        crate::backend::application::mounting::groups::build_skill_group_exclusive_mount_preview_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &input,
        )
        .await
    }

    pub(crate) async fn apply_skill_group_exclusive_mount(
        &self,
        input: SkillGroupExclusiveMountInput,
    ) -> AppResult<ApplySkillGroupExclusiveMountResult> {
        self.apply_skill_group_exclusive_mount_with_progress(input, |_, _, _| Ok(()))
            .await
    }

    pub(crate) async fn apply_skill_group_exclusive_mount_with_progress<BeforeItem>(
        &self,
        input: SkillGroupExclusiveMountInput,
        mut before_item: BeforeItem,
    ) -> AppResult<ApplySkillGroupExclusiveMountResult>
    where
        BeforeItem: FnMut(usize, usize, &str) -> AppResult<()>,
    {
        match self
            .run_batch_mount_workflow_with_progress(
                crate::backend::application::mounting::mounts::BatchMountWorkflowInput::Exclusive {
                    group_ids: input.group_ids,
                    profile_id: input.profile_id,
                },
                |index, total, asset_id| before_item(index, total, asset_id),
            )
            .await?
        {
            crate::backend::application::mounting::mounts::BatchMountWorkflowOutput::Exclusive(
                result,
            ) => Ok(result),
            _ => unreachable!("exclusive workflow returns an exclusive result"),
        }
    }
}
