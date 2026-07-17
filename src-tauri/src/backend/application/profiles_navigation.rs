use super::prelude::*;

impl AppService {
    pub(crate) fn list_profiles(&self) -> AppResult<Vec<TargetProfile>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::load_profiles_sqlx(&pool, &tenant_id).await
        })
    }

    pub(crate) fn create_profile(&self, input: TargetProfileInput) -> AppResult<TargetProfile> {
        let profile = capabilities::target_profile_from_input(input)?;
        if self
            .list_profiles()?
            .iter()
            .any(|candidate| candidate.id == profile.id)
        {
            return Err(format!("profile already exists: {}", profile.id));
        }
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let profile_to_save = profile.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_profile_sqlx(&pool, &tenant_id, &profile_to_save).await
        })?;
        Ok(profile)
    }

    pub(crate) fn update_profile(&self, profile: TargetProfile) -> AppResult<TargetProfile> {
        let profile = capabilities::normalize_target_profile_paths(profile)?;
        capabilities::validate_target_profile(&profile)?;
        let existing_profile = self
            .list_profiles()?
            .into_iter()
            .find(|candidate| candidate.id == profile.id);
        let Some(existing_profile) = existing_profile else {
            return Err(format!("profile not found: {}", profile.id));
        };
        capabilities::ensure_default_profile_update_is_allowed(&existing_profile, &profile)?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let profile_to_save = profile.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_profile_sqlx(&pool, &tenant_id, &profile_to_save).await
        })?;
        Ok(profile)
    }

    pub(crate) fn delete_profile(&self, id: String) -> AppResult<()> {
        if !self.list_profiles()?.iter().any(|profile| profile.id == id) {
            return Err(format!("profile not found: {id}"));
        }
        capabilities::ensure_profile_can_be_deleted_sqlx(&self.db, self.tenant_id(), &id)?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::delete_profile_sqlx(&pool, &tenant_id, &id).await
        })
    }

    pub(crate) fn navigation_model(&self) -> AppResult<crate::backend::dto::NavigationModel> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::load_navigation_model_sqlx(&pool, &tenant_id).await
        })
    }

    pub(crate) fn update_navigation_model(
        &self,
        model: NavigationModel,
    ) -> AppResult<NavigationModel> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::save_navigation_model_sqlx(&pool, &tenant_id, &model).await?;
            crate::backend::store::load_navigation_model_sqlx(&pool, &tenant_id).await
        })
    }

    pub(crate) fn list_app_shortcuts(&self) -> AppResult<Vec<crate::backend::dto::AppShortcut>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::load_app_shortcuts_sqlx(&pool, &tenant_id).await
        })
    }

    pub(crate) fn list_app_shortcut_settings(
        &self,
    ) -> AppResult<Vec<crate::backend::dto::AppShortcut>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::load_app_shortcut_settings_sqlx(&pool, &tenant_id).await
        })
    }

    pub(crate) fn update_app_shortcuts(
        &self,
        shortcuts: Vec<AppShortcut>,
    ) -> AppResult<Vec<AppShortcut>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::save_app_shortcuts_sqlx(&pool, &tenant_id, &shortcuts).await?;
            crate::backend::store::load_app_shortcut_settings_sqlx(&pool, &tenant_id).await
        })
    }
}
