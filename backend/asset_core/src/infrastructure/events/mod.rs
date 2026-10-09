use crate::backend::domain::conversations::DomainEvent;
use crate::backend::infrastructure::InfraError;
use std::path::PathBuf;

pub mod dispatcher;
pub mod handle;

pub use dispatcher::{EventDispatcher, EventDispatcherShutdownReport};
pub use handle::EventDispatcherHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequencedEvent {
    pub seq: i64,
    pub event: DomainEvent,
}

pub type ConsumerFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), InfraError>> + Send + 'a>>;

#[derive(Clone)]
pub struct ConsumerCx {
    pub pool: sqlx::SqlitePool,
    pub db_path: PathBuf,
    pub consumer_id: String,
    pub tenant_id: String,
    pub batch_last_seq: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitialPosition {
    GenesisZero,
    #[allow(dead_code)]
    BackfillThenCutoff,
}

pub trait DomainEventConsumer: Send + Sync {
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
