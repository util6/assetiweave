use crate::backend::{
    application::AppResult, infrastructure::target_catalog::TargetCatalog, store,
};
use sqlx::SqlitePool;

pub(crate) async fn seed_defaults_sqlx(pool: &SqlitePool) -> AppResult<()> {
    let catalog = TargetCatalog::builtin()?;
    super::defaults::seed_defaults_with_catalog(pool, &catalog).await
}

pub(crate) async fn seed_tenant_defaults_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    let catalog = TargetCatalog::builtin()?;
    super::defaults::seed_tenant_defaults_with_catalog(pool, tenant_id, &catalog).await
}

pub(crate) async fn init_test_database(pool: &SqlitePool) -> AppResult<()> {
    crate::backend::infrastructure::path_utils::ensure_app_library_dirs()?;
    seed_defaults_sqlx(pool).await?;
    let adapters =
        crate::backend::infrastructure::conversations::ensure_official_conversation_adapters()?;
    super::conversation_adapters::seed_prepared_builtin_adapters(
        pool,
        store::DEFAULT_TENANT_ID,
        &adapters,
    )
    .await
}
