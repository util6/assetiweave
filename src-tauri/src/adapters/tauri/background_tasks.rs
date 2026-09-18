//! Tauri 后台异步长任务管理与状态推送模块
//!
//! 支持会话同步、扫描索引、备份导入导出以及脚本安装卸载在内的异步后台任务注册、取消控制、状态快照与事件广播。

use crate::backend::{
    agent_market::types::{
        AgentLifecycleTaskSnapshot, AgentMarketError, LifecycleTaskPhase, LifecycleTaskState,
        ProgressSnapshot,
    },
    agents::types::AgentId,
    ai_execution::{
        AiExecutionCancellation, AiExecutionCleanupReport, AiExecutionError, AiExecutionErrorView,
        AiExecutionPhase, AiExecutionPurpose, AiExecutionResult,
    },
    application::{
        AgentMarketRefreshResult, ConversationAdapterPackageInstallParams,
        ConversationAdapterPackageUninstallParams, ConversationScriptInstallParams,
        ConversationSyncMode, ConversationSyncParams, SkillAcquireParams,
    },
    dto::CatalogAsset,
    extension_kernel::{
        LifecycleOp, LifecycleRequestKey, LifecycleReservationOutcome, LifecycleTaskCoordinator,
        PackageIdentity, PackageKind, ResourceKey,
    },
    runtime::tasks::{
        ExternalRegistrationOutcome, TaskFn, TaskKind, TaskRuntime, TaskSnapshot, TaskSpec,
        TaskState,
    },
    runtime::{AppErrorView, AppResult},
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
    pub(crate) cleanup: Option<AiExecutionCleanupReport>,
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
    pub(crate) error: Option<AppErrorView>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationUsageScanTaskProgress {
    pub(crate) phase: ConversationUsageScanProgressPhase,
    pub(crate) completed_source_count: usize,
    pub(crate) total_source_count: usize,
    pub(crate) current_source_name: Option<String>,
    pub(crate) total_events_ingested: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationUsageScanProgressPhase {
    Preparing,
    Scanning,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationUsageScanTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) source_id: Option<String>,
    pub(crate) mode: String,
    pub(crate) progress: ConversationUsageScanTaskProgress,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<AppErrorView>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationDataMaintenanceTaskProgress {
    pub(crate) phase: String,
    pub(crate) completed_stage: usize,
    pub(crate) total_stage: usize,
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationDataMaintenanceTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) operation: String,
    pub(crate) source_id: Option<String>,
    pub(crate) record_kind: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) progress: ConversationDataMaintenanceTaskProgress,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<AppErrorView>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct ConversationSearchIndexTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<AppErrorView>,
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
    pub(crate) error: Option<AppErrorView>,
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
    pub(crate) error: Option<AppErrorView>,
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
    pub(crate) error: Option<AppErrorView>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct SkillBackupTaskError {
    pub(crate) asset_id: Option<String>,
    pub(crate) error: AppErrorView,
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
    pub(crate) error: Option<AppErrorView>,
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
    pub(crate) error: Option<AppErrorView>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct RemoteSkillAcquireTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) url: String,
    pub(crate) branch: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) dry_run: bool,
    pub(crate) phase: String,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) result: Option<Value>,
    pub(crate) error: Option<AppErrorView>,
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

    #[allow(dead_code)]
    fn register_external_task(
        &self,
        kind: TaskKind,
        task_id: &str,
        dedup_key: Option<String>,
        conflict_keys: impl IntoIterator<Item = String>,
        detail: Value,
    ) -> AppResult<ExternalRegistrationOutcome> {
        self.register_external_task_for_tenant(
            None,
            kind,
            task_id,
            dedup_key,
            conflict_keys,
            detail,
        )
    }

    fn register_external_task_for_tenant(
        &self,
        tenant_id: Option<&str>,
        kind: TaskKind,
        task_id: &str,
        dedup_key: Option<String>,
        conflict_keys: impl IntoIterator<Item = String>,
        detail: Value,
    ) -> AppResult<ExternalRegistrationOutcome> {
        let mut spec = match tenant_id {
            Some(tenant_id) => TaskSpec::new(kind, dedup_key).with_tenant_id(tenant_id),
            None => TaskSpec::global(kind, dedup_key),
        }
        .with_task_id(task_id.to_string())
        .with_conflict_keys(conflict_keys);
        spec.detail = detail;
        match self.task_runtime.register_external(spec)? {
            ExternalRegistrationOutcome::Started(snapshot) => self
                .task_runtime
                .activate_external(task_id, snapshot.detail)
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
        self.register_projection_for_tenant(
            None,
            kind,
            task_id,
            dedup_key,
            conflict_keys,
            projection,
        )
    }

    fn register_projection_for_tenant<T: Serialize>(
        &self,
        tenant_id: Option<&str>,
        kind: TaskKind,
        task_id: &str,
        dedup_key: Option<String>,
        conflict_keys: impl IntoIterator<Item = String>,
        projection: &T,
    ) -> AppResult<ExternalRegistrationOutcome> {
        let detail = serde_json::to_value(projection)
            .map_err(|error| crate::backend::runtime::AppError::External(error.to_string()))?;
        self.register_external_task_for_tenant(
            tenant_id,
            kind,
            task_id,
            dedup_key,
            conflict_keys,
            detail,
        )
    }

    fn finish_external_task(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<TaskSnapshot> {
        self.task_runtime.complete_external(task_id, result)
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

    fn external_task_snapshot_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<TaskSnapshot> {
        self.task_runtime
            .get_for_tenant(tenant_id, task_id)
            .ok_or_else(|| {
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

    fn projection_for_tenant<T: BackgroundTaskProjection>(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<T> {
        let runtime = self.external_task_snapshot_for_tenant(tenant_id, task_id)?;
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

    fn list_projections<T: BackgroundTaskProjection>(&self, kind: TaskKind) -> AppResult<Vec<T>> {
        self.task_runtime
            .list(crate::backend::runtime::tasks::TaskFilter {
                kind: Some(kind),
                active_only: false,
                ..Default::default()
            })
            .into_iter()
            .map(|runtime| self.projection_from_runtime(&runtime))
            .collect()
    }

    fn list_projections_for_tenant<T: BackgroundTaskProjection>(
        &self,
        tenant_id: &str,
        kind: TaskKind,
    ) -> AppResult<Vec<T>> {
        self.task_runtime
            .list_for_tenant(
                tenant_id,
                crate::backend::runtime::tasks::TaskFilter {
                    kind: Some(kind),
                    active_only: false,
                    ..Default::default()
                },
            )
            .into_iter()
            .map(|runtime| self.projection_from_runtime(&runtime))
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

    fn cancel_external_task_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<TaskSnapshot> {
        match self.task_runtime.cancel_for_tenant(tenant_id, task_id) {
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
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
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
        result: AppResult<crate::backend::application::SourceScanResult>,
    ) -> AppResult<SourceScanTaskSnapshot> {
        let runtime_result = match result.as_ref() {
            Ok(value) => serde_json::to_value(&value.assets)
                .map_err(crate::backend::runtime::AppError::external),
            Err(error) => Err(crate::backend::runtime::AppError::from(error.view())),
        };
        let runtime = self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: SourceScanTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime
            .result
            .clone()
            .map(serde_json::from_value)
            .transpose()
            .map_err(crate::backend::runtime::AppError::external)?;
        self.write_projection(task_id, &snapshot)?;
        self.projection_from_runtime(&self.external_task_snapshot(task_id)?)
    }

    #[allow(dead_code)]
    pub(crate) fn source_scan_snapshot(&self, task_id: &str) -> AppResult<SourceScanTaskSnapshot> {
        self.projection(task_id)
    }

    pub(crate) fn source_scan_snapshot_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<SourceScanTaskSnapshot> {
        self.projection_for_tenant(tenant_id, task_id)
    }

    #[allow(dead_code)]
    pub(crate) fn source_scan_snapshots(&self) -> AppResult<Vec<SourceScanTaskSnapshot>> {
        let mut snapshots = self.list_projections::<SourceScanTaskSnapshot>(TaskKind::Scan)?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn source_scan_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<SourceScanTaskSnapshot>> {
        let mut snapshots =
            self.list_projections_for_tenant::<SourceScanTaskSnapshot>(tenant_id, TaskKind::Scan)?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    #[allow(dead_code)]
    pub(crate) fn cancel_source_scan(&self, task_id: &str) -> AppResult<SourceScanTaskSnapshot> {
        self.cancel_external_task(task_id)?;
        self.projection(task_id)
    }

    pub(crate) fn cancel_source_scan_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<SourceScanTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
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
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
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
        result: AppResult<Value>,
    ) -> AppResult<BatchMountTaskSnapshot> {
        let runtime = self.finish_external_result(task_id, result)?;
        let mut snapshot: BatchMountTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime.result.clone();
        self.write_projection(task_id, &snapshot)?;
        self.projection_from_runtime(&self.external_task_snapshot(task_id)?)
    }

    #[allow(dead_code)]
    pub(crate) fn batch_mount_snapshot(&self, task_id: &str) -> AppResult<BatchMountTaskSnapshot> {
        self.projection(task_id)
    }

    pub(crate) fn batch_mount_snapshot_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<BatchMountTaskSnapshot> {
        self.projection_for_tenant(tenant_id, task_id)
    }

    #[allow(dead_code)]
    pub(crate) fn batch_mount_snapshots(&self) -> AppResult<Vec<BatchMountTaskSnapshot>> {
        let mut snapshots =
            self.list_projections::<BatchMountTaskSnapshot>(TaskKind::BatchMount)?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn batch_mount_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<BatchMountTaskSnapshot>> {
        let mut snapshots = self.list_projections_for_tenant::<BatchMountTaskSnapshot>(
            tenant_id,
            TaskKind::BatchMount,
        )?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    #[allow(dead_code)]
    pub(crate) fn cancel_batch_mount(&self, task_id: &str) -> AppResult<BatchMountTaskSnapshot> {
        self.cancel_external_task(task_id)?;
        self.projection(task_id)
    }

    pub(crate) fn cancel_batch_mount_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<BatchMountTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
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
            TaskKind::AgentMarketRefresh,
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
        result: AppResult<AgentMarketRefreshResult>,
    ) -> AppResult<AgentMarketRefreshTaskSnapshot> {
        let runtime_result = match result.as_ref() {
            Ok(value) => {
                serde_json::to_value(value).map_err(crate::backend::runtime::AppError::external)
            }
            Err(error) => Err(crate::backend::runtime::AppError::from(error.view())),
        };
        let runtime = self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: AgentMarketRefreshTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime
            .result
            .clone()
            .map(serde_json::from_value)
            .transpose()
            .map_err(crate::backend::runtime::AppError::external)?;
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
            self.list_projections::<AgentMarketRefreshTaskSnapshot>(TaskKind::AgentMarketRefresh)?;
        snapshots.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        Ok(snapshots)
    }

    pub(crate) fn begin_remote_skill_acquire_for_tenant(
        &self,
        tenant_id: &str,
        params: &SkillAcquireParams,
    ) -> AppResult<(RemoteSkillAcquireTaskSnapshot, bool)> {
        let snapshot = RemoteSkillAcquireTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            url: params.url.clone(),
            branch: params.branch.clone(),
            path: params.path.clone(),
            name: params.name.clone(),
            dry_run: params.dry_run,
            phase: if params.dry_run {
                "preparing".to_string()
            } else {
                "queued".to_string()
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let identity = [
            params.url.trim(),
            params.branch.as_deref().unwrap_or_default(),
            params.path.as_deref().unwrap_or_default(),
            params.name.as_deref().unwrap_or_default(),
            if params.dry_run { "dry-run" } else { "import" },
        ]
        .join("|");
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::RemoteSkillAcquire,
            &snapshot.id,
            Some(format!("{tenant_id}:skill-acquire:{identity}")),
            [format!("skill-acquire:{tenant_id}")],
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(runtime)?,
                matches!(registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn update_remote_skill_acquire_phase(
        &self,
        task_id: &str,
        phase: impl Into<String>,
    ) -> AppResult<RemoteSkillAcquireTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: RemoteSkillAcquireTaskSnapshot = self.decode(&runtime)?;
        if runtime.state.is_active() {
            snapshot.phase = phase.into();
            self.write_projection(task_id, &snapshot)?;
        }
        self.projection(task_id)
    }

    pub(crate) fn finish_remote_skill_acquire(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<RemoteSkillAcquireTaskSnapshot> {
        let runtime = self.finish_external_result(task_id, result)?;
        let mut snapshot: RemoteSkillAcquireTaskSnapshot = self.decode(&runtime)?;
        match runtime.state {
            TaskState::Succeeded => {
                snapshot.phase = "completed".to_string();
                snapshot.result = runtime.result.clone();
                snapshot.error = None;
            }
            TaskState::Failed => {
                snapshot.phase = "failed".to_string();
                snapshot.result = None;
                snapshot.error = runtime.error.clone();
            }
            TaskState::Canceled => {
                snapshot.phase = "cancelled".to_string();
                snapshot.result = None;
                snapshot.error = runtime.error.clone();
            }
            TaskState::Pending | TaskState::Running | TaskState::Cancelling => {}
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn remote_skill_acquire_snapshot_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<RemoteSkillAcquireTaskSnapshot> {
        self.projection_for_tenant(tenant_id, task_id)
    }

    pub(crate) fn remote_skill_acquire_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<RemoteSkillAcquireTaskSnapshot>> {
        let mut snapshots = self.list_projections_for_tenant::<RemoteSkillAcquireTaskSnapshot>(
            tenant_id,
            TaskKind::RemoteSkillAcquire,
        )?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn cancel_remote_skill_acquire_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<RemoteSkillAcquireTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
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
        let (runtime_result, task_warnings, task_error) = match result {
            Ok((value, warnings)) => (Ok(value.unwrap_or(Value::Null)), warnings, None),
            Err(error) => {
                let view = (&error).into();
                (
                    Err(crate::backend::runtime::AppError::from(error)),
                    Vec::new(),
                    Some(view),
                )
            }
        };
        let runtime = self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: AgentLifecycleTaskSnapshot = self.decode(&runtime)?;
        snapshot.finished_at = runtime.finished_at.clone();
        snapshot.updated_at = Utc::now().to_rfc3339();
        snapshot.cancellable = false;
        if runtime.state == TaskState::Succeeded {
            snapshot.warnings = task_warnings;
        } else if let Some(error) = task_error {
            snapshot.error = Some(error);
        } else {
            snapshot.error = Some(
                (&AgentMarketError::new("task_state", "扩展生命周期任务未进入终态", false)).into(),
            );
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
            self.list_projections::<AgentLifecycleTaskSnapshot>(TaskKind::ExtensionLifecycle)?;
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

    pub(crate) fn begin_conversation_search_index_rebuild_for_tenant(
        &self,
        tenant_id: &str,
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
            Some(format!("{tenant_id}:conversation-search-index")),
        )
        .with_task_id(snapshot.id.clone())
        .with_tenant_id(tenant_id);
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
        result: AppResult<Value>,
    ) -> AppResult<ConversationSearchIndexTaskSnapshot> {
        let runtime = self.finish_external_task(task_id, result)?;
        let mut snapshot: ConversationSearchIndexTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime.result.clone();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    #[allow(dead_code)]
    pub(crate) fn conversation_search_index_snapshot(
        &self,
    ) -> AppResult<Option<ConversationSearchIndexTaskSnapshot>> {
        Ok(self
            .list_projections::<ConversationSearchIndexTaskSnapshot>(TaskKind::SearchIndexRebuild)?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn conversation_search_index_snapshot_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Option<ConversationSearchIndexTaskSnapshot>> {
        Ok(self
            .list_projections_for_tenant::<ConversationSearchIndexTaskSnapshot>(
                tenant_id,
                TaskKind::SearchIndexRebuild,
            )?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn begin_conversation_sync_for_tenant(
        &self,
        tenant_id: &str,
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
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::ConversationSync,
            &snapshot.id,
            Some(format!("{tenant_id}:{}", scope.dedup_key())),
            scope
                .conflict_keys()
                .into_iter()
                .map(|key| format!("{tenant_id}:{key}")),
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
        result: AppResult<Value>,
    ) -> AppResult<ConversationSyncTaskSnapshot> {
        let runtime = self.finish_external_task(task_id, result)?;
        let mut snapshot: ConversationSyncTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime.result.clone();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    #[allow(dead_code)]
    pub(crate) fn conversation_sync_snapshot(
        &self,
    ) -> AppResult<Option<ConversationSyncTaskSnapshot>> {
        Ok(self
            .list_projections::<ConversationSyncTaskSnapshot>(TaskKind::ConversationSync)?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn conversation_sync_snapshot_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Option<ConversationSyncTaskSnapshot>> {
        Ok(self
            .list_projections_for_tenant::<ConversationSyncTaskSnapshot>(
                tenant_id,
                TaskKind::ConversationSync,
            )?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    #[allow(dead_code)]
    pub(crate) fn conversation_sync_snapshots(
        &self,
    ) -> AppResult<Vec<ConversationSyncTaskSnapshot>> {
        let mut snapshots =
            self.list_projections::<ConversationSyncTaskSnapshot>(TaskKind::ConversationSync)?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn conversation_sync_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<ConversationSyncTaskSnapshot>> {
        let mut snapshots = self.list_projections_for_tenant::<ConversationSyncTaskSnapshot>(
            tenant_id,
            TaskKind::ConversationSync,
        )?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn cancel_conversation_sync_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<ConversationSyncTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
    }

    pub(crate) fn begin_conversation_usage_scan_for_tenant(
        &self,
        tenant_id: &str,
        source_id: Option<String>,
        mode: &str,
    ) -> AppResult<(ConversationUsageScanTaskSnapshot, bool)> {
        let snapshot = ConversationUsageScanTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            source_id: source_id.clone(),
            mode: mode.to_string(),
            progress: ConversationUsageScanTaskProgress {
                phase: ConversationUsageScanProgressPhase::Preparing,
                completed_source_count: 0,
                total_source_count: 0,
                current_source_name: None,
                total_events_ingested: 0,
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let dedup_key = format!(
            "{tenant_id}:conversation_usage_scan:{}",
            source_id.as_deref().unwrap_or("all")
        );
        let conflict_key = format!("{tenant_id}:conversation_usage_scan");
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::ConversationUsageScan,
            &snapshot.id,
            Some(dedup_key),
            std::iter::once(conflict_key),
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(runtime)?,
                matches!(&registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn update_conversation_usage_scan_progress(
        &self,
        task_id: &str,
        completed_source_count: usize,
        total_source_count: usize,
        current_source_name: Option<String>,
        total_events_ingested: usize,
    ) -> AppResult<ConversationUsageScanTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: ConversationUsageScanTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running {
            snapshot.progress = ConversationUsageScanTaskProgress {
                phase: ConversationUsageScanProgressPhase::Scanning,
                completed_source_count: completed_source_count.min(total_source_count),
                total_source_count,
                current_source_name,
                total_events_ingested,
            };
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_conversation_usage_scan(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<ConversationUsageScanTaskSnapshot> {
        let runtime = self.finish_external_task(task_id, result)?;
        let mut snapshot: ConversationUsageScanTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime.result.clone();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn begin_conversation_data_maintenance_for_tenant(
        &self,
        tenant_id: &str,
        operation: &str,
        source_id: Option<String>,
        record_kind: Option<String>,
        dry_run: bool,
    ) -> AppResult<(ConversationDataMaintenanceTaskSnapshot, bool)> {
        let snapshot = ConversationDataMaintenanceTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            operation: operation.to_string(),
            source_id,
            record_kind,
            dry_run,
            progress: ConversationDataMaintenanceTaskProgress {
                phase: "preparing".to_string(),
                completed_stage: 0,
                total_stage: 10,
                note: None,
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::ConversationDataMaintenance,
            &snapshot.id,
            Some(format!("{tenant_id}:conversation-data-maintenance")),
            [format!("{tenant_id}:conversation-data-maintenance")],
            &snapshot,
        )?;
        match registration {
            ExternalRegistrationOutcome::Started(ref runtime)
            | ExternalRegistrationOutcome::Existing(ref runtime)
            | ExternalRegistrationOutcome::Conflict(ref runtime) => Ok((
                self.projection_from_runtime(runtime)?,
                matches!(&registration, ExternalRegistrationOutcome::Started(_)),
            )),
        }
    }

    pub(crate) fn update_conversation_data_maintenance_progress(
        &self,
        task_id: &str,
        completed_stage: usize,
        total_stage: usize,
        note: Option<String>,
    ) -> AppResult<ConversationDataMaintenanceTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: ConversationDataMaintenanceTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running {
            snapshot.progress = ConversationDataMaintenanceTaskProgress {
                phase: note.clone().unwrap_or_else(|| "running".to_string()),
                completed_stage: completed_stage.min(total_stage),
                total_stage,
                note,
            };
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn finish_conversation_data_maintenance(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<ConversationDataMaintenanceTaskSnapshot> {
        let runtime = self.finish_external_task(task_id, result)?;
        let mut snapshot: ConversationDataMaintenanceTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime.result.clone();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn conversation_data_maintenance_snapshot_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Option<ConversationDataMaintenanceTaskSnapshot>> {
        Ok(self
            .list_projections_for_tenant::<ConversationDataMaintenanceTaskSnapshot>(
                tenant_id,
                TaskKind::ConversationDataMaintenance,
            )?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn conversation_data_maintenance_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<ConversationDataMaintenanceTaskSnapshot>> {
        let mut snapshots = self
            .list_projections_for_tenant::<ConversationDataMaintenanceTaskSnapshot>(
                tenant_id,
                TaskKind::ConversationDataMaintenance,
            )?;
        snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        Ok(snapshots)
    }

    pub(crate) fn cancel_conversation_data_maintenance_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<ConversationDataMaintenanceTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
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
        result: AppResult<Value>,
    ) -> AppResult<ConversationScriptInstallTaskSnapshot> {
        let runtime = self.finish_external_task(task_id, result)?;
        let mut snapshot: ConversationScriptInstallTaskSnapshot = self.decode(&runtime)?;
        snapshot.result = runtime.result.clone();
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    pub(crate) fn conversation_script_install_snapshot(
        &self,
    ) -> AppResult<Option<ConversationScriptInstallTaskSnapshot>> {
        Ok(self
            .list_projections::<ConversationScriptInstallTaskSnapshot>(
                TaskKind::ExtensionLifecycle,
            )?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn begin_skill_backup_for_tenant(
        &self,
        tenant_id: &str,
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
        let registration = self.register_projection_for_tenant(
            Some(tenant_id),
            TaskKind::Backup,
            &snapshot.id,
            Some(format!("{tenant_id}:skill-backup")),
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
        result: crate::backend::runtime::AppResult<Vec<CatalogAsset>>,
    ) -> AppResult<SkillBackupTaskSnapshot> {
        let runtime_result = match &result {
            Ok(assets) => serde_json::to_value(assets)
                .map(Some)
                .map_err(crate::backend::runtime::AppError::external),
            Err(error) => Err(crate::backend::runtime::AppError::from(error.view())),
        };
        let runtime_result = runtime_result.and_then(|result| {
            result.ok_or_else(|| {
                crate::backend::runtime::AppError::External(
                    "skill backup result was empty".to_string(),
                )
            })
        });
        self.finish_external_result(task_id, runtime_result)?;
        let mut snapshot: SkillBackupTaskSnapshot =
            self.decode(&self.external_task_snapshot(task_id)?)?;
        match result {
            Ok(assets) => snapshot.assets = assets,
            Err(error) => snapshot.errors.push(SkillBackupTaskError {
                asset_id: snapshot.current_asset_id.clone(),
                error: error.view(),
            }),
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    #[allow(dead_code)]
    pub(crate) fn skill_backup_snapshot(&self) -> AppResult<Option<SkillBackupTaskSnapshot>> {
        Ok(self
            .list_projections::<SkillBackupTaskSnapshot>(TaskKind::Backup)?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn skill_backup_snapshot_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Option<SkillBackupTaskSnapshot>> {
        Ok(self
            .list_projections_for_tenant::<SkillBackupTaskSnapshot>(tenant_id, TaskKind::Backup)?
            .into_iter()
            .max_by(|left, right| left.started_at.cmp(&right.started_at)))
    }

    pub(crate) fn begin_ai_execution_for_tenant(
        &self,
        tenant_id: &str,
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
            cleanup: None,
        };
        let runtime = match self.register_external_task_for_tenant(
            Some(tenant_id),
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
        if matches!(runtime.state, TaskState::Running | TaskState::Cancelling)
            && !snapshot.state.is_terminal()
        {
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

    pub(crate) fn update_ai_execution_cleanup(
        &self,
        task_id: &str,
        cleanup: AiExecutionCleanupReport,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: AiExecutionTaskSnapshot = self.decode(&runtime)?;
        if !snapshot.state.is_terminal() {
            snapshot.cleanup = Some(cleanup);
            snapshot.updated_at = Utc::now().to_rfc3339();
            self.write_projection(task_id, &snapshot)?;
        }
        self.projection(task_id)
    }

    #[allow(dead_code)]
    pub(crate) fn finish_ai_execution(
        &self,
        task_id: &str,
        result: Result<AiExecutionResult, AiExecutionError>,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        self.finish_ai_execution_with_phase(task_id, result, None)
    }

    pub(crate) fn finish_ai_execution_with_phase(
        &self,
        task_id: &str,
        result: Result<AiExecutionResult, AiExecutionError>,
        failure_phase: Option<AiExecutionPhase>,
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
                let mut error_view = error.to_view();
                // Cleanup is a separate lifecycle observation. Preserve the
                // phase in which the execution failed so a cleanup result
                // cannot hide the actionable root cause.
                error_view.phase = failure_phase.or(Some(snapshot.phase));
                snapshot.error = Some(error_view);
            }
        }
        self.write_projection(task_id, &snapshot)?;
        self.projection(task_id)
    }

    #[allow(dead_code)]
    pub(crate) fn cancel_ai_execution(&self, task_id: &str) -> AppResult<AiExecutionTaskSnapshot> {
        self.cancel_external_task(task_id)?;
        self.projection(task_id)
    }

    pub(crate) fn cancel_ai_execution_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        self.cancel_external_task_for_tenant(tenant_id, task_id)?;
        self.projection_for_tenant(tenant_id, task_id)
    }

    #[allow(dead_code)]
    pub(crate) fn ai_execution_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<Option<AiExecutionTaskSnapshot>> {
        match self.task_runtime.get(task_id) {
            Some(runtime) => self.projection_from_runtime(&runtime).map(Some),
            None => Ok(None),
        }
    }

    pub(crate) fn ai_execution_snapshot_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
    ) -> AppResult<Option<AiExecutionTaskSnapshot>> {
        match self.task_runtime.get_for_tenant(tenant_id, task_id) {
            Some(runtime) => self.projection_from_runtime(&runtime).map(Some),
            None => Ok(None),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn ai_execution_snapshots(&self) -> AppResult<Vec<AiExecutionTaskSnapshot>> {
        let mut snapshots =
            self.list_projections::<AiExecutionTaskSnapshot>(TaskKind::AiExecution)?;
        snapshots.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(snapshots)
    }

    pub(crate) fn ai_execution_snapshots_for_tenant(
        &self,
        tenant_id: &str,
    ) -> AppResult<Vec<AiExecutionTaskSnapshot>> {
        let mut snapshots = self.list_projections_for_tenant::<AiExecutionTaskSnapshot>(
            tenant_id,
            TaskKind::AiExecution,
        )?;
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
                ..Default::default()
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
                ..Default::default()
            })
            .len())
    }

    pub(crate) fn has_running_tasks(&self) -> bool {
        self.task_runtime.has_active_tasks()
    }

    pub(crate) fn active_conversation_usage_scan_id(&self, tenant_id: &str) -> Option<String> {
        self.task_runtime
            .list(crate::backend::runtime::tasks::TaskFilter {
                kind: Some(TaskKind::ConversationUsageScan),
                active_only: true,
                ..Default::default()
            })
            .into_iter()
            .find(|task| task.tenant_id.as_deref() == Some(tenant_id))
            .map(|task| task.task_id)
    }
}

fn runtime_error_message(snapshot: &TaskSnapshot) -> Option<AppErrorView> {
    snapshot.error.clone()
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
impl_basic_projection!(ConversationDataMaintenanceTaskSnapshot);
impl_basic_projection!(ConversationSearchIndexTaskSnapshot);
impl_basic_projection!(ConversationScriptInstallTaskSnapshot);
impl_basic_projection!(BatchMountTaskSnapshot);
impl_basic_projection!(ConversationUsageScanTaskSnapshot);

impl BackgroundTaskProjection for RemoteSkillAcquireTaskSnapshot {
    fn project_with_runtime(mut self, runtime: &TaskSnapshot) -> Self {
        self.status = background_task_status(runtime.state);
        self.finished_at = runtime.finished_at.clone();
        match runtime.state {
            TaskState::Succeeded => {
                self.phase = "completed".to_string();
                self.result = runtime.result.clone();
                self.error = None;
            }
            TaskState::Failed => {
                self.phase = "failed".to_string();
                self.result = None;
                self.error = runtime.error.clone();
            }
            TaskState::Canceled => {
                self.phase = "cancelled".to_string();
                self.result = None;
                self.error = runtime.error.clone();
            }
            TaskState::Pending | TaskState::Running | TaskState::Cancelling => {}
        }
        self
    }
}

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
                        let market_error =
                            AgentMarketError::new(&error.code, &error.message, error.retryable)
                                .with_details(error.details.clone());
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
                    let market_error =
                        AgentMarketError::new(&error.code, &error.message, error.retryable)
                            .with_details(error.details.clone());
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
                if !matches!(
                    self.phase,
                    AiExecutionPhase::Closing | AiExecutionPhase::CleaningUp
                ) {
                    self.phase = AiExecutionPhase::Cancelling;
                }
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
                self.result = None;
                self.error = Some(AiExecutionErrorView {
                    code: "cancelled".to_string(),
                    message: runtime_error_message(runtime)
                        .map(|error| error.message)
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
#[path = "background_tasks_tests.rs"]
mod tests;
