use crate::backend::domain::conversations::DomainEvent;
use crate::backend::infrastructure::events::{
    ConsumerCx, ConsumerFuture, DomainEventConsumer, InitialPosition, SequencedEvent,
};
use crate::backend::infrastructure::InfraError;
use crate::backend::store::Database;
use std::collections::BTreeSet;

pub struct SearchIndexAdvanceConsumer {
    pub database: Option<Database>,
}

impl SearchIndexAdvanceConsumer {
    pub fn new(database: Database) -> Self {
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
                super::conversation_search::rebuild_conversation_search_index_with_offset(
                    database.pool(),
                    &db_path,
                    &tenant_id,
                    &consumer_id,
                    batch_last_seq,
                )
                .await
                .map_err(InfraError::external)?;
            }
            Ok(())
        })
    }
}

pub struct SessionMemoryConsumer;

impl DomainEventConsumer for SessionMemoryConsumer {
    fn id(&self) -> &'static str {
        "memory.session_enqueue"
    }

    fn initial_position(&self) -> InitialPosition {
        InitialPosition::BackfillThenCutoff
    }

    fn backfill<'a>(&'a self, _cx: &'a ConsumerCx) -> ConsumerFuture<'a> {
        Box::pin(async move { Ok(()) })
    }

    fn interested(&self, event: &DomainEvent) -> bool {
        matches!(event, DomainEvent::ConversationSourceCommitted { .. })
    }

    fn handle<'a>(
        &'a self,
        _batch: &'a [SequencedEvent],
        _cx: &'a ConsumerCx,
    ) -> ConsumerFuture<'a> {
        Box::pin(async move { Ok(()) })
    }
}

#[cfg(test)]
#[path = "event_handlers_tests.rs"]
mod tests;
