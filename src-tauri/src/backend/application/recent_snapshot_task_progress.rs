use crate::backend::runtime::{
    tasks::{ProgressHandle, StageStatus, TaskFailure, TaskStage},
    AppError,
};
use chrono::Utc;

pub(super) fn recent_snapshot_task_stages() -> Vec<TaskStage> {
    [
        ("claim", "领取工作与租约"),
        ("load_facts", "加载上下文与事实"),
        ("agent_execution", "调用 Agent 提取"),
        ("validation", "校验记忆卡片"),
        ("publish", "持久化与发布"),
        ("cleanup_session", "清理 Agent 会话"),
    ]
    .into_iter()
    .map(|(id, name)| TaskStage {
        id: id.to_string(),
        name: name.to_string(),
        status: StageStatus::Pending,
        started_at: None,
        finished_at: None,
        duration_ms: None,
        progress: None,
        current_activities: Vec::new(),
        metrics: Vec::new(),
        failures: Vec::new(),
        skipped: Vec::new(),
        agent_session_ref: None,
    })
    .collect()
}

pub(super) struct RecentSnapshotTaskProgress {
    handle: ProgressHandle,
    current_stage: Option<&'static str>,
}

impl RecentSnapshotTaskProgress {
    pub(super) fn start(handle: ProgressHandle) -> Self {
        handle.set_stages(recent_snapshot_task_stages());
        handle.update_stage_status("claim", StageStatus::Running);
        Self {
            handle,
            current_stage: Some("claim"),
        }
    }

    pub(super) fn transition(&mut self, next_stage: &'static str) {
        if let Some(current) = self.current_stage.take() {
            self.handle.finish_stage(
                current,
                StageStatus::Succeeded,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
        }
        self.handle
            .update_stage_status(next_stage, StageStatus::Running);
        self.current_stage = Some(next_stage);
    }

    pub(super) fn skip_current_and_transition(&mut self, next_stage: &'static str) {
        if let Some(current) = self.current_stage.take() {
            self.handle.finish_stage(
                current,
                StageStatus::Skipped,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
        }
        self.handle
            .update_stage_status(next_stage, StageStatus::Running);
        self.current_stage = Some(next_stage);
    }

    pub(super) fn finish(&mut self) {
        if let Some(current) = self.current_stage.take() {
            self.handle.finish_stage(
                current,
                StageStatus::Succeeded,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
        }
    }

    pub(super) fn fail(&mut self, error: &AppError) {
        let Some(current) = self.current_stage.take() else {
            return;
        };
        let status = if matches!(error, AppError::Cancelled(_)) {
            StageStatus::Canceled
        } else {
            StageStatus::Failed
        };
        self.handle.finish_stage(
            current,
            status,
            Vec::new(),
            vec![TaskFailure {
                code: error.code(),
                message: error.to_string(),
                stage: current.to_string(),
                identity: None,
                retryable: error.retryable(),
                path: None,
                timestamp: Utc::now().to_rfc3339(),
            }],
            Vec::new(),
        );
    }
}

#[cfg(test)]
#[path = "recent_snapshot_task_progress_tests.rs"]
mod tests;
