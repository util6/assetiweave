//! Conversations Background Tasks: Sync

use super::super::BackgroundTaskRegistry;
use super::super::{
    background_task_status, dedupe_non_empty, extension_lifecycle_key, impl_basic_projection,
    runtime_error_message, BackgroundTaskProjection, BackgroundTaskStatus,
};
use crate::backend::{
    application::{
        AgentMarketRefreshResult, AppResult, ConversationAdapterPackageInstallParams,
        ConversationAdapterPackageUninstallParams, ConversationScriptInstallParams,
        ConversationSyncMode, ConversationSyncParams, SkillAcquireParams,
    },
    domain::{AppErrorView, CatalogAsset},
    infrastructure::agent_market::{AgentLifecycleTaskSnapshot, ProgressSnapshot},
    infrastructure::extensions::{
        LifecycleOp, LifecycleRequestKey, LifecycleReservationOutcome, LifecycleTaskCoordinator,
        PackageIdentity, PackageKind, ResourceKey,
    },
    infrastructure::tasks::{
        ExternalRegistrationOutcome, TaskFn, TaskKind, TaskRuntime, TaskSnapshot, TaskSpec,
        TaskState,
    },
};
use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

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
    #[serde(default)]
    pub(crate) completed_adapter_ids: Vec<String>,
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
            _ => Err(crate::backend::application::AppError::Validation(format!(
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

impl BackgroundTaskRegistry {
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
                completed_adapter_ids: Vec::new(),
            },
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            result: None,
            error: None,
        };
        let (title, category) = match scope {
            ConversationSyncScope::Session => ("会话记录同步", "conversation/session_sync"),
            ConversationSyncScope::Web => ("网页记录同步", "conversation/web_sync"),
            ConversationSyncScope::All => ("完整同步", "conversation/sync"),
        };
        let registration = self.register_projection_for_tenant_with_meta(
            Some(tenant_id),
            TaskKind::ConversationSync,
            &snapshot.id,
            Some(title.to_string()),
            Some(category.to_string()),
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
        completed_adapter_ids: Vec<String>,
    ) -> AppResult<ConversationSyncTaskSnapshot> {
        let runtime = self.external_task_snapshot(task_id)?;
        let mut snapshot: ConversationSyncTaskSnapshot = self.decode(&runtime)?;
        if runtime.state == TaskState::Running {
            snapshot.progress = ConversationSyncTaskProgress {
                phase: ConversationSyncProgressPhase::Syncing,
                completed_source_count: completed_source_count.min(total_source_count),
                total_source_count,
                current_source_name,
                completed_adapter_ids,
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
}

impl_basic_projection!(ConversationSyncTaskSnapshot);
