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
        struct TenantAdvance {
            session_ids: BTreeSet<String>,
            requires_rebuild: bool,
        }
        let mut tenants = std::collections::BTreeMap::<String, TenantAdvance>::new();
        for item in batch {
            let DomainEvent::ConversationSourceCommitted {
                tenant_id,
                changed_session_ids,
                ..
            } = &item.event
            else {
                continue;
            };
            let plan = tenants.entry(tenant_id.clone()).or_insert(TenantAdvance {
                session_ids: BTreeSet::new(),
                requires_rebuild: false,
            });
            match changed_session_ids {
                Some(session_ids) => plan.session_ids.extend(session_ids.iter().cloned()),
                // 事件载荷没有携带会话清单（变更超出可可靠承载的边界）：不能猜，
                // 只能请求一次受控整代重建，否则会静默漏掉更新。
                None => plan.requires_rebuild = true,
            }
        }
        let database = self.database.clone();
        let db_path = cx.db_path.clone();
        let consumer_id = cx.consumer_id.clone();
        let batch_last_seq = cx.batch_last_seq;
        Box::pin(async move {
            let Some(database) = database else {
                return Ok(());
            };
            for (tenant_id, plan) in tenants {
                let session_ids = plan.session_ids.into_iter().collect::<Vec<_>>();
                if plan.requires_rebuild || session_ids.is_empty() {
                    super::conversation_search::rebuild_conversation_search_index_with_offset(
                        database.pool(),
                        &db_path,
                        &tenant_id,
                        &consumer_id,
                        batch_last_seq,
                    )
                    .await
                    .map_err(InfraError::external)?;
                    continue;
                }
                super::conversation_search::advance_conversation_search_index(
                    database.pool(),
                    &db_path,
                    &tenant_id,
                    &session_ids,
                    Some((&consumer_id, batch_last_seq)),
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
