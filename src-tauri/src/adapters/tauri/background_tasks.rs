use crate::backend::{
    application::{ConversationScriptInstallParams, ConversationSyncParams},
    dto::{AppResult, CatalogAsset},
};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashSet, sync::Mutex};
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct ConversationScriptInstallTaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: BackgroundTaskStatus,
    pub(crate) item_id: String,
    pub(crate) catalog_url: Option<String>,
    pub(crate) dry_run: bool,
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

#[derive(Default)]
pub(crate) struct BackgroundTaskRegistry {
    conversation_sync: Mutex<Option<ConversationSyncTaskSnapshot>>,
    conversation_script_install: Mutex<Option<ConversationScriptInstallTaskSnapshot>>,
    skill_backup: Mutex<Option<SkillBackupTaskSnapshot>>,
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
            item_id,
            catalog_url: params.catalog_url.clone(),
            dry_run: params.dry_run,
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

    pub(crate) fn has_running_tasks(&self) -> bool {
        let conversation_sync_running = self
            .conversation_sync
            .lock()
            .map(|snapshot| {
                snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.status == BackgroundTaskStatus::Running)
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
        conversation_sync_running || conversation_script_install_running || skill_backup_running
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

    fn params() -> ConversationSyncParams {
        ConversationSyncParams {
            source_id: None,
            adapter_id: None,
            record_kind: None,
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
}
