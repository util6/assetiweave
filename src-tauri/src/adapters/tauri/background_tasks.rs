use crate::backend::{application::ConversationSyncParams, dto::AppResult};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ConversationSyncTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) source_id: Option<String>,
    pub(crate) adapter_id: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<String>,
}

#[derive(Default)]
pub(crate) struct BackgroundTaskRegistry {
    conversation_sync: Mutex<Option<ConversationSyncTaskSnapshot>>,
}

impl BackgroundTaskRegistry {
    pub(crate) fn begin_conversation_sync(
        &self,
        params: &ConversationSyncParams,
    ) -> AppResult<(ConversationSyncTaskSnapshot, bool)> {
        let mut current = self
            .conversation_sync
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(snapshot) = current
            .as_ref()
            .filter(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
        {
            return Ok((snapshot.clone(), false));
        }

        let snapshot = ConversationSyncTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            source_id: params.source_id.clone(),
            adapter_id: params.adapter_id.clone(),
            dry_run: params.dry_run,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        *current = Some(snapshot.clone());
        Ok((snapshot, true))
    }

    pub(crate) fn finish_conversation_sync(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<ConversationSyncTaskSnapshot> {
        let mut current = self
            .conversation_sync
            .lock()
            .map_err(|error| error.to_string())?;
        let snapshot = current
            .as_mut()
            .ok_or_else(|| "conversation sync task not found".to_string())?;
        if snapshot.id != task_id {
            return Err(format!(
                "conversation sync task is no longer current: {task_id}"
            ));
        }

        snapshot.finished_at = Some(Utc::now().to_rfc3339());
        match result {
            Ok(value) => {
                snapshot.status = BackgroundTaskStatus::Completed;
                snapshot.result = Some(value);
                snapshot.error = None;
            }
            Err(error) => {
                snapshot.status = BackgroundTaskStatus::Failed;
                snapshot.result = None;
                snapshot.error = Some(error);
            }
        }
        Ok(snapshot.clone())
    }

    pub(crate) fn conversation_sync_snapshot(
        &self,
    ) -> AppResult<Option<ConversationSyncTaskSnapshot>> {
        self.conversation_sync
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn has_running_tasks(&self) -> bool {
        self.conversation_sync
            .lock()
            .map(|snapshot| {
                snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
            })
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> ConversationSyncParams {
        ConversationSyncParams {
            source_id: None,
            adapter_id: None,
            dry_run: false,
        }
    }

    #[test]
    fn duplicate_start_reuses_the_running_sync_task() {
        let registry = BackgroundTaskRegistry::default();

        let (first, should_start_first) = registry.begin_conversation_sync(&params()).unwrap();
        let (second, should_start_second) = registry.begin_conversation_sync(&params()).unwrap();

        assert!(should_start_first);
        assert!(!should_start_second);
        assert_eq!(first.id, second.id);
        assert!(registry.has_running_tasks());
    }

    #[test]
    fn finishing_sync_records_success_or_failure() {
        let registry = BackgroundTaskRegistry::default();
        let (running, _) = registry.begin_conversation_sync(&params()).unwrap();

        let completed = registry
            .finish_conversation_sync(&running.id, Ok(serde_json::json!({ "results": [] })))
            .unwrap();

        assert_eq!(completed.status, BackgroundTaskStatus::Completed);
        assert!(completed.result.is_some());
        assert!(!registry.has_running_tasks());

        let (running, _) = registry.begin_conversation_sync(&params()).unwrap();
        let failed = registry
            .finish_conversation_sync(&running.id, Err("sync failed".to_string()))
            .unwrap();

        assert_eq!(failed.status, BackgroundTaskStatus::Failed);
        assert_eq!(failed.error.as_deref(), Some("sync failed"));
        assert!(!registry.has_running_tasks());
    }
}
