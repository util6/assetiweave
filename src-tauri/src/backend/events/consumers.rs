use super::{ConsumerCx, DomainEvent, DomainEventConsumer, InitialPosition, SequencedEvent};
use crate::backend::runtime::{AppError, AppResult};
use std::collections::BTreeSet;

pub(crate) struct SearchIndexAdvanceConsumer;

impl DomainEventConsumer for SearchIndexAdvanceConsumer {
    fn id(&self) -> &'static str {
        "search.index_advance"
    }

    fn initial_position(&self) -> InitialPosition {
        InitialPosition::GenesisZero
    }

    fn interested(&self, event: &DomainEvent) -> bool {
        matches!(event, DomainEvent::ConversationSourceCommitted { .. })
    }

    fn handle(&self, batch: &[SequencedEvent], cx: &ConsumerCx) -> Result<(), AppError> {
        let tenants = batch
            .iter()
            .filter_map(|item| match &item.event {
                DomainEvent::ConversationSourceCommitted { tenant_id, .. } => {
                    Some(tenant_id.clone())
                }
                DomainEvent::TeamRunConfirmed { .. } => None,
            })
            .collect::<BTreeSet<_>>();
        for tenant_id in tenants {
            crate::backend::search::conversation::rebuild_conversation_search_index_with_offset(
                &cx.database,
                &cx.db_path,
                &tenant_id,
                &cx.consumer_id,
                cx.batch_last_seq,
            )?;
        }
        Ok(())
    }
}

pub(crate) struct MemoryEvidenceStaleConsumer;

pub(crate) struct SessionMemoryConsumer;

impl DomainEventConsumer for SessionMemoryConsumer {
    fn id(&self) -> &'static str {
        "memory.session_enqueue"
    }

    fn initial_position(&self) -> InitialPosition {
        InitialPosition::BackfillThenCutoff
    }

    fn backfill(&self, cx: &ConsumerCx) -> Result<(), AppError> {
        cx.database
            .run_sync(crate::backend::store::backfill_session_memory_jobs_sqlx(
                &cx.pool,
                &cx.tenant_id,
                &chrono::Utc::now().to_rfc3339(),
            ))?;
        Ok(())
    }

    fn interested(&self, event: &DomainEvent) -> bool {
        matches!(event, DomainEvent::ConversationSourceCommitted { .. })
    }

    fn handle(&self, batch: &[SequencedEvent], cx: &ConsumerCx) -> Result<(), AppError> {
        for item in batch {
            let DomainEvent::ConversationSourceCommitted {
                event_id,
                tenant_id,
                sync_run_id,
                source_id,
                revision_end,
                changed_session_ids,
                ..
            } = &item.event
            else {
                continue;
            };
            if tenant_id != &cx.tenant_id {
                return Err(AppError::Conflict(
                    "领域事件租户与消费者租户不一致".to_string(),
                ));
            }
            cx.database
                .run_sync(crate::backend::store::enqueue_session_memory_jobs_sqlx(
                    &cx.pool,
                    &cx.tenant_id,
                    source_id,
                    sync_run_id,
                    *revision_end,
                    event_id,
                    changed_session_ids.as_deref(),
                    &chrono::Utc::now().to_rfc3339(),
                ))?;
        }
        Ok(())
    }
}

impl DomainEventConsumer for MemoryEvidenceStaleConsumer {
    fn id(&self) -> &'static str {
        "memory.evidence_stale"
    }

    fn initial_position(&self) -> InitialPosition {
        InitialPosition::GenesisZero
    }

    fn interested(&self, event: &DomainEvent) -> bool {
        matches!(event, DomainEvent::ConversationSourceCommitted { .. })
    }

    fn handle(&self, batch: &[SequencedEvent], cx: &ConsumerCx) -> Result<(), AppError> {
        for item in batch {
            let DomainEvent::ConversationSourceCommitted {
                tenant_id,
                sync_run_id,
                revision_end,
                changed_session_ids,
                ..
            } = &item.event
            else {
                continue;
            };
            let tenant_id = tenant_id.clone();
            let sync_run_id = sync_run_id.clone();
            let session_ids = changed_session_ids.clone();
            let revision = *revision_end;
            cx.database.block_on(async {
                mark_evidence_stale(
                    &cx.pool,
                    &tenant_id,
                    &sync_run_id,
                    session_ids.as_deref(),
                    revision,
                )
                .await
            })?;
        }
        Ok(())
    }
}

async fn mark_evidence_stale(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    sync_run_id: &str,
    changed_session_ids: Option<&[String]>,
    revision: i64,
) -> AppResult<()> {
    let session_ids = if let Some(ids) = changed_session_ids {
        ids.to_vec()
    } else {
        sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM conversation_sync_deltas WHERE tenant_id = ?1 AND sync_run_id = ?2 AND record_kind IN ('session', 'web')",
        )
        .bind(tenant_id)
        .bind(sync_run_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::Db)?
    };
    let marked_at = chrono::Utc::now().to_rfc3339();
    for session_id in session_ids {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO memory_evidence_staleness (
                tenant_id, evidence_id, record_kind, source_id,
                session_id, stale_since_revision, marked_at
            )
            SELECT tenant_id, id, record_kind, source_id, session_id, ?1, ?2
            FROM memory_evidence_snapshots
            WHERE tenant_id = ?3 AND session_id = ?4
            "#,
        )
        .bind(revision)
        .bind(&marked_at)
        .bind(tenant_id)
        .bind(session_id)
        .execute(pool)
        .await
        .map_err(AppError::Db)?;
    }
    Ok(())
}
