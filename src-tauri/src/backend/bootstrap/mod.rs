//! Infrastructure bootstrap for prepared built-in assets.
//!
//! This module is intentionally below `runtime` and `application`: it prepares
//! filesystem-backed built-ins and persists the prepared values without
//! making Runtime depend on an Application workflow module.

use crate::backend::{
    runtime::{AppError, AppResult},
    store,
};
use sqlx::SqlitePool;

pub(crate) async fn materialize_and_seed_builtin_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    let adapters = tokio::task::spawn_blocking(
        crate::backend::conversations::ensure_official_conversation_adapters,
    )
    .await
    .map_err(AppError::external)?
    .map_err(AppError::external)?;
    store::seed_prepared_builtin_conversation_adapters_sqlx(pool, tenant_id, adapters)
        .await
        .map_err(AppError::external)?;
    store::migrate_legacy_conversation_adapter_hashes_sqlx(pool, tenant_id)
        .await
        .map_err(AppError::external)?;
    store::normalize_conversation_paths_sqlx(pool, tenant_id)
        .await
        .map_err(AppError::external)?;
    Ok(())
}
