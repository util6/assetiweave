use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};

impl AppService {
    pub(crate) async fn list_tenants(&self) -> AppResult<Vec<Tenant>> {
        let pool = self.db.pool();
        let principal_id = &self.request_context().principal.id;
        Ok(crate::backend::store::list_tenants_for_principal_sqlx(pool, principal_id).await?)
    }

    pub(crate) async fn active_tenant(&self) -> AppResult<Tenant> {
        Ok(self.request_context().tenant.clone())
    }

    pub(crate) async fn create_tenant(&self, params: TenantCreateParams) -> AppResult<Tenant> {
        let pool = self.db.pool();
        let principal_id = &self.request_context().principal.id;
        let name = params.name;
        let slug = params.slug;
        let set_active = params.set_active;
        let target_catalog = self.runtime.target_catalog();
        let builtin_conversation_adapters = self.runtime.builtin_conversation_adapters();

        let tenant = crate::backend::store::create_local_tenant_sqlx(
            pool,
            principal_id,
            &name,
            slug.as_deref(),
        )
        .await?;
        crate::backend::application::system::defaults::seed_tenant_defaults_with_catalog(
            pool,
            &tenant.id,
            &target_catalog,
        )
        .await?;
        crate::backend::application::system::bootstrap::seed_prepared_builtin_adapters(
            pool,
            &tenant.id,
            builtin_conversation_adapters.as_ref(),
        )
        .await?;
        crate::backend::application::system::bootstrap::migrate_legacy_adapter_hashes(
            pool, &tenant.id,
        )
        .await?;

        if set_active {
            self.activate_tenant(&tenant.id).await?;
        }
        Ok(tenant)
    }

    pub(crate) async fn switch_tenant(&self, tenant_id: String) -> AppResult<Tenant> {
        let tenant_id = tenant_id.trim();
        if tenant_id.is_empty() {
            return Err(AppError::Validation("tenant id is required".to_string()));
        }
        crate::backend::application::system::conversation_adapters::reconcile_app_conversation_adapters(
            self.pool(),
            tenant_id,
        )
        .await?;
        self.activate_tenant(tenant_id).await
    }

    pub(crate) async fn activate_tenant(&self, tenant_id: &str) -> AppResult<Tenant> {
        super::tenant_activation::activate_tenant(&self.runtime, tenant_id).await
    }
}
