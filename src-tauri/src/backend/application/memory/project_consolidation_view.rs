use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::memory::SourceAvailability;
use crate::backend::domain::{
    L2MemoryItemView, L2ProjectMemoryView, L2SourceAvailabilityItem, L2SourceReferenceView,
    L2SupersededIndexItem, MemoryItemCategory, MemoryItemStatus,
};
use sqlx::{Row, SqlitePool};

pub(crate) async fn load_l2_items(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
) -> AppResult<Vec<L2MemoryItemView>> {
    let rows = sqlx::query(
        "SELECT mi.id as item_id, mi.lifecycle, mi.updated_at, \
                mir.id as revision_id, mir.revision_number, mir.category, mir.status, \
                mir.title, mir.summary, mir.rationale \
         FROM memory_items mi \
         JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
         WHERE mi.tenant_id = ?1 AND mi.project_key = ?2 AND mi.layer = 'l2' AND mi.lifecycle = 'current' \
         ORDER BY mi.created_at ASC",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    let mut items = Vec::new();
    for row in rows {
        let item_id: String = row.get("item_id");
        let revision_id: String = row.get("revision_id");
        let revision_number: i64 = row.get("revision_number");
        let category_raw: String = row.get("category");
        let status_raw: String = row.get("status");
        let title: String = row.get("title");
        let summary: String = row.get("summary");
        let rationale: String = row.get("rationale");
        let lifecycle: String = row.get("lifecycle");
        let updated_at: String = row.get("updated_at");

        let category = match category_raw.as_str() {
            "decision" => MemoryItemCategory::Decision,
            "research" => MemoryItemCategory::Research,
            "verification" => MemoryItemCategory::Verification,
            "blocker" => MemoryItemCategory::Blocker,
            "follow_up" => MemoryItemCategory::FollowUp,
            _ => MemoryItemCategory::Progress,
        };

        let status = match status_raw.as_str() {
            "blocked" => MemoryItemStatus::Blocked,
            "waiting" => MemoryItemStatus::Waiting,
            "completed" => MemoryItemStatus::Completed,
            "verified" => MemoryItemStatus::Verified,
            "abandoned" => MemoryItemStatus::Abandoned,
            "superseded" => MemoryItemStatus::Superseded,
            _ => MemoryItemStatus::Active,
        };

        // 查询引用
        let ref_rows = sqlx::query(
            "SELECT source_id, session_id, reference_key, availability, unavailable_reason \
             FROM memory_item_source_references \
             WHERE tenant_id = ?1 AND item_revision_id = ?2",
        )
        .bind(tenant_id)
        .bind(&revision_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;

        let mut source_references = Vec::new();
        let mut available_count = 0;
        let total_count = ref_rows.len();

        for r in ref_rows {
            let availability_str: String = r.get("availability");
            let is_avail = availability_str == "available";
            if is_avail {
                available_count += 1;
            }
            source_references.push(L2SourceReferenceView {
                source_id: r.get("source_id"),
                session_id: r.get("session_id"),
                reference_key: r.get("reference_key"),
                available: is_avail,
                unavailable_reason: r.get("unavailable_reason"),
            });
        }

        let source_availability = if total_count == 0 || available_count == total_count {
            SourceAvailability::Available
        } else if available_count == 0 {
            SourceAvailability::Unavailable
        } else {
            SourceAvailability::PartiallyUnavailable
        };

        items.push(L2MemoryItemView {
            item_id,
            revision_id,
            revision_number,
            category,
            status,
            title,
            summary,
            rationale,
            lifecycle,
            source_availability,
            source_references,
            updated_at,
        });
    }

    Ok(items)
}

/// 加载 L2 项目完整视图
pub(crate) async fn load_l2_project_memory_view(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
) -> AppResult<Option<L2ProjectMemoryView>> {
    let items = load_l2_items(pool, tenant_id, project_key).await?;

    let state_row = sqlx::query(
        "SELECT project_path, last_successful_consolidation_at, revision_hash \
         FROM project_memory_state \
         WHERE tenant_id = ?1 AND project_key = ?2",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    if items.is_empty() && state_row.is_none() {
        return Ok(None);
    }

    let (project_path, last_successful_consolidation_at, revision_hash) = match state_row {
        Some(r) => (
            r.get("project_path"),
            r.get("last_successful_consolidation_at"),
            r.get::<Option<String>, _>("revision_hash")
                .unwrap_or_default(),
        ),
        None => (None, None, String::new()),
    };

    Ok(Some(L2ProjectMemoryView {
        project_key: project_key.to_string(),
        project_path,
        items,
        last_successful_consolidation_at,
        revision_hash,
    }))
}

pub(crate) async fn load_superseded_index(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
) -> AppResult<Vec<L2SupersededIndexItem>> {
    let rows = sqlx::query(
        "SELECT mis.superseded_item_id, mis.superseding_item_id, mis.reason \
         FROM memory_item_supersessions mis \
         JOIN memory_items mi ON mis.tenant_id = mi.tenant_id AND mis.superseded_item_id = mi.id \
         WHERE mis.tenant_id = ?1 AND mi.project_key = ?2",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    Ok(rows
        .into_iter()
        .map(|r| L2SupersededIndexItem {
            old_item_id: r.get("superseded_item_id"),
            superseding_item_id: r.get("superseding_item_id"),
            reason: r.get("reason"),
        })
        .collect())
}

pub(crate) async fn load_source_availability_summary(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
) -> AppResult<Vec<L2SourceAvailabilityItem>> {
    let rows = sqlx::query(
        "SELECT misr.reference_key, misr.session_id, misr.availability, misr.unavailable_reason \
         FROM memory_item_source_references misr \
         JOIN memory_item_revisions mir ON misr.tenant_id = mir.tenant_id AND misr.item_revision_id = mir.id \
         JOIN memory_items mi ON mir.tenant_id = mi.tenant_id AND mir.item_id = mi.id \
         WHERE misr.tenant_id = ?1 AND mi.project_key = ?2 AND mi.layer = 'l2' AND misr.availability = 'unavailable'",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let avail_str: String = r.get("availability");
            L2SourceAvailabilityItem {
                reference_key: r.get("reference_key"),
                session_id: r.get("session_id"),
                available: avail_str == "available",
                reason: r.get("unavailable_reason"),
            }
        })
        .collect())
}
