use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::memory::SourceAvailability;
use crate::backend::domain::{
    L3GlobalMemoryView, L3MemoryItemView, L3SourceReferenceView, MemoryItemCategory,
    MemoryItemStatus,
};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

pub(crate) async fn get_global_memory_l3_view(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Option<L3GlobalMemoryView>> {
    let state_row = sqlx::query(
        "SELECT last_successful_consolidation_at, revision_hash \
         FROM global_memory_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    // 加载所有 current 状态的 L3 条目 (M35-L3-06)
    let rows = sqlx::query(
        "SELECT mi.id as item_id, mi.updated_at, \
                mir.id as revision_id, mir.revision_number, mir.category, mir.status, \
                mir.title, mir.summary, mir.rationale \
         FROM memory_items mi \
         JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
         WHERE mi.tenant_id = ?1 AND mi.layer = 'l3' AND mi.lifecycle = 'current' \
         ORDER BY mir.occurred_at DESC, mi.id ASC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    if rows.is_empty() && state_row.is_none() {
        return Ok(None);
    }

    let mut items = Vec::new();
    for r in rows {
        let item_id: String = r.get("item_id");
        let revision_id: String = r.get("revision_id");
        let revision_number: i64 = r.get("revision_number");
        let category_raw: String = r.get("category");
        let status_raw: String = r.get("status");
        let title: String = r.get("title");
        let summary: String = r.get("summary");
        let rationale: String = r.get("rationale");
        let updated_at: String = r.get("updated_at");

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

        // 加载关联引用
        let ref_rows = sqlx::query(
            "SELECT source_id, session_id, reference_key, question_id, turn_id, part_id, node_id, availability, unavailable_reason \
             FROM memory_item_source_references \
             WHERE tenant_id = ?1 AND item_revision_id = ?2",
        )
        .bind(tenant_id)
        .bind(&revision_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;

        let mut source_refs = Vec::new();
        let mut has_available = false;
        let mut has_unavailable = false;

        for ref_r in ref_rows {
            let availability: String = ref_r.get("availability");
            let is_avail = availability == "available";
            if is_avail {
                has_available = true;
            } else {
                has_unavailable = true;
            }
            source_refs.push(L3SourceReferenceView {
                source_id: ref_r.get("source_id"),
                session_id: ref_r.get("session_id"),
                reference_key: ref_r.get("reference_key"),
                project_key: None,
                question_id: ref_r.get("question_id"),
                turn_id: ref_r.get("turn_id"),
                part_id: ref_r.get("part_id"),
                node_id: ref_r.get("node_id"),
                available: is_avail,
                unavailable_reason: ref_r.get("unavailable_reason"),
            });
        }

        let source_availability = if !source_refs.is_empty() && !has_available {
            SourceAvailability::Unavailable
        } else if has_available && has_unavailable {
            SourceAvailability::PartiallyUnavailable
        } else {
            SourceAvailability::Available
        };

        items.push(L3MemoryItemView {
            item_id,
            revision_id,
            revision_number,
            category,
            status,
            title,
            summary,
            rationale,
            lifecycle: "current".to_string(),
            source_availability,
            source_references: source_refs,
            updated_at,
        });
    }

    let (last_successful_consolidation_at, revision_hash) = match state_row {
        Some(r) => (
            r.get("last_successful_consolidation_at"),
            r.get::<Option<String>, _>("revision_hash")
                .unwrap_or_default(),
        ),
        None => (None, String::new()),
    };

    Ok(Some(L3GlobalMemoryView {
        tenant_id: tenant_id.to_string(),
        items,
        last_successful_consolidation_at,
        revision_hash,
    }))
}

/// 执行 Global Consolidation 协调流程 (M35-L3-01 ~ M35-L3-06)
///
/// 1. 评估 L3 候选；若 0 候选则立即退出（0 Agent 调用）
/// 2. 检查触发门禁（周级维护或阈值达到，或显式手动）
/// 3. 单 Tenant 互斥锁加锁
/// 4. 组装事实输入并计算指纹；若指纹未变则跳过（0 Agent 调用）
/// 5. 执行操作：Create, Revise, Supersede, Keep
/// 6. 单一原子事务提交更新；失败回滚保留当前 L3

/// 长期来源失效协调器 (M35-L3-04)
///
/// 当外部 session 删除、缺失、排除或 source 禁用时：
/// 1. 仅更新 `memory_item_source_references` 的 availability 为 'unavailable'，并记录 unavailable_reason。
/// 2. 已晋升的 L2/L3 长期知识条目继续存在（lifecycle 保持 'current'），绝不自动删除或退出。
/// 3. 未晋升的 L1 条目若所有引用失效，退出为 'retired'。
pub(crate) async fn reconcile_source_invalidation(
    pool: &SqlitePool,
    tenant_id: &str,
    now: DateTime<Utc>,
    excluded_source_ids: &[String],
    excluded_session_ids: &[String],
) -> AppResult<usize> {
    let now_str = now.to_rfc3339();
    let mut affected_total = 0usize;

    // 1. source 被禁用 (enabled = 0)
    let res = sqlx::query(
        "UPDATE memory_item_source_references \
         SET availability = 'unavailable', \
             unavailable_reason = 'source_disabled', \
             unavailable_at = COALESCE(unavailable_at, ?2) \
         WHERE tenant_id = ?1 \
           AND availability = 'available' \
           AND source_id IN (SELECT id FROM conversation_sources WHERE tenant_id = ?1 AND enabled = 0)",
    )
    .bind(tenant_id)
    .bind(&now_str)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    affected_total += res.rows_affected() as usize;

    // 2. session 缺失 (missing = 1)
    let res = sqlx::query(
        "UPDATE memory_item_source_references \
         SET availability = 'unavailable', \
             unavailable_reason = 'missing', \
             unavailable_at = COALESCE(unavailable_at, ?2) \
         WHERE tenant_id = ?1 \
           AND availability = 'available' \
           AND session_id IN (SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND missing = 1)",
    )
    .bind(tenant_id)
    .bind(&now_str)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    affected_total += res.rows_affected() as usize;

    // 3. source 或 session 已被删除
    let res = sqlx::query(
        "UPDATE memory_item_source_references \
         SET availability = 'unavailable', \
             unavailable_reason = 'deleted', \
             unavailable_at = COALESCE(unavailable_at, ?2) \
         WHERE tenant_id = ?1 \
           AND availability = 'available' \
           AND ( \
               source_id NOT IN (SELECT id FROM conversation_sources WHERE tenant_id = ?1) \
               OR session_id NOT IN (SELECT id FROM conversation_sessions WHERE tenant_id = ?1) \
           )",
    )
    .bind(tenant_id)
    .bind(&now_str)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    affected_total += res.rows_affected() as usize;

    // 4. 设置中排除的 source
    for excluded_src in excluded_source_ids {
        let res = sqlx::query(
            "UPDATE memory_item_source_references \
             SET availability = 'unavailable', \
                 unavailable_reason = 'excluded', \
                 unavailable_at = COALESCE(unavailable_at, ?2) \
             WHERE tenant_id = ?1 \
               AND availability = 'available' \
               AND source_id = ?3",
        )
        .bind(tenant_id)
        .bind(&now_str)
        .bind(excluded_src)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
        affected_total += res.rows_affected() as usize;
    }

    // 5. 设置中排除的 session
    for excluded_sess in excluded_session_ids {
        let res = sqlx::query(
            "UPDATE memory_item_source_references \
             SET availability = 'unavailable', \
                 unavailable_reason = 'excluded', \
                 unavailable_at = COALESCE(unavailable_at, ?2) \
             WHERE tenant_id = ?1 \
               AND availability = 'available' \
               AND session_id = ?3",
        )
        .bind(tenant_id)
        .bind(&now_str)
        .bind(excluded_sess)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
        affected_total += res.rows_affected() as usize;
    }

    // 6. M35-L3-04: 来源失效使未晋升 L1 退出 (layer = 'l1' AND lifecycle = 'current')
    // 注意：L2 / L3 条目绝不因来源不可用自动 retired 或删除！
    sqlx::query(
        "UPDATE memory_items \
         SET lifecycle = 'retired', \
             updated_at = ?2 \
         WHERE tenant_id = ?1 \
           AND layer = 'l1' \
           AND lifecycle = 'current' \
           AND NOT EXISTS ( \
               SELECT 1 \
               FROM memory_item_source_references sr \
               JOIN memory_item_revisions mir ON sr.item_revision_id = mir.id \
               WHERE mir.tenant_id = memory_items.tenant_id \
                 AND mir.item_id = memory_items.id \
                 AND sr.availability = 'available' \
           )",
    )
    .bind(tenant_id)
    .bind(&now_str)
    .execute(pool)
    .await
    .map_err(AppError::external)?;

    Ok(affected_total)
}
