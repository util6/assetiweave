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

/// 事实提交后推进 FTS5 派生语料索引（ADR-0023）。
///
/// 与事实事务彻底解耦：事实写成功即代表同步成功，这里只负责让派生索引用最新快照收敛。
/// 同一批次内按「租户 + 会话」合并，重复变更只写最后一次已提交快照；每个会话一次小事务，
/// 因此失败重试是幂等的，也不会反噬事实同步。
pub struct ConversationCorpusAdvanceConsumer {
    pub database: Option<Database>,
}

impl ConversationCorpusAdvanceConsumer {
    pub fn new(database: Database) -> Self {
        Self {
            database: Some(database),
        }
    }
}

/// 一个租户在本批次内需要推进的派生索引范围。
#[derive(Default)]
struct CorpusAdvancePlan {
    session_ids: BTreeSet<String>,
    /// 事件载荷未携带会话清单（变更超过 256 个）时按源整体刷新。
    source_ids: BTreeSet<String>,
}

impl DomainEventConsumer for ConversationCorpusAdvanceConsumer {
    fn id(&self) -> &'static str {
        "conversation.corpus_advance"
    }

    fn initial_position(&self) -> InitialPosition {
        InitialPosition::GenesisZero
    }

    fn interested(&self, event: &DomainEvent) -> bool {
        matches!(event, DomainEvent::ConversationSourceCommitted { .. })
    }

    fn handle<'a>(
        &'a self,
        batch: &'a [SequencedEvent],
        _cx: &'a ConsumerCx,
    ) -> ConsumerFuture<'a> {
        let mut plans = std::collections::BTreeMap::<String, CorpusAdvancePlan>::new();
        for item in batch {
            let DomainEvent::ConversationSourceCommitted {
                tenant_id,
                source_id,
                changed_session_ids,
                ..
            } = &item.event;
            let plan = plans.entry(tenant_id.clone()).or_default();
            match changed_session_ids {
                Some(session_ids) => plan.session_ids.extend(session_ids.iter().cloned()),
                None => {
                    plan.source_ids.insert(source_id.clone());
                }
            }
        }
        let database = self.database.clone();
        Box::pin(async move {
            let Some(database) = database else {
                return Ok(());
            };
            for (tenant_id, plan) in plans {
                advance_tenant_corpus(database.pool(), &tenant_id, plan)
                    .await
                    .map_err(InfraError::external)?;
            }
            Ok(())
        })
    }
}

async fn advance_tenant_corpus(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    plan: CorpusAdvancePlan,
) -> crate::backend::store::StoreResult<()> {
    use crate::backend::store;

    if store::ensure_conversation_search_corpus_current_sqlx(pool, tenant_id).await? {
        // 语料口径已整体重建过一轮，本批次的变更已被覆盖。
        return Ok(());
    }
    for source_id in &plan.source_ids {
        store::replace_source_search_corpus_sqlx(pool, tenant_id, source_id).await?;
    }
    for session_id in &plan.session_ids {
        store::replace_session_search_corpus_sqlx(pool, tenant_id, session_id).await?;
    }
    let started = std::time::Instant::now();
    tracing::debug!(
        action = "conversation.corpus_advance",
        tenant_id = %tenant_id,
        session_count = plan.session_ids.len(),
        source_count = plan.source_ids.len(),
        elapsed_ms = %started.elapsed().as_millis(),
        "advanced conversation search corpus"
    );
    Ok(())
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
