//! Infrastructure bootstrap for prepared built-in assets.
//!
//! This module is intentionally below `runtime` and `application`: it prepares
//! filesystem-backed built-ins and persists the prepared values without
//! making Runtime depend on an Application workflow module.

use crate::backend::{runtime::AppResult, store};
use sqlx::SqlitePool;

pub(crate) async fn materialize_and_seed_builtin_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    let adapters = tokio::task::spawn_blocking(
        crate::backend::conversations::ensure_official_conversation_adapters,
    )
    .await
    .map_err(crate::backend::runtime::AppError::from)??;
    store::seed_prepared_builtin_conversation_adapters_sqlx(pool, tenant_id, adapters).await?;
    store::migrate_legacy_conversation_adapter_hashes_sqlx(pool, tenant_id).await?;
    store::normalize_conversation_paths_sqlx(pool, tenant_id).await?;
    Ok(())
}
