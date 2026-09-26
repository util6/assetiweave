use crate::backend::application::prelude::*;
use crate::backend::domain::memory::RecentMemoryStateView;

impl AppService {
    pub(crate) async fn get_recent_memory_snapshot(&self) -> AppResult<RecentMemoryStateView> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        super::recent_snapshot_view::load_recent_memory_state_view(&pool, &tenant_id).await
    }
}

#[cfg(test)]
#[path = "recent_snapshot_tests.rs"]
mod tests;
