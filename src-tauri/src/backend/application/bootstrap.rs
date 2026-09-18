//! Application projection of prepared, application-owned built-in adapters.

use crate::backend::{models::ConversationAdapter, runtime::AppResult};
use sqlx::SqlitePool;

/// Seed one tenant from the immutable built-in environment prepared by
/// `AppRuntime`. This boundary performs no filesystem writes.
pub(crate) async fn seed_prepared_builtin_adapters(
    pool: &SqlitePool,
    tenant_id: &str,
    adapters: &[ConversationAdapter],
) -> AppResult<()> {
    crate::backend::bootstrap::seed_prepared_builtin_adapters(pool, tenant_id, adapters).await
}

#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;
