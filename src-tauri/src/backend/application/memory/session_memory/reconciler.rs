use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::memory::SessionMemoryJobStatus;
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
impl AppService {
    pub(crate) async fn reconcile_session_memory_jobs_for_tenant_at(
        &self,
        tenant_id: &str,
        now: DateTime<Utc>,
    ) -> AppResult<usize> {
        if !self.backend_settings()?.is_memory_generation_enabled() {
            return Ok(0);
        }
        let pool = self.db.pool().clone();
        let now_text = now.to_rfc3339();
        store::recover_expired_session_memory_leases_sqlx(&pool, tenant_id, &now_text).await?;
        const MAX_SESSION_MEMORY_HOURLY_BUDGET: i64 = 60;
        let one_hour_ago = (now - Duration::hours(1)).to_rfc3339();
        let hourly_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM session_memory_jobs WHERE tenant_id = ?1 AND updated_at >= ?2 AND status IN ('running', 'succeeded', 'failed')",
        )
        .bind(tenant_id)
        .bind(&one_hour_ago)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

        if hourly_count >= MAX_SESSION_MEMORY_HOURLY_BUDGET {
            tracing::warn!(
                tenant_id,
                hourly_count,
                "Session memory hourly budget exceeded; pausing dispatch for this tenant"
            );
            return Ok(0);
        }

        let recent_failures: Vec<Option<String>> = sqlx::query_scalar(
            "SELECT last_error FROM session_memory_jobs WHERE tenant_id = ?1 AND status = 'failed' AND retry_at IS NOT NULL AND updated_at >= ?2 AND last_error IS NOT NULL AND last_error NOT IN ('lease_expired', 'session_memory_persist_failed') ORDER BY updated_at DESC LIMIT 3",
        )
        .bind(tenant_id)
        .bind(&one_hour_ago)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        if recent_failures.len() >= 3 {
            let first_err = recent_failures[0].as_deref().unwrap_or("");
            if !first_err.is_empty()
                && recent_failures
                    .iter()
                    .all(|e| e.as_deref() == Some(first_err))
            {
                tracing::warn!(
                    tenant_id,
                    error = first_err,
                    "Session memory circuit breaker tripped: 3 consecutive identical failures. Pausing dispatch."
                );
                return Ok(0);
            }
        }

        let job_ids =
            store::list_session_memory_job_ids_for_scheduler_sqlx(&pool, tenant_id, &now_text, 32)
                .await?;
        let mut scheduled = 0usize;
        for job_id in job_ids {
            if self
                .runtime
                .task_runtime()
                .list_for_tenant(
                    tenant_id,
                    crate::backend::infrastructure::tasks::TaskFilter {
                        kind: Some(crate::backend::infrastructure::tasks::TaskKind::Memory),
                        active_only: true,
                        ..Default::default()
                    },
                )
                .len()
                >= MAX_SESSION_MEMORY_CONCURRENCY
            {
                break;
            }
            let Some(job) = store::load_session_memory_job_sqlx(&pool, tenant_id, &job_id).await?
            else {
                continue;
            };
            let detail = match store::load_conversation_session_detail_sqlx(
                &pool,
                tenant_id,
                &job.session_id,
            )
            .await
            {
                Ok(detail) => detail,
                Err(crate::backend::store::error::StoreError::NotFound(_)) => continue,
                Err(error) => return Err(AppError::from(error)),
            };
            let completed = session_has_completion_signal(&detail);
            let idle_ready = session_idle_ready(&detail, now);
            let not_before_ready = DateTime::parse_from_rfc3339(&job.not_before)
                .map(|value| now >= value.with_timezone(&Utc))
                .unwrap_or(false);
            if !completed && (!idle_ready || !not_before_ready) {
                continue;
            }
            let task_id = format!("session-memory-{}", job.id);
            let runtime = self.runtime.clone();
            let job_id_for_task = job.id.clone();
            let tenant_id_for_task = tenant_id.to_string();
            let session_id = job.session_id.clone();
            let run_at = now;
            let short_id: String = session_id.chars().take(8).collect();
            let title = format!("会话记忆生成 (#{short_id})");
            let _ = self.runtime.task_runtime().remove_terminal(&task_id);
            let mut spec = crate::backend::infrastructure::tasks::TaskSpec::new(
                crate::backend::infrastructure::tasks::TaskKind::Memory,
                Some(format!("session-memory-job:{tenant_id}:{job_id}")),
            )
            .with_task_id(task_id)
            .with_tenant_id(tenant_id.to_string())
            .with_title(title)
            .with_capabilities(TaskCapabilities {
                cancellable: true,
                retryable: true,
                clearable: true,
            })
            .with_conflict_key(format!("session-memory-session:{tenant_id}:{session_id}"));
            spec.detail = json!({
                "domain": "session_memory",
                "scope": "session",
                "job_id": job.id,
                "session_id": session_id,
                "attempt_count": job.attempt_count,
            });
            match self
                .runtime
                .task_runtime()
                .spawn_async(spec, move |context| async move {
                    let service = AppService::from_runtime(&runtime)
                        .for_tenant(&tenant_id_for_task)
                        .await?;
                    let result = service
                        .run_session_memory_phase1_for_tenant_at(
                            &tenant_id_for_task,
                            &job_id_for_task,
                            run_at,
                            context,
                        )
                        .await;
                    let phase1_terminal = store::load_session_memory_job_sqlx(
                        service.db.pool(),
                        &tenant_id_for_task,
                        &job_id_for_task,
                    )
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|job| {
                        matches!(
                            job.status,
                            SessionMemoryJobStatus::Succeeded
                                | SessionMemoryJobStatus::Skipped
                                | SessionMemoryJobStatus::Failed
                                | SessionMemoryJobStatus::Canceled
                        )
                    });
                    if phase1_terminal {
                        if let Err(error) = service
                            .reconcile_recent_memory_jobs_for_tenant_at(
                                &tenant_id_for_task,
                                run_at,
                            )
                            .await
                        {
                            tracing::warn!(
                                action = "session_memory.reconcile_recent_after_terminal",
                                tenant_id = %tenant_id_for_task,
                                job_id = %job_id_for_task,
                                error = %error,
                                "Recent Snapshot reconciliation after Session Memory terminal state failed"
                            );
                        }
                    }
                    result.map(|memory| {
                        json!({
                            "domain": "session_memory",
                            "job_id": job_id_for_task,
                            "projected": memory.is_some(),
                        })
                    })
                }) {
                Ok(crate::backend::infrastructure::tasks::SpawnOutcome::Started) => {
                    scheduled += 1;
                }
                Ok(crate::backend::infrastructure::tasks::SpawnOutcome::Existing) => {}
                Err(error) => return Err(AppError::from(error)),
            }
        }
        Ok(scheduled)
    }
}
