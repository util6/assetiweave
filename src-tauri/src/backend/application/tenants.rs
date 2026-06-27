use super::prelude::*;

impl AppService {
    pub(crate) fn list_tenants(&self) -> AppResult<Vec<Tenant>> {
        let pool = self.db.pool().clone();
        let principal_id = self.request_context().principal.id.clone();
        self.db.block_on(async move {
            crate::backend::store::list_tenants_for_principal_sqlx(&pool, &principal_id).await
        })
    }

    pub(crate) fn active_tenant(&self) -> AppResult<Tenant> {
        Ok(self.request_context().tenant.clone())
    }

    pub(crate) fn create_tenant(&self, params: TenantCreateParams) -> AppResult<Tenant> {
        let pool = self.db.pool().clone();
        let principal_id = self.request_context().principal.id.clone();
        let name = params.name;
        let slug = params.slug;
        let set_active = params.set_active;
        self.db.block_on(async move {
            let tenant = crate::backend::store::create_local_tenant_sqlx(
                &pool,
                &principal_id,
                &name,
                slug.as_deref(),
            )
            .await?;
            crate::backend::store::seed_tenant_defaults_sqlx(&pool, &tenant.id).await?;
            if set_active {
                crate::backend::store::set_active_tenant_sqlx(&pool, &principal_id, &tenant.id)
                    .await?;
            }
            AppResult::Ok(tenant)
        })
    }

    pub(crate) fn switch_tenant(&self, tenant_id: String) -> AppResult<Tenant> {
        let tenant_id = tenant_id.trim();
        if tenant_id.is_empty() {
            return Err("tenant id is required".to_string());
        }
        let pool = self.db.pool().clone();
        let principal_id = self.request_context().principal.id.clone();
        let tenant_id = tenant_id.to_string();
        self.db.block_on(async move {
            crate::backend::store::set_active_tenant_sqlx(&pool, &principal_id, &tenant_id).await
        })
    }
}
