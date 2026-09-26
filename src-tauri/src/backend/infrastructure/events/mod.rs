use crate::backend::domain::conversations::DomainEvent;
use crate::backend::infrastructure::InfraError;
use std::path::PathBuf;

pub(crate) mod dispatcher;
pub(crate) mod handle;

pub(crate) use dispatcher::{EventDispatcher, EventDispatcherShutdownReport};
pub(crate) use handle::EventDispatcherHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SequencedEvent {
    pub(crate) seq: i64,
    pub(crate) event: DomainEvent,
}

pub(crate) type ConsumerFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), InfraError>> + Send + 'a>>;

#[derive(Clone)]
pub(crate) struct ConsumerCx {
    pub(crate) pool: sqlx::SqlitePool,
    pub(crate) db_path: PathBuf,
    pub(crate) consumer_id: String,
    pub(crate) tenant_id: String,
    pub(crate) batch_last_seq: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InitialPosition {
    GenesisZero,
    #[allow(dead_code)]
    BackfillThenCutoff,
}

pub(crate) trait DomainEventConsumer: Send + Sync {
    fn id(&self) -> &'static str;
    fn initial_position(&self) -> InitialPosition;
    fn backfill<'a>(&'a self, _cx: &'a ConsumerCx) -> ConsumerFuture<'a> {
        Box::pin(async move {
            Err(InfraError::Conflict(format!(
                "consumer {} requires a backfill-and-cutoff registration",
                self.id()
            )))
        })
    }
    fn interested(&self, event: &DomainEvent) -> bool;
    fn handle<'a>(&'a self, batch: &'a [SequencedEvent], cx: &'a ConsumerCx) -> ConsumerFuture<'a>;
}
