use chrono::{DateTime, Utc};

use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::store;

impl AppService {
    /// 检查当前租户是否存在待处理的 Memory 任务或事实
    pub async fn has_pending_memory_work(&self, now: DateTime<Utc>) -> AppResult<bool> {
        let pool = self.db.pool();
        let tenant_id = self.tenant_id();
        let now_text = now.to_rfc3339();

        // 1. 恢复过期的租约，避免中断的 running 孤儿任务导致死锁
        let _ = store::recover_expired_session_memory_leases_sqlx(pool, tenant_id, &now_text).await;
        let _ = store::recover_expired_recent_memory_leases_sqlx(pool, tenant_id, &now_text).await;
        let _ =
            store::recover_expired_memory_maintenance_leases_sqlx(pool, tenant_id, &now_text).await;

        // 清理超期(>48h)的孤儿 queued recent 任务，防止历史残留数据锁死调度探测
        let stale_watermark = (now - chrono::Duration::hours(48)).to_rfc3339();
        let _ = sqlx::query(
            "UPDATE recent_memory_jobs SET status = 'stale', updated_at = ?1 \
             WHERE tenant_id = ?2 AND status = 'queued' AND target_watermark_utc < ?3",
        )
        .bind(&now_text)
        .bind(tenant_id)
        .bind(&stale_watermark)
        .execute(pool)
        .await;

        // 2. 检查是否有等待调度的持久化 Job (queued 或达到退避重试时间的 failed)
        if !store::list_session_memory_job_ids_for_scheduler_sqlx(pool, tenant_id, &now_text, 1)
            .await?
            .is_empty()
            || !store::list_recent_memory_job_ids_for_scheduler_sqlx(pool, tenant_id, &now_text, 1)
                .await?
                .is_empty()
            || !store::list_memory_maintenance_job_ids_for_scheduler_sqlx(
                pool, tenant_id, &now_text, 1,
            )
            .await?
            .is_empty()
        {
            return Ok(true);
        }

        // 3. 检查是否有新会话进入 Phase 1
        if let Some((_target, prepared)) = self
            .ensure_recent_snapshot_phase1_jobs_at(now, false)
            .await?
        {
            if prepared > 0 {
                return Ok(true);
            }
        }

        // 4. 检查是否需要生成新的 Recent 快照（且排除重试次数超限或处于退避期的 job）
        if let Some(prep) = self
            .prepare_recent_snapshot_generation_for_pipeline(Some(now), false)
            .await?
        {
            let existing: Option<(String, i64, Option<String>)> = sqlx::query_as(
                "SELECT status, retry_count, retry_at FROM recent_memory_jobs \
                 WHERE tenant_id = ?1 AND target_watermark_utc = ?2 AND window_hours = ?3 \
                 AND target_fingerprint = ?4 AND content_fingerprint = ?5 LIMIT 1",
            )
            .bind(tenant_id)
            .bind(&prep.target.target_watermark_utc.to_rfc3339())
            .bind(prep.target.window_hours)
            .bind(&prep.target_fingerprint)
            .bind(&prep.content_fingerprint)
            .fetch_optional(pool)
            .await?;

            if let Some((status, retry_count, retry_at)) = existing {
                match status.as_str() {
                    "queued" => return Ok(true),
                    "failed" => {
                        if retry_count < store::MAX_RECENT_MEMORY_JOB_RETRIES {
                            if let Some(at) = retry_at {
                                if at <= now_text {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            } else {
                return Ok(true);
            }
        }

        Ok(false)
    }
}
