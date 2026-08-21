//! Tauri 后台异步长任务管理与状态推送模块
//!
//! 支持包含会话同步、内存整理 (Memory Run)、扫描索引、备份导入导出以及脚本安装卸载在内的异步后台任务注册、取消控制、状态快照与事件广播。

use crate::backend::{
    agent_market::types::{
        AgentLifecycleTaskSnapshot, AgentMarketError, LifecycleTaskPhase, LifecycleTaskState,
        ProgressSnapshot,
    },
    agents::types::AgentId,
    ai_execution::{
        AiExecutionCancellation, AiExecutionError, AiExecutionErrorView, AiExecutionPhase,
        AiExecutionPurpose, AiExecutionResult,
    },
    application::{
        AgentMarketRefreshResult, ConversationAdapterPackageInstallParams,
        ConversationAdapterPackageUninstallParams, ConversationScriptInstallParams,
        ConversationSyncMode, ConversationSyncParams, MemoryTaskStartParams,
    },
    dto::CatalogAsset,
    extension_kernel::{
        LifecycleOp, LifecycleRequestKey, LifecycleReservationOutcome, LifecycleTaskCoordinator,
        PackageIdentity, PackageKind, ResourceKey,
    },
    models::{MemoryDreamTrigger, MemoryRunKind, MemoryScope},
    runtime::tasks::{
        ExternalRegistrationOutcome, TaskFn, TaskKind, TaskRuntime, TaskSnapshot, TaskSpec,
        TaskState,
    },
    runtime::AppResult,
};
use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

/// 后台异步任务的状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackgroundTaskStatus {
    /// 任务正在后台运行中
    Running,
    /// 任务已收到取消请求，等待 worker 收敛
    Cancelling,
    /// 任务已成功完成
    Completed,
    /// 任务运行失败
    Failed,
    /// 任务已被用户取消
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AiExecutionTaskState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl AiExecutionTaskState {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct AiExecutionPublicResult {
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct AiExecutionTaskSnapshot {
    pub(crate) id: String,
    pub(crate) purpose: AiExecutionPurpose,
    pub(crate) agent_id: String,
    pub(crate) state: AiExecutionTaskState,
    pub(crate) phase: AiExecutionPhase,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<AiExecutionPublicResult>,
    pub(crate) error: Option<AiExecutionErrorView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AiExecutionShutdownReport {
    pub(crate) cancelled_count: usize,
    pub(crate) remaining_count: usize,
    pub(crate) converged: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AiExecutionTaskGetParams {
    pub(crate) task_id: String,
}

/// 会话同步任务进度快照
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationSyncTaskProgress {
    /// 当前运行阶段
    pub(crate) phase: ConversationSyncProgressPhase,
    /// 已完成处理的数据源数量
    pub(crate) completed_source_count: usize,
    /// 需要处理的总数据源数量
    pub(crate) total_source_count: usize,
    /// 当前正在同步的数据源名称
    pub(crate) current_source_name: Option<String>,
}

/// 会话同步任务阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationSyncProgressPhase {
    /// 正在准备与初始化同步环境
    Preparing,
    /// 正在同步会话记录
    Syncing,
    /// 同步完成
    Completed,
    /// 同步过程发生错误
    Failed,
    /// 同步已被取消
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationSyncTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) source_id: Option<String>,
    pub(crate) adapter_id: Option<String>,
    pub(crate) record_kind: Option<String>,
    pub(crate) mode: ConversationSyncMode,
    pub(crate) dry_run: bool,
    pub(crate) progress: ConversationSyncTaskProgress,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationSearchIndexTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceScanScope {
    All,
    Skills,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceScanProgressPhase {
    Preparing,
    Scanning,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct SourceScanTaskProgress {
    pub(crate) phase: SourceScanProgressPhase,
    pub(crate) completed_source_count: u64,
    pub(crate) total_source_count: Option<u64>,
    pub(crate) current_source_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct SourceScanTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) scope: SourceScanScope,
    pub(crate) kind: Option<crate::backend::models::AssetKind>,
    pub(crate) progress: SourceScanTaskProgress,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Vec<CatalogAsset>>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct BatchMountTaskProgress {
    pub(crate) phase: String,
    pub(crate) completed: u64,
    pub(crate) total: Option<u64>,
    pub(crate) current_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct BatchMountTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) mode: String,
    pub(crate) profile_id: String,
    pub(crate) progress: BatchMountTaskProgress,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ConversationSyncScope {
    All,
    Session,
    Web,
}

impl ConversationSyncScope {
    fn from_record_kind(record_kind: Option<&str>) -> AppResult<Self> {
        let Some(record_kind) = record_kind.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(Self::All);
        };
        match record_kind {
            "session" | "sessions" | "conversation" | "conversations" => Ok(Self::Session),
            "web" | "web-record" | "web_record" | "web-records" | "web_records" => Ok(Self::Web),
            _ => Err(crate::backend::runtime::AppError::Validation(format!(
                "unsupported conversation record kind: {record_kind}"
            ))),
        }
    }

    fn record_kind(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Session => Some("session"),
            Self::Web => Some("web"),
        }
    }

    fn dedup_key(self) -> &'static str {
        match self {
            Self::All => "conversation-sync:all",
            Self::Session => "conversation-sync:session",
            Self::Web => "conversation-sync:web",
        }
    }

    fn conflict_keys(self) -> Vec<String> {
        match self {
            Self::All => vec![
                "conversation-sync:session".to_string(),
                "conversation-sync:web".to_string(),
            ],
            Self::Session => vec!["conversation-sync:session".to_string()],
            Self::Web => vec!["conversation-sync:web".to_string()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationScriptInstallTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) item_id: String,
    pub(crate) package_id: String,
    pub(crate) action: String,
    pub(crate) version: Option<String>,
    pub(crate) catalog_url: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) phase: Option<String>,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct SkillBackupTaskError {
    pub(crate) asset_id: Option<String>,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SkillBackupTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) asset_ids: Vec<String>,
    pub(crate) total_count: usize,
    pub(crate) completed_count: usize,
    pub(crate) failed_count: usize,
    pub(crate) current_asset_id: Option<String>,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    #[serde(default)]
    pub(crate) assets: Vec<CatalogAsset>,
    pub(crate) errors: Vec<SkillBackupTaskError>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct MemoryTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) kind: MemoryRunKind,
    pub(crate) scope: MemoryScope,
    pub(crate) scope_fingerprint: String,
    pub(crate) trigger: MemoryDreamTrigger,
    pub(crate) dry_run: bool,
    pub(crate) phase: String,
    pub(crate) processed_count: usize,
    pub(crate) total_count: usize,
    pub(crate) run_id: Option<String>,
    pub(crate) cancel_requested: bool,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentMarketRefreshTaskState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentMarketRefreshTaskSnapshot {
    pub(crate) id: String,
    pub(crate) state: AgentMarketRefreshTaskState,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<AgentMarketRefreshResult>,
    pub(crate) error: Option<String>,
}

pub(crate) struct BackgroundTaskRegistry {
    /// The backend TaskRuntime is the only mutable lifecycle authority. The
    /// Tauri layer stores no parallel task maps; its DTOs are serialized in
    /// `TaskSnapshot.detail` and projected on read.
    task_runtime: TaskRuntime,
    lifecycle: LifecycleTaskCoordinator,
}

impl Default for BackgroundTaskRegistry {
    fn default() -> Self {
        Self::with_task_runtime(TaskRuntime::new())
    }
}

impl BackgroundTaskRegistry {
    pub(crate) fn with_task_runtime(task_runtime: TaskRuntime) -> Self {
        Self {
            lifecycle: LifecycleTaskCoordinator::new(task_runtime.clone()),
            task_runtime,
        }
    }

    pub(crate) fn task_runtime(&self) -> Option<TaskRuntime> {
        Some(self.task_runtime.clone())
    }

    fn register_external_task(
        &self,
        kind: TaskKind,
        task_id: &str,
        dedup_key: Option<String>,
        conflict_keys: impl IntoIterator<Item = String>,
        detail: Value,
    ) -> AppResult<ExternalRegistrationOutcome> {
        let mut spec = TaskSpec::new(kind, dedup_key)
            .with_task_id(task_id.to_string())
            .with_conflict_keys(conflict_keys);
        spec.detail = detail;
        match self.task_runtime.register_external(spec)? {
            ExternalRegistrationOutcome::Started(_snapshot) => self
                .task_runtime
                .start_external(task_id)
                .map(ExternalRegistrationOutcome::Started),
            outcome @ ExternalRegistrationOutcome::Existing(_)
            | outcome @ ExternalRegistrationOutcome::Conflict(_) => Ok(outcome),
        }
    }

    fn register_projection<T: Serialize>(
        &self,
        kind: TaskKind,
        task_id: &str,
        dedup_key: Option<String>,
        conflict_keys: impl IntoIterator<Item = String>,
        projection: &T,
    ) -> AppResult<ExternalRegistrationOutcome> {
        let detail = serde_json::to_value(projection)
            .map_err(|error| crate::backend::runtime::AppError::External(error.to_string()))?;
        self.register_external_task(kind, task_id, dedup_key, conflict_keys, detail)
    }

    fn finish_external_task(
        &self,
        task_id: &str,
        result: Result<Value, String>,
    ) -> AppResult<TaskSnapshot> {
        self.task_runtime.complete_external(
            task_id,
            result.map_err(crate::backend::runtime::AppError::external),
        )
    }

    fn finish_external_result(
        &self,
        task_id: &str,
        result: crate::backend::runtime::AppResult<Value>,
    ) -> AppResult<TaskSnapshot> {
        self.task_runtime.complete_external(task_id, result)
    }

    fn external_task_snapshot(&self, task_id: &str) -> AppResult<TaskSnapshot> {
        self.task_runtime.get(task_id).ok_or_else(|| {
            crate::backend::runtime::AppError::NotFound(format!(
                "background task not found: {task_id}"
            ))
        })
    }

    fn decode<T: DeserializeOwned>(&self, runtime: &TaskSnapshot) -> AppResult<T> {
        serde_json::from_value(runtime.detail.clone()).map_err(|error| {
            crate::backend::runtime::AppError::External(format!(
                "task projection {} could not be decoded: {error}",
                runtime.task_id
            ))
        })
    }

    fn projection<T: BackgroundTaskProjection>(&self, task_id: &str) -> AppResult<T> {
        let runtime = self.external_task_snapshot(task_id)?;
        Ok(self.decode::<T>(&runtime)?.project_with_runtime(&runtime))
    }

    fn projection_from_runtime<T: BackgroundTaskProjection>(
        &self,
        runtime: &TaskSnapshot,
    ) -> AppResult<T> {
        Ok(self.decode::<T>(runtime)?.project_with_runtime(runtime))
    }

    fn write_projection<T: Serialize>(&self, task_id: &str, projection: &T) -> AppResult<()> {
        let detail = serde_json::to_value(projection)
            .map_err(|error| crate::backend::runtime::AppError::External(error.to_string()))?;
        self.task_runtime.update_detail(task_id, detail).map(|_| ())
    }

    fn list_projections<T: BackgroundTaskProjection>(&self, kind: TaskKind) -> Vec<T> {
        self.task_runtime
            .list(crate::backend::runtime::tasks::TaskFilter {
                kind: Some(kind),
                active_only: false,
            })
            .into_iter()
            .filter_map(|runtime| self.projection_from_runtime(&runtime).ok())
            .collect()
    }

    fn cancel_external_task(&self, task_id: &str) -> AppResult<TaskSnapshot> {
        match self.task_runtime.cancel(task_id) {
            crate::backend::runtime::tasks::CancelOutcome::Requested(snapshot)
            | crate::backend::runtime::tasks::CancelOutcome::AlreadyFinished(snapshot) => {
                Ok(snapshot)
            }
            crate::backend::runtime::tasks::CancelOutcome::NotFound => {
                Err(crate::backend::runtime::AppError::NotFound(format!(
                    "background task not found: {task_id}"
                )))
            }
        }
    }

    pub(crate) fn begin_source_scan(
        &self,
        tenant_id: &str,
        scope: SourceScanScope,
        kind: Option<crate::backend::models::AssetKind>,
    ) -> AppResult<(SourceScanTaskSnapshot, bool)> {
        let id = Uuid::new_v4().to_string();
        let snapshot = SourceScanTaskSnapshot {
            id: id.clone(),
            status: BackgroundTaskStatus::Running,
            scope,
            kind,
            progress: SourceScanTaskProgress {
                phase: SourceScanProgressPhase::Preparing,
                completed_source_count: 0,
                total_source_count: None,
                current_source_name: None,
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let dedup_key = format!(
            "scan:{tenant_id}:{}:{}",
            match scope {
                SourceScanScope::All => "all",
                SourceScanScope::Skills => "skills",
            },
            kind.map(|value| format!("{value:?}"))
                .unwrap_or_else(|| "all".to_string())
        );
        let registration = self.register_projection(
            TaskKind::Scan,
            &id,
            Some(dedup_key),
            [format!("catalog-write:{tenant_id}")],
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

    pub(crate) fn finish_source_scan(
        &self,
        task_id: &str,
        result: Result<crate::backend::application::SourceScanResult, String>,
    ) -> AppResult<SourceScanTaskSnapshot> {
        let runtime_result = result
            .as_ref()
            .map(|value| serde_json::to_value(&value.assets).unwrap_or(Value::Null))
            .map_err(|error| crate::backend::runtime::AppError::external(error.clone()));
        let runtime = self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: SourceScanTaskSnapshot = self.decode(&runtime)?;
        if let Ok(result) = result {
            snapshot.result = Some(result.assets);
        } else if runtime.state == TaskState::Canceled {
            snapshot.result = None;
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection_from_runtime(&self.external_task_snapshot(task_id)?)
    }

    pub(crate) fn source_scan_snapshot(&self, task_id: &str) -> AppResult<SourceScanTaskSnapshot> {
        self.projection(task_id)
    }

    pub(crate) fn source_scan_snapshots(&self) -> AppResult<Vec<SourceScanTaskSnapshot>> {
        let mut snapshots = self.list_projections::<SourceScanTaskSnapshot>(TaskKind::Scan);
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn cancel_source_scan(&self, task_id: &str) -> AppResult<SourceScanTaskSnapshot> {
        self.cancel_external_task(task_id)?;
        self.projection(task_id)
    }

    pub(crate) fn begin_batch_mount(
        &self,
        tenant_id: &str,
        mode: &str,
        profile_id: &str,
        dedup_suffix: &str,
    ) -> AppResult<(BatchMountTaskSnapshot, bool)> {
        let id = Uuid::new_v4().to_string();
        let snapshot = BatchMountTaskSnapshot {
            id: id.clone(),
            status: BackgroundTaskStatus::Running,
            mode: mode.to_string(),
            profile_id: profile_id.to_string(),
            progress: BatchMountTaskProgress {
                phase: "preparing".to_string(),
                completed: 0,
                total: None,
                current_id: None,
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let registration = self.register_projection(
            TaskKind::BatchMount,
            &id,
            Some(format!(
                "mount:{tenant_id}:{mode}:{profile_id}:{dedup_suffix}"
            )),
            [format!("mount-profile:{tenant_id}:{profile_id}")],
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(&runtime)?,
                matches!(&registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn update_batch_mount_progress(
        &self,
        task_id: &str,
        completed: u64,
        total: Option<u64>,
        current_id: Option<&str>,
    ) -> AppResult<BatchMountTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: BatchMountTaskSnapshot = self.decode(&runtime)?;
        if runtime.state.is_active() {
            snapshot.progress.completed = completed;
            snapshot.progress.total = total;
            snapshot.progress.current_id = current_id.map(str::to_string);
        }
        self.task_runtime
            .set_progress(task_id, completed, total, current_id)?;
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_batch_mount(
        &self,
        task_id: &str,
        result: Result<Value, String>,
    ) -> AppResult<BatchMountTaskSnapshot> {
        let runtime_result = result
            .clone()
            .map_err(crate::backend::runtime::AppError::external);
        let runtime = self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: BatchMountTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = result.ok();
        self.write_projection(task_id, &snapshot)?;
        self.projection_from_runtime(&self.external_task_snapshot(task_id)?)
    }

    pub(crate) fn batch_mount_snapshot(&self, task_id: &str) -> AppResult<BatchMountTaskSnapshot> {
        self.projection(task_id)
    }

    pub(crate) fn batch_mount_snapshots(&self) -> AppResult<Vec<BatchMountTaskSnapshot>> {
        let mut snapshots = self.list_projections::<BatchMountTaskSnapshot>(TaskKind::BatchMount);
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn cancel_batch_mount(&self, task_id: &str) -> AppResult<BatchMountTaskSnapshot> {
        self.cancel_external_task(task_id)?;
        self.projection(task_id)
    }

    pub(crate) fn spawn_extension_lifecycle(
        &self,
        task_id: &str,
        task: TaskFn,
    ) -> crate::backend::runtime::AppResult<TaskSnapshot> {
        let detail = self
            .task_runtime
            .get(task_id)
            .map(|snapshot| snapshot.detail)
            .unwrap_or(Value::Null);
        self.lifecycle.spawn(task_id, detail, task)
    }

    pub(crate) fn begin_agent_market_refresh(
        &self,
    ) -> AppResult<(AgentMarketRefreshTaskSnapshot, bool)> {
        let now = Utc::now().to_rfc3339();
        let snapshot = AgentMarketRefreshTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            state: AgentMarketRefreshTaskState::Running,
            created_at: now.clone(),
            updated_at: now,
            finished_at: None,
            result: None,
            error: None,
        };
        let registration = self.register_projection(
            TaskKind::Other,
            &snapshot.id,
            Some("agent-market-refresh".to_string()),
            Vec::new(),
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(&runtime)?,
                matches!(&registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn finish_agent_market_refresh(
        &self,
        task_id: &str,
        result: Result<AgentMarketRefreshResult, String>,
    ) -> AppResult<AgentMarketRefreshTaskSnapshot> {
        let runtime_result = result
            .as_ref()
            .map(|value| serde_json::to_value(value).unwrap_or(Value::Null))
            .map_err(|error| crate::backend::runtime::AppError::external(error.clone()));
        self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: AgentMarketRefreshTaskSnapshot =
            self.decode(&self.external_task_snapshot(task_id)?)?;
        snapshot.result = result.ok();
        snapshot.updated_at = Utc::now().to_rfc3339();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn agent_market_refresh_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<AgentMarketRefreshTaskSnapshot> {
        self.projection(task_id)
    }

    pub(crate) fn agent_market_refresh_snapshots(
        &self,
    ) -> AppResult<Vec<AgentMarketRefreshTaskSnapshot>> {
        let mut snapshots =
            self.list_projections::<AgentMarketRefreshTaskSnapshot>(TaskKind::Other);
        snapshots.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        Ok(snapshots)
    }

    pub(crate) fn begin_agent_lifecycle(
        &self,
        agent_id: String,
        action: String,
        catalog_version: Option<String>,
        agent_version: Option<String>,
        distribution_id: Option<String>,
        distribution_type: Option<crate::backend::agent_market::types::DistributionType>,
        ownership: Option<crate::backend::agent_market::types::Ownership>,
    ) -> AppResult<(
        AgentLifecycleTaskSnapshot,
        tokio_util::sync::CancellationToken,
        bool,
    )> {
        let lifecycle_key = extension_lifecycle_key(
            PackageKind::Agent,
            &agent_id,
            agent_version.as_deref(),
            &action,
        )?;
        let now = Utc::now().to_rfc3339();
        let snapshot = AgentLifecycleTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            agent_id,
            action,
            state: LifecycleTaskState::Queued,
            phase: LifecycleTaskPhase::Queued,
            catalog_version,
            agent_version,
            distribution_id,
            distribution_type,
            ownership,
            progress: ProgressSnapshot {
                completed_units: 0,
                total_units: None,
                downloaded_bytes: None,
                total_bytes: None,
            },
            cancellable: true,
            created_at: now.clone(),
            updated_at: now,
            finished_at: None,
            result: None,
            error: None,
            warnings: Vec::new(),
        };
        match self.lifecycle.reserve(snapshot.id.clone(), lifecycle_key)? {
            LifecycleReservationOutcome::Existing(existing_id) => {
                let runtime = self.external_task_snapshot(&existing_id)?;
                let cancellation = self.task_runtime.cancellation_token(&existing_id)?;
                Ok((self.projection_from_runtime(&runtime)?, cancellation, false))
            }
            LifecycleReservationOutcome::Started => {
                self.write_projection(&snapshot.id, &snapshot)?;
                let cancellation = self.task_runtime.cancellation_token(&snapshot.id)?;
                Ok((self.projection(&snapshot.id)?, cancellation, true))
            }
        }
    }

    pub(crate) fn update_agent_lifecycle(
        &self,
        task_id: &str,
        phase: LifecycleTaskPhase,
        completed_units: u64,
        downloaded_bytes: Option<u64>,
        warnings: Vec<String>,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        if runtime.state == TaskState::Pending {
            self.task_runtime
                .activate_external(task_id, runtime.detail.clone())?;
        }
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: AgentLifecycleTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running && !snapshot.state.is_terminal() {
            snapshot.state = LifecycleTaskState::Running;
            snapshot.phase = phase;
            snapshot.progress.completed_units = completed_units;
            snapshot.progress.downloaded_bytes = downloaded_bytes;
            snapshot.warnings = warnings;
            snapshot.updated_at = Utc::now().to_rfc3339();
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_agent_lifecycle(
        &self,
        task_id: &str,
        result: Result<(Option<Value>, Vec<String>), AgentMarketError>,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        let runtime_result = result
            .as_ref()
            .map(|(value, _)| value.clone().unwrap_or(Value::Null))
            .map_err(|error| crate::backend::runtime::AppError::from(error.clone()));
        let runtime = self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: AgentLifecycleTaskSnapshot = self.decode(&runtime)?;
        snapshot.finished_at = runtime.finished_at.clone();
        snapshot.updated_at = Utc::now().to_rfc3339();
        snapshot.cancellable = false;
        match result {
            Ok((value, warnings)) if runtime.state == TaskState::Succeeded => {
                snapshot.result = value;
                snapshot.warnings = warnings;
            }
            Err(error) => snapshot.error = Some((&error).into()),
            Ok(_) => {
                snapshot.error = Some(
                    (&AgentMarketError::new("task_state", "扩展生命周期任务未进入终态", false))
                        .into(),
                )
            }
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn agent_lifecycle_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        self.projection(task_id)
    }

    pub(crate) fn agent_lifecycle_snapshots(&self) -> AppResult<Vec<AgentLifecycleTaskSnapshot>> {
        let mut snapshots =
            self.list_projections::<AgentLifecycleTaskSnapshot>(TaskKind::ExtensionLifecycle);
        snapshots.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        Ok(snapshots)
    }

    pub(crate) fn cancel_agent_lifecycle(
        &self,
        task_id: &str,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        self.lifecycle.cancel(task_id);
        self.projection(task_id)
    }

    pub(crate) fn begin_conversation_search_index_rebuild(
        &self,
    ) -> AppResult<(ConversationSearchIndexTaskSnapshot, bool)> {
        let snapshot = ConversationSearchIndexTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let detail = serde_json::to_value(&snapshot)
            .map_err(|error| crate::backend::runtime::AppError::External(error.to_string()))?;
        let mut spec = TaskSpec::new(
            TaskKind::SearchIndexRebuild,
            Some("conversation-search-index".to_string()),
        )
        .with_task_id(snapshot.id.clone());
        spec.detail = detail;
        let registration = self.task_runtime.register_external(spec)?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(&runtime)?,
                matches!(&registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn finish_conversation_search_index_rebuild(
        &self,
        task_id: &str,
        result: Result<Value, String>,
    ) -> AppResult<ConversationSearchIndexTaskSnapshot> {
        self.finish_external_task(task_id, result.clone())?;
        let mut snapshot: ConversationSearchIndexTaskSnapshot =
            self.decode(&self.external_task_snapshot(task_id)?)?;
        snapshot.result = result.ok();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn conversation_search_index_snapshot(
        &self,
    ) -> AppResult<Option<ConversationSearchIndexTaskSnapshot>> {
        Ok(self
            .list_projections::<ConversationSearchIndexTaskSnapshot>(TaskKind::SearchIndexRebuild)
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn begin_conversation_sync(
        &self,
        params: &ConversationSyncParams,
    ) -> AppResult<(ConversationSyncTaskSnapshot, bool)> {
        let scope = ConversationSyncScope::from_record_kind(params.record_kind.as_deref())?;
        let snapshot = ConversationSyncTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            source_id: params.source_id.clone(),
            adapter_id: params.adapter_id.clone(),
            record_kind: scope.record_kind().map(str::to_string),
            mode: params.mode,
            dry_run: params.dry_run,
            progress: ConversationSyncTaskProgress {
                phase: ConversationSyncProgressPhase::Preparing,
                completed_source_count: 0,
                total_source_count: 0,
                current_source_name: None,
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let registration = self.register_projection(
            TaskKind::ConversationSync,
            &snapshot.id,
            Some(scope.dedup_key().to_string()),
            scope.conflict_keys(),
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(&runtime)?,
                matches!(&registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn update_conversation_sync_progress(
        &self,
        task_id: &str,
        completed_source_count: usize,
        total_source_count: usize,
        current_source_name: Option<String>,
    ) -> AppResult<ConversationSyncTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: ConversationSyncTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running {
            snapshot.progress = ConversationSyncTaskProgress {
                phase: ConversationSyncProgressPhase::Syncing,
                completed_source_count: completed_source_count.min(total_source_count),
                total_source_count,
                current_source_name,
            };
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_conversation_sync(
        &self,
        task_id: &str,
        result: Result<Value, String>,
    ) -> AppResult<ConversationSyncTaskSnapshot> {
        self.finish_external_task(task_id, result.clone())?;
        let mut snapshot: ConversationSyncTaskSnapshot =
            self.decode(&self.external_task_snapshot(task_id)?)?;
        snapshot.result = result.ok();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn conversation_sync_snapshot(
        &self,
    ) -> AppResult<Option<ConversationSyncTaskSnapshot>> {
        Ok(self
            .list_projections::<ConversationSyncTaskSnapshot>(TaskKind::ConversationSync)
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn conversation_sync_snapshots(
        &self,
    ) -> AppResult<Vec<ConversationSyncTaskSnapshot>> {
        let mut snapshots =
            self.list_projections::<ConversationSyncTaskSnapshot>(TaskKind::ConversationSync);
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    fn begin_conversation_script_projection(
        &self,
        snapshot: &ConversationScriptInstallTaskSnapshot,
        key: LifecycleRequestKey,
    ) -> AppResult<(ConversationScriptInstallTaskSnapshot, bool)> {
        match self.lifecycle.reserve(snapshot.id.clone(), key)? {
            LifecycleReservationOutcome::Existing(existing_id) => {
                Ok((self.projection(&existing_id)?, false))
            }
            LifecycleReservationOutcome::Started => {
                self.write_projection(&snapshot.id, snapshot)?;
                Ok((self.projection(&snapshot.id)?, true))
            }
        }
    }

    pub(crate) fn begin_conversation_script_install(
        &self,
        params: &ConversationScriptInstallParams,
    ) -> AppResult<(ConversationScriptInstallTaskSnapshot, bool)> {
        let item_id = params.item_id.trim().to_string();
        if item_id.is_empty() {
            return Err(crate::backend::runtime::AppError::Validation(
                "conversation script install requires an item id".to_string(),
            ));
        }
        let snapshot = ConversationScriptInstallTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            item_id: item_id.clone(),
            package_id: item_id,
            action: "install".to_string(),
            version: None,
            catalog_url: params.catalog_url.clone(),
            dry_run: params.dry_run,
            phase: Some("installing".to_string()),
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        self.begin_conversation_script_projection(
            &snapshot,
            extension_lifecycle_key(
                PackageKind::ConversationAdapter,
                &snapshot.package_id,
                snapshot.version.as_deref(),
                "install",
            )?,
        )
    }

    pub(crate) fn begin_conversation_adapter_package_install(
        &self,
        params: &ConversationAdapterPackageInstallParams,
    ) -> AppResult<(ConversationScriptInstallTaskSnapshot, bool)> {
        self.begin_conversation_adapter_package_change(params, "install", "installing")
    }

    pub(crate) fn begin_conversation_adapter_package_update(
        &self,
        params: &ConversationAdapterPackageInstallParams,
    ) -> AppResult<(ConversationScriptInstallTaskSnapshot, bool)> {
        self.begin_conversation_adapter_package_change(params, "update", "updating")
    }

    fn begin_conversation_adapter_package_change(
        &self,
        params: &ConversationAdapterPackageInstallParams,
        action: &str,
        phase: &str,
    ) -> AppResult<(ConversationScriptInstallTaskSnapshot, bool)> {
        let package_id = params.package_id.trim().to_string();
        if package_id.is_empty() {
            return Err(crate::backend::runtime::AppError::Validation(
                "conversation adapter package install requires a package id".to_string(),
            ));
        }
        let snapshot = ConversationScriptInstallTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            item_id: package_id.clone(),
            package_id,
            action: action.to_string(),
            version: params.version.clone(),
            catalog_url: params.catalog_url.clone(),
            dry_run: params.dry_run,
            phase: Some(phase.to_string()),
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        self.begin_conversation_script_projection(
            &snapshot,
            extension_lifecycle_key(
                PackageKind::ConversationAdapter,
                &snapshot.package_id,
                snapshot.version.as_deref(),
                action,
            )?,
        )
    }

    pub(crate) fn begin_conversation_adapter_package_uninstall(
        &self,
        params: &ConversationAdapterPackageUninstallParams,
    ) -> AppResult<(ConversationScriptInstallTaskSnapshot, bool)> {
        let package_id = params.package_id.trim().to_string();
        if package_id.is_empty() {
            return Err(crate::backend::runtime::AppError::Validation(
                "conversation adapter package uninstall requires a package id".to_string(),
            ));
        }
        let snapshot = ConversationScriptInstallTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            item_id: package_id.clone(),
            package_id,
            action: "uninstall".to_string(),
            version: None,
            catalog_url: None,
            dry_run: params.dry_run,
            phase: Some("uninstalling".to_string()),
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        self.begin_conversation_script_projection(
            &snapshot,
            extension_lifecycle_key(
                PackageKind::ConversationAdapter,
                &snapshot.package_id,
                None,
                "uninstall",
            )?,
        )
    }

    pub(crate) fn finish_conversation_script_install(
        &self,
        task_id: &str,
        result: Result<Value, String>,
    ) -> AppResult<ConversationScriptInstallTaskSnapshot> {
        self.finish_external_task(task_id, result.clone())?;
        let mut snapshot: ConversationScriptInstallTaskSnapshot =
            self.decode(&self.external_task_snapshot(task_id)?)?;
        snapshot.result = result.ok();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn conversation_script_install_snapshot(
        &self,
    ) -> AppResult<Option<ConversationScriptInstallTaskSnapshot>> {
        Ok(self
            .list_projections::<ConversationScriptInstallTaskSnapshot>(TaskKind::ExtensionLifecycle)
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn begin_skill_backup(
        &self,
        asset_ids: Vec<String>,
    ) -> AppResult<(SkillBackupTaskSnapshot, bool)> {
        let asset_ids = dedupe_non_empty(asset_ids);
        if asset_ids.is_empty() {
            return Err(crate::backend::runtime::AppError::Validation(
                "skill backup requires at least one asset id".to_string(),
            ));
        }
        let snapshot = SkillBackupTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            total_count: asset_ids.len(),
            completed_count: 0,
            failed_count: 0,
            current_asset_id: asset_ids.first().cloned(),
            asset_ids,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            assets: Vec::new(),
            errors: Vec::new(),
            error: None,
        };
        let registration = self.register_projection(
            TaskKind::Backup,
            &snapshot.id,
            Some("skill-backup".to_string()),
            Vec::new(),
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(&runtime)?,
                matches!(&registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn update_skill_backup_progress(
        &self,
        task_id: &str,
        completed_count: usize,
        current_asset_id: Option<String>,
    ) -> AppResult<SkillBackupTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: SkillBackupTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running {
            snapshot.completed_count = completed_count.min(snapshot.total_count);
            snapshot.current_asset_id = current_asset_id;
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_skill_backup(
        &self,
        task_id: &str,
        result: Result<Vec<CatalogAsset>, String>,
    ) -> AppResult<SkillBackupTaskSnapshot> {
        let runtime_result = result
            .as_ref()
            .map(|assets| serde_json::to_value(assets).unwrap_or(Value::Null))
            .map_err(|error| crate::backend::runtime::AppError::external(error.clone()));
        self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: SkillBackupTaskSnapshot =
            self.decode(&self.external_task_snapshot(task_id)?)?;
        match result {
            Ok(assets) => snapshot.assets = assets,
            Err(error) => snapshot.errors.push(SkillBackupTaskError {
                asset_id: snapshot.current_asset_id.clone(),
                message: error,
            }),
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn skill_backup_snapshot(&self) -> AppResult<Option<SkillBackupTaskSnapshot>> {
        Ok(self
            .list_projections::<SkillBackupTaskSnapshot>(TaskKind::Backup)
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn begin_memory_task(
        &self,
        params: &MemoryTaskStartParams,
    ) -> AppResult<(MemoryTaskSnapshot, AiExecutionCancellation, bool)> {
        let scope_fingerprint = params
            .scope
            .fingerprint()
            .map_err(crate::backend::runtime::AppError::external)?;
        let id = Uuid::new_v4().to_string();
        let snapshot = MemoryTaskSnapshot {
            id: id.clone(),
            status: BackgroundTaskStatus::Running,
            kind: params.kind,
            scope: params.scope.clone(),
            scope_fingerprint: scope_fingerprint.clone(),
            trigger: params.trigger,
            dry_run: params.dry_run,
            phase: "queued".to_string(),
            processed_count: 0,
            total_count: 0,
            run_id: None,
            cancel_requested: false,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let registration = self.register_projection(
            TaskKind::Other,
            &id,
            Some(format!("memory:{:?}:{scope_fingerprint}", params.kind)),
            vec![format!("memory-scope:{scope_fingerprint}")],
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Conflict(runtime) => {
                let existing: MemoryTaskSnapshot = self.decode(&runtime)?;
                Err(crate::backend::runtime::AppError::Conflict(format!(
                    "Memory scope is already running task {} ({:?})",
                    runtime.task_id, existing.kind
                )))
            }
            ExternalRegistrationOutcome::Started(runtime) => {
                let cancellation = AiExecutionCancellation::from_token(
                    self.task_runtime.cancellation_token(&runtime.task_id)?,
                );
                Ok((self.projection_from_runtime(&runtime)?, cancellation, true))
            }
            ExternalRegistrationOutcome::Existing(runtime) => {
                let cancellation = AiExecutionCancellation::from_token(
                    self.task_runtime.cancellation_token(&runtime.task_id)?,
                );
                Ok((self.projection_from_runtime(&runtime)?, cancellation, false))
            }
        }
    }

    pub(crate) fn update_memory_task(
        &self,
        task_id: &str,
        phase: &str,
        processed_count: usize,
        total_count: usize,
        run_id: Option<String>,
    ) -> AppResult<MemoryTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: MemoryTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running {
            snapshot.phase = phase.to_string();
            snapshot.processed_count = processed_count.min(total_count);
            snapshot.total_count = total_count;
            if run_id.is_some() {
                snapshot.run_id = run_id;
            }
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_memory_task(
        &self,
        task_id: &str,
        result: Result<Value, String>,
    ) -> AppResult<MemoryTaskSnapshot> {
        self.finish_external_task(task_id, result.clone())?;
        let mut snapshot: MemoryTaskSnapshot =
            self.decode(&self.external_task_snapshot(task_id)?)?;
        snapshot.result = result.ok();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn cancel_memory_task(&self, task_id: &str) -> AppResult<MemoryTaskSnapshot> {
        self.cancel_external_task(task_id)?;
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: MemoryTaskSnapshot = self.decode(&runtime)?;
        snapshot.cancel_requested = true;
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn memory_task_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<Option<MemoryTaskSnapshot>> {
        Ok(self.projection::<MemoryTaskSnapshot>(task_id).ok())
    }

    pub(crate) fn memory_task_snapshots(&self) -> AppResult<Vec<MemoryTaskSnapshot>> {
        let mut snapshots = self.list_projections::<MemoryTaskSnapshot>(TaskKind::Other);
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn begin_ai_execution(
        &self,
        purpose: AiExecutionPurpose,
        agent_id: &AgentId,
    ) -> AppResult<(AiExecutionTaskSnapshot, AiExecutionCancellation)> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let snapshot = AiExecutionTaskSnapshot {
            id: id.clone(),
            purpose,
            agent_id: agent_id.as_str().to_string(),
            state: AiExecutionTaskState::Queued,
            phase: AiExecutionPhase::Queued,
            created_at: now.clone(),
            updated_at: now,
            finished_at: None,
            result: None,
            error: None,
        };
        let runtime = match self.register_external_task(
            TaskKind::AiExecution,
            &id,
            None,
            Vec::new(),
            serde_json::to_value(&snapshot)
                .map_err(|error| crate::backend::runtime::AppError::External(error.to_string()))?,
        )? {
            ExternalRegistrationOutcome::Started(runtime) => runtime,
            ExternalRegistrationOutcome::Existing(runtime)
            | ExternalRegistrationOutcome::Conflict(runtime) => {
                return Err(crate::backend::runtime::AppError::Conflict(format!(
                    "AI execution task id was already registered: {}",
                    runtime.task_id
                )))
            }
        };
        let cancellation = AiExecutionCancellation::from_token(
            self.task_runtime.cancellation_token(&runtime.task_id)?,
        );
        Ok((self.projection_from_runtime(&runtime)?, cancellation))
    }

    pub(crate) fn update_ai_execution_phase(
        &self,
        task_id: &str,
        phase: AiExecutionPhase,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: AiExecutionTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running && !snapshot.state.is_terminal() {
            snapshot.state = if phase == AiExecutionPhase::Queued {
                AiExecutionTaskState::Queued
            } else {
                AiExecutionTaskState::Running
            };
            snapshot.phase = phase;
            snapshot.updated_at = Utc::now().to_rfc3339();
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_ai_execution(
        &self,
        task_id: &str,
        result: Result<AiExecutionResult, AiExecutionError>,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        let runtime_result = result
            .as_ref()
            .map(|value| serde_json::json!({"text": value.text}))
            .map_err(|error| {
                let view = error.to_view();
                crate::backend::runtime::AppError::Domain {
                    code: view.code,
                    message: view.message,
                    retryable: view.retryable,
                    details: view.phase.map(|phase| serde_json::json!({"phase": phase})),
                }
            });
        self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: AiExecutionTaskSnapshot =
            self.decode(&self.external_task_snapshot(task_id)?)?;
        match result {
            Ok(result) => {
                snapshot.result = Some(AiExecutionPublicResult { text: result.text });
            }
            Err(error) => {
                snapshot.phase = AiExecutionPhase::CleaningUp;
                let mut error_view = error.to_view();
                error_view.phase = Some(AiExecutionPhase::CleaningUp);
                snapshot.error = Some(error_view);
            }
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn cancel_ai_execution(&self, task_id: &str) -> AppResult<AiExecutionTaskSnapshot> {
        self.cancel_external_task(task_id)?;
        self.projection(task_id)
    }

    pub(crate) fn ai_execution_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<Option<AiExecutionTaskSnapshot>> {
        Ok(self.projection::<AiExecutionTaskSnapshot>(task_id).ok())
    }

    pub(crate) fn ai_execution_snapshots(&self) -> AppResult<Vec<AiExecutionTaskSnapshot>> {
        let mut snapshots = self.list_projections::<AiExecutionTaskSnapshot>(TaskKind::AiExecution);
        snapshots.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(snapshots)
    }

    pub(crate) fn cancel_all_ai_executions(&self) -> AppResult<Vec<AiExecutionTaskSnapshot>> {
        let task_ids = self
            .task_runtime
            .list(crate::backend::runtime::tasks::TaskFilter {
                kind: Some(TaskKind::AiExecution),
                active_only: true,
            })
            .into_iter()
            .map(|snapshot| snapshot.task_id)
            .collect::<Vec<_>>();
        let mut cancelled = Vec::new();
        for task_id in task_ids {
            self.cancel_external_task(&task_id)?;
            if let Ok(snapshot) = self.projection::<AiExecutionTaskSnapshot>(&task_id) {
                cancelled.push(snapshot);
            }
        }
        cancelled.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(cancelled)
    }

    pub(crate) async fn cancel_ai_executions_and_wait(
        &self,
        timeout: Duration,
        poll_interval: Duration,
    ) -> AppResult<AiExecutionShutdownReport> {
        let cancelled_count = self.cancel_all_ai_executions()?.len();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining_count = self.active_ai_execution_count()?;
            if remaining_count == 0 {
                return Ok(AiExecutionShutdownReport {
                    cancelled_count,
                    remaining_count,
                    converged: true,
                });
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Ok(AiExecutionShutdownReport {
                    cancelled_count,
                    remaining_count,
                    converged: false,
                });
            }
            let next_poll = now + poll_interval.max(Duration::from_millis(1));
            tokio::time::sleep_until(next_poll.min(deadline)).await;
        }
    }

    fn active_ai_execution_count(&self) -> AppResult<usize> {
        Ok(self
            .task_runtime
            .list(crate::backend::runtime::tasks::TaskFilter {
                kind: Some(TaskKind::AiExecution),
                active_only: true,
            })
            .len())
    }

    pub(crate) fn has_running_tasks(&self) -> bool {
        self.task_runtime.has_active_tasks()
    }
}

fn runtime_error_message(snapshot: &TaskSnapshot) -> Option<String> {
    snapshot.error.as_ref().map(|error| error.message.clone())
}

fn background_task_status(state: TaskState) -> BackgroundTaskStatus {
    match state {
        TaskState::Pending | TaskState::Running => BackgroundTaskStatus::Running,
        TaskState::Cancelling => BackgroundTaskStatus::Cancelling,
        TaskState::Succeeded => BackgroundTaskStatus::Completed,
        TaskState::Failed => BackgroundTaskStatus::Failed,
        TaskState::Canceled => BackgroundTaskStatus::Cancelled,
    }
}

trait BackgroundTaskProjection: DeserializeOwned {
    fn project_with_runtime(self, runtime: &TaskSnapshot) -> Self;
}

macro_rules! impl_basic_projection {
    ($ty:ty) => {
        impl BackgroundTaskProjection for $ty {
            fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
                self.status = background_task_status(runtime.state);
                self.finished_at = runtime.finished_at.clone();
                if runtime.state == TaskState::Canceled {
                    self.result = None;
                    self.error = runtime_error_message(runtime);
                } else if runtime.state == TaskState::Failed && self.error.is_none() {
                    self.error = runtime_error_message(runtime);
                }
                if runtime.state == TaskState::Succeeded {
                    self.result = runtime.result.clone();
                    self.error = None;
                }
                self
            }
        }
    };
}

impl_basic_projection!(ConversationSyncTaskSnapshot);
impl_basic_projection!(ConversationSearchIndexTaskSnapshot);
impl_basic_projection!(ConversationScriptInstallTaskSnapshot);
impl_basic_projection!(BatchMountTaskSnapshot);
impl_basic_projection!(MemoryTaskSnapshot);

impl BackgroundTaskProjection for SourceScanTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        self.status = background_task_status(runtime.state);
        self.finished_at = runtime.finished_at.clone();
        if let Some(progress) = &runtime.progress {
            self.progress.completed_source_count = progress.current;
            self.progress.total_source_count = progress.total;
            self.progress.current_source_name = progress.note.clone();
        }
        match runtime.state {
            TaskState::Running | TaskState::Pending => {
                self.progress.phase = SourceScanProgressPhase::Scanning;
            }
            TaskState::Cancelling => {}
            TaskState::Succeeded => {
                self.progress.phase = SourceScanProgressPhase::Completed;
                self.result = runtime
                    .result
                    .clone()
                    .and_then(|value| serde_json::from_value(value).ok());
                self.error = None;
            }
            TaskState::Failed => {
                self.progress.phase = SourceScanProgressPhase::Failed;
                self.error = runtime_error_message(runtime);
            }
            TaskState::Canceled => {
                self.progress.phase = SourceScanProgressPhase::Cancelled;
                self.result = None;
                self.error = runtime_error_message(runtime);
            }
        }
        self
    }
}

impl BackgroundTaskProjection for SkillBackupTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        self.status = background_task_status(runtime.state);
        self.finished_at = runtime.finished_at.clone();
        match runtime.state {
            TaskState::Succeeded => {
                self.completed_count = self.total_count;
                self.failed_count = 0;
                self.assets = runtime
                    .result
                    .clone()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default();
                self.errors.clear();
                self.error = None;
            }
            TaskState::Failed => {
                self.failed_count = 1;
                self.error = runtime_error_message(runtime);
            }
            TaskState::Canceled => {
                self.failed_count = 0;
                self.assets.clear();
                self.errors.clear();
                self.error = runtime_error_message(runtime);
            }
            TaskState::Pending | TaskState::Running | TaskState::Cancelling => {}
        }
        self
    }
}

impl BackgroundTaskProjection for AgentMarketRefreshTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        self.state = match runtime.state {
            TaskState::Pending | TaskState::Running | TaskState::Cancelling => {
                AgentMarketRefreshTaskState::Running
            }
            TaskState::Succeeded => AgentMarketRefreshTaskState::Succeeded,
            TaskState::Failed => AgentMarketRefreshTaskState::Failed,
            TaskState::Canceled => AgentMarketRefreshTaskState::Cancelled,
        };
        self.finished_at = runtime.finished_at.clone();
        if runtime.state == TaskState::Succeeded {
            self.result = runtime
                .result
                .clone()
                .and_then(|value| serde_json::from_value(value).ok());
            self.error = None;
        } else if runtime.state == TaskState::Failed || runtime.state == TaskState::Canceled {
            self.result = None;
            self.error = runtime_error_message(runtime);
        }
        self
    }
}

impl BackgroundTaskProjection for AgentLifecycleTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        match runtime.state {
            TaskState::Pending => {
                self.state = LifecycleTaskState::Queued;
                self.phase = LifecycleTaskPhase::Queued;
                self.cancellable = true;
            }
            TaskState::Running => {
                if !self.state.is_terminal() {
                    self.state = LifecycleTaskState::Running;
                    self.cancellable = true;
                }
            }
            TaskState::Cancelling => {
                self.state = LifecycleTaskState::Cancelling;
                self.phase = LifecycleTaskPhase::Cancelling;
                self.cancellable = false;
            }
            TaskState::Succeeded => {
                self.state = LifecycleTaskState::Succeeded;
                self.phase = LifecycleTaskPhase::Succeeded;
                self.cancellable = false;
                self.result = runtime.result.clone();
                self.error = None;
            }
            TaskState::Failed => {
                self.state = LifecycleTaskState::Failed;
                self.phase = LifecycleTaskPhase::Failed;
                self.cancellable = false;
                if self.error.is_none() {
                    self.error = runtime.error.as_ref().map(|error| {
                        let mut market_error =
                            AgentMarketError::new(&error.code, &error.message, error.retryable);
                        market_error.details = error.details.clone();
                        (&market_error).into()
                    });
                }
            }
            TaskState::Canceled => {
                self.state = LifecycleTaskState::Cancelled;
                self.phase = LifecycleTaskPhase::Cancelled;
                self.cancellable = false;
                self.result = None;
                self.error = runtime.error.as_ref().map(|error| {
                    let mut market_error =
                        AgentMarketError::new(&error.code, &error.message, error.retryable);
                    market_error.details = error.details.clone();
                    (&market_error).into()
                });
            }
        }
        self.finished_at = runtime.finished_at.clone();
        self
    }
}

impl BackgroundTaskProjection for AiExecutionTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        match runtime.state {
            TaskState::Pending => self.state = AiExecutionTaskState::Queued,
            TaskState::Running => {
                if !self.state.is_terminal() {
                    self.state = if self.phase == AiExecutionPhase::Queued {
                        AiExecutionTaskState::Queued
                    } else {
                        AiExecutionTaskState::Running
                    };
                }
            }
            TaskState::Cancelling => {
                self.state = AiExecutionTaskState::Running;
                self.phase = AiExecutionPhase::Cancelling;
            }
            TaskState::Succeeded => {
                self.state = AiExecutionTaskState::Succeeded;
                self.result = runtime.result.clone().and_then(|value| {
                    value
                        .get("text")
                        .and_then(Value::as_str)
                        .map(|text| AiExecutionPublicResult {
                            text: text.to_string(),
                        })
                });
                self.error = None;
            }
            TaskState::Failed => {
                self.state = AiExecutionTaskState::Failed;
                if self.error.is_none() {
                    self.error = runtime.error.as_ref().map(|error| AiExecutionErrorView {
                        code: error.code.clone(),
                        message: error.message.clone(),
                        retryable: error.retryable,
                        phase: Some(self.phase),
                    });
                }
            }
            TaskState::Canceled => {
                self.state = AiExecutionTaskState::Cancelled;
                self.phase = AiExecutionPhase::CleaningUp;
                self.result = None;
                self.error = Some(AiExecutionErrorView {
                    code: "cancelled".to_string(),
                    message: runtime_error_message(runtime)
                        .unwrap_or_else(|| "AI execution task was cancelled".to_string()),
                    phase: Some(self.phase),
                    retryable: false,
                });
            }
        }
        self.finished_at = runtime.finished_at.clone();
        self
    }
}

fn extension_lifecycle_key(
    kind: PackageKind,
    package_id: &str,
    version: Option<&str>,
    action: &str,
) -> AppResult<LifecycleRequestKey> {
    let version = version.unwrap_or("0.0.0");
    let version = semver::Version::parse(version)
        .map_err(|error| crate::backend::runtime::AppError::Validation(error.to_string()))?;
    Ok(LifecycleRequestKey {
        resource: ResourceKey::new(PackageIdentity {
            kind,
            package_id: package_id.to_string(),
            version,
        }),
        operation: match action {
            "install" => LifecycleOp::Install,
            "update" => LifecycleOp::Upgrade,
            "reinstall" => LifecycleOp::Install,
            "uninstall" => LifecycleOp::Remove,
            "enable" => LifecycleOp::Enable,
            "disable" => LifecycleOp::Disable,
            "probe" => LifecycleOp::Probe,
            _ => {
                return Err(crate::backend::runtime::AppError::Validation(format!(
                    "unsupported lifecycle action: {action}"
                )))
            }
        },
    })
}

fn dedupe_non_empty(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(record_kind: Option<&str>) -> ConversationSyncParams {
        ConversationSyncParams {
            source_id: None,
            adapter_id: None,
            record_kind: record_kind.map(str::to_string),
            mode: ConversationSyncMode::Incremental,
            dry_run: false,
        }
    }

    #[test]
    fn duplicate_start_reuses_the_running_sync_task() {
        let registry = BackgroundTaskRegistry::default();

        let (first, should_start_first) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();
        let (second, should_start_second) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();

        assert!(should_start_first);
        assert!(!should_start_second);
        assert_eq!(first.id, second.id);
        assert!(registry.has_running_tasks());
    }

    #[test]
    fn source_scan_deduplicates_same_scope_and_projects_cancellation() {
        let registry = BackgroundTaskRegistry::default();
        let (first, first_started) = registry
            .begin_source_scan("tenant-a", SourceScanScope::All, None)
            .expect("start source scan");
        let (second, second_started) = registry
            .begin_source_scan("tenant-a", SourceScanScope::All, None)
            .expect("deduplicate source scan");

        assert!(first_started);
        assert!(!second_started);
        assert_eq!(first.id, second.id);
        let cancelling = registry
            .cancel_source_scan(&first.id)
            .expect("cancel source scan");
        assert_eq!(cancelling.status, BackgroundTaskStatus::Cancelling);
    }

    #[test]
    fn batch_mount_uses_profile_conflict_and_projects_terminal_result() {
        let registry = BackgroundTaskRegistry::default();
        let (first, first_started) = registry
            .begin_batch_mount("tenant-a", "group", "profile-a", "group-a:true")
            .expect("start batch mount");
        let (second, second_started) = registry
            .begin_batch_mount("tenant-a", "exclusive", "profile-a", "group-b")
            .expect("conflict batch mount");

        assert!(first_started);
        assert!(!second_started);
        assert_eq!(first.id, second.id);

        let finished = registry
            .finish_batch_mount(&first.id, Ok(serde_json::json!({ "updated_count": 1 })))
            .expect("finish batch mount");
        assert_eq!(finished.status, BackgroundTaskStatus::Completed);
        assert_eq!(finished.result.expect("result")["updated_count"], 1);
    }

    #[test]
    fn duplicate_search_index_rebuild_reuses_running_task() {
        let registry = BackgroundTaskRegistry::default();
        let (first, should_start_first) =
            registry.begin_conversation_search_index_rebuild().unwrap();
        let (second, should_start_second) =
            registry.begin_conversation_search_index_rebuild().unwrap();

        assert!(should_start_first);
        assert!(!should_start_second);
        assert_eq!(first.id, second.id);
        assert!(registry.has_running_tasks());

        let finished = registry
            .finish_conversation_search_index_rebuild(
                &first.id,
                Ok(serde_json::json!({ "document_count": 1 })),
            )
            .unwrap();
        assert_eq!(finished.status, BackgroundTaskStatus::Completed);
        assert!(!registry.has_running_tasks());
    }

    #[test]
    fn search_index_registration_leaves_worker_start_to_task_runtime() {
        let registry = BackgroundTaskRegistry::default();
        let (first, should_start) = registry
            .begin_conversation_search_index_rebuild()
            .expect("register search index task");
        assert!(should_start);

        let executions = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let executions_for_worker = executions.clone();
        registry
            .task_runtime()
            .expect("shared task runtime")
            .start_external_with(
                &first.id,
                Value::Null,
                Box::new(move |_| {
                    executions_for_worker.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(Value::Null)
                }),
            )
            .expect("start search index worker");

        for _ in 0..100 {
            if registry
                .task_runtime()
                .and_then(|runtime| runtime.get(&first.id))
                .is_some_and(|snapshot| snapshot.state.is_terminal())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let runtime = registry.task_runtime().expect("shared task runtime");
        assert_eq!(executions.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            runtime.get(&first.id).expect("completed task").state,
            TaskState::Succeeded
        );
    }

    #[test]
    fn production_registry_can_expose_the_shared_task_runtime() {
        let runtime = TaskRuntime::new();
        let registry = BackgroundTaskRegistry::with_task_runtime(runtime.clone());

        assert!(registry.task_runtime().is_some());
        runtime.shutdown_with_grace(Duration::ZERO);
    }

    #[test]
    fn conversation_and_agent_lifecycle_use_one_kernel_task_runtime() {
        let runtime = TaskRuntime::new();
        let registry = BackgroundTaskRegistry::with_task_runtime(runtime.clone());
        let (agent, _, agent_should_start) = registry
            .begin_agent_lifecycle(
                "shared-kernel-agent".to_string(),
                "install".to_string(),
                None,
                Some("1.0.0".to_string()),
                None,
                None,
                None,
            )
            .unwrap();
        let (adapter, adapter_should_start) = registry
            .begin_conversation_adapter_package_install(&ConversationAdapterPackageInstallParams {
                catalog_url: None,
                package_id: "shared-kernel-agent".to_string(),
                version: Some("1.0.0".to_string()),
                dry_run: false,
                yes: true,
            })
            .unwrap();

        assert!(agent_should_start);
        assert!(adapter_should_start);
        assert_ne!(agent.id, adapter.id);

        let (agent_release, agent_wait) = std::sync::mpsc::channel();
        let (adapter_release, adapter_wait) = std::sync::mpsc::channel();
        let agent_task = registry
            .spawn_extension_lifecycle(
                &agent.id,
                Box::new(move |_| {
                    agent_wait
                        .recv_timeout(Duration::from_secs(1))
                        .map_err(|error| crate::backend::runtime::AppError::external(error))?;
                    Ok(serde_json::json!({ "domain": "agent" }))
                }),
            )
            .unwrap();
        let adapter_task = registry
            .spawn_extension_lifecycle(
                &adapter.id,
                Box::new(move |_| {
                    adapter_wait
                        .recv_timeout(Duration::from_secs(1))
                        .map_err(|error| crate::backend::runtime::AppError::external(error))?;
                    Ok(serde_json::json!({ "domain": "conversation" }))
                }),
            )
            .unwrap();

        assert_eq!(
            agent_task.kind,
            crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle
        );
        assert_eq!(
            adapter_task.kind,
            crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle
        );
        assert_eq!(
            runtime
                .list(crate::backend::runtime::tasks::TaskFilter {
                    kind: Some(crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle),
                    active_only: true,
                })
                .len(),
            2
        );

        agent_release.send(()).unwrap();
        adapter_release.send(()).unwrap();
        for _ in 0..100 {
            if runtime
                .list(crate::backend::runtime::tasks::TaskFilter {
                    kind: Some(crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle),
                    active_only: true,
                })
                .is_empty()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(runtime
            .list(crate::backend::runtime::tasks::TaskFilter {
                kind: Some(crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle),
                active_only: true,
            })
            .is_empty());

        registry
            .finish_agent_lifecycle(&agent.id, Ok((None, Vec::new())))
            .unwrap();
        registry
            .finish_conversation_script_install(&adapter.id, Ok(serde_json::json!({})))
            .unwrap();
        runtime.shutdown_with_grace(Duration::ZERO);
    }

    #[test]
    fn session_and_web_sync_tasks_run_independently() {
        let registry = BackgroundTaskRegistry::default();

        let (session, should_start_session) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();
        let (web, should_start_web) = registry
            .begin_conversation_sync(&params(Some("web")))
            .unwrap();

        assert!(should_start_session);
        assert!(should_start_web);
        assert_ne!(session.id, web.id);

        registry
            .finish_conversation_sync(&session.id, Ok(serde_json::json!({ "results": [] })))
            .unwrap();
        assert!(registry.has_running_tasks());
    }

    #[test]
    fn full_sync_owns_the_all_record_scope_and_blocks_scoped_syncs() {
        let registry = BackgroundTaskRegistry::default();
        let mut full_params = params(None);
        full_params.mode = ConversationSyncMode::Full;

        let (full, should_start_full) = registry.begin_conversation_sync(&full_params).unwrap();
        let (session, should_start_session) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();

        assert!(should_start_full);
        assert!(!should_start_session);
        assert_eq!(session.id, full.id);
        assert_eq!(full.record_kind, None);
        assert_eq!(full.mode, ConversationSyncMode::Full);
    }

    #[test]
    fn conversation_sync_progress_tracks_completed_and_current_sources() {
        let registry = BackgroundTaskRegistry::default();
        let (running, _) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();

        let updated = registry
            .update_conversation_sync_progress(&running.id, 1, 3, Some("Gemini Web".to_string()))
            .unwrap();

        assert_eq!(updated.progress.completed_source_count, 1);
        assert_eq!(updated.progress.total_source_count, 3);
        assert_eq!(
            updated.progress.current_source_name.as_deref(),
            Some("Gemini Web")
        );
    }

    #[test]
    fn finishing_sync_records_success_or_failure() {
        let registry = BackgroundTaskRegistry::default();
        let (running, _) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();

        let completed = registry
            .finish_conversation_sync(&running.id, Ok(serde_json::json!({ "results": [] })))
            .unwrap();

        assert_eq!(completed.status, BackgroundTaskStatus::Completed);
        assert!(completed.result.is_some());
        assert!(!registry.has_running_tasks());

        let (running, _) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();
        let failed = registry
            .finish_conversation_sync(&running.id, Err("sync failed".to_string()))
            .unwrap();

        assert_eq!(failed.status, BackgroundTaskStatus::Failed);
        assert_eq!(failed.error.as_deref(), Some("sync failed"));
        assert!(!registry.has_running_tasks());
    }

    #[test]
    fn sync_projection_uses_task_runtime_cancellation_before_domain_result() {
        let registry = BackgroundTaskRegistry::default();
        let (running, _) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();
        let runtime = registry.task_runtime().expect("shared task runtime");
        assert!(matches!(
            runtime.cancel(&running.id),
            crate::backend::runtime::tasks::CancelOutcome::Requested(_)
        ));

        let finished = registry
            .finish_conversation_sync(&running.id, Ok(serde_json::json!({"results": []})))
            .unwrap();

        assert_eq!(finished.status, BackgroundTaskStatus::Cancelled);
        assert_eq!(
            runtime.get(&running.id).unwrap().state,
            crate::backend::runtime::tasks::TaskState::Canceled
        );
    }

    #[test]
    fn lifecycle_projection_does_not_resume_after_runtime_cancellation() {
        let registry = BackgroundTaskRegistry::default();
        let (running, _, _) = registry
            .begin_agent_lifecycle(
                "runtime-cancelled-agent".to_string(),
                "install".to_string(),
                None,
                Some("1.0.0".to_string()),
                None,
                None,
                None,
            )
            .unwrap();
        let runtime = registry.task_runtime().expect("shared task runtime");
        assert!(matches!(
            runtime.cancel(&running.id),
            crate::backend::runtime::tasks::CancelOutcome::Requested(_)
        ));

        let projected = registry
            .update_agent_lifecycle(
                &running.id,
                LifecycleTaskPhase::Downloading,
                7,
                Some(512),
                vec!["late update".to_string()],
            )
            .unwrap();

        assert_eq!(projected.state, LifecycleTaskState::Cancelling);
        assert_eq!(projected.phase, LifecycleTaskPhase::Cancelling);
        assert_eq!(projected.progress.completed_units, 0);
        assert!(projected.warnings.is_empty());
    }

    #[test]
    fn skill_backup_tracks_progress_and_blocks_duplicate_start() {
        let registry = BackgroundTaskRegistry::default();

        let (running, should_start) = registry
            .begin_skill_backup(vec![
                "skill-a".to_string(),
                "skill-a".to_string(),
                " ".to_string(),
                "skill-b".to_string(),
            ])
            .unwrap();
        let (duplicate, should_start_duplicate) = registry
            .begin_skill_backup(vec!["skill-c".to_string()])
            .unwrap();

        assert!(should_start);
        assert!(!should_start_duplicate);
        assert_eq!(running.id, duplicate.id);
        assert_eq!(running.asset_ids, vec!["skill-a", "skill-b"]);
        assert_eq!(running.total_count, 2);
        assert_eq!(running.current_asset_id.as_deref(), Some("skill-a"));
        assert!(registry.has_running_tasks());

        let progress = registry
            .update_skill_backup_progress(&running.id, 1, Some("skill-b".to_string()))
            .unwrap();
        assert_eq!(progress.completed_count, 1);
        assert_eq!(progress.current_asset_id.as_deref(), Some("skill-b"));

        let failed = registry
            .finish_skill_backup(&running.id, Err("copy failed".to_string()))
            .unwrap();
        assert_eq!(failed.status, BackgroundTaskStatus::Failed);
        assert_eq!(failed.failed_count, 1);
        assert_eq!(failed.errors[0].asset_id.as_deref(), Some("skill-b"));
        assert!(!registry.has_running_tasks());

        let completed_copy_registry = BackgroundTaskRegistry::default();
        let (running, _) = completed_copy_registry
            .begin_skill_backup(vec!["skill-a".to_string()])
            .unwrap();
        completed_copy_registry
            .update_skill_backup_progress(&running.id, 1, None)
            .unwrap();
        let refresh_failed = completed_copy_registry
            .finish_skill_backup(&running.id, Err("catalog refresh failed".to_string()))
            .unwrap();
        assert_eq!(refresh_failed.errors[0].asset_id, None);
    }

    #[test]
    fn conversation_script_install_blocks_duplicate_start_and_finishes() {
        let registry = BackgroundTaskRegistry::default();
        let params = ConversationScriptInstallParams {
            catalog_url: Some("https://example.test/catalog.json".to_string()),
            item_id: "codex-session".to_string(),
            dry_run: false,
            yes: true,
        };

        let (running, should_start) = registry
            .begin_conversation_script_install(&params)
            .expect("start install task");
        let (duplicate, should_start_duplicate) = registry
            .begin_conversation_script_install(&ConversationScriptInstallParams {
                item_id: "codex-session".to_string(),
                ..params
            })
            .expect("reuse running install task");

        assert!(should_start);
        assert!(!should_start_duplicate);
        assert_eq!(running.id, duplicate.id);
        assert_eq!(running.item_id, "codex-session");
        assert!(registry.has_running_tasks());

        let completed = registry
            .finish_conversation_script_install(
                &running.id,
                Ok(serde_json::json!({ "installed": true })),
            )
            .expect("finish install task");

        assert_eq!(completed.status, BackgroundTaskStatus::Completed);
        assert_eq!(
            completed.result,
            Some(serde_json::json!({ "installed": true }))
        );
        assert!(!registry.has_running_tasks());
    }

    #[test]
    fn conversation_package_uninstall_uses_the_shared_background_task_registry() {
        let registry = BackgroundTaskRegistry::default();
        let params = ConversationAdapterPackageUninstallParams {
            package_id: "io.github.util6.codex-session".to_string(),
            dry_run: false,
            yes: true,
        };

        let (running, should_start) = registry
            .begin_conversation_adapter_package_uninstall(&params)
            .expect("start uninstall task");

        assert!(should_start);
        assert_eq!(running.action, "uninstall");
        assert_eq!(running.phase.as_deref(), Some("uninstalling"));
        assert!(registry.has_running_tasks());
    }

    #[test]
    fn memory_task_deduplicates_scope_reports_progress_and_cancels() {
        let registry = BackgroundTaskRegistry::default();
        let params = MemoryTaskStartParams {
            kind: MemoryRunKind::AutoDream,
            scope: MemoryScope {
                project_path: Some("~/project".to_string()),
                ..MemoryScope::default()
            },
            trigger: MemoryDreamTrigger::Manual,
            dry_run: false,
            recall: None,
            synthesize: false,
        };
        let (first, cancellation, should_start) = registry
            .begin_memory_task(&params)
            .expect("begin Memory task");
        let (duplicate, _, should_start_duplicate) = registry
            .begin_memory_task(&params)
            .expect("deduplicate Memory task");

        assert!(should_start);
        assert!(!should_start_duplicate);
        assert_eq!(first.id, duplicate.id);
        let progress = registry
            .update_memory_task(&first.id, "dreaming", 1, 3, Some("run-1".to_string()))
            .expect("update Memory task");
        assert_eq!(progress.processed_count, 1);
        assert_eq!(progress.total_count, 3);
        assert_eq!(progress.run_id.as_deref(), Some("run-1"));

        let cancelling = registry
            .cancel_memory_task(&first.id)
            .expect("cancel Memory task");
        assert!(cancelling.cancel_requested);
        assert!(cancellation.is_cancelled());
        let cancelled = registry
            .finish_memory_task(&first.id, Err("cancelled".to_string()))
            .expect("finish cancelled Memory task");
        assert_eq!(cancelled.status, BackgroundTaskStatus::Cancelled);
        assert!(!registry.has_running_tasks());
    }

    #[test]
    fn task_01_02_begin_and_phase_update_preserve_safe_snapshot_state() {
        let registry = BackgroundTaskRegistry::default();
        let (queued, cancellation) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        std::thread::sleep(Duration::from_millis(1));

        let running = registry
            .update_ai_execution_phase(&queued.id, AiExecutionPhase::Initializing)
            .unwrap();

        assert_eq!(queued.state, AiExecutionTaskState::Queued);
        assert_eq!(queued.phase, AiExecutionPhase::Queued);
        assert!(!cancellation.is_cancelled());
        assert_eq!(running.state, AiExecutionTaskState::Running);
        assert_eq!(running.phase, AiExecutionPhase::Initializing);
        assert_ne!(running.updated_at, queued.updated_at);
        assert!(registry.has_running_tasks());
    }

    #[test]
    fn task_03_04_finish_records_public_result_or_stable_error() {
        let registry = BackgroundTaskRegistry::default();
        let (success, _) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        let completed = registry
            .finish_ai_execution(&success.id, Ok(ai_result("translated")))
            .unwrap();
        assert_eq!(completed.state, AiExecutionTaskState::Succeeded);
        assert_eq!(
            completed.result,
            Some(AiExecutionPublicResult {
                text: "translated".to_string()
            })
        );
        assert!(completed.finished_at.is_some());

        let (failure, _) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        let failed = registry
            .finish_ai_execution(
                &failure.id,
                Err(AiExecutionError::Protocol {
                    operation: "SECRET_OPERATION",
                }),
            )
            .unwrap();
        assert_eq!(failed.state, AiExecutionTaskState::Failed);
        assert_eq!(failed.error.as_ref().unwrap().code, "protocol_failed");
        assert_eq!(
            failed.error.as_ref().unwrap().phase,
            Some(AiExecutionPhase::CleaningUp)
        );
        assert!(!format!("{failed:?}").contains("SECRET_OPERATION"));
        assert!(!registry.has_running_tasks());
    }

    #[test]
    fn task_05_06_07_cancel_is_active_for_queued_and_running_and_idempotent_for_terminal() {
        let registry = BackgroundTaskRegistry::default();
        let (queued, queued_token) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        let cancelling = registry.cancel_ai_execution(&queued.id).unwrap();
        assert!(queued_token.is_cancelled());
        assert_eq!(cancelling.state, AiExecutionTaskState::Running);
        assert_eq!(cancelling.phase, AiExecutionPhase::Cancelling);
        let cancelled = registry
            .finish_ai_execution(&queued.id, Err(cancelled_error()))
            .unwrap();
        assert_eq!(cancelled.state, AiExecutionTaskState::Cancelled);
        assert_eq!(registry.cancel_ai_execution(&queued.id).unwrap(), cancelled);

        let (running, running_token) = registry
            .begin_ai_execution(AiExecutionPurpose::ConnectionTest, &opencode_id())
            .unwrap();
        registry
            .update_ai_execution_phase(&running.id, AiExecutionPhase::Prompting)
            .unwrap();
        registry.cancel_ai_execution(&running.id).unwrap();
        assert!(running_token.is_cancelled());
    }

    #[test]
    fn task_08_unknown_cancel_returns_not_found() {
        let registry = BackgroundTaskRegistry::default();

        let error = registry.cancel_ai_execution("missing").unwrap_err();

        assert!(error.to_string().contains("not found"));
        assert_eq!(registry.ai_execution_snapshot("missing").unwrap(), None);
    }

    #[test]
    fn task_09_list_snapshot_has_no_prompt_workspace_environment_or_stderr_fields() {
        let registry = BackgroundTaskRegistry::default();
        let _ = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();

        let serialized =
            serde_json::to_string(&registry.ai_execution_snapshots().unwrap()).unwrap();

        for forbidden in ["prompt", "workspace", "cwd", "environment", "stderr"] {
            assert!(!serialized.contains(forbidden), "leaked field: {forbidden}");
        }
    }

    #[test]
    fn task_runtime_result_is_authoritative_and_not_duplicated_in_projection_detail() {
        let registry = BackgroundTaskRegistry::default();
        let (task, _) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        let large_text = "large-result-".to_string() + &"x".repeat(4096);
        registry
            .finish_ai_execution(
                &task.id,
                Ok(AiExecutionResult {
                    text: large_text.clone(),
                    ..ai_result("unused")
                }),
            )
            .unwrap();

        let runtime = registry.task_runtime().unwrap();
        let runtime_snapshot = runtime.get(&task.id).expect("runtime snapshot");
        let detail = serde_json::to_string(&runtime_snapshot.detail).unwrap();
        assert!(!detail.contains(&large_text));
        assert_eq!(runtime_snapshot.result.unwrap()["text"], large_text);
        assert_eq!(
            registry
                .ai_execution_snapshot(&task.id)
                .unwrap()
                .unwrap()
                .result
                .unwrap()
                .text,
            large_text
        );
    }

    #[test]
    fn task_10_time_retention_prunes_expired_terminal_tasks() {
        let registry = BackgroundTaskRegistry::default();
        let (task, _) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        registry
            .finish_ai_execution(&task.id, Ok(ai_result("done")))
            .unwrap();
        registry
            .task_runtime()
            .unwrap()
            .set_finished_at_for_test(
                &task.id,
                (Utc::now()
                    - chrono::Duration::from_std(
                        crate::backend::runtime::tasks::TASK_TERMINAL_RETENTION,
                    )
                    .unwrap()
                    - chrono::Duration::seconds(1))
                .to_rfc3339(),
            )
            .unwrap();

        assert!(registry.ai_execution_snapshots().unwrap().is_empty());
    }

    #[test]
    fn task_11_12_count_retention_keeps_100_terminal_tasks_and_all_running_tasks() {
        let registry = BackgroundTaskRegistry::default();
        let (running, _) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        for index in 0..101 {
            let (task, _) = registry
                .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
                .unwrap();
            registry
                .finish_ai_execution(&task.id, Ok(ai_result(&format!("result-{index}"))))
                .unwrap();
        }

        let snapshots = registry.ai_execution_snapshots().unwrap();
        assert_eq!(
            snapshots
                .iter()
                .filter(|snapshot| snapshot.state.is_terminal())
                .count(),
            100
        );
        assert!(snapshots.iter().any(|snapshot| snapshot.id == running.id));
    }

    #[test]
    fn task_13_14_ai_tasks_count_as_running_and_cancel_all_sets_every_token() {
        let registry = BackgroundTaskRegistry::default();
        let (first, first_token) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        let (_second, second_token) = registry
            .begin_ai_execution(AiExecutionPurpose::ConnectionTest, &opencode_id())
            .unwrap();
        assert!(registry.has_running_tasks());

        let snapshots = registry.cancel_all_ai_executions().unwrap();

        assert_eq!(snapshots.len(), 2);
        assert!(first_token.is_cancelled());
        assert!(second_token.is_cancelled());
        assert_eq!(
            registry
                .ai_execution_snapshot(&first.id)
                .unwrap()
                .unwrap()
                .phase,
            AiExecutionPhase::Cancelling
        );
    }

    #[tokio::test]
    async fn tauri_07_08_app_close_cancels_all_ai_tasks_and_waits_for_cleanup() {
        let registry = std::sync::Arc::new(BackgroundTaskRegistry::default());
        let (task, token) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        let finisher = registry.clone();
        let task_id = task.id.clone();
        tokio::spawn(async move {
            token.cancelled().await;
            finisher
                .finish_ai_execution(&task_id, Err(cancelled_error()))
                .unwrap();
        });

        let report = registry
            .cancel_ai_executions_and_wait(
                std::time::Duration::from_secs(1),
                std::time::Duration::from_millis(5),
            )
            .await
            .unwrap();

        assert_eq!(report.cancelled_count, 1);
        assert_eq!(report.remaining_count, 0);
        assert!(report.converged);
        assert!(!registry.has_running_tasks());
    }

    #[tokio::test]
    async fn tauri_08_app_close_wait_is_bounded_and_reports_pending_cleanup() {
        let registry = BackgroundTaskRegistry::default();
        registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();

        let report = registry
            .cancel_ai_executions_and_wait(
                std::time::Duration::from_millis(25),
                std::time::Duration::from_millis(5),
            )
            .await
            .unwrap();

        assert_eq!(report.cancelled_count, 1);
        assert_eq!(report.remaining_count, 1);
        assert!(!report.converged);
    }

    #[test]
    fn agent_lifecycle_task_deduplicates_same_agent_and_finishes_with_stable_state() {
        let registry = BackgroundTaskRegistry::default();
        let (first, cancellation, should_start) = registry
            .begin_agent_lifecycle(
                "fixture-agent".to_string(),
                "install".to_string(),
                Some("catalog-v1".to_string()),
                Some("1.0.0".to_string()),
                Some("fixture-system".to_string()),
                None,
                None,
            )
            .unwrap();
        let (duplicate, duplicate_cancellation, duplicate_should_start) = registry
            .begin_agent_lifecycle(
                "fixture-agent".to_string(),
                "install".to_string(),
                Some("catalog-v1".to_string()),
                Some("1.0.0".to_string()),
                Some("fixture-system".to_string()),
                None,
                None,
            )
            .unwrap();

        assert!(should_start);
        assert!(!duplicate_should_start);
        assert_eq!(first.id, duplicate.id);
        let running = registry
            .update_agent_lifecycle(
                &first.id,
                LifecycleTaskPhase::Downloading,
                2,
                Some(128),
                vec!["fixture warning".to_string()],
            )
            .unwrap();
        assert_eq!(running.state, LifecycleTaskState::Running);
        assert_eq!(running.progress.completed_units, 2);
        assert_eq!(running.progress.downloaded_bytes, Some(128));

        let cancelled = registry.cancel_agent_lifecycle(&first.id).unwrap();
        assert_eq!(cancelled.state, LifecycleTaskState::Cancelling);
        assert_eq!(cancelled.phase, LifecycleTaskPhase::Cancelling);
        assert!(!cancelled.state.is_terminal());
        assert_eq!(
            registry
                .task_runtime()
                .unwrap()
                .get(&first.id)
                .unwrap()
                .state,
            TaskState::Cancelling
        );
        assert!(cancellation.is_cancelled());
        assert!(duplicate_cancellation.is_cancelled());
        let terminal = registry
            .finish_agent_lifecycle(&first.id, Ok((None, Vec::new())))
            .unwrap();
        assert_eq!(terminal.state, LifecycleTaskState::Cancelled);
        assert_eq!(terminal.phase, LifecycleTaskPhase::Cancelled);
        assert!(!terminal.cancellable);
        assert!(!registry.has_running_tasks());
    }

    #[test]
    fn lifecycle_projection_does_not_return_another_operation_snapshot() {
        let registry = BackgroundTaskRegistry::default();
        registry
            .begin_agent_lifecycle(
                "fixture-agent".to_string(),
                "install".to_string(),
                None,
                Some("1.0.0".to_string()),
                None,
                None,
                None,
            )
            .unwrap();

        let error = registry
            .begin_agent_lifecycle(
                "fixture-agent".to_string(),
                "update".to_string(),
                None,
                Some("2.0.0".to_string()),
                None,
                None,
                None,
            )
            .expect_err("a different active operation must not reuse the install task");
        assert!(error.to_string().contains("conflicts"));
    }

    #[test]
    fn agent_market_refresh_deduplicates_running_task_and_retains_terminal_snapshot() {
        let registry = BackgroundTaskRegistry::default();
        let (first, should_start) = registry.begin_agent_market_refresh().unwrap();
        let (duplicate, duplicate_should_start) = registry.begin_agent_market_refresh().unwrap();
        assert!(should_start);
        assert!(!duplicate_should_start);
        assert_eq!(first.id, duplicate.id);

        let result = AgentMarketRefreshResult {
            status: "updated".to_string(),
            catalog_version: "catalog-v1".to_string(),
            active_catalog_version: "catalog-v1".to_string(),
            downloaded_catalog_version: "catalog-v1".to_string(),
            item_count: 1,
            source: "bundled".to_string(),
            etag: None,
        };
        let finished = registry
            .finish_agent_market_refresh(&first.id, Ok(result.clone()))
            .unwrap();
        assert_eq!(finished.state, AgentMarketRefreshTaskState::Succeeded);
        assert_eq!(finished.result, Some(result));
        assert_eq!(registry.agent_market_refresh_snapshots().unwrap().len(), 1);
    }

    #[test]
    fn task_runtime_deletion_removes_running_authority_from_the_projection() {
        let registry = BackgroundTaskRegistry::default();
        let (task, _) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();
        let runtime = registry.task_runtime().expect("shared task runtime");

        assert!(registry.has_running_tasks());
        assert!(runtime.remove(&task.id).is_some());
        assert!(!registry.has_running_tasks());

        let (replacement, should_start) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();
        assert!(should_start);
        assert_ne!(replacement.id, task.id);
    }

    #[test]
    fn projection_getter_returns_not_found_after_runtime_deletion() {
        let registry = BackgroundTaskRegistry::default();
        let (task, _) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();
        let runtime = registry.task_runtime().expect("shared task runtime");

        assert!(runtime.remove(&task.id).is_some());
        assert!(registry.conversation_sync_snapshot().unwrap().is_none());
        assert!(registry.conversation_sync_snapshots().unwrap().is_empty());
        assert!(registry
            .update_conversation_sync_progress(&task.id, 1, 1, None)
            .is_err());
    }

    #[test]
    fn all_projection_lists_drop_orphan_running_entries() {
        let registry = BackgroundTaskRegistry::default();
        let (agent, _, _) = registry
            .begin_agent_lifecycle(
                "orphan-agent".to_string(),
                "install".to_string(),
                None,
                Some("1.0.0".to_string()),
                None,
                None,
                None,
            )
            .unwrap();
        let (market, _) = registry.begin_agent_market_refresh().unwrap();
        let (search, _) = registry.begin_conversation_search_index_rebuild().unwrap();
        let (sync, _) = registry
            .begin_conversation_sync(&params(Some("session")))
            .unwrap();
        let (script, _) = registry
            .begin_conversation_script_install(&ConversationScriptInstallParams {
                catalog_url: None,
                item_id: "orphan-script".to_string(),
                dry_run: false,
                yes: true,
            })
            .unwrap();
        let (backup, _) = registry
            .begin_skill_backup(vec!["orphan-skill".to_string()])
            .unwrap();
        let memory_params = MemoryTaskStartParams {
            kind: MemoryRunKind::AutoDream,
            scope: MemoryScope {
                project_path: Some("~/orphan".to_string()),
                ..MemoryScope::default()
            },
            trigger: MemoryDreamTrigger::Manual,
            dry_run: true,
            recall: None,
            synthesize: false,
        };
        let (memory, _, _) = registry.begin_memory_task(&memory_params).unwrap();
        let (ai, _) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        let runtime = registry.task_runtime().expect("shared task runtime");

        for task_id in [
            agent.id.as_str(),
            market.id.as_str(),
            search.id.as_str(),
            sync.id.as_str(),
            script.id.as_str(),
            backup.id.as_str(),
            memory.id.as_str(),
            ai.id.as_str(),
        ] {
            assert!(runtime.remove(task_id).is_some(), "remove {task_id}");
        }

        assert!(registry.agent_lifecycle_snapshots().unwrap().is_empty());
        assert!(registry
            .agent_market_refresh_snapshots()
            .unwrap()
            .is_empty());
        assert!(registry
            .conversation_search_index_snapshot()
            .unwrap()
            .is_none());
        assert!(registry.conversation_sync_snapshots().unwrap().is_empty());
        assert!(registry
            .conversation_script_install_snapshot()
            .unwrap()
            .is_none());
        assert!(registry.skill_backup_snapshot().unwrap().is_none());
        assert!(registry.memory_task_snapshots().unwrap().is_empty());
        assert!(registry.ai_execution_snapshots().unwrap().is_empty());
    }

    fn opencode_id() -> AgentId {
        AgentId::parse("opencode").unwrap()
    }

    fn ai_result(text: &str) -> AiExecutionResult {
        AiExecutionResult {
            text: text.to_string(),
            agent_id: opencode_id(),
            protocol: crate::backend::agents::types::AgentProtocol::Acp,
            requested_model: None,
            elapsed_ms: 1,
        }
    }

    fn cancelled_error() -> AiExecutionError {
        AiExecutionError::Cancelled {
            program: std::path::PathBuf::from("opencode"),
        }
    }
}
