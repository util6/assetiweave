use super::*;

pub(crate) async fn persist_session_memory_sqlx(
    pool: &SqlitePool,
    input: &SessionMemoryPersistInput,
) -> StoreResult<()> {
    let mut tx = pool.begin().await.map_err(StoreError::Db)?;

    // E09: 晚到防覆盖校验
    // 检查是否已有更新的 active session memory 存在（即更高 source_revision，或同 revision 但生成时间更新）
    let newer_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM session_memories WHERE tenant_id = ?1 AND session_id = ?2 AND status = 'active' AND (source_revision > ?3 OR (source_revision = ?3 AND generated_at > ?4))",
    )
    .bind(&input.tenant_id)
    .bind(&input.session_id)
    .bind(input.source_revision)
    .bind(&input.generated_at)
    .fetch_one(&mut *tx)
    .await
    .map_err(StoreError::Db)?;

    if newer_count > 0 {
        // 当前任务已被更新的目标超越，将当前 job 标为 skipped 并记录 superseding 原因，不覆盖新目标
        sqlx::query(
            "UPDATE session_memory_jobs SET status = 'skipped', last_error = 'superseded_by_newer_target', finished_at = ?1, updated_at = ?1, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL WHERE tenant_id = ?2 AND id = (SELECT id FROM session_memory_jobs WHERE tenant_id = ?2 AND session_id = ?3 AND source_revision = ?4 AND status = 'running' AND ownership_token = ?5 LIMIT 1)",
        )
        .bind(&input.generated_at)
        .bind(&input.tenant_id)
        .bind(&input.session_id)
        .bind(input.source_revision)
        .bind(&input.ownership_token)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;

        tx.commit().await.map_err(StoreError::Db)?;
        return Ok(());
    }

    let claimed = sqlx::query(
        "UPDATE session_memory_jobs SET status = 'succeeded', finished_at = ?1, updated_at = ?1, last_error = NULL, ownership_token = NULL, lease_expires_at = NULL, heartbeat_at = NULL, retry_at = NULL, watermark = source_revision WHERE tenant_id = ?2 AND id = (SELECT id FROM session_memory_jobs WHERE tenant_id = ?2 AND session_id = ?3 AND source_revision = ?4 AND source_fingerprint = ?5 AND contract_version = ?6 AND prompt_version = ?7 AND status = 'running' AND ownership_token = ?8 LIMIT 1) AND status = 'running' AND ownership_token = ?8",
    )
    .bind(&input.generated_at)
    .bind(&input.tenant_id)
    .bind(&input.session_id)
    .bind(input.source_revision)
    .bind(&input.source_fingerprint)
    .bind(&input.contract_version)
    .bind(&input.prompt_version)
    .bind(&input.ownership_token)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::Db)?;
    if claimed.rows_affected() != 1 {
        return Err(StoreError::Conflict(
            "Session Memory job lease is no longer owned".to_string(),
        ));
    }
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO session_memories (
            tenant_id, id, session_id, source_id, source_revision,
            source_fingerprint, contract_version, prompt_version, status,
            project_path, summary, goal, result, decisions_json,
            verification_json, blockers_json, follow_up_json, topics_json,
            raw_output_json, generated_at, created_at, updated_at,
            recipe_id, recipe_content_hash, work_order_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19, ?19, ?20, ?21, ?22)
        "#,
    )
    .bind(&input.tenant_id)
    .bind(&input.memory_id)
    .bind(&input.session_id)
    .bind(&input.source_id)
    .bind(input.source_revision)
    .bind(&input.source_fingerprint)
    .bind(&input.contract_version)
    .bind(&input.prompt_version)
    .bind(&input.project_path)
    .bind(&input.summary)
    .bind(&input.goal)
    .bind(&input.result)
    .bind(&input.decisions_json)
    .bind(&input.verification_json)
    .bind(&input.blockers_json)
    .bind(&input.follow_up_json)
    .bind(&input.topics_json)
    .bind(&input.raw_output_json)
    .bind(&input.generated_at)
    .bind(&input.recipe_id)
    .bind(&input.recipe_content_hash)
    .bind(&input.work_order_json)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::Db)?;
    let memory_id = input.memory_id.clone();
    for (index, reference) in input.references.iter().enumerate() {
        let reference_id = format!(
            "session-memory-ref-{}",
            digest(&format!("{}\0{}", memory_id, reference.reference_key))
        );
        insert_source_reference_sqlx(&mut tx, input, reference, &reference_id, &memory_id)
            .await
            .map_err(|error| {
                StoreError::Storage(format!(
                    "source reference {index} could not be stored: {error}"
                ))
            })?;
    }
    for event in &input.events {
        let event_id = format!(
            "recent-event-{}",
            digest(&format!("{}\0{}", memory_id, event.fingerprint))
        );
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO recent_memory_events (
                tenant_id, id, memory_id, session_id, category, title, summary,
                occurred_at, source_reference_id, fingerprint, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
        )
        .bind(&input.tenant_id)
        .bind(event_id)
        .bind(&memory_id)
        .bind(&input.session_id)
        .bind(event.category.as_str())
        .bind(&event.title)
        .bind(&event.summary)
        .bind(&event.occurred_at)
        .bind(&event.source_reference_id)
        .bind(&event.fingerprint)
        .bind(&input.generated_at)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::Db)?;
    }
    // Issue #35 cutover: Session Phase 1 only publishes SQLite source facts.
    // Project/Global promotion is triggered by the v2 Recent Snapshot worker,
    // never by the legacy project_memory_jobs chain.
    tx.commit().await.map_err(StoreError::Db)
}

pub(crate) async fn insert_source_reference_sqlx(
    tx: &mut Transaction<'_, Sqlite>,
    input: &SessionMemoryPersistInput,
    reference: &SessionMemoryReferenceInput,
    reference_id: &str,
    memory_id: &str,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO session_memory_source_references (
            tenant_id, id, memory_id, source_id, session_id, record_kind,
            question_id, turn_id, part_id, node_id, node_order,
            reference_key, source_revision, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, 'session', ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
    )
    .bind(&input.tenant_id)
    .bind(reference_id)
    .bind(memory_id)
    .bind(&reference.source_id)
    .bind(&reference.session_id)
    .bind(&reference.question_id)
    .bind(&reference.turn_id)
    .bind(&reference.part_id)
    .bind(&reference.node_id)
    .bind(reference.node_order.map(|value| value as i64))
    .bind(&reference.reference_key)
    .bind(reference.source_revision)
    .bind(&input.generated_at)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::Db)?;
    Ok(())
}
