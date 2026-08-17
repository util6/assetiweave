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
    dto::{AppResult, CatalogAsset},
    models::{MemoryDreamTrigger, MemoryRunKind, MemoryScope},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

const AI_EXECUTION_TERMINAL_RETENTION: Duration = Duration::from_secs(10 * 60);
const AI_EXECUTION_TERMINAL_LIMIT: usize = 100;
const AGENT_LIFECYCLE_TERMINAL_RETENTION: Duration = Duration::from_secs(10 * 60);
const AGENT_LIFECYCLE_TERMINAL_LIMIT: usize = 100;

/// 后台异步任务的状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BackgroundTaskStatus {
    /// 任务正在后台运行中
    Running,
    /// 任务已成功完成
    Completed,
    /// 任务运行失败
    Failed,
    /// 任务已被用户取消
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AiExecutionPublicResult {
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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

struct AiExecutionTaskEntry {
    snapshot: AiExecutionTaskSnapshot,
    cancellation: AiExecutionCancellation,
    terminal_at: Option<Instant>,
}

/// 会话同步任务进度快照
#[derive(Debug, Clone, PartialEq, Serialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ConversationSearchIndexTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
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
            _ => Err(format!(
                "unsupported conversation record kind: {record_kind}"
            )),
        }
    }

    fn record_kind(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Session => Some("session"),
            Self::Web => Some("web"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SkillBackupTaskError {
    pub(crate) asset_id: Option<String>,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Serialize)]
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
    pub(crate) assets: Vec<CatalogAsset>,
    pub(crate) errors: Vec<SkillBackupTaskError>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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

struct MemoryTaskEntry {
    snapshot: MemoryTaskSnapshot,
    cancellation: AiExecutionCancellation,
}

struct AgentLifecycleTaskEntry {
    snapshot: AgentLifecycleTaskSnapshot,
    cancellation: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentMarketRefreshTaskState {
    Running,
    Succeeded,
    Failed,
}

impl AgentMarketRefreshTaskState {
    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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

struct AgentMarketRefreshTaskEntry {
    snapshot: AgentMarketRefreshTaskSnapshot,
    terminal_at: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct BackgroundTaskRegistry {
    conversation_sync: Mutex<HashMap<ConversationSyncScope, ConversationSyncTaskSnapshot>>,
    conversation_script_install: Mutex<Option<ConversationScriptInstallTaskSnapshot>>,
    skill_backup: Mutex<Option<SkillBackupTaskSnapshot>>,
    conversation_search_index: Mutex<Option<ConversationSearchIndexTaskSnapshot>>,
    memory_tasks: Mutex<HashMap<String, MemoryTaskEntry>>,
    ai_executions: Mutex<HashMap<String, AiExecutionTaskEntry>>,
    agent_lifecycle_tasks: Mutex<HashMap<String, AgentLifecycleTaskEntry>>,
    agent_market_refresh_tasks: Mutex<HashMap<String, AgentMarketRefreshTaskEntry>>,
}

impl BackgroundTaskRegistry {
    pub(crate) fn begin_agent_market_refresh(
        &self,
    ) -> AppResult<(AgentMarketRefreshTaskSnapshot, bool)> {
        let mut tasks = self
            .agent_market_refresh_tasks
            .lock()
            .map_err(|error| error.to_string())?;
        prune_agent_market_refresh_tasks(&mut tasks);
        if let Some(entry) = tasks
            .values()
            .find(|entry| !entry.snapshot.state.is_terminal())
        {
            return Ok((entry.snapshot.clone(), false));
        }
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
        tasks.insert(
            snapshot.id.clone(),
            AgentMarketRefreshTaskEntry {
                snapshot: snapshot.clone(),
                terminal_at: None,
            },
        );
        Ok((snapshot, true))
    }

    pub(crate) fn finish_agent_market_refresh(
        &self,
        task_id: &str,
        result: Result<AgentMarketRefreshResult, String>,
    ) -> AppResult<AgentMarketRefreshTaskSnapshot> {
        let mut tasks = self
            .agent_market_refresh_tasks
            .lock()
            .map_err(|error| error.to_string())?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| "Agent Market refresh task not found".to_string())?;
        entry.snapshot.finished_at = Some(Utc::now().to_rfc3339());
        entry.snapshot.updated_at = Utc::now().to_rfc3339();
        entry.terminal_at = Some(Instant::now());
        match result {
            Ok(result) => {
                entry.snapshot.state = AgentMarketRefreshTaskState::Succeeded;
                entry.snapshot.result = Some(result);
            }
            Err(error) => {
                entry.snapshot.state = AgentMarketRefreshTaskState::Failed;
                entry.snapshot.error = Some(error);
            }
        }
        Ok(entry.snapshot.clone())
    }

    pub(crate) fn agent_market_refresh_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<AgentMarketRefreshTaskSnapshot> {
        self.agent_market_refresh_tasks
            .lock()
            .map_err(|error| error.to_string())?
            .get(task_id)
            .map(|entry| entry.snapshot.clone())
            .ok_or_else(|| "Agent Market refresh task not found".to_string())
    }

    pub(crate) fn agent_market_refresh_snapshots(
        &self,
    ) -> AppResult<Vec<AgentMarketRefreshTaskSnapshot>> {
        let mut snapshots = self
            .agent_market_refresh_tasks
            .lock()
            .map_err(|error| error.to_string())?
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect::<Vec<_>>();
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
    ) -> AppResult<(AgentLifecycleTaskSnapshot, Arc<AtomicBool>, bool)> {
        let mut tasks = self
            .agent_lifecycle_tasks
            .lock()
            .map_err(|error| error.to_string())?;
        prune_agent_lifecycle_tasks(&mut tasks);
        if let Some(entry) = tasks.values().find(|entry| {
            entry.snapshot.agent_id == agent_id
                && matches!(
                    entry.snapshot.state,
                    LifecycleTaskState::Queued | LifecycleTaskState::Running
                )
        }) {
            return Ok((entry.snapshot.clone(), entry.cancellation.clone(), false));
        }
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
        let cancellation = Arc::new(AtomicBool::new(false));
        tasks.insert(
            snapshot.id.clone(),
            AgentLifecycleTaskEntry {
                snapshot: snapshot.clone(),
                cancellation: cancellation.clone(),
            },
        );
        Ok((snapshot, cancellation, true))
    }

    pub(crate) fn update_agent_lifecycle(
        &self,
        task_id: &str,
        phase: LifecycleTaskPhase,
        completed_units: u64,
        downloaded_bytes: Option<u64>,
        warnings: Vec<String>,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        let mut tasks = self
            .agent_lifecycle_tasks
            .lock()
            .map_err(|error| error.to_string())?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| "agent lifecycle task not found".to_string())?;
        if !matches!(entry.snapshot.state, LifecycleTaskState::Cancelled) {
            entry.snapshot.state = LifecycleTaskState::Running;
            entry.snapshot.phase = phase;
            entry.snapshot.progress.completed_units = completed_units;
            entry.snapshot.progress.downloaded_bytes = downloaded_bytes;
            entry.snapshot.warnings = warnings;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
        }
        Ok(entry.snapshot.clone())
    }

    pub(crate) fn finish_agent_lifecycle(
        &self,
        task_id: &str,
        result: Result<(Option<Value>, Vec<String>), AgentMarketError>,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        let mut tasks = self
            .agent_lifecycle_tasks
            .lock()
            .map_err(|error| error.to_string())?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| "agent lifecycle task not found".to_string())?;
        if matches!(entry.snapshot.state, LifecycleTaskState::Cancelled) {
            entry.snapshot.phase = LifecycleTaskPhase::Cancelled;
            entry.snapshot.cancellable = false;
            entry
                .snapshot
                .finished_at
                .get_or_insert_with(|| Utc::now().to_rfc3339());
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
        } else {
            entry.snapshot.finished_at = Some(Utc::now().to_rfc3339());
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.cancellable = false;
            match result {
                Ok((result, warnings)) => {
                    entry.snapshot.state = LifecycleTaskState::Succeeded;
                    entry.snapshot.phase = LifecycleTaskPhase::Succeeded;
                    entry.snapshot.result = result;
                    entry.snapshot.warnings = warnings;
                }
                Err(error) => {
                    entry.snapshot.state = LifecycleTaskState::Failed;
                    entry.snapshot.phase = LifecycleTaskPhase::Failed;
                    entry.snapshot.error = Some((&error).into());
                }
            }
        }
        Ok(entry.snapshot.clone())
    }

    pub(crate) fn agent_lifecycle_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        self.agent_lifecycle_tasks
            .lock()
            .map_err(|error| error.to_string())?
            .get(task_id)
            .map(|entry| entry.snapshot.clone())
            .ok_or_else(|| "agent lifecycle task not found".to_string())
    }

    pub(crate) fn agent_lifecycle_snapshots(&self) -> AppResult<Vec<AgentLifecycleTaskSnapshot>> {
        let mut snapshots = self
            .agent_lifecycle_tasks
            .lock()
            .map_err(|error| error.to_string())?
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        Ok(snapshots)
    }

    pub(crate) fn cancel_agent_lifecycle(
        &self,
        task_id: &str,
    ) -> AppResult<AgentLifecycleTaskSnapshot> {
        let mut tasks = self
            .agent_lifecycle_tasks
            .lock()
            .map_err(|error| error.to_string())?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| "agent lifecycle task not found".to_string())?;
        entry.cancellation.store(true, Ordering::SeqCst);
        if matches!(
            entry.snapshot.state,
            LifecycleTaskState::Queued | LifecycleTaskState::Running
        ) {
            entry.snapshot.state = LifecycleTaskState::Cancelled;
            entry.snapshot.phase = LifecycleTaskPhase::Cancelled;
            entry.snapshot.cancellable = false;
            entry.snapshot.finished_at = Some(Utc::now().to_rfc3339());
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
        }
        Ok(entry.snapshot.clone())
    }

    pub(crate) fn begin_conversation_search_index_rebuild(
        &self,
    ) -> AppResult<(ConversationSearchIndexTaskSnapshot, bool)> {
        let mut current = self
            .conversation_search_index
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(snapshot) = current
            .as_ref()
            .filter(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
        {
            return Ok((snapshot.clone(), false));
        }
        let snapshot = ConversationSearchIndexTaskSnapshot {
            id: Uuid::new_v4().to_string(),
            status: BackgroundTaskStatus::Running,
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        *current = Some(snapshot.clone());
        Ok((snapshot, true))
    }

    pub(crate) fn finish_conversation_search_index_rebuild(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<ConversationSearchIndexTaskSnapshot> {
        let mut current = self
            .conversation_search_index
            .lock()
            .map_err(|error| error.to_string())?;
        let snapshot = current
            .as_mut()
            .filter(|snapshot| snapshot.id == task_id)
            .ok_or_else(|| "conversation search index task not found".to_string())?;
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

    pub(crate) fn conversation_search_index_snapshot(
        &self,
    ) -> AppResult<Option<ConversationSearchIndexTaskSnapshot>> {
        self.conversation_search_index
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn begin_conversation_sync(
        &self,
        params: &ConversationSyncParams,
    ) -> AppResult<(ConversationSyncTaskSnapshot, bool)> {
        let scope = ConversationSyncScope::from_record_kind(params.record_kind.as_deref())?;
        let mut current = self
            .conversation_sync
            .lock()
            .map_err(|error| error.to_string())?;
        let running = match scope {
            ConversationSyncScope::All => current
                .values()
                .find(|snapshot| snapshot.status == BackgroundTaskStatus::Running),
            _ => current
                .get(&ConversationSyncScope::All)
                .filter(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
                .or_else(|| {
                    current
                        .get(&scope)
                        .filter(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
                }),
        };
        if let Some(snapshot) = running {
            return Ok((snapshot.clone(), false));
        }

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
        current.insert(scope, snapshot.clone());
        Ok((snapshot, true))
    }

    pub(crate) fn update_conversation_sync_progress(
        &self,
        task_id: &str,
        completed_source_count: usize,
        total_source_count: usize,
        current_source_name: Option<String>,
    ) -> AppResult<ConversationSyncTaskSnapshot> {
        let mut current = self
            .conversation_sync
            .lock()
            .map_err(|error| error.to_string())?;
        let snapshot = current
            .values_mut()
            .find(|snapshot| snapshot.id == task_id)
            .ok_or_else(|| "conversation sync task not found".to_string())?;
        snapshot.progress = ConversationSyncTaskProgress {
            phase: ConversationSyncProgressPhase::Syncing,
            completed_source_count: completed_source_count.min(total_source_count),
            total_source_count,
            current_source_name,
        };
        Ok(snapshot.clone())
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
            .values_mut()
            .find(|snapshot| snapshot.id == task_id)
            .ok_or_else(|| "conversation sync task not found".to_string())?;

        snapshot.finished_at = Some(Utc::now().to_rfc3339());
        match result {
            Ok(value) => {
                snapshot.status = BackgroundTaskStatus::Completed;
                snapshot.progress.phase = ConversationSyncProgressPhase::Completed;
                snapshot.progress.completed_source_count = snapshot.progress.total_source_count;
                snapshot.progress.current_source_name = None;
                snapshot.result = Some(value);
                snapshot.error = None;
            }
            Err(error) => {
                snapshot.status = BackgroundTaskStatus::Failed;
                snapshot.progress.phase = ConversationSyncProgressPhase::Failed;
                snapshot.progress.current_source_name = None;
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
            .map(|snapshots| {
                snapshots
                    .values()
                    .max_by(|left, right| left.started_at.cmp(&right.started_at))
                    .cloned()
            })
            .map_err(|error| error.to_string())
    }

    pub(crate) fn conversation_sync_snapshots(
        &self,
    ) -> AppResult<Vec<ConversationSyncTaskSnapshot>> {
        self.conversation_sync
            .lock()
            .map(|snapshots| {
                let mut snapshots = snapshots.values().cloned().collect::<Vec<_>>();
                snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
                snapshots
            })
            .map_err(|error| error.to_string())
    }

    pub(crate) fn begin_conversation_script_install(
        &self,
        params: &ConversationScriptInstallParams,
    ) -> AppResult<(ConversationScriptInstallTaskSnapshot, bool)> {
        let mut current = self
            .conversation_script_install
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(snapshot) = current
            .as_ref()
            .filter(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
        {
            return Ok((snapshot.clone(), false));
        }

        let item_id = params.item_id.trim().to_string();
        if item_id.is_empty() {
            return Err("conversation script install requires an item id".to_string());
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
        *current = Some(snapshot.clone());
        Ok((snapshot, true))
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
        let mut current = self
            .conversation_script_install
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(snapshot) = current
            .as_ref()
            .filter(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
        {
            return Ok((snapshot.clone(), false));
        }

        let package_id = params.package_id.trim().to_string();
        if package_id.is_empty() {
            return Err("conversation adapter package install requires a package id".to_string());
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
        *current = Some(snapshot.clone());
        Ok((snapshot, true))
    }

    pub(crate) fn begin_conversation_adapter_package_uninstall(
        &self,
        params: &ConversationAdapterPackageUninstallParams,
    ) -> AppResult<(ConversationScriptInstallTaskSnapshot, bool)> {
        let mut current = self
            .conversation_script_install
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(snapshot) = current
            .as_ref()
            .filter(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
        {
            return Ok((snapshot.clone(), false));
        }
        let package_id = params.package_id.trim().to_string();
        if package_id.is_empty() {
            return Err("conversation adapter package uninstall requires a package id".to_string());
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
        *current = Some(snapshot.clone());
        Ok((snapshot, true))
    }

    pub(crate) fn finish_conversation_script_install(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<ConversationScriptInstallTaskSnapshot> {
        let mut current = self
            .conversation_script_install
            .lock()
            .map_err(|error| error.to_string())?;
        let snapshot = current
            .as_mut()
            .ok_or_else(|| "conversation script install task not found".to_string())?;
        if snapshot.id != task_id {
            return Err(format!(
                "conversation script install task is no longer current: {task_id}"
            ));
        }

        snapshot.finished_at = Some(Utc::now().to_rfc3339());
        match result {
            Ok(value) => {
                snapshot.status = BackgroundTaskStatus::Completed;
                snapshot.phase = Some("completed".to_string());
                snapshot.result = Some(value);
                snapshot.error = None;
            }
            Err(error) => {
                snapshot.status = BackgroundTaskStatus::Failed;
                snapshot.phase = Some("failed".to_string());
                snapshot.result = None;
                snapshot.error = Some(error);
            }
        }
        Ok(snapshot.clone())
    }

    pub(crate) fn conversation_script_install_snapshot(
        &self,
    ) -> AppResult<Option<ConversationScriptInstallTaskSnapshot>> {
        self.conversation_script_install
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn begin_skill_backup(
        &self,
        asset_ids: Vec<String>,
    ) -> AppResult<(SkillBackupTaskSnapshot, bool)> {
        let mut current = self
            .skill_backup
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(snapshot) = current
            .as_ref()
            .filter(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
        {
            return Ok((snapshot.clone(), false));
        }

        let asset_ids = dedupe_non_empty(asset_ids);
        if asset_ids.is_empty() {
            return Err("skill backup requires at least one asset id".to_string());
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
        *current = Some(snapshot.clone());
        Ok((snapshot, true))
    }

    pub(crate) fn update_skill_backup_progress(
        &self,
        task_id: &str,
        completed_count: usize,
        current_asset_id: Option<String>,
    ) -> AppResult<SkillBackupTaskSnapshot> {
        let mut current = self
            .skill_backup
            .lock()
            .map_err(|error| error.to_string())?;
        let snapshot = current
            .as_mut()
            .ok_or_else(|| "skill backup task not found".to_string())?;
        if snapshot.id != task_id {
            return Err(format!("skill backup task is no longer current: {task_id}"));
        }
        snapshot.completed_count = completed_count.min(snapshot.total_count);
        snapshot.current_asset_id = current_asset_id;
        Ok(snapshot.clone())
    }

    pub(crate) fn finish_skill_backup(
        &self,
        task_id: &str,
        result: AppResult<Vec<CatalogAsset>>,
    ) -> AppResult<SkillBackupTaskSnapshot> {
        let mut current = self
            .skill_backup
            .lock()
            .map_err(|error| error.to_string())?;
        let snapshot = current
            .as_mut()
            .ok_or_else(|| "skill backup task not found".to_string())?;
        if snapshot.id != task_id {
            return Err(format!("skill backup task is no longer current: {task_id}"));
        }

        snapshot.finished_at = Some(Utc::now().to_rfc3339());
        snapshot.current_asset_id = None;
        match result {
            Ok(assets) => {
                snapshot.status = BackgroundTaskStatus::Completed;
                snapshot.completed_count = snapshot.total_count;
                snapshot.failed_count = 0;
                snapshot.assets = assets;
                snapshot.errors.clear();
                snapshot.error = None;
            }
            Err(error) => {
                snapshot.status = BackgroundTaskStatus::Failed;
                snapshot.failed_count = 1;
                snapshot.assets.clear();
                snapshot.errors = vec![SkillBackupTaskError {
                    asset_id: snapshot.asset_ids.get(snapshot.completed_count).cloned(),
                    message: error.clone(),
                }];
                snapshot.error = Some(error);
            }
        }
        Ok(snapshot.clone())
    }

    pub(crate) fn skill_backup_snapshot(&self) -> AppResult<Option<SkillBackupTaskSnapshot>> {
        self.skill_backup
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn begin_memory_task(
        &self,
        params: &MemoryTaskStartParams,
    ) -> AppResult<(MemoryTaskSnapshot, AiExecutionCancellation, bool)> {
        let scope_fingerprint = params.scope.fingerprint()?;
        let mut tasks = self
            .memory_tasks
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(entry) = tasks.values().find(|entry| {
            entry.snapshot.status == BackgroundTaskStatus::Running
                && entry.snapshot.scope_fingerprint == scope_fingerprint
                && entry.snapshot.kind == params.kind
        }) {
            return Ok((entry.snapshot.clone(), entry.cancellation.clone(), false));
        }
        if let Some(entry) = tasks.values().find(|entry| {
            entry.snapshot.status == BackgroundTaskStatus::Running
                && entry.snapshot.scope_fingerprint == scope_fingerprint
        }) {
            return Err(format!(
                "Memory scope is already running task {} ({:?})",
                entry.snapshot.id, entry.snapshot.kind
            ));
        }

        let id = Uuid::new_v4().to_string();
        let cancellation = AiExecutionCancellation::default();
        let snapshot = MemoryTaskSnapshot {
            id: id.clone(),
            status: BackgroundTaskStatus::Running,
            kind: params.kind,
            scope: params.scope.clone(),
            scope_fingerprint,
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
        tasks.insert(
            id,
            MemoryTaskEntry {
                snapshot: snapshot.clone(),
                cancellation: cancellation.clone(),
            },
        );
        Ok((snapshot, cancellation, true))
    }

    pub(crate) fn update_memory_task(
        &self,
        task_id: &str,
        phase: &str,
        processed_count: usize,
        total_count: usize,
        run_id: Option<String>,
    ) -> AppResult<MemoryTaskSnapshot> {
        let mut tasks = self
            .memory_tasks
            .lock()
            .map_err(|error| error.to_string())?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("Memory task was not found: {task_id}"))?;
        if entry.snapshot.status != BackgroundTaskStatus::Running {
            return Ok(entry.snapshot.clone());
        }
        entry.snapshot.phase = phase.to_string();
        entry.snapshot.processed_count = processed_count.min(total_count);
        entry.snapshot.total_count = total_count;
        if run_id.is_some() {
            entry.snapshot.run_id = run_id;
        }
        Ok(entry.snapshot.clone())
    }

    pub(crate) fn finish_memory_task(
        &self,
        task_id: &str,
        result: AppResult<Value>,
    ) -> AppResult<MemoryTaskSnapshot> {
        let mut tasks = self
            .memory_tasks
            .lock()
            .map_err(|error| error.to_string())?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("Memory task was not found: {task_id}"))?;
        entry.snapshot.finished_at = Some(Utc::now().to_rfc3339());
        match result {
            Ok(value) => {
                entry.snapshot.status = if entry.snapshot.cancel_requested {
                    BackgroundTaskStatus::Cancelled
                } else {
                    BackgroundTaskStatus::Completed
                };
                entry.snapshot.phase = if entry.snapshot.cancel_requested {
                    "cancelled".to_string()
                } else {
                    "completed".to_string()
                };
                entry.snapshot.processed_count = entry.snapshot.total_count;
                entry.snapshot.result = Some(value);
                entry.snapshot.error = None;
            }
            Err(error) => {
                entry.snapshot.status = if entry.snapshot.cancel_requested {
                    BackgroundTaskStatus::Cancelled
                } else {
                    BackgroundTaskStatus::Failed
                };
                entry.snapshot.phase = if entry.snapshot.cancel_requested {
                    "cancelled".to_string()
                } else {
                    "failed".to_string()
                };
                entry.snapshot.result = None;
                entry.snapshot.error = Some(error);
            }
        }
        Ok(entry.snapshot.clone())
    }

    pub(crate) fn cancel_memory_task(&self, task_id: &str) -> AppResult<MemoryTaskSnapshot> {
        let mut tasks = self
            .memory_tasks
            .lock()
            .map_err(|error| error.to_string())?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("Memory task was not found: {task_id}"))?;
        if entry.snapshot.status == BackgroundTaskStatus::Running {
            entry.snapshot.cancel_requested = true;
            entry.snapshot.phase = "cancelling".to_string();
            entry.cancellation.cancel();
        }
        Ok(entry.snapshot.clone())
    }

    pub(crate) fn memory_task_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<Option<MemoryTaskSnapshot>> {
        self.memory_tasks
            .lock()
            .map(|tasks| tasks.get(task_id).map(|entry| entry.snapshot.clone()))
            .map_err(|error| error.to_string())
    }

    pub(crate) fn memory_task_snapshots(&self) -> AppResult<Vec<MemoryTaskSnapshot>> {
        self.memory_tasks
            .lock()
            .map(|tasks| {
                let mut snapshots = tasks
                    .values()
                    .map(|entry| entry.snapshot.clone())
                    .collect::<Vec<_>>();
                snapshots.sort_by(|left, right| left.started_at.cmp(&right.started_at));
                snapshots
            })
            .map_err(|error| error.to_string())
    }

    pub(crate) fn begin_ai_execution(
        &self,
        purpose: AiExecutionPurpose,
        agent_id: &AgentId,
    ) -> AppResult<(AiExecutionTaskSnapshot, AiExecutionCancellation)> {
        let mut tasks = self
            .ai_executions
            .lock()
            .map_err(|_| "AI execution task registry is unavailable".to_string())?;
        prune_ai_executions(&mut tasks, Instant::now());
        let id = Uuid::new_v4().to_string();
        let cancellation = AiExecutionCancellation::default();
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
        tasks.insert(
            id,
            AiExecutionTaskEntry {
                snapshot: snapshot.clone(),
                cancellation: cancellation.clone(),
                terminal_at: None,
            },
        );
        Ok((snapshot, cancellation))
    }

    pub(crate) fn update_ai_execution_phase(
        &self,
        task_id: &str,
        phase: AiExecutionPhase,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        let mut tasks = self
            .ai_executions
            .lock()
            .map_err(|_| "AI execution task registry is unavailable".to_string())?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("AI execution task was not found: {task_id}"))?;
        if entry.snapshot.state.is_terminal() {
            return Ok(entry.snapshot.clone());
        }
        entry.snapshot.state = if phase == AiExecutionPhase::Queued {
            AiExecutionTaskState::Queued
        } else {
            AiExecutionTaskState::Running
        };
        entry.snapshot.phase = phase;
        entry.snapshot.updated_at = Utc::now().to_rfc3339();
        Ok(entry.snapshot.clone())
    }

    pub(crate) fn finish_ai_execution(
        &self,
        task_id: &str,
        result: Result<AiExecutionResult, AiExecutionError>,
    ) -> AppResult<AiExecutionTaskSnapshot> {
        let mut tasks = self
            .ai_executions
            .lock()
            .map_err(|_| "AI execution task registry is unavailable".to_string())?;
        let now = Instant::now();
        prune_ai_executions(&mut tasks, now);
        let snapshot = {
            let entry = tasks
                .get_mut(task_id)
                .ok_or_else(|| format!("AI execution task was not found: {task_id}"))?;
            if entry.snapshot.state.is_terminal() {
                return Ok(entry.snapshot.clone());
            }
            let cancelled = entry.cancellation.is_cancelled()
                || matches!(&result, Err(AiExecutionError::Cancelled { .. }));
            entry.snapshot.phase = AiExecutionPhase::CleaningUp;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
            entry.snapshot.finished_at = Some(entry.snapshot.updated_at.clone());
            entry.terminal_at = Some(now);
            match result {
                Ok(result) if !cancelled => {
                    entry.snapshot.state = AiExecutionTaskState::Succeeded;
                    entry.snapshot.result = Some(AiExecutionPublicResult { text: result.text });
                    entry.snapshot.error = None;
                }
                Ok(_) => {
                    entry.snapshot.state = AiExecutionTaskState::Cancelled;
                    entry.snapshot.result = None;
                    entry.snapshot.error = None;
                }
                Err(error) => {
                    let mut error = error.to_view();
                    error.phase = Some(entry.snapshot.phase);
                    entry.snapshot.state = if cancelled {
                        AiExecutionTaskState::Cancelled
                    } else {
                        AiExecutionTaskState::Failed
                    };
                    entry.snapshot.result = None;
                    entry.snapshot.error = Some(error);
                }
            }
            entry.snapshot.clone()
        };
        prune_ai_executions(&mut tasks, now);
        Ok(snapshot)
    }

    pub(crate) fn cancel_ai_execution(&self, task_id: &str) -> AppResult<AiExecutionTaskSnapshot> {
        let mut tasks = self
            .ai_executions
            .lock()
            .map_err(|_| "AI execution task registry is unavailable".to_string())?;
        let entry = tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("AI execution task was not found: {task_id}"))?;
        if !entry.snapshot.state.is_terminal() {
            entry.cancellation.cancel();
            entry.snapshot.state = AiExecutionTaskState::Running;
            entry.snapshot.phase = AiExecutionPhase::Cancelling;
            entry.snapshot.updated_at = Utc::now().to_rfc3339();
        }
        Ok(entry.snapshot.clone())
    }

    pub(crate) fn ai_execution_snapshot(
        &self,
        task_id: &str,
    ) -> AppResult<Option<AiExecutionTaskSnapshot>> {
        self.ai_executions
            .lock()
            .map(|tasks| tasks.get(task_id).map(|entry| entry.snapshot.clone()))
            .map_err(|_| "AI execution task registry is unavailable".to_string())
    }

    pub(crate) fn ai_execution_snapshots(&self) -> AppResult<Vec<AiExecutionTaskSnapshot>> {
        let mut tasks = self
            .ai_executions
            .lock()
            .map_err(|_| "AI execution task registry is unavailable".to_string())?;
        prune_ai_executions(&mut tasks, Instant::now());
        let mut snapshots = tasks
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(snapshots)
    }

    pub(crate) fn cancel_all_ai_executions(&self) -> AppResult<Vec<AiExecutionTaskSnapshot>> {
        let mut tasks = self
            .ai_executions
            .lock()
            .map_err(|_| "AI execution task registry is unavailable".to_string())?;
        let now = Utc::now().to_rfc3339();
        let mut cancelled = Vec::new();
        for entry in tasks.values_mut() {
            if !entry.snapshot.state.is_terminal() {
                entry.cancellation.cancel();
                entry.snapshot.state = AiExecutionTaskState::Running;
                entry.snapshot.phase = AiExecutionPhase::Cancelling;
                entry.snapshot.updated_at = now.clone();
                cancelled.push(entry.snapshot.clone());
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
        self.ai_executions
            .lock()
            .map(|tasks| {
                tasks
                    .values()
                    .filter(|entry| !entry.snapshot.state.is_terminal())
                    .count()
            })
            .map_err(|_| "AI execution task registry is unavailable".to_string())
    }

    pub(crate) fn has_running_tasks(&self) -> bool {
        let conversation_sync_running = self
            .conversation_sync
            .lock()
            .map(|snapshots| {
                snapshots
                    .values()
                    .any(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
            })
            .unwrap_or(true);
        let skill_backup_running = self
            .skill_backup
            .lock()
            .map(|snapshot| {
                snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
            })
            .unwrap_or(true);
        let conversation_script_install_running = self
            .conversation_script_install
            .lock()
            .map(|snapshot| {
                snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
            })
            .unwrap_or(true);
        let conversation_search_index_running = self
            .conversation_search_index
            .lock()
            .map(|snapshot| {
                snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
            })
            .unwrap_or(true);
        let memory_running = self
            .memory_tasks
            .lock()
            .map(|tasks| {
                tasks
                    .values()
                    .any(|entry| entry.snapshot.status == BackgroundTaskStatus::Running)
            })
            .unwrap_or(true);
        let ai_execution_running = self
            .ai_executions
            .lock()
            .map(|tasks| {
                tasks
                    .values()
                    .any(|entry| !entry.snapshot.state.is_terminal())
            })
            .unwrap_or(true);
        let agent_lifecycle_running = self
            .agent_lifecycle_tasks
            .lock()
            .map(|tasks| {
                tasks.values().any(|entry| {
                    matches!(
                        entry.snapshot.state,
                        LifecycleTaskState::Queued | LifecycleTaskState::Running
                    )
                })
            })
            .unwrap_or(true);
        conversation_sync_running
            || conversation_script_install_running
            || skill_backup_running
            || conversation_search_index_running
            || memory_running
            || ai_execution_running
            || agent_lifecycle_running
    }
}

fn prune_agent_lifecycle_tasks(tasks: &mut HashMap<String, AgentLifecycleTaskEntry>) {
    let now = Utc::now();
    tasks.retain(|_, entry| {
        if !entry.snapshot.state.is_terminal() {
            return true;
        }
        entry
            .snapshot
            .finished_at
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|finished| {
                now.signed_duration_since(finished.with_timezone(&Utc))
                    .to_std()
                    .unwrap_or_default()
                    <= AGENT_LIFECYCLE_TERMINAL_RETENTION
            })
            .unwrap_or(true)
    });
    let mut terminal = tasks
        .values()
        .filter(|entry| entry.snapshot.state.is_terminal())
        .map(|entry| (entry.snapshot.id.clone(), entry.snapshot.created_at.clone()))
        .collect::<Vec<_>>();
    if terminal.len() > AGENT_LIFECYCLE_TERMINAL_LIMIT {
        terminal.sort_by(|left, right| left.1.cmp(&right.1));
        let excess = terminal.len() - AGENT_LIFECYCLE_TERMINAL_LIMIT;
        for (id, _) in terminal.into_iter().take(excess) {
            tasks.remove(&id);
        }
    }
}

fn prune_agent_market_refresh_tasks(tasks: &mut HashMap<String, AgentMarketRefreshTaskEntry>) {
    let now = Instant::now();
    tasks.retain(|_, entry| {
        !entry.snapshot.state.is_terminal()
            || entry.terminal_at.is_none_or(|finished| {
                now.duration_since(finished) <= AGENT_LIFECYCLE_TERMINAL_RETENTION
            })
    });
    if tasks.len() <= AGENT_LIFECYCLE_TERMINAL_LIMIT {
        return;
    }
    let mut terminal = tasks
        .iter()
        .filter(|(_, entry)| entry.snapshot.state.is_terminal())
        .map(|(id, entry)| (id.clone(), entry.snapshot.updated_at.clone()))
        .collect::<Vec<_>>();
    terminal.sort_by(|left, right| left.1.cmp(&right.1));
    for (id, _) in terminal
        .into_iter()
        .take(tasks.len() - AGENT_LIFECYCLE_TERMINAL_LIMIT)
    {
        tasks.remove(&id);
    }
}

fn prune_ai_executions(tasks: &mut HashMap<String, AiExecutionTaskEntry>, now: Instant) {
    tasks.retain(|_, entry| {
        entry.terminal_at.is_none_or(|terminal_at| {
            now.saturating_duration_since(terminal_at) <= AI_EXECUTION_TERMINAL_RETENTION
        })
    });

    let mut terminal = tasks
        .iter()
        .filter_map(|(id, entry)| entry.terminal_at.map(|at| (id.clone(), at)))
        .collect::<Vec<_>>();
    if terminal.len() <= AI_EXECUTION_TERMINAL_LIMIT {
        return;
    }
    terminal.sort_by(|(left_id, left_at), (right_id, right_at)| {
        right_at.cmp(left_at).then_with(|| right_id.cmp(left_id))
    });
    for (id, _) in terminal.into_iter().skip(AI_EXECUTION_TERMINAL_LIMIT) {
        tasks.remove(&id);
    }
}

fn dedupe_non_empty(values: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for value in values {
        let value = value.trim().to_string();
        if !value.is_empty() && seen.insert(value.clone()) {
            deduped.push(value);
        }
    }
    deduped
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
                item_id: "opencode-session".to_string(),
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

        assert!(error.contains("not found"));
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
    fn task_10_time_retention_prunes_expired_terminal_tasks() {
        let registry = BackgroundTaskRegistry::default();
        let (task, _) = registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .unwrap();
        registry
            .finish_ai_execution(&task.id, Ok(ai_result("done")))
            .unwrap();
        registry
            .ai_executions
            .lock()
            .unwrap()
            .get_mut(&task.id)
            .unwrap()
            .terminal_at =
            Some(Instant::now() - AI_EXECUTION_TERMINAL_RETENTION - Duration::from_secs(1));

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
                "update".to_string(),
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();

        assert!(should_start);
        assert!(!duplicate_should_start);
        assert_eq!(first.id, duplicate.id);
        assert!(Arc::ptr_eq(&cancellation, &duplicate_cancellation));

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
        assert_eq!(cancelled.state, LifecycleTaskState::Cancelled);
        assert!(cancellation.load(Ordering::SeqCst));
        let terminal = registry
            .finish_agent_lifecycle(&first.id, Ok((None, Vec::new())))
            .unwrap();
        assert_eq!(terminal.phase, LifecycleTaskPhase::Cancelled);
        assert!(!terminal.cancellable);
        assert!(!registry.has_running_tasks());
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
    fn task_15_poisoned_ai_registry_returns_errors_and_running_check_fails_closed() {
        let registry = std::sync::Arc::new(BackgroundTaskRegistry::default());
        let poison_target = registry.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poison_target.ai_executions.lock().unwrap();
            panic!("poison AI task registry");
        })
        .join();

        assert!(registry.ai_execution_snapshots().is_err());
        assert!(registry
            .begin_ai_execution(AiExecutionPurpose::Translation, &opencode_id())
            .is_err());
        assert!(registry.has_running_tasks());
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
            stdout: Vec::new(),
            stderr: Vec::new(),
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }
}
