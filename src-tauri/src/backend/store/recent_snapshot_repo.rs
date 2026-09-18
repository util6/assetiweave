use crate::backend::dto::{
    RecentMemoryErrorView, RecentMemoryItemView, RecentMemorySnapshotView, RecentMemoryStateView,
    RecentMemoryStatus, RecentProjectView, RecentSessionReferenceView,
    RecentSnapshotPublicationKind, SourceAvailability,
};
use crate::backend::runtime::{AppError, AppResult};
use sqlx::{Row, SqlitePool};

pub(crate) const MAX_RECENT_MEMORY_JOB_RETRIES: i64 = 5;
pub(crate) const RECENT_MEMORY_JOB_LEASE_SECONDS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentMemoryJob {
    pub(crate) tenant_id: String,
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) ownership_token: Option<String>,
    pub(crate) lease_expires_at: Option<String>,
    pub(crate) heartbeat_at: Option<String>,
    pub(crate) attempt_count: i64,
    pub(crate) retry_count: i64,
    pub(crate) retry_at: Option<String>,
    pub(crate) target_watermark_utc: String,
    pub(crate) window_hours: i64,
    pub(crate) target_fingerprint: String,
    pub(crate) content_fingerprint: String,
    pub(crate) work_order_json: String,
    pub(crate) last_error_code: Option<String>,
    pub(crate) last_error_message: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

pub(crate) async fn enqueue_recent_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    target_watermark_utc: &str,
    window_hours: i64,
    target_fingerprint: &str,
    content_fingerprint: &str,
    work_order_json: &str,
    now: &str,
) -> AppResult<String> {
    sqlx::query(
        "INSERT OR IGNORE INTO recent_memory_jobs (\
            tenant_id, id, status, target_watermark_utc, window_hours, target_fingerprint, \
            content_fingerprint, work_order_json, created_at, updated_at\
         ) VALUES (?1, ?2, 'queued', ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
    )
    .bind(tenant_id)
    .bind(job_id)
    .bind(target_watermark_utc)
    .bind(window_hours)
    .bind(target_fingerprint)
    .bind(content_fingerprint)
    .bind(work_order_json)
    .bind(now)
    .execute(pool)
    .await
    .map_err(AppError::external)?;

    sqlx::query_scalar(
        "SELECT id FROM recent_memory_jobs WHERE tenant_id = ?1 AND target_watermark_utc = ?2 \
         AND window_hours = ?3 AND target_fingerprint = ?4 AND content_fingerprint = ?5 \
         ORDER BY created_at ASC, id ASC LIMIT 1",
    )
    .bind(tenant_id)
    .bind(target_watermark_utc)
    .bind(window_hours)
    .bind(target_fingerprint)
    .bind(content_fingerprint)
    .fetch_one(pool)
    .await
    .map_err(AppError::external)
}

pub(crate) async fn load_recent_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
) -> AppResult<Option<RecentMemoryJob>> {
    let row = sqlx::query(
        "SELECT tenant_id, id, status, ownership_token, lease_expires_at, heartbeat_at, \
         attempt_count, retry_count, retry_at, target_watermark_utc, window_hours, \
         target_fingerprint, content_fingerprint, work_order_json, last_error_code, \
         last_error_message, started_at, finished_at, created_at, updated_at \
         FROM recent_memory_jobs WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    Ok(row.map(|row| RecentMemoryJob {
        tenant_id: row.get("tenant_id"),
        id: row.get("id"),
        status: row.get("status"),
        ownership_token: row.get("ownership_token"),
        lease_expires_at: row.get("lease_expires_at"),
        heartbeat_at: row.get("heartbeat_at"),
        attempt_count: row.get("attempt_count"),
        retry_count: row.get("retry_count"),
        retry_at: row.get("retry_at"),
        target_watermark_utc: row.get("target_watermark_utc"),
        window_hours: row.get("window_hours"),
        target_fingerprint: row.get("target_fingerprint"),
        content_fingerprint: row.get("content_fingerprint"),
        work_order_json: row.get("work_order_json"),
        last_error_code: row.get("last_error_code"),
        last_error_message: row.get("last_error_message"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }))
}

pub(crate) async fn recover_expired_recent_memory_leases_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE recent_memory_jobs SET \
            status = CASE WHEN retry_count + 1 >= ?1 THEN 'failed' ELSE 'queued' END, \
            ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, \
            retry_count = retry_count + 1, \
            retry_at = CASE WHEN retry_count + 1 >= ?1 THEN NULL ELSE ?2 END, \
            last_error_code = 'LEASE_EXPIRED', last_error_message = 'recent memory job lease expired', \
            finished_at = CASE WHEN retry_count + 1 >= ?1 THEN ?2 ELSE NULL END, updated_at = ?2 \
         WHERE tenant_id = ?3 AND status = 'running' AND (lease_expires_at IS NULL OR lease_expires_at <= ?2)",
    )
    .bind(MAX_RECENT_MEMORY_JOB_RETRIES)
    .bind(now)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn list_recent_memory_job_ids_for_scheduler_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    now: &str,
    limit: i64,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar(
        "SELECT id FROM recent_memory_jobs \
         WHERE tenant_id = ?1 AND retry_count < ?2 AND \
         (status = 'queued' OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?3)) \
         ORDER BY created_at ASC, id ASC LIMIT ?4",
    )
    .bind(tenant_id)
    .bind(MAX_RECENT_MEMORY_JOB_RETRIES)
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)
}

pub(crate) async fn claim_recent_memory_job_with_lease_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    now: &str,
) -> AppResult<bool> {
    let lease_expires_at = chrono::DateTime::parse_from_rfc3339(now)
        .map(|value| {
            (value.with_timezone(&chrono::Utc)
                + chrono::Duration::seconds(RECENT_MEMORY_JOB_LEASE_SECONDS))
            .to_rfc3339()
        })
        .map_err(|_| AppError::Validation("invalid recent memory lease timestamp".to_string()))?;
    let result = sqlx::query(
        "UPDATE recent_memory_jobs SET status = 'running', ownership_token = ?1, \
         lease_expires_at = ?2, heartbeat_at = ?3, started_at = COALESCE(started_at, ?3), \
         attempt_count = attempt_count + 1, updated_at = ?3 \
         WHERE tenant_id = ?4 AND id = ?5 AND retry_count < ?6 AND \
         (status = 'queued' OR (status = 'failed' AND retry_at IS NOT NULL AND retry_at <= ?3))",
    )
    .bind(ownership_token)
    .bind(&lease_expires_at)
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .bind(MAX_RECENT_MEMORY_JOB_RETRIES)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn heartbeat_recent_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    now: &str,
) -> AppResult<bool> {
    let lease_expires_at = chrono::DateTime::parse_from_rfc3339(now)
        .map(|value| {
            (value.with_timezone(&chrono::Utc)
                + chrono::Duration::seconds(RECENT_MEMORY_JOB_LEASE_SECONDS))
            .to_rfc3339()
        })
        .map_err(|_| AppError::Validation("invalid recent memory lease timestamp".to_string()))?;
    let result = sqlx::query(
        "UPDATE recent_memory_jobs SET heartbeat_at = ?1, lease_expires_at = ?2, updated_at = ?1 \
         WHERE tenant_id = ?3 AND id = ?4 AND status = 'running' AND ownership_token = ?5 \
         AND lease_expires_at > ?1",
    )
    .bind(now)
    .bind(&lease_expires_at)
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn finish_recent_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    ownership_token: &str,
    status: &str,
    error_code: Option<&str>,
    error_message: Option<&str>,
    retryable: bool,
    now: &str,
) -> AppResult<bool> {
    let row = sqlx::query(
        "SELECT retry_count FROM recent_memory_jobs WHERE tenant_id = ?1 AND id = ?2 \
         AND status = 'running' AND ownership_token = ?3",
    )
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;
    let Some(row) = row else {
        return Ok(false);
    };
    let retry_count: i64 = row.get("retry_count");
    let can_retry = retryable && retry_count < MAX_RECENT_MEMORY_JOB_RETRIES;
    let retry_at = if can_retry {
        Some(
            (chrono::DateTime::parse_from_rfc3339(now)
                .map_err(|_| {
                    AppError::Validation("invalid recent memory retry timestamp".to_string())
                })?
                .with_timezone(&chrono::Utc)
                + chrono::Duration::seconds(match retry_count {
                    0 => 5,
                    1 => 15,
                    2 => 30,
                    3 => 60,
                    _ => 120,
                }))
            .to_rfc3339(),
        )
    } else {
        None
    };
    let final_status = if can_retry { "failed" } else { status };
    let updated = sqlx::query(
        "UPDATE recent_memory_jobs SET status = ?1, last_error_code = ?2, last_error_message = ?3, \
         finished_at = CASE WHEN ?4 THEN NULL ELSE ?5 END, retry_at = ?6, ownership_token = NULL, \
         lease_expires_at = NULL, heartbeat_at = NULL, retry_count = retry_count + CASE WHEN ?4 THEN 1 ELSE 0 END, \
         updated_at = ?5 WHERE tenant_id = ?7 AND id = ?8 AND status = 'running' AND ownership_token = ?9",
    )
    .bind(final_status)
    .bind(error_code)
    .bind(error_message)
    .bind(can_retry)
    .bind(now)
    .bind(retry_at)
    .bind(tenant_id)
    .bind(job_id)
    .bind(ownership_token)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(updated.rows_affected() == 1)
}

pub(crate) async fn retry_recent_memory_job_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "UPDATE recent_memory_jobs SET status = 'queued', retry_count = 0, retry_at = NULL, last_error_code = NULL, last_error_message = NULL, finished_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status = 'failed'",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(result.rows_affected() == 1)
}

/// An explicit rebuild is a new user intent, not another automatic retry.
/// It may therefore reopen any terminal job while resetting the bounded retry
/// budget. Automatic reconciliation never calls this function.
pub(crate) async fn restart_recent_memory_job_for_rebuild_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    job_id: &str,
    now: &str,
) -> AppResult<bool> {
    let result = sqlx::query(
        "UPDATE recent_memory_jobs SET status = 'queued', retry_count = 0, retry_at = NULL, last_error_code = NULL, last_error_message = NULL, started_at = NULL, finished_at = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3 AND status IN ('succeeded', 'reused', 'failed', 'canceled', 'stale')",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(job_id)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn record_recent_memory_attempt_task_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    task_id: &str,
    now: &str,
) -> AppResult<()> {
    let state_id = format!("state-{tenant_id}");
    sqlx::query(
        "INSERT INTO recent_memory_state (\
            tenant_id, id, last_successful_snapshot_id, latest_attempt_task_id, \
            latest_attempt_error_code, latest_attempt_error_message, created_at, updated_at\
         ) VALUES (?1, ?2, NULL, ?3, NULL, NULL, ?4, ?4) \
         ON CONFLICT (tenant_id) DO UPDATE SET \
            latest_attempt_task_id = excluded.latest_attempt_task_id, \
            latest_attempt_error_code = NULL, \
            latest_attempt_error_message = NULL, \
            updated_at = excluded.updated_at",
    )
    .bind(tenant_id)
    .bind(&state_id)
    .bind(task_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn list_memory_v2_project_paths_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar(
        "SELECT project_path FROM project_memory_state \
         WHERE tenant_id = ?1 AND project_path IS NOT NULL AND trim(project_path) <> '' \
         UNION \
         SELECT project_path FROM recent_memory_snapshot_projects \
         WHERE tenant_id = ?1 AND project_path IS NOT NULL AND trim(project_path) <> '' \
         ORDER BY project_path",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct FixtureSessionReferenceInput {
    pub(crate) id: String,
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) session_title: String,
    pub(crate) source_agent: String,
    pub(crate) last_activity_at: String,
    pub(crate) reference_key: String,
    pub(crate) source_revision: i64,
    pub(crate) availability: SourceAvailability,
    pub(crate) unavailable_reason: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct FixtureSnapshotItemInput {
    pub(crate) item_id: String,
    pub(crate) revision_id: String,
    pub(crate) category: String,
    pub(crate) status: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) rationale: String,
    pub(crate) occurred_at: String,
    pub(crate) recommendation_rank: Option<i64>,
    pub(crate) evidence_fingerprint: String,
    pub(crate) display_date: String,
    pub(crate) sort_order: i64,
    pub(crate) session_references: Vec<FixtureSessionReferenceInput>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct FixtureSnapshotProjectInput {
    pub(crate) project_key: String,
    pub(crate) project_title: String,
    pub(crate) project_path: Option<String>,
    pub(crate) summary: String,
    pub(crate) no_material_change: bool,
    pub(crate) latest_activity_at: String,
    pub(crate) source_session_count: i64,
    pub(crate) sort_order: i64,
    pub(crate) items: Vec<FixtureSnapshotItemInput>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct FixtureRecentSnapshotInput {
    pub(crate) tenant_id: String,
    pub(crate) snapshot_id: String,
    pub(crate) sequence: i64,
    pub(crate) target_watermark_utc: String,
    pub(crate) local_watermark_date: String,
    pub(crate) local_watermark_time: String,
    pub(crate) timezone_offset_minutes: i64,
    pub(crate) window_hours: i64,
    pub(crate) window_start_utc: String,
    pub(crate) window_end_utc: String,
    pub(crate) publication_kind: RecentSnapshotPublicationKind,
    pub(crate) reused_from_snapshot_id: Option<String>,
    pub(crate) target_fingerprint: String,
    pub(crate) content_fingerprint: String,
    pub(crate) generation_skill_asset_id: Option<String>,
    pub(crate) generation_skill_revision: Option<i64>,
    pub(crate) generation_skill_content_hash: Option<String>,
    pub(crate) contract_version: String,
    pub(crate) budget_policy_version: String,
    pub(crate) projection_policy_version: String,
    pub(crate) content_generated_at: String,
    pub(crate) published_at: String,
    pub(crate) projects: Vec<FixtureSnapshotProjectInput>,
}

pub(crate) async fn load_recent_memory_state_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<RecentMemoryStateView> {
    // 1. Check if there is a running Job for this tenant
    let running_job_opt = sqlx::query(
        "SELECT id FROM recent_memory_jobs WHERE tenant_id = ?1 AND status IN ('queued', 'running') ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    // 2. Check latest state pointer
    let state_row_opt = sqlx::query(
        "SELECT last_successful_snapshot_id, latest_attempt_task_id, latest_attempt_error_code, latest_attempt_error_message \
         FROM recent_memory_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    let last_successful_snapshot_id = state_row_opt
        .as_ref()
        .and_then(|row| row.get::<Option<String>, _>("last_successful_snapshot_id"));

    // 3. If there is a snapshot id, load snapshot view
    let snapshot_view = if let Some(ref snapshot_id) = last_successful_snapshot_id {
        load_recent_snapshot_by_id_sqlx(pool, tenant_id, snapshot_id).await?
    } else {
        None
    };

    // 4. Check for latest attempt error
    let latest_attempt_error = state_row_opt.as_ref().and_then(|row| {
        let code = row.get::<Option<String>, _>("latest_attempt_error_code")?;
        let message = row.get::<Option<String>, _>("latest_attempt_error_message")?;
        Some(RecentMemoryErrorView {
            code,
            message,
            retryable: false,
        })
    });

    let latest_attempt_task_id = state_row_opt
        .as_ref()
        .and_then(|row| row.get::<Option<String>, _>("latest_attempt_task_id"));

    // 5. Compute status:
    // status: empty | generating | ready | update_failed
    let status = if running_job_opt.is_some() {
        RecentMemoryStatus::Generating
    } else if latest_attempt_error.is_some() {
        RecentMemoryStatus::UpdateFailed
    } else if snapshot_view.is_some() {
        RecentMemoryStatus::Ready
    } else {
        RecentMemoryStatus::Empty
    };

    Ok(RecentMemoryStateView {
        status,
        snapshot: snapshot_view,
        latest_attempt_task_id,
        latest_attempt_error,
    })
}

pub(crate) async fn load_recent_snapshot_by_id_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    snapshot_id: &str,
) -> AppResult<Option<RecentMemorySnapshotView>> {
    let snapshot_row_opt = sqlx::query(
        "SELECT id, sequence, target_watermark_utc, window_start_utc, window_end_utc, window_hours, \
         publication_kind, reused_from_snapshot_id, content_generated_at, published_at \
         FROM recent_memory_snapshots WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(snapshot_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    let snapshot_row = match snapshot_row_opt {
        Some(row) => row,
        None => return Ok(None),
    };

    let pub_kind_str: String = snapshot_row.get("publication_kind");
    let publication_kind = match pub_kind_str.as_str() {
        "reused" => RecentSnapshotPublicationKind::Reused,
        _ => RecentSnapshotPublicationKind::Generated,
    };

    // Load projects
    let project_rows = sqlx::query(
        "SELECT id, project_key, project_title, project_path, summary, no_material_change, \
         latest_activity_at, source_session_count \
         FROM recent_memory_snapshot_projects \
         WHERE tenant_id = ?1 AND snapshot_id = ?2 \
         ORDER BY sort_order ASC, id ASC",
    )
    .bind(tenant_id)
    .bind(snapshot_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    // Load items for this snapshot
    let item_rows = sqlx::query(
        "SELECT si.project_key, si.item_id, si.item_revision_id, si.display_date, \
                ir.category, ir.status, ir.title, ir.summary, ir.rationale, ir.recommendation_rank, ir.occurred_at \
         FROM recent_memory_snapshot_items si \
         JOIN memory_item_revisions ir ON ir.tenant_id = si.tenant_id AND ir.id = si.item_revision_id \
         WHERE si.tenant_id = ?1 AND si.snapshot_id = ?2 \
         ORDER BY si.sort_order ASC, si.id ASC",
    )
    .bind(tenant_id)
    .bind(snapshot_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    // Load all references for revisions in this snapshot
    let ref_rows = sqlx::query(
        "SELECT sr.item_revision_id, sr.source_id, sr.session_id, sr.availability, sr.unavailable_reason, \
                COALESCE(cs.title, sr.session_id) as session_title, \
                COALESCE(ca.name, cs.adapter_id, 'unknown') as source_agent, \
                COALESCE(cs.updated_at, cs.started_at, sr.created_at) as last_activity_at \
         FROM memory_item_source_references sr \
         JOIN recent_memory_snapshot_items si ON si.tenant_id = sr.tenant_id AND si.item_revision_id = sr.item_revision_id \
         LEFT JOIN conversation_sessions cs ON cs.tenant_id = sr.tenant_id AND cs.id = sr.session_id \
         LEFT JOIN conversation_adapters ca ON ca.tenant_id = cs.tenant_id AND ca.id = cs.adapter_id \
         WHERE sr.tenant_id = ?1 AND si.snapshot_id = ?2 \
         ORDER BY sr.node_order ASC, sr.id ASC",
    )
    .bind(tenant_id)
    .bind(snapshot_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    // Group references by item_revision_id
    let mut refs_by_revision: std::collections::HashMap<String, Vec<RecentSessionReferenceView>> =
        std::collections::HashMap::new();
    for r in ref_rows {
        let revision_id: String = r.get("item_revision_id");
        let availability_str: String = r.get("availability");
        let available = availability_str == "available";
        refs_by_revision
            .entry(revision_id)
            .or_default()
            .push(RecentSessionReferenceView {
                source_id: r.get("source_id"),
                session_id: r.get("session_id"),
                session_title: r.get("session_title"),
                source_agent: r.get("source_agent"),
                last_activity_at: r.get("last_activity_at"),
                available,
                unavailable_reason: r.get("unavailable_reason"),
            });
    }

    // Group items by project_key
    let mut items_by_project: std::collections::HashMap<String, Vec<RecentMemoryItemView>> =
        std::collections::HashMap::new();
    for row in item_rows {
        let project_key: String = row.get("project_key");
        let revision_id: String = row.get("item_revision_id");
        let refs = refs_by_revision.remove(&revision_id).unwrap_or_default();

        let source_availability = if refs.is_empty() {
            SourceAvailability::Available
        } else {
            let available_count = refs.iter().filter(|r| r.available).count();
            if available_count == refs.len() {
                SourceAvailability::Available
            } else if available_count == 0 {
                SourceAvailability::Unavailable
            } else {
                SourceAvailability::PartiallyUnavailable
            }
        };

        items_by_project
            .entry(project_key)
            .or_default()
            .push(RecentMemoryItemView {
                item_id: row.get("item_id"),
                revision_id,
                category: row.get("category"),
                status: row.get("status"),
                title: row.get("title"),
                summary: row.get("summary"),
                rationale: row.get("rationale"),
                occurred_at: row.get("occurred_at"),
                recommendation_rank: row.get("recommendation_rank"),
                source_availability,
                session_references: refs,
            });
    }

    let mut projects = Vec::new();
    for p_row in project_rows {
        let p_key: String = p_row.get("project_key");
        let items = items_by_project.remove(&p_key).unwrap_or_default();
        let no_material_change_int: i64 = p_row.get("no_material_change");
        projects.push(RecentProjectView {
            project_key: p_key,
            project_title: p_row.get("project_title"),
            project_path: p_row.get("project_path"),
            summary: p_row.get("summary"),
            no_material_change: no_material_change_int == 1,
            latest_activity_at: p_row.get("latest_activity_at"),
            source_session_count: p_row.get("source_session_count"),
            items,
        });
    }

    Ok(Some(RecentMemorySnapshotView {
        snapshot_id: snapshot_row.get("id"),
        sequence: snapshot_row.get("sequence"),
        target_watermark: snapshot_row.get("target_watermark_utc"),
        window_start: snapshot_row.get("window_start_utc"),
        window_end: snapshot_row.get("window_end_utc"),
        window_hours: snapshot_row.get("window_hours"),
        publication_kind,
        reused_from_snapshot_id: snapshot_row.get("reused_from_snapshot_id"),
        content_generated_at: snapshot_row.get("content_generated_at"),
        published_at: snapshot_row.get("published_at"),
        projects,
    }))
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct RecentSnapshotMetadata {
    pub(crate) id: String,
    pub(crate) sequence: i64,
    pub(crate) target_watermark_utc: String,
    pub(crate) content_fingerprint: String,
    pub(crate) target_fingerprint: String,
    pub(crate) content_generated_at: String,
    pub(crate) publication_kind: RecentSnapshotPublicationKind,
    pub(crate) reused_from_snapshot_id: Option<String>,
}

#[allow(dead_code)]
pub(crate) async fn load_recent_snapshot_meta_by_id_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    snapshot_id: &str,
) -> AppResult<Option<RecentSnapshotMetadata>> {
    let row_opt = sqlx::query(
        "SELECT id, sequence, target_watermark_utc, content_fingerprint, target_fingerprint, \
         content_generated_at, publication_kind, reused_from_snapshot_id \
         FROM recent_memory_snapshots WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(snapshot_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    Ok(row_opt.map(|row| {
        let pub_kind_str: String = row.get("publication_kind");
        let publication_kind = match pub_kind_str.as_str() {
            "reused" => RecentSnapshotPublicationKind::Reused,
            _ => RecentSnapshotPublicationKind::Generated,
        };
        RecentSnapshotMetadata {
            id: row.get("id"),
            sequence: row.get("sequence"),
            target_watermark_utc: row.get("target_watermark_utc"),
            content_fingerprint: row.get("content_fingerprint"),
            target_fingerprint: row.get("target_fingerprint"),
            content_generated_at: row.get("content_generated_at"),
            publication_kind,
            reused_from_snapshot_id: row.get("reused_from_snapshot_id"),
        }
    }))
}

#[allow(dead_code)]
pub(crate) async fn save_fixture_recent_snapshot_sqlx(
    pool: &SqlitePool,
    input: &FixtureRecentSnapshotInput,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;

    // 1. Insert snapshot
    let pub_kind = match input.publication_kind {
        RecentSnapshotPublicationKind::Generated => "generated",
        RecentSnapshotPublicationKind::Reused => "reused",
    };

    sqlx::query(
        "INSERT INTO recent_memory_snapshots (\
            tenant_id, id, sequence, target_watermark_utc, local_watermark_date, local_watermark_time, \
            timezone_offset_minutes, window_hours, window_start_utc, window_end_utc, publication_kind, \
            reused_from_snapshot_id, target_fingerprint, content_fingerprint, generation_skill_asset_id, \
            generation_skill_revision, generation_skill_content_hash, contract_version, budget_policy_version, \
            projection_policy_version, content_generated_at, published_at\
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
    )
    .bind(&input.tenant_id)
    .bind(&input.snapshot_id)
    .bind(input.sequence)
    .bind(&input.target_watermark_utc)
    .bind(&input.local_watermark_date)
    .bind(&input.local_watermark_time)
    .bind(input.timezone_offset_minutes)
    .bind(input.window_hours)
    .bind(&input.window_start_utc)
    .bind(&input.window_end_utc)
    .bind(pub_kind)
    .bind(&input.reused_from_snapshot_id)
    .bind(&input.target_fingerprint)
    .bind(&input.content_fingerprint)
    .bind(&input.generation_skill_asset_id)
    .bind(input.generation_skill_revision)
    .bind(&input.generation_skill_content_hash)
    .bind(&input.contract_version)
    .bind(&input.budget_policy_version)
    .bind(&input.projection_policy_version)
    .bind(&input.content_generated_at)
    .bind(&input.published_at)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    for project in &input.projects {
        let project_id = format!("{}-{}", input.snapshot_id, project.project_key);
        sqlx::query(
            "INSERT INTO recent_memory_snapshot_projects (\
                tenant_id, id, snapshot_id, project_key, project_title, project_path, summary, \
                no_material_change, latest_activity_at, source_session_count, sort_order\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .bind(&input.tenant_id)
        .bind(&project_id)
        .bind(&input.snapshot_id)
        .bind(&project.project_key)
        .bind(&project.project_title)
        .bind(&project.project_path)
        .bind(&project.summary)
        .bind(if project.no_material_change { 1 } else { 0 })
        .bind(&project.latest_activity_at)
        .bind(project.source_session_count)
        .bind(project.sort_order)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;

        for item in &project.items {
            // Ensure memory_item exists
            sqlx::query(
                "INSERT INTO memory_items (\
                    tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                    first_seen_at, last_seen_at, created_at, updated_at\
                 ) VALUES (?1, ?2, 'l1', ?3, ?4, 'current', ?5, ?5, ?5, ?5) \
                 ON CONFLICT (tenant_id, id) DO UPDATE SET \
                    current_revision_id = excluded.current_revision_id, \
                    last_seen_at = excluded.last_seen_at, \
                    updated_at = excluded.updated_at",
            )
            .bind(&input.tenant_id)
            .bind(&item.item_id)
            .bind(&project.project_key)
            .bind(&item.revision_id)
            .bind(&item.occurred_at)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;

            // Insert memory_item_revision
            sqlx::query(
                "INSERT INTO memory_item_revisions (\
                    tenant_id, id, item_id, revision_number, category, status, title, summary, \
                    rationale, recommendation_rank, promotion_nomination, occurred_at, \
                    evidence_fingerprint, generated_by_snapshot_id, created_at\
                 ) VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?8, ?9, 'none', ?10, ?11, ?12, ?10)",
            )
            .bind(&input.tenant_id)
            .bind(&item.revision_id)
            .bind(&item.item_id)
            .bind(&item.category)
            .bind(&item.status)
            .bind(&item.title)
            .bind(&item.summary)
            .bind(&item.rationale)
            .bind(item.recommendation_rank)
            .bind(&item.occurred_at)
            .bind(&item.evidence_fingerprint)
            .bind(&input.snapshot_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;

            // Insert snapshot_item membership
            let membership_id = format!("{}-{}", input.snapshot_id, item.item_id);
            sqlx::query(
                "INSERT INTO recent_memory_snapshot_items (\
                    tenant_id, id, snapshot_id, project_key, item_id, item_revision_id, display_date, sort_order\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(&input.tenant_id)
            .bind(&membership_id)
            .bind(&input.snapshot_id)
            .bind(&project.project_key)
            .bind(&item.item_id)
            .bind(&item.revision_id)
            .bind(&item.display_date)
            .bind(item.sort_order)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;

            // Insert references
            for r in &item.session_references {
                let avail = match r.availability {
                    SourceAvailability::Available => "available",
                    _ => "unavailable",
                };
                sqlx::query(
                    "INSERT INTO memory_item_source_references (\
                        tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                        reference_key, source_revision, availability, unavailable_reason, created_at\
                     ) VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )
                .bind(&input.tenant_id)
                .bind(&r.id)
                .bind(&item.revision_id)
                .bind(&r.source_id)
                .bind(&r.session_id)
                .bind(&r.reference_key)
                .bind(r.source_revision)
                .bind(avail)
                .bind(&r.unavailable_reason)
                .bind(&item.occurred_at)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;
            }
        }
    }

    // Update state pointer
    let state_id = format!("recent-state-{}", input.tenant_id);
    sqlx::query(
        "INSERT INTO recent_memory_state (\
            tenant_id, id, last_successful_snapshot_id, created_at, updated_at\
         ) VALUES (?1, ?2, ?3, ?4, ?4) \
         ON CONFLICT(tenant_id) DO UPDATE SET \
            last_successful_snapshot_id = excluded.last_successful_snapshot_id, \
            updated_at = excluded.updated_at",
    )
    .bind(&input.tenant_id)
    .bind(&state_id)
    .bind(&input.snapshot_id)
    .bind(&input.published_at)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    tx.commit().await.map_err(AppError::external)?;
    Ok(())
}

#[cfg(test)]
#[path = "recent_snapshot_repo_tests.rs"]
mod tests;
