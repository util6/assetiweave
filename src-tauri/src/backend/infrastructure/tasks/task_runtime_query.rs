use super::task_models::*;
use super::tasks::{TaskEntry, TaskRuntime};
use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::time::Instant;

impl TaskRuntime {
    pub(crate) fn get(&self, task_id: &str) -> Option<TaskSnapshot> {
        let mut tasks = self.tasks.lock().ok()?;
        Self::prune_terminal_tasks_locked(&mut tasks);
        tasks.get(task_id).map(|e| e.snapshot.clone())
    }

    pub(crate) fn get_for_tenant(&self, tenant_id: &str, task_id: &str) -> Option<TaskSnapshot> {
        self.get(task_id).filter(|snapshot| {
            snapshot.tenant_id.is_none() || snapshot.tenant_id.as_deref() == Some(tenant_id)
        })
    }

    pub(crate) fn list(&self, filter: TaskFilter) -> Vec<TaskSnapshot> {
        let Ok(mut tasks) = self.tasks.lock() else {
            return Vec::new();
        };
        Self::prune_terminal_tasks_locked(&mut tasks);
        let mut snapshots = tasks
            .values()
            .filter(|entry| filter.kind.is_none_or(|kind| kind == entry.snapshot.kind))
            .filter(|entry| !filter.user_visible_only || entry.snapshot.user_visible)
            .filter(|entry| {
                !filter.active_only
                    || matches!(
                        entry.snapshot.state,
                        TaskState::Pending | TaskState::Running | TaskState::Cancelling
                    )
            })
            .map(|entry| entry.snapshot.clone())
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| {
            left.started_at
                .cmp(&right.started_at)
                .then_with(|| left.task_id.cmp(&right.task_id))
        });
        snapshots
    }

    pub(crate) fn list_for_tenant(&self, tenant_id: &str, filter: TaskFilter) -> Vec<TaskSnapshot> {
        self.list(filter)
            .into_iter()
            .filter(|snapshot| {
                snapshot.tenant_id.is_none() || snapshot.tenant_id.as_deref() == Some(tenant_id)
            })
            .collect()
    }

    pub(crate) fn cancel(&self, task_id: &str) -> CancelOutcome {
        let Ok(mut tasks) = self.tasks.lock() else {
            return CancelOutcome::NotFound;
        };
        Self::prune_terminal_tasks_locked(&mut tasks);
        let Some(entry) = tasks.get_mut(task_id) else {
            return CancelOutcome::NotFound;
        };
        if entry.snapshot.state.is_active() {
            entry.cancellation.cancel();
            entry.snapshot.state = TaskState::Cancelling;
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            let snapshot = entry.snapshot.clone();
            drop(tasks);
            self.publish(&snapshot);
            return CancelOutcome::Requested(snapshot);
        }
        CancelOutcome::AlreadyFinished(entry.snapshot.clone())
    }

    pub(crate) fn cancel_for_tenant(&self, tenant_id: &str, task_id: &str) -> CancelOutcome {
        if self.get_for_tenant(tenant_id, task_id).is_none() {
            return CancelOutcome::NotFound;
        }
        self.cancel(task_id)
    }

    pub(crate) fn stop_accepting(&self) {
        self.accepting.store(false, Ordering::Release);
        self.tracker.close();
    }

    pub(crate) async fn shutdown_until(&self, deadline: Instant) -> ShutdownReport {
        self.stop_accepting();
        {
            let tasks = self
                .tasks
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            for entry in tasks
                .values()
                .filter(|entry| entry.snapshot.state.is_active())
            {
                entry.cancellation.cancel();
            }
        }
        self.tracker.close();
        let remaining = deadline.saturating_duration_since(Instant::now());
        let _ = tokio::time::timeout(remaining, self.tracker.wait()).await;

        let unfinished_task_ids = self
            .tasks
            .lock()
            .map(|tasks| {
                tasks
                    .values()
                    .filter(|entry| entry.snapshot.state.is_active())
                    .map(|entry| entry.snapshot.task_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        ShutdownReport {
            unfinished_task_ids,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn shutdown_with_grace(&self, grace: Duration) -> ShutdownReport {
        self.shutdown_until(Instant::now() + grace).await
    }

    pub(crate) fn clear_terminal(&self, tenant_id: Option<&str>) -> usize {
        let Ok(mut tasks) = self.tasks.lock() else {
            return 0;
        };
        let mut to_remove = Vec::new();
        for (task_id, entry) in tasks.iter() {
            if entry.snapshot.state.is_terminal() {
                if tenant_id.is_none()
                    || entry.snapshot.tenant_id.as_deref() == tenant_id
                    || entry.snapshot.tenant_id.is_none()
                {
                    to_remove.push(task_id.clone());
                }
            }
        }
        let count = to_remove.len();
        for task_id in to_remove {
            if let Some(mut entry) = tasks.remove(&task_id) {
                let _ = entry.tracking.take();
            }
        }
        count
    }

    pub(crate) fn prune_terminal_tasks_locked(tasks: &mut HashMap<String, TaskEntry>) {
        let now = Utc::now();
        let retention = chrono::Duration::from_std(TASK_TERMINAL_RETENTION)
            .unwrap_or_else(|_| chrono::Duration::zero());
        let mut terminal = tasks
            .values()
            .filter(|entry| entry.snapshot.state.is_terminal())
            .map(|entry| {
                let finished_at = entry
                    .snapshot
                    .finished_at
                    .as_deref()
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .map(|value| value.with_timezone(&Utc));
                (
                    entry.snapshot.task_id.clone(),
                    finished_at,
                    entry.snapshot.user_visible,
                )
            })
            .collect::<Vec<_>>();

        let mut remove_ids = terminal
            .iter()
            .filter_map(|(task_id, finished_at, user_visible)| {
                (!user_visible
                    && finished_at.is_some_and(|f| now.signed_duration_since(f) >= retention))
                .then_some(task_id.clone())
            })
            .collect::<Vec<_>>();
        terminal.retain(|(task_id, _, _)| !remove_ids.iter().any(|removed| removed == task_id));

        terminal.sort_by(|(_, left, _), (_, right, _)| left.cmp(right));
        let excess = terminal.len().saturating_sub(TASK_TERMINAL_LIMIT);
        remove_ids.extend(
            terminal
                .into_iter()
                .take(excess)
                .map(|(task_id, _, _)| task_id),
        );
        for task_id in remove_ids {
            tasks.remove(&task_id);
        }
    }
}

pub(crate) fn sanitize_task_detail(mut detail: Value) -> Value {
    if let Some(object) = detail.as_object_mut() {
        object.remove("result");
        object.remove("assets");
    }
    detail
}
