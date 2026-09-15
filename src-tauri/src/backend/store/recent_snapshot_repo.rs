use crate::backend::dto::{
    RecentMemoryErrorView, RecentMemoryItemView, RecentMemorySnapshotView, RecentMemoryStateView,
    RecentMemoryStatus, RecentProjectView, RecentSessionReferenceView,
    RecentSnapshotPublicationKind, SourceAvailability,
};
use crate::backend::runtime::{AppError, AppResult};
use sqlx::{Row, SqlitePool};

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
