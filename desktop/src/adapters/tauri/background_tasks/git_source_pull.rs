//! Background Tasks: Git Source Pull
//!
//! 支持以仓库为单位更新 Git 来源技能源的后台异步任务模型与注册管理。

use super::BackgroundTaskRegistry;
use super::{
    background_task_status, runtime_error_message, BackgroundTaskProjection, BackgroundTaskStatus,
};
use crate::backend::{
    application::{AppError, AppResult},
    domain::AppErrorView,
    infrastructure::tasks::{ExternalRegistrationOutcome, TaskKind, TaskSnapshot, TaskState},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct GitSourcePullItemResult {
    pub(crate) source_id: String,
    pub(crate) source_name: String,
    pub(crate) success: bool,
    pub(crate) changed: bool,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct GitSourcePullTaskProgress {
    pub(crate) completed_count: usize,
    pub(crate) total_count: usize,
    pub(crate) current_source_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct GitSourcePullTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) source_ids: Vec<String>,
    pub(crate) progress: GitSourcePullTaskProgress,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) results: Vec<GitSourcePullItemResult>,
    pub(crate) error: Option<AppErrorView>,
}

impl BackgroundTaskProjection for GitSourcePullTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        self.status = background_task_status(runtime.state);
        self.finished_at = runtime.finished_at.clone();
        match runtime.state {
            TaskState::Succeeded => {
                if let Some(val) = runtime.result.as_ref() {
                    if let Ok(res) = serde_json::from_value(val.clone()) {
                        self.results = res;
                    }
                }
                self.error = None;
            }
            TaskState::Failed => {
                self.error = runtime_error_message(runtime);
            }
            TaskState::Canceled => {
                self.error = runtime_error_message(runtime);
            }
            TaskState::Pending | TaskState::Running | TaskState::Cancelling => {}
        }
        self
    }
}

impl BackgroundTaskRegistry {
    pub(crate) fn begin_git_source_pull(
        &self,
        tenant_id: &str,
        source_ids: Vec<String>,
    ) -> AppResult<(GitSourcePullTaskSnapshot, bool)> {
        let id = Uuid::new_v4().to_string();
        let total = source_ids.len();
        let snapshot = GitSourcePullTaskSnapshot {
            id: id.clone(),
            status: BackgroundTaskStatus::Running,
            source_ids: source_ids.clone(),
            progress: GitSourcePullTaskProgress {
                completed_count: 0,
                total_count: total,
                current_source_name: None,
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            results: Vec::new(),
            error: None,
        };
        let conflict_keys = source_ids
            .iter()
            .map(|source_id| format!("git-pull:{tenant_id}:{source_id}"));
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::GitSourcePull,
            &id,
            Some(format!("git-pull:{tenant_id}:{id}")),
            conflict_keys,
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(runtime) => {
                Ok((self.projection_from_runtime(&runtime)?, true))
            }
            ExternalRegistrationOutcome::Existing(runtime)
            | ExternalRegistrationOutcome::Conflict(runtime) => {
                Ok((self.projection_from_runtime(&runtime)?, false))
            }
        }
    }

    pub(crate) fn update_git_source_pull_progress(
        &self,
        task_id: &str,
        completed_count: usize,
        total_count: usize,
        current_source_name: Option<String>,
        item_result: Option<GitSourcePullItemResult>,
    ) -> AppResult<GitSourcePullTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: GitSourcePullTaskSnapshot = self.decode(&runtime)?;
        if runtime.state.is_active() {
            snapshot.progress.completed_count = completed_count;
            snapshot.progress.total_count = total_count;
            snapshot.progress.current_source_name = current_source_name;
            if let Some(res) = item_result {
                snapshot.results.push(res);
            }
            self.write_projection(task_id, &snapshot)?;
        }
        self.projection(task_id)
    }

    pub(crate) fn finish_git_source_pull(
        &self,
        task_id: &str,
        results: Vec<GitSourcePullItemResult>,
        error: Option<AppError>,
    ) -> AppResult<GitSourcePullTaskSnapshot> {
        let runtime_result = match error {
            None => serde_json::to_value(&results).map_err(AppError::external),
            Some(err) => Err(err),
        };
        let runtime = self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: GitSourcePullTaskSnapshot = self.decode(&runtime)?;
        snapshot.results = results;
        self.write_projection(task_id, &snapshot)?;
        self.projection_from_runtime(&self.external_task_snapshot(task_id)?)
    }

    pub(crate) fn git_source_pull_snapshot_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<GitSourcePullTaskSnapshot> {
        self.projection_for_tenant(tenant_id, task_id)
    }

    pub(crate) fn git_source_pull_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<GitSourcePullTaskSnapshot>> {
        let mut snapshots = self.list_projections_for_tenant::<GitSourcePullTaskSnapshot>(
            tenant_id,
            TaskKind::GitSourcePull,
        )?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn cancel_git_source_pull_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<GitSourcePullTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
    }
}

#[cfg(test)]
#[path = "git_source_pull_tests.rs"]
mod tests;
