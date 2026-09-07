use super::{
    ConsumerCx, ConsumerFuture, DomainEvent, DomainEventConsumer, InitialPosition, SequencedEvent,
};
use crate::backend::{runtime::AppError, store::Database};
use std::collections::BTreeSet;

pub(crate) struct SearchIndexAdvanceConsumer {
    pub(crate) database: Option<Database>,
}

impl SearchIndexAdvanceConsumer {
    pub(crate) fn new(database: Database) -> Self {
        Self {
            database: Some(database),
        }
    }
}

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

    fn handle<'a>(&'a self, batch: &'a [SequencedEvent], cx: &'a ConsumerCx) -> ConsumerFuture<'a> {
        let tenants = batch
            .iter()
            .filter_map(|item| match &item.event {
                DomainEvent::ConversationSourceCommitted { tenant_id, .. } => {
                    Some(tenant_id.clone())
                }
                DomainEvent::TeamRunConfirmed { .. } => None,
            })
            .collect::<BTreeSet<_>>();
        let database = self.database.clone();
        let db_path = cx.db_path.clone();
        let consumer_id = cx.consumer_id.clone();
        let batch_last_seq = cx.batch_last_seq;
        Box::pin(async move {
            let Some(database) = database else {
                return Ok(());
            };
            for tenant_id in tenants {
                crate::backend::search::conversation::rebuild_conversation_search_index_with_offset(
                    database.pool(),
                    &db_path,
                    &tenant_id,
                    &consumer_id,
                    batch_last_seq,
                )
                .await?;
            }
            Ok(())
        })
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

    fn backfill<'a>(&'a self, cx: &'a ConsumerCx) -> ConsumerFuture<'a> {
        let internal_agent_workspace =
            crate::backend::ai_execution::agent_execution_workspace_root(&cx.db_path);
        let pool = cx.pool.clone();
        let tenant_id = cx.tenant_id.clone();
        Box::pin(async move {
            crate::backend::store::backfill_session_memory_jobs_sqlx(
                &pool,
                &tenant_id,
                &internal_agent_workspace,
                &chrono::Utc::now().to_rfc3339(),
            )
            .await?;
            Ok(())
        })
    }

    fn interested(&self, event: &DomainEvent) -> bool {
        matches!(event, DomainEvent::ConversationSourceCommitted { .. })
    }

    fn handle<'a>(&'a self, batch: &'a [SequencedEvent], cx: &'a ConsumerCx) -> ConsumerFuture<'a> {
        let internal_agent_workspace =
            crate::backend::ai_execution::agent_execution_workspace_root(&cx.db_path);
        let pool = cx.pool.clone();
        let cx_tenant_id = cx.tenant_id.clone();
        let events = batch.to_vec();
        Box::pin(async move {
            for item in events {
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
                if tenant_id != &cx_tenant_id {
                    return Err(AppError::Conflict(
                        "领域事件租户与消费者租户不一致".to_string(),
                    ));
                }
                crate::backend::store::enqueue_session_memory_jobs_sqlx(
                    &pool,
                    &cx_tenant_id,
                    source_id,
                    sync_run_id,
                    *revision_end,
                    event_id,
                    changed_session_ids.as_deref(),
                    &internal_agent_workspace,
                    &chrono::Utc::now().to_rfc3339(),
                )
                .await?;
            }
            Ok(())
        })
    }
}
