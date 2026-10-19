use chrono::{DateTime, Utc};
use futures::stream::{FuturesUnordered, StreamExt};
use std::sync::Arc;

use crate::backend::application::AppService;
use crate::backend::infrastructure::tasks::{InfraError, StageGuard, StageStatus, TaskContext};
use crate::backend::store;

impl AppService {
    /// 执行 Memory 流水线阶段 1 (提取会话事实)
    pub(super) async fn run_session_facts_pipeline_stage(
        &self,
        guard: &mut StageGuard,
        now: DateTime<Utc>,
        force_rebuild: bool,
        cx: &TaskContext,
    ) -> Result<(usize, usize), InfraError> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let now_text = now.to_rfc3339();

        let _ = self
            .ensure_recent_snapshot_phase1_jobs_at(now, force_rebuild)
            .await;
        let _ =
            store::recover_expired_session_memory_leases_sqlx(&pool, &tenant_id, &now_text).await;

        // 过期租约回收由 recover_expired_session_memory_leases_sqlx 统一收敛，遵循退避指数，避免清空 retry_at 造成重试风暴

        let job_ids =
            store::list_session_memory_job_ids_for_scheduler_sqlx(&pool, &tenant_id, &now_text, 64)
                .await
                .unwrap_or_default();
        let total = job_ids.len();
        guard.record_metric("sessions_total", total as u64);

        if let Some(first_jid) = job_ids.first() {
            guard.set_agent_session_ref(Some(
                crate::backend::domain::agents::AgentSessionRef::new(format!("sm-{}", first_jid)),
            ));
        }

        let mut s1_ok = 0usize;
        let mut s1_fail = 0usize;

        if total > 0 {
            let sem = Arc::new(tokio::sync::Semaphore::new(2));
            let mut futs = FuturesUnordered::new();
            for jid in &job_ids {
                let sid: String = sqlx::query_scalar(
                    "SELECT session_id FROM session_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
                )
                .bind(&tenant_id)
                .bind(jid)
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| jid.clone());

                guard.activity(jid, "提取会话事实", Some(sid.clone()));
                let (sem, s, tid, cancel, jid_c, now_text_c) = (
                    sem.clone(),
                    self.clone(),
                    tenant_id.clone(),
                    cx.cancellation().clone(),
                    jid.clone(),
                    now_text.clone(),
                );
                futs.push(async move {
                    let _p = sem
                        .acquire()
                        .await
                        .map_err(|_| (jid_c.clone(), sid.clone(), "semaphore closed".into()))?;
                    if cancel.is_cancelled() {
                        return Err((jid_c, sid, "Task cancelled".into()));
                    }
                    let ref_val = format!("sm-{}", jid_c);
                    let s_for_idle = s.clone();
                    let ref_for_idle = ref_val.clone();
                    let check_idle = move || {
                        s_for_idle
                            .runtime
                            .session_streams()
                            .elapsed_since_active_by_ref(&ref_for_idle)
                            .unwrap_or(std::time::Duration::ZERO)
                    };

                    let watchdog_res = super::activity_watchdog::run_with_activity_watchdog(
                        s.run_session_memory_phase1_for_tenant_at(
                            &tid,
                            &jid_c,
                            now,
                            TaskContext::detached("memory_pipeline_session_worker"),
                        ),
                        cancel.clone(),
                        check_idle,
                        super::activity_watchdog::ActivityWatchdogConfig::default(),
                    )
                    .await;

                    match watchdog_res {
                        Ok(Ok(_)) => Ok((jid_c, sid)),
                        Ok(Err(e)) => Err((jid_c, sid, e.to_string())),
                        Err(timeout_kind) => {
                            let err_code = timeout_kind.error_code();
                            let err_msg = timeout_kind.display_message("会话事实提取");
                            s.runtime.session_streams().mark_terminal_by_ref(
                                &format!("sm-{}", jid_c),
                                Some(crate::backend::domain::agents::AgentSessionTerminalView {
                                    state: "failed".to_string(),
                                    code: Some(err_code.to_string()),
                                    message: Some(err_msg.clone()),
                                    retryable: true,
                                }),
                            );
                            let retry_at_text = (now + chrono::Duration::seconds(15)).to_rfc3339();
                            let _ = sqlx::query(
                                "UPDATE session_memory_jobs SET status = 'failed', last_error = ?1, \
                                 lease_expires_at = NULL, ownership_token = NULL, retry_count = retry_count + 1, \
                                 retry_at = ?2, updated_at = ?3 WHERE tenant_id = ?4 AND id = ?5 AND status = 'running'",
                            )
                            .bind(err_code)
                            .bind(&retry_at_text)
                            .bind(&now_text_c)
                            .bind(&tid)
                            .bind(&jid_c)
                            .execute(s.db.pool())
                            .await;
                            Err((jid_c, sid, err_msg))
                        }
                    }
                });
            }

            let mut processed = 0u64;
            while let Some(res) = futs.next().await {
                processed += 1;
                match res {
                    Ok((jid, sid)) => {
                        s1_ok += 1;
                        guard.remove_activity(&jid);
                        guard.record_step(
                            "session_fact_extracted",
                            Some(format!("会话 {sid} 事实提取完成")),
                            Some(processed),
                            Some(total as u64),
                        );
                    }
                    Err((jid, sid, msg)) => {
                        s1_fail += 1;
                        guard.remove_activity(&jid);
                        const MAX_STAGE_FAILURE_SAMPLES: usize = 3;
                        if s1_fail <= MAX_STAGE_FAILURE_SAMPLES {
                            guard.record_failure(
                                "session_fact_failed",
                                format!("会话 {sid} 失败: {msg}"),
                                true,
                            );
                        }
                        guard.record_step(
                            "session_fact_failed",
                            Some(format!("会话 {sid} 提取失败: {msg}")),
                            Some(processed),
                            Some(total as u64),
                        );
                    }
                }
                guard.progress(
                    processed,
                    Some(total as u64),
                    Some(format!("提取会话事实: {processed}/{total}")),
                );
            }

            const MAX_STAGE_FAILURE_SAMPLES: usize = 3;
            if s1_fail > MAX_STAGE_FAILURE_SAMPLES {
                guard.record_failure(
                    "session_facts_partial_failed",
                    format!(
                        "共 {s1_fail} 个会话事实提取失败（前 {MAX_STAGE_FAILURE_SAMPLES} 条已采样展示，其余见步骤日志）"
                    ),
                    true,
                );
            }
        } else {
            guard.record_step(
                "check_session_facts",
                Some("所有会话事实均为最新，跳过提取".into()),
                Some(0),
                Some(0),
            );
            guard.progress(0, Some(0), Some("无待提取会话".into()));
        }

        guard.record_metric("sessions_succeeded", s1_ok as u64);
        guard.record_metric("sessions_failed", s1_fail as u64);

        let existing: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM session_memories WHERE tenant_id = ?1 AND status = 'active'",
        )
        .bind(&tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

        let s1_status = if s1_fail > 0 && (s1_ok > 0 || existing > 0) {
            StageStatus::PartialSuccess
        } else if s1_fail > 0 && s1_ok == 0 && existing == 0 {
            StageStatus::Failed
        } else {
            StageStatus::Succeeded
        };

        guard.finish_with_status(s1_status);
        if s1_status == StageStatus::Failed {
            return Err(InfraError::External(format!(
                "Memory 流水线阶段 1 (提取会话事实) 失败: {s1_fail} 个会话全部处理失败"
            )));
        }

        Ok((s1_ok, s1_fail))
    }
}
