use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};
impl AppService {
    pub(crate) async fn remove_conflicting_recent_snapshots(
        conn: &mut sqlx::SqliteConnection,
        tenant_id: &str,
        target_watermark_str: &str,
        window_hours: i64,
        exclude_snapshot_id: Option<&str>,
    ) -> AppResult<()> {
        let existing_snapshot_ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM recent_memory_snapshots \
             WHERE tenant_id = ?1 AND target_watermark_utc = ?2 AND window_hours = ?3 AND contract_version = 'memory.contract.v2'",
        )
        .bind(tenant_id)
        .bind(target_watermark_str)
        .bind(window_hours)
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::external)?;

        for old_id in &existing_snapshot_ids {
            if let Some(exclude) = exclude_snapshot_id {
                if old_id == exclude {
                    continue;
                }
            }

            sqlx::query(
                "UPDATE recent_memory_state SET last_successful_snapshot_id = NULL \
                 WHERE tenant_id = ?1 AND last_successful_snapshot_id = ?2",
            )
            .bind(tenant_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::external)?;

            sqlx::query(
                "UPDATE recent_memory_snapshots SET reused_from_snapshot_id = NULL \
                 WHERE tenant_id = ?1 AND reused_from_snapshot_id = ?2",
            )
            .bind(tenant_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::external)?;

            sqlx::query(
                "DELETE FROM memory_promotion_observations WHERE tenant_id = ?1 AND snapshot_id = ?2",
            )
            .bind(tenant_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::external)?;

            sqlx::query(
                "DELETE FROM recent_memory_snapshot_items WHERE tenant_id = ?1 AND snapshot_id = ?2",
            )
            .bind(tenant_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::external)?;

            sqlx::query(
                "DELETE FROM recent_memory_snapshot_projects WHERE tenant_id = ?1 AND snapshot_id = ?2",
            )
            .bind(tenant_id)
            .bind(old_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::external)?;

            sqlx::query("DELETE FROM recent_memory_snapshots WHERE tenant_id = ?1 AND id = ?2")
                .bind(tenant_id)
                .bind(old_id)
                .execute(&mut *conn)
                .await
                .map_err(AppError::external)?;
        }

        Ok(())
    }
}
