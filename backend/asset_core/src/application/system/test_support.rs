use crate::backend::{
    application::{AppError, AppResult},
    infrastructure::target_catalog::TargetCatalog,
    store,
};
use sqlx::SqlitePool;

pub async fn seed_defaults_sqlx(pool: &SqlitePool) -> AppResult<()> {
    let catalog = TargetCatalog::builtin()?;
    super::defaults::seed_defaults_with_catalog(pool, &catalog).await
}

pub async fn seed_tenant_defaults_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    let catalog = TargetCatalog::builtin()?;
    super::defaults::seed_tenant_defaults_with_catalog(pool, tenant_id, &catalog).await
}

pub async fn init_test_database(pool: &SqlitePool) -> AppResult<()> {
    crate::backend::infrastructure::path_utils::ensure_app_library_dirs()?;
    seed_defaults_sqlx(pool).await?;
    // Production binaries ship no adapter scripts; tests stage a realistic
    // managed directory from the repository fixtures before discovery.
    let managed_root = crate::backend::infrastructure::app_settings::conversation_adapter_dir()
        .map_err(AppError::external)?;
    crate::backend::infrastructure::conversations::test_fixtures::materialize_builtin_adapter_fixtures(
        &managed_root,
    )
    .map_err(AppError::external)?;
    let adapters =
        crate::backend::infrastructure::conversations::discover_managed_conversation_adapters()?;
    super::conversation_adapters::seed_prepared_builtin_adapters(
        pool,
        store::DEFAULT_TENANT_ID,
        &adapters,
    )
    .await
}
