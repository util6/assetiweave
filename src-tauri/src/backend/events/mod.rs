//! Durable domain facts shared by the resident dispatcher and one-shot writers.
//!
//! A new event variant must express a committed fact, be reconstructible from
//! persisted state, have a planned consumer, and carry a tenant plus an
//! ordered range. Otherwise it belongs on the task-progress or transport
//! channel instead of this closed enum.

use crate::backend::runtime::{AppError, AppResult};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, Transaction};
use std::path::PathBuf;
use uuid::Uuid;

mod consumers;
mod dispatcher;

pub(crate) use consumers::{SearchIndexAdvanceConsumer, SessionMemoryConsumer};
pub(crate) use dispatcher::{EventDispatcher, EventDispatcherHandle};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub(crate) enum DomainEvent {
    ConversationSourceCommitted {
        event_id: String,
        tenant_id: String,
        sync_run_id: String,
        source_id: String,
        revision_start: i64,
        revision_end: i64,
        changed_session_ids: Option<Vec<String>>,
    },
    TeamRunConfirmed {
        event_id: String,
        tenant_id: String,
        run_id: String,
        team_id: String,
    },
}

impl DomainEvent {
    pub(crate) fn conversation_source_committed(
        tenant_id: &str,
        sync_run_id: &str,
        source_id: &str,
        revision: i64,
        changed_session_ids: impl IntoIterator<Item = String>,
    ) -> Self {
        Self::ConversationSourceCommitted {
            event_id: format!("evt-{}", Uuid::new_v4().simple()),
            tenant_id: tenant_id.to_string(),
            sync_run_id: sync_run_id.to_string(),
            source_id: source_id.to_string(),
            revision_start: revision,
            revision_end: revision,
            changed_session_ids: cap_changed_session_ids(changed_session_ids),
        }
    }

    pub(crate) fn team_run_confirmed(tenant_id: &str, run_id: &str, team_id: &str) -> Self {
        Self::TeamRunConfirmed {
            event_id: format!("evt-{}", Uuid::new_v4().simple()),
            tenant_id: tenant_id.to_string(),
            run_id: run_id.to_string(),
            team_id: team_id.to_string(),
        }
    }

    fn metadata(&self) -> (&str, &str, Option<&str>, Option<i64>, Option<i64>) {
        match self {
            Self::ConversationSourceCommitted {
                tenant_id,
                source_id,
                revision_start,
                revision_end,
                ..
            } => (
                tenant_id,
                "conversation_source_committed",
                Some(source_id),
                Some(*revision_start),
                Some(*revision_end),
            ),
            Self::TeamRunConfirmed {
                tenant_id, team_id, ..
            } => (tenant_id, "team_run_confirmed", Some(team_id), None, None),
        }
    }
}

pub(crate) fn cap_changed_session_ids(
    ids: impl IntoIterator<Item = String>,
) -> Option<Vec<String>> {
    let mut ids = ids.into_iter().collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    (ids.len() <= 256).then_some(ids)
}

pub(crate) async fn append_outbox_event_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    event: &DomainEvent,
) -> AppResult<()> {
    let (tenant_id, event_type, source_id, revision_start, revision_end) = event.metadata();
    let payload =
        serde_json::to_string(event).map_err(|error| AppError::External(error.to_string()))?;
    let event_id = match event {
        DomainEvent::ConversationSourceCommitted { event_id, .. } => event_id,
        DomainEvent::TeamRunConfirmed { event_id, .. } => event_id,
    };
    sqlx::query(
        r#"
        INSERT INTO domain_event_outbox (
            event_id, tenant_id, event_type, source_id,
            revision_start, revision_end, payload, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(event_id)
    .bind(tenant_id)
    .bind(event_type)
    .bind(source_id)
    .bind(revision_start)
    .bind(revision_end)
    .bind(payload)
    .bind(Utc::now().to_rfc3339())
    .execute(&mut **tx)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SequencedEvent {
    pub(crate) seq: i64,
    pub(crate) event: DomainEvent,
}

#[derive(Clone)]
pub(crate) struct ConsumerCx {
    pub(crate) database: crate::backend::store::Database,
    pub(crate) pool: sqlx::SqlitePool,
    pub(crate) db_path: PathBuf,
    pub(crate) consumer_id: String,
    pub(crate) tenant_id: String,
    pub(crate) batch_last_seq: i64,
}

/// Starting position is part of the consumer registration contract.
///
/// `GenesisZero` is valid only for consumers shipped with the initial outbox
/// release. A consumer added after retention has to backfill its read model
/// first and then register at the captured cutoff; it must never silently
/// inherit the zero offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InitialPosition {
    GenesisZero,
    #[allow(dead_code)]
    BackfillThenCutoff,
}

pub(crate) trait DomainEventConsumer: Send + Sync {
    fn id(&self) -> &'static str;
    /// Declare the registration position explicitly. The dispatcher rejects
    /// `BackfillThenCutoff` until the domain's backfill and cutoff migration
    /// are supplied, rather than degrading it to `last_seq = 0`.
    fn initial_position(&self) -> InitialPosition;
    fn backfill(&self, _cx: &ConsumerCx) -> Result<(), AppError> {
        Err(AppError::Conflict(format!(
            "consumer {} requires a backfill-and-cutoff registration",
            self.id()
        )))
    }
    fn interested(&self, event: &DomainEvent) -> bool;
    fn handle(&self, batch: &[SequencedEvent], cx: &ConsumerCx) -> Result<(), AppError>;
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn changed_session_ids_are_capped_and_stable() {
        let ids = (0..257).map(|index| format!("session-{index}"));
        assert_eq!(cap_changed_session_ids(ids), None);
        assert_eq!(
            cap_changed_session_ids(["b".to_string(), "a".to_string(), "a".to_string()]),
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }
}
