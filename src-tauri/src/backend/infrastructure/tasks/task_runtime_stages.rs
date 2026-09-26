use super::task_models::*;
use super::tasks::TaskRuntime;
use super::{InfraError, InfraResult};
use chrono::Utc;

impl TaskRuntime {
    pub(crate) fn set_stages(
        &self,
        task_id: &str,
        stages: Vec<TaskStage>,
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            entry.snapshot.stages = stages;
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn set_stage_agent_session_ref(
        &self,
        task_id: &str,
        stage_id: &str,
        session_ref: Option<crate::backend::domain::agents::AgentSessionRef>,
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            if let Some(stage) = entry.snapshot.stages.iter_mut().find(|s| s.id == stage_id) {
                stage.agent_session_ref = session_ref.clone();
            }
            if entry.snapshot.agent_session_ref.is_none() {
                entry.snapshot.agent_session_ref = session_ref;
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn update_stage_status(
        &self,
        task_id: &str,
        stage_id: &str,
        status: StageStatus,
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            let now = Utc::now().to_rfc3339();
            if let Some(stage) = entry.snapshot.stages.iter_mut().find(|s| s.id == stage_id) {
                stage.status = status;
                if status == StageStatus::Running && stage.started_at.is_none() {
                    stage.started_at = Some(now.clone());
                } else if status.is_terminal() && stage.finished_at.is_none() {
                    stage.finished_at = Some(now.clone());
                    if let Some(started_at) = &stage.started_at {
                        if let (Ok(s), Ok(f)) = (
                            chrono::DateTime::parse_from_rfc3339(started_at),
                            chrono::DateTime::parse_from_rfc3339(&now),
                        ) {
                            if let Ok(duration) = (f - s).to_std() {
                                stage.duration_ms = Some(duration.as_millis() as u64);
                            }
                        }
                    }
                }
            } else {
                let stage = TaskStage {
                    id: stage_id.to_string(),
                    name: stage_id.to_string(),
                    status,
                    started_at: if status == StageStatus::Running {
                        Some(now.clone())
                    } else {
                        None
                    },
                    finished_at: if status.is_terminal() {
                        Some(now.clone())
                    } else {
                        None
                    },
                    duration_ms: None,
                    progress: None,
                    current_activities: Vec::new(),
                    metrics: Vec::new(),
                    failures: Vec::new(),
                    skipped: Vec::new(),
                    agent_session_ref: None,
                    steps: Vec::new(),
                };
                entry.snapshot.stages.push(stage);
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now;
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn set_stage_progress(
        &self,
        task_id: &str,
        stage_id: &str,
        current: u64,
        total: Option<u64>,
        note: Option<String>,
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            let now = Utc::now().to_rfc3339();
            if let Some(stage) = entry.snapshot.stages.iter_mut().find(|s| s.id == stage_id) {
                stage.progress = Some(TaskProgress {
                    current,
                    total,
                    note,
                });
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now;
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn record_activity(
        &self,
        task_id: &str,
        activity: TaskActivity,
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            let now = Utc::now().to_rfc3339();
            if let Some(stage) = entry
                .snapshot
                .stages
                .iter_mut()
                .find(|s| s.id == activity.stage_id)
            {
                if let Some(existing) = stage
                    .current_activities
                    .iter_mut()
                    .find(|a| a.worker_id == activity.worker_id)
                {
                    *existing = activity;
                } else {
                    stage.current_activities.push(activity);
                }
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now;
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn record_activity_silent(
        &self,
        task_id: &str,
        activity: TaskActivity,
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            let now = Utc::now().to_rfc3339();
            if let Some(stage) = entry
                .snapshot
                .stages
                .iter_mut()
                .find(|s| s.id == activity.stage_id)
            {
                if let Some(existing) = stage
                    .current_activities
                    .iter_mut()
                    .find(|a| a.worker_id == activity.worker_id)
                {
                    *existing = activity;
                } else {
                    stage.current_activities.push(activity);
                }
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now;
            entry.snapshot.clone()
        };
        // silent 模式只更新内存快照，不广播 publish，消除事件风暴
        Ok(snapshot)
    }

    pub(crate) fn remove_activity(
        &self,
        task_id: &str,
        stage_id: &str,
        worker_id: &str,
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            let now = Utc::now().to_rfc3339();
            if let Some(stage) = entry.snapshot.stages.iter_mut().find(|s| s.id == stage_id) {
                stage
                    .current_activities
                    .retain(|a| a.worker_id != worker_id);
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now;
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn record_stage_step(
        &self,
        task_id: &str,
        stage_id: &str,
        operation: impl Into<String>,
        detail: Option<String>,
        current: Option<u64>,
        total: Option<u64>,
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            let now = Utc::now().to_rfc3339();
            if let Some(stage) = entry.snapshot.stages.iter_mut().find(|s| s.id == stage_id) {
                let step = TaskStageStep {
                    timestamp: now.clone(),
                    operation: operation.into(),
                    detail,
                    current,
                    total,
                };
                stage.steps.push(step);
                if stage.steps.len() > 100 {
                    stage.steps.remove(0);
                }
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now;
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn finish_stage(
        &self,
        task_id: &str,
        stage_id: &str,
        status: StageStatus,
        metrics: Vec<TaskMetric>,
        failures: Vec<TaskFailure>,
        skipped: Vec<TaskSkippedGroup>,
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            let now = Utc::now().to_rfc3339();
            if let Some(stage) = entry.snapshot.stages.iter_mut().find(|s| s.id == stage_id) {
                stage.status = status;
                stage.finished_at = Some(now.clone());
                if stage.started_at.is_none() {
                    stage.started_at = Some(now.clone());
                }
                if let (Some(started_at), Some(finished_at)) =
                    (&stage.started_at, &stage.finished_at)
                {
                    if let (Ok(s), Ok(f)) = (
                        chrono::DateTime::parse_from_rfc3339(started_at),
                        chrono::DateTime::parse_from_rfc3339(finished_at),
                    ) {
                        if let Ok(duration) = (f - s).to_std() {
                            stage.duration_ms = Some(duration.as_millis() as u64);
                        }
                    }
                }
                stage.current_activities.clear();
                stage.metrics.extend(metrics.clone());
                stage.failures.extend(failures.clone());
                stage.skipped.extend(skipped.clone());
            } else {
                let stage = TaskStage {
                    id: stage_id.to_string(),
                    name: stage_id.to_string(),
                    status,
                    started_at: Some(now.clone()),
                    finished_at: Some(now.clone()),
                    duration_ms: Some(0),
                    progress: None,
                    current_activities: Vec::new(),
                    metrics: metrics.clone(),
                    failures: failures.clone(),
                    skipped: skipped.clone(),
                    agent_session_ref: None,
                    steps: Vec::new(),
                };
                entry.snapshot.stages.push(stage);
            }
            for metric in metrics {
                if let Some(existing) = entry
                    .snapshot
                    .metrics
                    .iter_mut()
                    .find(|m| m.code == metric.code)
                {
                    existing.value += metric.value;
                } else {
                    entry.snapshot.metrics.push(metric);
                }
            }
            entry.snapshot.failures.extend(failures);
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = now;
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn set_outcome(
        &self,
        task_id: &str,
        outcome: TaskOutcome,
        result_summary: Option<String>,
        error_summary: Option<String>,
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            entry.snapshot.outcome = Some(outcome);
            if let Some(r) = result_summary {
                entry.snapshot.result_summary = Some(r);
            }
            if let Some(e) = error_summary {
                entry.snapshot.error_summary = Some(e);
            }
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }

    pub(crate) fn set_result_summary(
        &self,
        task_id: &str,
        summary: String,
    ) -> InfraResult<TaskSnapshot> {
        let snapshot = {
            let mut tasks = self
                .tasks
                .lock()
                .map_err(|_| InfraError::Conflict("任务注册表不可用".to_string()))?;
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| InfraError::NotFound(format!("任务不存在: {task_id}")))?;
            entry.snapshot.result_summary = Some(summary);
            entry.snapshot.revision += 1;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.clone()
        };
        self.publish(&snapshot);
        Ok(snapshot)
    }
}
