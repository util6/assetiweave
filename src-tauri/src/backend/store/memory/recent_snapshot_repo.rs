use crate::backend::domain::memory::RecentSnapshotPublicationKind;
use crate::backend::store::{StoreError, StoreResult};
use sqlx::{Row, SqlitePool};

pub(crate) use super::recent_snapshot_fixtures::*;
pub(crate) use super::recent_snapshot_jobs::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentMemoryStateFact {
    pub(crate) has_active_job: bool,
    pub(crate) last_successful_snapshot_id: Option<String>,
    pub(crate) latest_attempt_task_id: Option<String>,
    pub(crate) latest_attempt_error_code: Option<String>,
    pub(crate) latest_attempt_error_message: Option<String>,
    pub(crate) latest_attempt_error_retryable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentSnapshotReferenceFact {
    pub(crate) item_revision_id: String,
    pub(crate) source_id: String,
    pub(crate) session_id: String,
    pub(crate) session_title: String,
    pub(crate) source_agent: String,
    pub(crate) last_activity_at: String,
    pub(crate) available: bool,
    pub(crate) unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentSnapshotItemFact {
    pub(crate) project_key: String,
    pub(crate) item_id: String,
    pub(crate) item_revision_id: String,
    pub(crate) display_date: Option<String>,
    pub(crate) category: String,
    pub(crate) status: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) rationale: Option<String>,
    pub(crate) recommendation_rank: Option<i64>,
    pub(crate) occurred_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentSnapshotProjectFact {
    pub(crate) project_key: String,
    pub(crate) project_title: String,
    pub(crate) project_path: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) no_material_change: bool,
    pub(crate) latest_activity_at: Option<String>,
    pub(crate) source_session_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentSnapshotFact {
    pub(crate) id: String,
    pub(crate) sequence: i64,
    pub(crate) target_watermark_utc: String,
    pub(crate) window_start_utc: String,
    pub(crate) window_end_utc: String,
    pub(crate) window_hours: i64,
    pub(crate) publication_kind: RecentSnapshotPublicationKind,
    pub(crate) reused_from_snapshot_id: Option<String>,
    pub(crate) content_generated_at: String,
    pub(crate) published_at: String,
    pub(crate) projects: Vec<RecentSnapshotProjectFact>,
    pub(crate) items: Vec<RecentSnapshotItemFact>,
    pub(crate) references: Vec<RecentSnapshotReferenceFact>,
}

pub(crate) async fn list_memory_project_paths_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Vec<String>> {
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
    .map_err(StoreError::external)
}

pub(crate) async fn load_recent_memory_state_fact_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<RecentMemoryStateFact> {
    let running_job_opt = sqlx::query(
        "SELECT id FROM recent_memory_jobs WHERE tenant_id = ?1 AND status IN ('queued', 'running') ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;

    let state_row_opt = sqlx::query(
        "SELECT last_successful_snapshot_id, latest_attempt_task_id, latest_attempt_error_code, latest_attempt_error_message, latest_attempt_error_retryable \
         FROM recent_memory_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;

    let retryable_raw: Option<i64> = state_row_opt
        .as_ref()
        .and_then(|row| row.get::<Option<i64>, _>("latest_attempt_error_retryable"));

    Ok(RecentMemoryStateFact {
        has_active_job: running_job_opt.is_some(),
        last_successful_snapshot_id: state_row_opt
            .as_ref()
            .and_then(|row| row.get("last_successful_snapshot_id")),
        latest_attempt_task_id: state_row_opt
            .as_ref()
            .and_then(|row| row.get("latest_attempt_task_id")),
        latest_attempt_error_code: state_row_opt
            .as_ref()
            .and_then(|row| row.get("latest_attempt_error_code")),
        latest_attempt_error_message: state_row_opt
            .as_ref()
            .and_then(|row| row.get("latest_attempt_error_message")),
        latest_attempt_error_retryable: retryable_raw.map(|val| val != 0),
    })
}

pub(crate) async fn load_recent_snapshot_fact_by_id_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    snapshot_id: &str,
) -> StoreResult<Option<RecentSnapshotFact>> {
    let snapshot_row_opt = sqlx::query(
        "SELECT id, sequence, target_watermark_utc, window_start_utc, window_end_utc, window_hours, \
         publication_kind, reused_from_snapshot_id, content_generated_at, published_at \
         FROM recent_memory_snapshots WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(snapshot_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;

    let snapshot_row = match snapshot_row_opt {
        Some(row) => row,
        None => return Ok(None),
    };

    let pub_kind_str: String = snapshot_row.get("publication_kind");
    let publication_kind = match pub_kind_str.as_str() {
        "reused" => RecentSnapshotPublicationKind::Reused,
        _ => RecentSnapshotPublicationKind::Generated,
    };

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
    .map_err(StoreError::external)?;

    let projects = project_rows
        .into_iter()
        .map(|p_row| {
            let no_material_change_int: i64 = p_row.get("no_material_change");
            RecentSnapshotProjectFact {
                project_key: p_row.get("project_key"),
                project_title: p_row.get("project_title"),
                project_path: p_row.get("project_path"),
                summary: p_row.get("summary"),
                no_material_change: no_material_change_int == 1,
                latest_activity_at: p_row.get("latest_activity_at"),
                source_session_count: p_row.get("source_session_count"),
            }
        })
        .collect();

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
    .map_err(StoreError::external)?;

    let items = item_rows
        .into_iter()
        .map(|row| RecentSnapshotItemFact {
            project_key: row.get("project_key"),
            item_id: row.get("item_id"),
            item_revision_id: row.get("item_revision_id"),
            display_date: row.get("display_date"),
            category: row.get("category"),
            status: row.get("status"),
            title: row.get("title"),
            summary: row.get("summary"),
            rationale: row.get("rationale"),
            recommendation_rank: row.get("recommendation_rank"),
            occurred_at: row.get("occurred_at"),
        })
        .collect();

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
    .map_err(StoreError::external)?;

    let references = ref_rows
        .into_iter()
        .map(|r| {
            let availability_str: String = r.get("availability");
            RecentSnapshotReferenceFact {
                item_revision_id: r.get("item_revision_id"),
                source_id: r.get("source_id"),
                session_id: r.get("session_id"),
                session_title: r.get("session_title"),
                source_agent: r.get("source_agent"),
                last_activity_at: r.get("last_activity_at"),
                available: availability_str == "available",
                unavailable_reason: r.get("unavailable_reason"),
            }
        })
        .collect();

    Ok(Some(RecentSnapshotFact {
        id: snapshot_row.get("id"),
        sequence: snapshot_row.get("sequence"),
        target_watermark_utc: snapshot_row.get("target_watermark_utc"),
        window_start_utc: snapshot_row.get("window_start_utc"),
        window_end_utc: snapshot_row.get("window_end_utc"),
        window_hours: snapshot_row.get("window_hours"),
        publication_kind,
        reused_from_snapshot_id: snapshot_row.get("reused_from_snapshot_id"),
        content_generated_at: snapshot_row.get("content_generated_at"),
        published_at: snapshot_row.get("published_at"),
        projects,
        items,
        references,
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
) -> StoreResult<Option<RecentSnapshotMetadata>> {
    let row_opt = sqlx::query(
        "SELECT id, sequence, target_watermark_utc, content_fingerprint, target_fingerprint, \
         content_generated_at, publication_kind, reused_from_snapshot_id \
         FROM recent_memory_snapshots WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(snapshot_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;

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
#[cfg(test)]
#[path = "recent_snapshot_repo_tests.rs"]
mod tests;
