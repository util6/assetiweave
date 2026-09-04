use super::prelude::*;
use crate::backend::runtime::{AppError, AppResult};

impl AppService {
    pub(crate) async fn list_tenants(&self) -> AppResult<Vec<Tenant>> {
        let pool = self.db.pool();
        let principal_id = &self.request_context().principal.id;
        crate::backend::store::list_tenants_for_principal_sqlx(pool, principal_id).await
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
        crate::backend::store::seed_tenant_defaults_sqlx_with_catalog(
            pool,
            &tenant.id,
            &target_catalog,
        )
        .await
        .map_err(AppError::external)?;
        crate::backend::application::bootstrap::seed_prepared_builtin_adapters(
            pool,
            &tenant.id,
            builtin_conversation_adapters.as_ref(),
        )
        .await?;

        if set_active {
            self.runtime.activate_tenant(&tenant.id).await?;
        }
        Ok(tenant)
    }

    pub(crate) async fn switch_tenant(&self, tenant_id: String) -> AppResult<Tenant> {
        let tenant_id = tenant_id.trim();
        if tenant_id.is_empty() {
            return Err(AppError::Validation("tenant id is required".to_string()));
        }
        self.runtime.activate_tenant(tenant_id).await
    }
}
