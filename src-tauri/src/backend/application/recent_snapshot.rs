use super::prelude::*;
use crate::backend::dto::RecentMemoryStateView;

impl AppService {
    pub(crate) async fn get_recent_memory_snapshot(&self) -> AppResult<RecentMemoryStateView> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        crate::backend::store::recent_snapshot_repo::load_recent_memory_state_sqlx(
            &pool, &tenant_id,
        )
        .await
    }
}

#[cfg(test)]
#[path = "recent_snapshot_tests.rs"]
mod tests;
