use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum DomainEvent {
    ConversationSourceCommitted {
        event_id: String,
        tenant_id: String,
        sync_run_id: String,
        source_id: String,
        revision_start: i64,
        revision_end: i64,
        changed_session_ids: Option<Vec<String>>,
    },
}

impl DomainEvent {
    pub fn conversation_source_committed(
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

    pub fn metadata(&self) -> (&str, &str, Option<&str>, Option<i64>, Option<i64>) {
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
        }
    }
}

pub fn cap_changed_session_ids(ids: impl IntoIterator<Item = String>) -> Option<Vec<String>> {
    let mut ids = ids.into_iter().collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    (ids.len() <= 256).then_some(ids)
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
