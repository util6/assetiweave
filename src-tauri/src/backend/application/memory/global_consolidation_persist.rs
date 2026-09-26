use crate::backend::application::memory::global_consolidation_candidates::{
    promotion_nomination_for_source_refs, resolve_global_reference,
};
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::{
    GlobalConsolidationOperation, L3CandidateReferenceView, L3PromotionCandidate,
    MemoryItemCategory, MemoryPromotionNomination,
};
use crate::backend::store;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

pub(crate) async fn apply_global_consolidation_patch(
    pool: &SqlitePool,
    tenant_id: &str,
    now: DateTime<Utc>,
    operations: Vec<GlobalConsolidationOperation>,
    candidates: &[L3PromotionCandidate],
    strict_reference_validation: bool,
    candidate_reference_map: &HashMap<String, L3CandidateReferenceView>,
    input_fingerprint: &str,
    job_lease: Option<(&str, &str)>,
) -> AppResult<()> {
    let now_str = now.to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;

    for op in operations {
        match op {
            GlobalConsolidationOperation::Create {
                category,
                title,
                statement,
                rationale,
                source_refs,
            } => {
                let promotion_nomination = if strict_reference_validation {
                    promotion_nomination_for_source_refs(&candidates, &source_refs)
                } else {
                    MemoryPromotionNomination::GlobalRule
                };
                let new_item_id = format!("mem-l3-{}", uuid::Uuid::new_v4());
                let new_rev_id = format!("rev-l3-{}", uuid::Uuid::new_v4());

                sqlx::query(
                    "INSERT INTO memory_items \
                     (tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                      first_seen_at, last_seen_at, created_at, updated_at) \
                     VALUES (?1, ?2, 'l3', NULL, ?3, 'current', ?4, ?4, ?4, ?4)",
                )
                .bind(tenant_id)
                .bind(&new_item_id)
                .bind(&new_rev_id)
                .bind(&now_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                let category_str = match category {
                    MemoryItemCategory::Decision => "decision",
                    MemoryItemCategory::Research => "research",
                    MemoryItemCategory::Verification => "verification",
                    MemoryItemCategory::Blocker => "blocker",
                    MemoryItemCategory::FollowUp => "follow_up",
                    MemoryItemCategory::Progress => "progress",
                };

                let mut hasher = Sha256::new();
                hasher.update(title.as_bytes());
                hasher.update(statement.as_bytes());
                let evidence_fingerprint = format!("{:x}", hasher.finalize());

                sqlx::query(
                    "INSERT INTO memory_item_revisions \
                     (tenant_id, id, item_id, revision_number, category, status, title, summary, \
                      rationale, recommendation_rank, promotion_nomination, occurred_at, \
                      evidence_fingerprint, generated_by_snapshot_id, supersedes_revision_id, created_at) \
                     VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, ?8, ?9, ?10, NULL, NULL, ?9)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category_str)
                .bind(&title)
                .bind(&statement)
                .bind(&rationale)
                .bind(promotion_nomination.as_str())
                .bind(&now_str)
                .bind(&evidence_fingerprint)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                for ref_key in source_refs {
                    let source_ref = resolve_global_reference(
                        &candidate_reference_map,
                        &ref_key,
                        strict_reference_validation,
                    )?;
                    let ref_id = format!("ref-l3-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references \
                         (tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                          question_id, turn_id, part_id, node_id, reference_key, source_revision, availability, created_at) \
                         VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'available', ?12)",
                    )
                    .bind(tenant_id)
                    .bind(&ref_id)
                    .bind(&new_rev_id)
                    .bind(&source_ref.source_id)
                    .bind(&source_ref.session_id)
                    .bind(&source_ref.question_id)
                    .bind(&source_ref.turn_id)
                    .bind(&source_ref.part_id)
                    .bind(&source_ref.node_id)
                    .bind(&source_ref.reference_key)
                    .bind(source_ref.source_revision)
                    .bind(&now_str)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
            GlobalConsolidationOperation::Revise {
                item_id,
                statement,
                rationale,
                source_refs,
            } => {
                let promotion_nomination = if strict_reference_validation {
                    promotion_nomination_for_source_refs(&candidates, &source_refs)
                } else {
                    MemoryPromotionNomination::GlobalRule
                };
                let item_row = sqlx::query(
                    "SELECT mi.current_revision_id, mir.revision_number, mir.category, mir.title \
                     FROM memory_items mi \
                     JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
                     WHERE mi.tenant_id = ?1 AND mi.id = ?2 AND mi.layer = 'l3' AND mi.lifecycle = 'current'",
                )
                .bind(tenant_id)
                .bind(&item_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(AppError::external)?;

                let prev_rev_id: String = item_row.get("current_revision_id");
                let prev_rev_number: i64 = item_row.get("revision_number");
                let category_str: String = item_row.get("category");
                let title: String = item_row.get("title");

                let new_rev_id = format!("rev-l3-{}", uuid::Uuid::new_v4());
                let new_rev_number = prev_rev_number + 1;

                let mut hasher = Sha256::new();
                hasher.update(title.as_bytes());
                hasher.update(statement.as_bytes());
                let evidence_fingerprint = format!("{:x}", hasher.finalize());

                sqlx::query(
                    "INSERT INTO memory_item_revisions \
                     (tenant_id, id, item_id, revision_number, category, status, title, summary, \
                      rationale, recommendation_rank, promotion_nomination, occurred_at, \
                      evidence_fingerprint, generated_by_snapshot_id, supersedes_revision_id, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?7, ?8, NULL, ?9, ?10, ?11, NULL, ?12, ?10)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&item_id)
                .bind(new_rev_number)
                .bind(&category_str)
                .bind(&title)
                .bind(&statement)
                .bind(&rationale)
                .bind(promotion_nomination.as_str())
                .bind(&now_str)
                .bind(&evidence_fingerprint)
                .bind(&prev_rev_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                sqlx::query(
                    "UPDATE memory_items \
                     SET current_revision_id = ?3, last_seen_at = ?4, updated_at = ?4 \
                     WHERE tenant_id = ?1 AND id = ?2",
                )
                .bind(tenant_id)
                .bind(&item_id)
                .bind(&new_rev_id)
                .bind(&now_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                for ref_key in source_refs {
                    let source_ref = resolve_global_reference(
                        &candidate_reference_map,
                        &ref_key,
                        strict_reference_validation,
                    )?;
                    let ref_id = format!("ref-l3-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references \
                         (tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                          question_id, turn_id, part_id, node_id, reference_key, source_revision, availability, created_at) \
                         VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'available', ?12)",
                    )
                    .bind(tenant_id)
                    .bind(&ref_id)
                    .bind(&new_rev_id)
                    .bind(&source_ref.source_id)
                    .bind(&source_ref.session_id)
                    .bind(&source_ref.question_id)
                    .bind(&source_ref.turn_id)
                    .bind(&source_ref.part_id)
                    .bind(&source_ref.node_id)
                    .bind(&source_ref.reference_key)
                    .bind(source_ref.source_revision)
                    .bind(&now_str)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
            GlobalConsolidationOperation::Supersede {
                old_item_id,
                replacement_title,
                replacement_statement,
                rationale,
                category,
                source_refs,
            } => {
                let promotion_nomination = if strict_reference_validation {
                    promotion_nomination_for_source_refs(&candidates, &source_refs)
                } else {
                    MemoryPromotionNomination::GlobalRule
                };
                // 标记旧条目为 superseded
                sqlx::query(
                    "UPDATE memory_items \
                     SET lifecycle = 'superseded', updated_at = ?3 \
                     WHERE tenant_id = ?1 AND id = ?2",
                )
                .bind(tenant_id)
                .bind(&old_item_id)
                .bind(&now_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 创建新条目
                let new_item_id = format!("mem-l3-{}", uuid::Uuid::new_v4());
                let new_rev_id = format!("rev-l3-{}", uuid::Uuid::new_v4());

                sqlx::query(
                    "INSERT INTO memory_items \
                     (tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                      first_seen_at, last_seen_at, created_at, updated_at) \
                     VALUES (?1, ?2, 'l3', NULL, ?3, 'current', ?4, ?4, ?4, ?4)",
                )
                .bind(tenant_id)
                .bind(&new_item_id)
                .bind(&new_rev_id)
                .bind(&now_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                let category_str = match category {
                    MemoryItemCategory::Decision => "decision",
                    MemoryItemCategory::Research => "research",
                    MemoryItemCategory::Verification => "verification",
                    MemoryItemCategory::Blocker => "blocker",
                    MemoryItemCategory::FollowUp => "follow_up",
                    MemoryItemCategory::Progress => "progress",
                };

                let mut hasher = Sha256::new();
                hasher.update(replacement_title.as_bytes());
                hasher.update(replacement_statement.as_bytes());
                let evidence_fingerprint = format!("{:x}", hasher.finalize());

                sqlx::query(
                    "INSERT INTO memory_item_revisions \
                     (tenant_id, id, item_id, revision_number, category, status, title, summary, \
                      rationale, recommendation_rank, promotion_nomination, occurred_at, \
                      evidence_fingerprint, generated_by_snapshot_id, supersedes_revision_id, created_at) \
                     VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, ?8, ?9, ?10, NULL, NULL, ?9)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category_str)
                .bind(&replacement_title)
                .bind(&replacement_statement)
                .bind(&rationale)
                .bind(promotion_nomination.as_str())
                .bind(&now_str)
                .bind(&evidence_fingerprint)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 记录 supersession 关系 (M35-L3-05)
                let super_id = format!("super-{}", uuid::Uuid::new_v4());
                sqlx::query(
                    "INSERT INTO memory_item_supersessions \
                     (tenant_id, id, superseded_item_id, superseding_item_id, reason, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .bind(tenant_id)
                .bind(&super_id)
                .bind(&old_item_id)
                .bind(&new_item_id)
                .bind(&rationale)
                .bind(&now_str)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                for ref_key in source_refs {
                    let source_ref = resolve_global_reference(
                        &candidate_reference_map,
                        &ref_key,
                        strict_reference_validation,
                    )?;
                    let ref_id = format!("ref-l3-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references \
                         (tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                          question_id, turn_id, part_id, node_id, reference_key, source_revision, availability, created_at) \
                         VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'available', ?12)",
                    )
                    .bind(tenant_id)
                    .bind(&ref_id)
                    .bind(&new_rev_id)
                    .bind(&source_ref.source_id)
                    .bind(&source_ref.session_id)
                    .bind(&source_ref.question_id)
                    .bind(&source_ref.turn_id)
                    .bind(&source_ref.part_id)
                    .bind(&source_ref.node_id)
                    .bind(&source_ref.reference_key)
                    .bind(source_ref.source_revision)
                    .bind(&now_str)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
            GlobalConsolidationOperation::Keep { .. } => {}
        }
    }

    // 计算新的 revision_hash
    let active_l3_revs: Vec<String> = sqlx::query_scalar(
        "SELECT mir.id \
         FROM memory_items mi \
         JOIN memory_item_revisions mir ON mi.current_revision_id = mir.id \
         WHERE mi.tenant_id = ?1 AND mi.layer = 'l3' AND mi.lifecycle = 'current' \
         ORDER BY mir.occurred_at DESC, mi.id ASC",
    )
    .bind(tenant_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(AppError::external)?;

    let mut hash_input = active_l3_revs.join(":");
    if hash_input.is_empty() {
        hash_input = format!("l3-empty-{}", tenant_id);
    }
    let mut hasher = Sha256::new();
    hasher.update(hash_input.as_bytes());
    let revision_hash = format!("{:x}", hasher.finalize());

    // 更新 global_memory_state 指针
    sqlx::query(
        "INSERT INTO global_memory_state \
         (tenant_id, last_successful_consolidation_at, last_input_fingerprint, revision_hash, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?2, ?2) \
         ON CONFLICT(tenant_id) DO UPDATE SET \
            last_successful_consolidation_at = excluded.last_successful_consolidation_at, \
            last_input_fingerprint = excluded.last_input_fingerprint, \
            revision_hash = excluded.revision_hash, \
            updated_at = excluded.updated_at",
    )
    .bind(tenant_id)
    .bind(&now_str)
    .bind(&input_fingerprint)
    .bind(&revision_hash)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    if let Some((job_id, ownership_token)) = job_lease {
        let completed = store::complete_memory_maintenance_job_tx(
            &mut tx,
            tenant_id,
            job_id,
            ownership_token,
            &now_str,
        )
        .await?;
        if !completed {
            return Err(AppError::Conflict(
                "memory maintenance lease is no longer owned".to_string(),
            ));
        }
    }

    tx.commit().await.map_err(AppError::external)?;

    Ok(())
}
