use crate::backend::application::prelude::*;
use sqlx::{AssertSqlSafe, SqlitePool};

use super::conversation_maintenance_ops::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

impl AppService {
    pub(crate) async fn audit_conversation_data(
        &self,
        params: ConversationDataAuditParams,
    ) -> AppResult<Value> {
        self.audit_conversation_data_with_progress(params, |_, _, _| {})
            .await
    }

    pub(crate) async fn audit_conversation_data_with_progress<F>(
        &self,
        params: ConversationDataAuditParams,
        mut on_progress: F,
    ) -> AppResult<Value>
    where
        F: FnMut(usize, usize, Option<String>) + Send,
    {
        self.audit_conversation_data_with_progress_and_cancellation(params, None, &mut on_progress)
            .await
    }

    pub(crate) async fn audit_conversation_data_with_progress_and_cancellation<F>(
        &self,
        params: ConversationDataAuditParams,
        cancellation: Option<&tokio_util::sync::CancellationToken>,
        on_progress: &mut F,
    ) -> AppResult<Value>
    where
        F: FnMut(usize, usize, Option<String>) + Send,
    {
        validate_conversation_maintenance_scope(
            params.record_kind.as_deref(),
            params.source_id.as_deref(),
        )?;
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let include_resolved = params.include_resolved;
        let record_kind = params.record_kind.clone();
        let source_id = params.source_id.clone();
        audit_conversation_data_sqlx(
            pool,
            tenant_id,
            record_kind.as_deref(),
            source_id.as_deref(),
            include_resolved,
            cancellation,
            on_progress,
        )
        .await
    }

    pub(crate) async fn repair_conversation_data(
        &self,
        params: ConversationDataRepairParams,
    ) -> AppResult<Value> {
        self.repair_conversation_data_with_progress(params, |_, _, _| {})
            .await
    }

    pub(crate) async fn repair_conversation_data_with_progress<F>(
        &self,
        params: ConversationDataRepairParams,
        mut on_progress: F,
    ) -> AppResult<Value>
    where
        F: FnMut(usize, usize, Option<String>) + Send,
    {
        self.repair_conversation_data_with_progress_and_cancellation(params, None, &mut on_progress)
            .await
    }

    pub(crate) async fn repair_conversation_data_with_progress_and_cancellation<F>(
        &self,
        params: ConversationDataRepairParams,
        cancellation: Option<&tokio_util::sync::CancellationToken>,
        on_progress: &mut F,
    ) -> AppResult<Value>
    where
        F: FnMut(usize, usize, Option<String>) + Send,
    {
        validate_conversation_maintenance_scope(
            params.record_kind.as_deref(),
            params.source_id.as_deref(),
        )?;
        ensure_maintenance_not_cancelled(cancellation)?;
        let audit = self
            .audit_conversation_data_with_progress_and_cancellation(
                ConversationDataAuditParams {
                    source_id: params.source_id.clone(),
                    record_kind: params.record_kind.clone(),
                    include_resolved: false,
                },
                cancellation,
                &mut |current, total, note| {
                    on_progress(current.min(2), total.max(AUDIT_STAGE_COUNT), note)
                },
            )
            .await?;
        ensure_maintenance_not_cancelled(cancellation)?;
        if params.dry_run {
            return Ok(json!({
                "schema_version": AUDIT_SCHEMA_VERSION,
                "dry_run": true,
                "audit": audit,
                "backup": Value::Null,
                "resync": if params.resync { json!({ "planned": true }) } else { Value::Null },
                "applied": Value::Null,
                "rollback": Value::Null,
            }));
        }
        if !params.yes {
            return Err(AppError::Validation(
                "conversation.data.repair requires yes=true".to_string(),
            ));
        }

        on_progress(2, AUDIT_STAGE_COUNT, Some("backup".to_string()));
        ensure_maintenance_not_cancelled(cancellation)?;
        let backup = create_conversation_repair_backup(self)?;
        ensure_maintenance_not_cancelled(cancellation)?;

        let mut resync = Value::Null;
        if params.resync {
            on_progress(3, AUDIT_STAGE_COUNT, Some("resync".to_string()));
            ensure_maintenance_not_cancelled(cancellation)?;
            let source_id = params.source_id.clone();
            let record_kind = params.record_kind.clone();
            resync = self
                .sync_conversations_with_progress_and_cancellation(
                    ConversationSyncParams {
                        source_id,
                        adapter_id: None,
                        record_kind,
                        mode: ConversationSyncMode::Full,
                        dry_run: false,
                    },
                    cancellation,
                    &mut |completed: usize, total: usize, note: Option<String>| {
                        let stage = if total == 0 {
                            4
                        } else {
                            3 + ((completed.saturating_mul(2)) / total).min(2)
                        };
                        on_progress(stage, AUDIT_STAGE_COUNT, note);
                    },
                )
                .await
                .map(|value| json!(value))?;
            ensure_maintenance_not_cancelled(cancellation)?;
        }

        on_progress(5, AUDIT_STAGE_COUNT, Some("apply".to_string()));
        ensure_maintenance_not_cancelled(cancellation)?;
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let repair_record_kind = params.record_kind.clone();
        let repair_source_id = params.source_id.clone();
        let applied = apply_safe_conversation_repairs_sqlx(
            pool,
            tenant_id,
            repair_record_kind.as_deref(),
            repair_source_id.as_deref(),
            cancellation,
        )
        .await?;

        on_progress(6, AUDIT_STAGE_COUNT, Some("reindex".to_string()));
        ensure_maintenance_not_cancelled(cancellation)?;
        let index = self
            .rebuild_conversation_search_index_with_cancellation(cancellation)
            .await
            .map(|report| json!(report))?;

        on_progress(8, AUDIT_STAGE_COUNT, Some("verify".to_string()));
        ensure_maintenance_not_cancelled(cancellation)?;
        let verification_source_id = params.source_id.clone();
        let verification_record_kind = params.record_kind.clone();
        let verification = self
            .audit_conversation_data_with_progress_and_cancellation(
                ConversationDataAuditParams {
                    source_id: verification_source_id.clone(),
                    record_kind: verification_record_kind.clone(),
                    include_resolved: false,
                },
                cancellation,
                &mut |_, _, _| {},
            )
            .await?;
        let active_fingerprints = verification["issues"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|issue| {
                let category = issue["category"].as_str()?;
                let record_kind = issue["details"]["record_kind"].as_str().unwrap_or("all");
                Some(conversation_audit_fingerprint(
                    record_kind,
                    category,
                    audit_issue_source_scope(category, verification_source_id.as_deref()),
                ))
            })
            .collect::<HashSet<_>>();
        let resolution_source_id = verification_source_id.clone();
        let resolved = resolve_safe_conversation_audit_issues_sqlx(
            pool,
            tenant_id,
            verification_record_kind.as_deref(),
            resolution_source_id.as_deref(),
            &active_fingerprints,
            cancellation,
        )
        .await?;
        ensure_maintenance_not_cancelled(cancellation)?;
        let backup_path = backup
            .targets
            .first()
            .map(|target| target.backup_path.clone())
            .ok_or_else(|| {
                AppError::Validation(
                    "conversation repair backup did not produce a rollback target".to_string(),
                )
            })?;
        let rollback = json!({
            "backup_path": backup_path,
            "requires_app_restart": true,
            "operation": "conversation.data.rollback",
        });
        on_progress(
            AUDIT_STAGE_COUNT,
            AUDIT_STAGE_COUNT,
            Some("completed".to_string()),
        );
        Ok(json!({
            "schema_version": AUDIT_SCHEMA_VERSION,
            "dry_run": false,
            "audit": audit,
            "backup": backup,
            "resync": resync,
            "applied": applied,
            "index": index,
            "verification": verification,
            "resolved_audit_issues": resolved,
            "rollback": rollback,
        }))
    }

    pub(crate) async fn rollback_conversation_data(
        &self,
        params: ConversationDataRollbackParams,
    ) -> AppResult<Value> {
        let backup_path = PathBuf::from(params.backup_path.trim());
        if !backup_path.is_file() {
            return Err(AppError::NotFound(format!(
                "conversation backup does not exist: {}",
                backup_path.display()
            )));
        }
        if backup_path == self.db_path {
            return Err(AppError::Validation(
                "conversation backup must be different from the active database".to_string(),
            ));
        }
        let preview = json!({
            "dry_run": params.dry_run,
            "restored": false,
            "backup_path": backup_path,
            "database_path": self.db_path,
            "requires_app_restart": true,
            "operation": "copy backup over database after stopping AssetIWeave, then restart",
        });
        if params.dry_run {
            return Ok(preview);
        }
        if !params.yes {
            return Err(AppError::Validation(
                "conversation.data.rollback requires yes=true".to_string(),
            ));
        }

        let pool = self.pool();
        crate::backend::store::checkpoint_database_wal_sqlx(pool).await?;
        self.pool().close().await;
        std::fs::copy(&backup_path, &self.db_path).map_err(AppError::external)?;
        Ok(json!({
            "dry_run": false,
            "restored": true,
            "backup_path": backup_path,
            "database_path": self.db_path,
            "requires_app_restart": true,
        }))
    }
}

#[cfg(not(test))]
fn create_conversation_repair_backup(
    service: &AppService,
) -> AppResult<crate::backend::infrastructure::backup::DatabaseBackupReport> {
    let settings = service.app_settings_value();
    Ok(
        crate::backend::infrastructure::backup::backup_database_from_settings_value(
            &service.db_path,
            &settings,
        )?,
    )
}

#[cfg(test)]
fn create_conversation_repair_backup(
    service: &AppService,
) -> AppResult<crate::backend::infrastructure::backup::DatabaseBackupReport> {
    let backup_root = service
        .db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("conversation-repair-backups");
    Ok(
        crate::backend::infrastructure::backup::backup_database_to_directories(
            &service.db_path,
            &[backup_root],
        )?,
    )
}
