use super::{ConsumerCx, DomainEvent, DomainEventConsumer, InitialPosition, SequencedEvent};
use crate::backend::runtime::AppError;
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

pub(crate) struct SessionMemoryConsumer;

impl DomainEventConsumer for SessionMemoryConsumer {
    fn id(&self) -> &'static str {
        "memory.session_enqueue"
    }

    fn initial_position(&self) -> InitialPosition {
        InitialPosition::BackfillThenCutoff
    }

    fn backfill(&self, cx: &ConsumerCx) -> Result<(), AppError> {
        let internal_agent_workspace =
            crate::backend::ai_execution::agent_execution_workspace_root(&cx.db_path);
        cx.database
            .run_sync(crate::backend::store::backfill_session_memory_jobs_sqlx(
                &cx.pool,
                &cx.tenant_id,
                &internal_agent_workspace,
                &chrono::Utc::now().to_rfc3339(),
            ))?;
        Ok(())
    }

    fn interested(&self, event: &DomainEvent) -> bool {
        matches!(event, DomainEvent::ConversationSourceCommitted { .. })
    }

    fn handle(&self, batch: &[SequencedEvent], cx: &ConsumerCx) -> Result<(), AppError> {
        let internal_agent_workspace =
            crate::backend::ai_execution::agent_execution_workspace_root(&cx.db_path);
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
                    &internal_agent_workspace,
                    &chrono::Utc::now().to_rfc3339(),
                ))?;
        }
        Ok(())
    }
}
