use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult as RuntimeAppResult};

impl AppService {
    pub(crate) async fn list_profiles(&self) -> RuntimeAppResult<Vec<TargetProfile>> {
        super::profile_ops::load_target_profiles_sqlx(self.db.pool(), self.tenant_id()).await
    }

    pub(crate) async fn create_profile(
        &self,
        input: TargetProfileInput,
    ) -> RuntimeAppResult<TargetProfile> {
        let profile = super::profile_ops::target_profile_from_input(input)?;
        if self
            .list_profiles()
            .await?
            .iter()
            .any(|candidate| candidate.id == profile.id)
        {
            return Err(AppError::Conflict(format!(
                "profile already exists: {}",
                profile.id
            )));
        }
        super::profile_ops::upsert_target_profile_sqlx(self.db.pool(), self.tenant_id(), &profile)
            .await?;
        Ok(profile)
    }

    pub(crate) async fn update_profile(
        &self,
        profile: TargetProfile,
    ) -> RuntimeAppResult<TargetProfile> {
        let profile = super::profile_ops::normalize_target_profile_paths(profile)?;
        super::profile_ops::validate_target_profile(&profile)?;
        let existing_profile = self
            .list_profiles()
            .await?
            .into_iter()
            .find(|candidate| candidate.id == profile.id);
        let Some(existing_profile) = existing_profile else {
            return Err(AppError::NotFound(format!(
                "profile not found: {}",
                profile.id
            )));
        };
        super::profile_ops::ensure_default_profile_update_is_allowed(&existing_profile, &profile)?;
        super::profile_ops::upsert_target_profile_sqlx(self.db.pool(), self.tenant_id(), &profile)
            .await?;
        Ok(profile)
    }

    pub(crate) async fn delete_profile(&self, id: String) -> RuntimeAppResult<()> {
        if !self
            .list_profiles()
            .await?
            .iter()
            .any(|profile| profile.id == id)
        {
            return Err(AppError::NotFound(format!("profile not found: {id}")));
        }
        super::profile_ops::ensure_profile_can_be_deleted_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &id,
        )
        .await?;
        Ok(
            crate::backend::store::delete_profile_sqlx(self.db.pool(), self.tenant_id(), &id)
                .await?,
        )
    }

    pub(crate) async fn navigation_model(
        &self,
    ) -> RuntimeAppResult<crate::backend::application::system::NavigationModel> {
        Ok(
            crate::backend::store::load_navigation_model_sqlx(self.db.pool(), self.tenant_id())
                .await?,
        )
    }

    pub(crate) async fn update_navigation_model(
        &self,
        model: NavigationModel,
    ) -> RuntimeAppResult<NavigationModel> {
        crate::backend::store::save_navigation_model_sqlx(self.db.pool(), self.tenant_id(), &model)
            .await?;
        Ok(
            crate::backend::store::load_navigation_model_sqlx(self.db.pool(), self.tenant_id())
                .await?,
        )
    }

    pub(crate) async fn list_app_shortcuts(
        &self,
    ) -> RuntimeAppResult<Vec<crate::backend::domain::system::AppShortcut>> {
        Ok(
            crate::backend::store::load_app_shortcuts_sqlx(self.db.pool(), self.tenant_id())
                .await?,
        )
    }

    pub(crate) async fn list_app_shortcut_settings(
        &self,
    ) -> RuntimeAppResult<Vec<crate::backend::domain::system::AppShortcut>> {
        Ok(
            crate::backend::store::load_app_shortcut_settings_sqlx(
                self.db.pool(),
                self.tenant_id(),
            )
            .await?,
        )
    }

    pub(crate) async fn update_app_shortcuts(
        &self,
        shortcuts: Vec<AppShortcut>,
    ) -> RuntimeAppResult<Vec<AppShortcut>> {
        crate::backend::store::save_app_shortcuts_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &shortcuts,
        )
        .await?;
        Ok(
            crate::backend::store::load_app_shortcut_settings_sqlx(
                self.db.pool(),
                self.tenant_id(),
            )
            .await?,
        )
    }
}
