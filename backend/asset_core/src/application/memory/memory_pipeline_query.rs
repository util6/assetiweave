use chrono::{DateTime, Utc};

use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::store;

impl AppService {
    /// 检查当前租户是否存在待处理的 Memory 任务或事实
    pub async fn has_pending_memory_work(&self, _now: DateTime<Utc>) -> AppResult<bool> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();

        let watermarks = store::load_watermarks_sqlx(pool, tenant_id).await?;
        let sessions =
            store::list_conversation_sessions_sqlx(pool, tenant_id, None, None, None, 1000, 0)
                .await
                .map_err(AppError::from)?;

        for item in sessions {
            let session = item.session;
            let updated_at = session.updated_at.as_deref().unwrap_or_default();
            match watermarks.get(&session.id) {
                Some(wm) => {
                    if wm.source_updated_at != updated_at {
                        return Ok(true);
                    }
                }
                None => {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}
