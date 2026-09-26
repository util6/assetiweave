use crate::backend::domain::memory::{RecentSnapshotPublicationKind, SourceAvailability};
use crate::backend::store::{StoreError, StoreResult};
use sqlx::SqlitePool;
use uuid::Uuid;

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

pub(crate) async fn save_fixture_recent_snapshot_sqlx(
    pool: &SqlitePool,
    input: &FixtureRecentSnapshotInput,
) -> StoreResult<()> {
    let mut tx = pool.begin().await.map_err(StoreError::external)?;

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
    .map_err(StoreError::external)?;

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
        .map_err(StoreError::external)?;

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
            .map_err(StoreError::external)?;

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
            .map_err(StoreError::external)?;

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
            .map_err(StoreError::external)?;

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
                .map_err(StoreError::external)?;
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
    .map_err(StoreError::external)?;

    tx.commit().await.map_err(StoreError::external)?;
    Ok(())
}
