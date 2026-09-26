use crate::backend::application::memory::project_consolidation_candidates::{
    parse_promotion_nomination, promotion_nomination_for_source_refs, resolve_project_reference,
};
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::{
    L2PromotionCandidate, MemoryPromotionNomination, ProjectConsolidationOperation,
    ProjectConsolidationResult,
};
use crate::backend::store;
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

pub(crate) async fn apply_project_consolidation_patch(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
    project_path: Option<&str>,
    candidates: &[L2PromotionCandidate],
    consolidation_result: ProjectConsolidationResult,
    input_fingerprint: &str,
    job_lease: Option<(&str, &str)>,
    strict_reference_validation: bool,
) -> AppResult<()> {
    // Agent 只能引用本次 Work Order 通过准入的、仍可用的候选引用。
    // 这张映射同时保留真实 source/session locator，避免把 Agent 返回的
    // reference_key 当成可写入的来源身份。
    let candidate_reference_map: HashMap<_, _> = candidates
        .iter()
        .flat_map(|candidate| candidate.session_references.iter())
        .filter(|reference| reference.available)
        .map(|reference| (reference.reference_key.clone(), reference.clone()))
        .collect();

    // 6. 应用准入与单事务原子提交 (M35-L2-06: 失败保留 current L2)
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let now = Utc::now().to_rfc3339();

    for op in consolidation_result.operations {
        match op {
            ProjectConsolidationOperation::Create {
                category,
                title,
                statement,
                rationale,
                source_refs,
            } => {
                let promotion_nomination = promotion_nomination_for_source_refs(
                    &candidates,
                    &source_refs,
                    MemoryPromotionNomination::None,
                );
                let new_item_id = format!("item-l2-{}", uuid::Uuid::new_v4());
                let new_rev_id = format!("rev-l2-{}", uuid::Uuid::new_v4());

                // 写入 memory_items (layer = 'l2')
                sqlx::query(
                    "INSERT INTO memory_items (\
                        tenant_id, id, layer, project_key, current_revision_id, \
                        lifecycle, first_seen_at, last_seen_at, created_at, updated_at\
                     ) VALUES (?1, ?2, 'l2', ?3, ?4, 'current', ?5, ?5, ?5, ?5)",
                )
                .bind(tenant_id)
                .bind(&new_item_id)
                .bind(project_key)
                .bind(&new_rev_id)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 计算证据指纹
                let mut fp_hasher = Sha256::new();
                for r in &source_refs {
                    fp_hasher.update(r.as_bytes());
                }
                let evidence_fp = format!("{:x}", fp_hasher.finalize());

                // 写入 memory_item_revisions
                sqlx::query(
                    "INSERT INTO memory_item_revisions (\
                        tenant_id, id, item_id, revision_number, category, status, \
                        title, summary, rationale, recommendation_rank, promotion_nomination, \
                        occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                        supersedes_revision_id, created_at\
                     ) VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, ?8, ?9, ?10, NULL, NULL, ?9)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category.as_str())
                .bind(&title)
                .bind(&statement)
                .bind(&rationale)
                .bind(promotion_nomination.as_str())
                .bind(&now)
                .bind(&evidence_fp)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 关联来源引用
                for ref_key in source_refs {
                    let source_ref = resolve_project_reference(
                        &candidate_reference_map,
                        &ref_key,
                        strict_reference_validation,
                    )?;
                    let ref_id = format!("ref-l2-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references (\
                            tenant_id, id, item_revision_id, record_kind, source_id, \
                            session_id, question_id, turn_id, part_id, node_id, \
                            node_order, reference_key, source_revision, availability, \
                            unavailable_reason, unavailable_at, created_at\
                         ) VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, 'available', NULL, NULL, ?12)",
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
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
            ProjectConsolidationOperation::Revise {
                item_id,
                statement,
                rationale,
                source_refs,
            } => {
                // 查询当前版本号
                let cur_row = sqlx::query(
                    "SELECT mir.revision_number, mir.category, mir.title, mir.promotion_nomination \
                     FROM memory_items mi \
                     JOIN memory_item_revisions mir ON mi.tenant_id = mir.tenant_id AND mi.current_revision_id = mir.id \
                     WHERE mi.tenant_id = ?1 AND mi.id = ?2 AND mi.layer = 'l2' AND mi.lifecycle = 'current'",
                )
                .bind(tenant_id)
                .bind(&item_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::external)?;

                if let Some(r) = cur_row {
                    let cur_rev_num: i64 = r.get("revision_number");
                    let category: String = r.get("category");
                    let title: String = r.get("title");
                    let previous_nomination = parse_promotion_nomination(
                        r.get::<String, _>("promotion_nomination").as_str(),
                    );
                    let promotion_nomination = promotion_nomination_for_source_refs(
                        &candidates,
                        &source_refs,
                        previous_nomination,
                    );
                    let new_rev_num = cur_rev_num + 1;
                    let new_rev_id = format!("rev-l2-{}", uuid::Uuid::new_v4());

                    let mut fp_hasher = Sha256::new();
                    for r in &source_refs {
                        fp_hasher.update(r.as_bytes());
                    }
                    let evidence_fp = format!("{:x}", fp_hasher.finalize());

                    // 写入新 revision
                    sqlx::query(
                        "INSERT INTO memory_item_revisions (\
                            tenant_id, id, item_id, revision_number, category, status, \
                            title, summary, rationale, recommendation_rank, promotion_nomination, \
                            occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                            supersedes_revision_id, created_at\
                         ) VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?7, ?8, NULL, ?9, ?10, ?11, NULL, NULL, ?10)",
                    )
                    .bind(tenant_id)
                    .bind(&new_rev_id)
                    .bind(&item_id)
                    .bind(new_rev_num)
                    .bind(&category)
                    .bind(&title)
                    .bind(&statement)
                    .bind(&rationale)
                    .bind(promotion_nomination.as_str())
                    .bind(&now)
                    .bind(&evidence_fp)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;

                    // 更新 memory_items 指向新 revision
                    sqlx::query(
                        "UPDATE memory_items \
                         SET current_revision_id = ?1, last_seen_at = ?2, updated_at = ?2 \
                         WHERE tenant_id = ?3 AND id = ?4",
                    )
                    .bind(&new_rev_id)
                    .bind(&now)
                    .bind(tenant_id)
                    .bind(&item_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;

                    for ref_key in source_refs {
                        let source_ref = resolve_project_reference(
                            &candidate_reference_map,
                            &ref_key,
                            strict_reference_validation,
                        )?;
                        let ref_id = format!("ref-l2-{}", uuid::Uuid::new_v4());
                        sqlx::query(
                            "INSERT INTO memory_item_source_references (\
                                tenant_id, id, item_revision_id, record_kind, source_id, \
                                session_id, question_id, turn_id, part_id, node_id, \
                                node_order, reference_key, source_revision, availability, \
                                unavailable_reason, unavailable_at, created_at\
                             ) VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, 'available', NULL, NULL, ?12)",
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
                        .bind(&now)
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::external)?;
                    }
                }
            }
            ProjectConsolidationOperation::Supersede {
                old_item_id,
                replacement_title,
                replacement_statement,
                rationale,
                category,
                source_refs,
            } => {
                let promotion_nomination = promotion_nomination_for_source_refs(
                    &candidates,
                    &source_refs,
                    MemoryPromotionNomination::None,
                );
                // 标记旧条目为 superseded
                sqlx::query(
                    "UPDATE memory_items \
                     SET lifecycle = 'superseded', updated_at = ?1 \
                     WHERE tenant_id = ?2 AND id = ?3 AND layer = 'l2' AND lifecycle = 'current'",
                )
                .bind(&now)
                .bind(tenant_id)
                .bind(&old_item_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 创建替代新条目
                let new_item_id = format!("item-l2-{}", uuid::Uuid::new_v4());
                let new_rev_id = format!("rev-l2-{}", uuid::Uuid::new_v4());

                sqlx::query(
                    "INSERT INTO memory_items (\
                        tenant_id, id, layer, project_key, current_revision_id, \
                        lifecycle, first_seen_at, last_seen_at, created_at, updated_at\
                     ) VALUES (?1, ?2, 'l2', ?3, ?4, 'current', ?5, ?5, ?5, ?5)",
                )
                .bind(tenant_id)
                .bind(&new_item_id)
                .bind(project_key)
                .bind(&new_rev_id)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                let mut fp_hasher = Sha256::new();
                for r in &source_refs {
                    fp_hasher.update(r.as_bytes());
                }
                let evidence_fp = format!("{:x}", fp_hasher.finalize());

                sqlx::query(
                    "INSERT INTO memory_item_revisions (\
                        tenant_id, id, item_id, revision_number, category, status, \
                        title, summary, rationale, recommendation_rank, promotion_nomination, \
                        occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                        supersedes_revision_id, created_at\
                     ) VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, ?7, NULL, ?8, ?9, ?10, NULL, NULL, ?9)",
                )
                .bind(tenant_id)
                .bind(&new_rev_id)
                .bind(&new_item_id)
                .bind(category.as_str())
                .bind(&replacement_title)
                .bind(&replacement_statement)
                .bind(&rationale)
                .bind(promotion_nomination.as_str())
                .bind(&now)
                .bind(&evidence_fp)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;

                // 记录 supersession 关系
                let sup_id = format!("sup-{}", uuid::Uuid::new_v4());
                sqlx::query(
                    "INSERT INTO memory_item_supersessions (\
                        tenant_id, id, superseded_item_id, superseding_item_id, reason, created_at\
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .bind(tenant_id)
                .bind(&sup_id)
                .bind(&old_item_id)
                .bind(&new_item_id)
                .bind(&rationale)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;
                for ref_key in source_refs {
                    let source_ref = resolve_project_reference(
                        &candidate_reference_map,
                        &ref_key,
                        strict_reference_validation,
                    )?;
                    let ref_id = format!("ref-l2-{}", uuid::Uuid::new_v4());
                    sqlx::query(
                        "INSERT INTO memory_item_source_references (\
                            tenant_id, id, item_revision_id, record_kind, source_id, \
                            session_id, question_id, turn_id, part_id, node_id, \
                            node_order, reference_key, source_revision, availability, \
                            unavailable_reason, unavailable_at, created_at\
                         ) VALUES (?1, ?2, ?3, 'session', ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11, 'available', NULL, NULL, ?12)",
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
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }
            ProjectConsolidationOperation::Keep { item_id } => {
                // 保持原样，仅刷新 last_seen_at
                sqlx::query(
                    "UPDATE memory_items SET last_seen_at = ?1, updated_at = ?1 \
                     WHERE tenant_id = ?2 AND id = ?3",
                )
                .bind(&now)
                .bind(tenant_id)
                .bind(&item_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;
            }
        }
    }

    // 计算新的 revision_hash (所有 active L2 revisions 的确定性指纹)
    let rev_ids: Vec<String> = sqlx::query_scalar(
        "SELECT current_revision_id FROM memory_items \
         WHERE tenant_id = ?1 AND project_key = ?2 AND layer = 'l2' AND lifecycle = 'current' \
         ORDER BY id ASC",
    )
    .bind(tenant_id)
    .bind(project_key)
    .fetch_all(&mut *tx)
    .await
    .map_err(AppError::external)?;

    let mut rev_hasher = Sha256::new();
    rev_hasher.update(project_key.as_bytes());
    for r in rev_ids {
        rev_hasher.update(r.as_bytes());
    }
    let revision_hash = format!("{:x}", rev_hasher.finalize());

    // 更新 project_memory_state 指针
    sqlx::query(
        "INSERT INTO project_memory_state (\
            tenant_id, project_key, project_path, last_successful_consolidation_at, \
            last_input_fingerprint, revision_hash, created_at, updated_at\
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?4, ?4) \
         ON CONFLICT (tenant_id, project_key) DO UPDATE SET \
            project_path = excluded.project_path, \
            last_successful_consolidation_at = excluded.last_successful_consolidation_at, \
            last_input_fingerprint = excluded.last_input_fingerprint, \
            revision_hash = excluded.revision_hash, \
            updated_at = excluded.updated_at",
    )
    .bind(tenant_id)
    .bind(project_key)
    .bind(project_path)
    .bind(&now)
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
            &now,
        )
        .await?;
        if !completed {
            return Err(AppError::Conflict(
                "memory maintenance lease is no longer owned".to_string(),
            ));
        }
    }

    // 事务提交
    tx.commit().await.map_err(AppError::external)?;

    Ok(())
}
