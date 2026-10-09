//! Conversations Background Tasks: Packages & Scripts

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

impl BackgroundTaskRegistry {
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
            return Err(crate::backend::application::AppError::Validation(
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
            return Err(crate::backend::application::AppError::Validation(
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
            return Err(crate::backend::application::AppError::Validation(
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
}

impl_basic_projection!(ConversationScriptInstallTaskSnapshot);
