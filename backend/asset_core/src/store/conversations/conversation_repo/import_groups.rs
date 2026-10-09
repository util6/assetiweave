use super::*;

pub(super) async fn ensure_question_groups_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    now: &str,
) -> StoreResult<()> {
    reject_invalid_conversation_question_turns_for_session_sqlx_tx(tx, tenant_id, session_id)
        .await?;
    let turns = load_session_turns_sqlx_tx(tx, tenant_id, session_id).await?;
    if turns.is_empty() {
        return Ok(());
    }

    let manual_fenced_turn_ids = sqlx::query_scalar::<_, String>(
        r#"
        SELECT qt.turn_id
        FROM conversation_question_turns qt
        JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND t.session_id = ?2
          AND qt.assignment_origin = 'manual'
        ORDER BY t.turn_index ASC, t.id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?
    .into_iter()
    .collect::<BTreeSet<_>>();

    // Automatic rows are a derived relationship. Rebuilding only these rows
    // makes full, repeated full, and equivalent incremental imports converge to
    // the same stable question IDs while preserving all manual rows.
    sqlx::query(
        r#"
        DELETE FROM conversation_question_turns
        WHERE tenant_id = ?1
          AND assignment_origin <> 'manual'
          AND turn_id IN (
              SELECT id FROM conversation_turns
              WHERE tenant_id = ?1 AND session_id = ?2
          )
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    let mut automatic_segment = Vec::new();
    let mut automatic_segments = Vec::new();
    for turn in &turns {
        if manual_fenced_turn_ids.contains(&turn.id) {
            if !automatic_segment.is_empty() {
                automatic_segments.push(std::mem::take(&mut automatic_segment));
            }
        } else {
            automatic_segment.push((turn.id.clone(), turn.user_text.clone()));
        }
    }
    if !automatic_segment.is_empty() {
        automatic_segments.push(automatic_segment);
    }

    for segment in automatic_segments {
        for group in group_turn_ids_by_question(segment) {
            let first_turn_id = group.turn_ids.first().ok_or_else(|| {
                StoreError::external("empty conversation question group".to_string())
            })?;
            let question_id = stable_id("conversation-question", &[session_id, first_turn_id]);
            match load_conversation_question_sqlx_tx(tx, tenant_id, &question_id).await? {
                Some(question) if question.session_id != session_id => {
                    return Err(StoreError::Validation(format!(
                        "question id belongs to another session: {question_id}"
                    )));
                }
                Some(_) => {}
                None => {
                    sqlx::query(
                        r#"
                        INSERT INTO conversation_questions (
                            tenant_id, id, session_id, title, created_at, updated_at
                        )
                        VALUES (?1, ?2, ?3, NULL, ?4, ?4)
                        "#,
                    )
                    .bind(tenant_id)
                    .bind(&question_id)
                    .bind(session_id)
                    .bind(now)
                    .execute(&mut **tx)
                    .await
                    .map_err(StoreError::external)?;
                }
            }

            for (turn_order, turn_id) in group.turn_ids.iter().enumerate() {
                ensure_question_turn_scope_sqlx_tx(tx, tenant_id, &question_id, turn_id).await?;
                sqlx::query(
                    r#"
                    INSERT INTO conversation_question_turns (
                        tenant_id, question_id, turn_id, turn_order,
                        assignment_origin, assigned_at, updated_at
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                    "#,
                )
                .bind(tenant_id)
                .bind(&question_id)
                .bind(turn_id)
                .bind(turn_order as i64)
                .bind(encode_enum(group.origin)?)
                .bind(now)
                .execute(&mut **tx)
                .await
                .map_err(StoreError::external)?;
            }
        }
    }

    prune_orphan_questions_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    renumber_questions_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    Ok(())
}

pub(super) async fn ensure_question_turn_scope_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
    turn_id: &str,
) -> StoreResult<()> {
    let same_session = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT 1
        FROM conversation_questions q
        JOIN conversation_turns t
          ON t.tenant_id = q.tenant_id
         AND t.session_id = q.session_id
         AND t.id = ?3
        WHERE q.tenant_id = ?1 AND q.id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .bind(turn_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    if same_session.is_none() {
        return Err(StoreError::Validation(format!(
            "question turn membership must use the same tenant and session: question={question_id}, turn={turn_id}"
        )));
    }
    Ok(())
}

#[derive(Debug, FromRow)]
pub(super) struct InvalidConversationQuestionTurnRow {
    question_id: String,
    turn_id: String,
    reason: String,
}

pub(super) async fn reject_invalid_conversation_question_turns_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<()> {
    let row = sqlx::query_as::<_, InvalidConversationQuestionTurnRow>(
        r#"
        SELECT qt.question_id, qt.turn_id,
               CASE
                   WHEN q.id IS NULL THEN 'missing_question'
                   WHEN t.id IS NULL THEN 'missing_turn'
                   ELSE 'cross_session'
               END AS reason
        FROM conversation_question_turns qt
        LEFT JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        LEFT JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND (q.session_id = ?2 OR t.session_id = ?2)
          AND (q.id IS NULL OR t.id IS NULL OR q.session_id <> t.session_id)
        ORDER BY qt.question_id ASC, qt.turn_id ASC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    if let Some(first) = row {
        return Err(StoreError::Validation(format!(
            "invalid question turn membership ({}): question={}, turn={}",
            first.reason, first.question_id, first.turn_id
        )));
    }
    Ok(())
}

pub(super) async fn prune_orphan_questions_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<()> {
    let orphan_ids = sqlx::query_scalar::<_, String>(
        r#"
        SELECT q.id
        FROM conversation_questions q
        LEFT JOIN conversation_question_turns qt
          ON qt.tenant_id = q.tenant_id AND qt.question_id = q.id
        WHERE q.tenant_id = ?1 AND q.session_id = ?2
        GROUP BY q.id
        HAVING COUNT(qt.turn_id) = 0
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    if orphan_ids.is_empty() {
        return Ok(());
    }

    for orphan_id in &orphan_ids {
        sqlx::query(
            r#"
            DELETE FROM conversation_question_fts
            WHERE tenant_id = ?1 AND question_id = ?2
            "#,
        )
        .bind(tenant_id)
        .bind(orphan_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;

        sqlx::query(
            r#"
            DELETE FROM conversation_questions
            WHERE tenant_id = ?1 AND id = ?2
            "#,
        )
        .bind(tenant_id)
        .bind(orphan_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }

    Ok(())
}

pub(super) async fn reject_invalid_conversation_question_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
) -> StoreResult<()> {
    let rows = sqlx::query_as::<_, InvalidConversationQuestionTurnRow>(
        r#"
        SELECT qt.question_id, qt.turn_id,
               CASE
                   WHEN q.id IS NULL THEN 'missing_question'
                   WHEN t.id IS NULL THEN 'missing_turn'
                   ELSE 'cross_session'
               END AS reason
        FROM conversation_question_turns qt
        LEFT JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        LEFT JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND (q.id IS NULL OR t.id IS NULL OR q.session_id <> t.session_id)
        ORDER BY qt.question_id ASC, qt.turn_id ASC
        "#,
    )
    .bind(tenant_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    if let Some(first) = rows.first() {
        return Err(StoreError::Validation(format!(
            "invalid question turn membership ({}): question={}, turn={}",
            first.reason, first.question_id, first.turn_id
        )));
    }
    Ok(())
}

pub(super) async fn audit_invalid_conversation_question_turns_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<()> {
    let rows = sqlx::query_as::<_, InvalidConversationQuestionTurnRow>(
        r#"
        SELECT qt.question_id, qt.turn_id,
               CASE
                   WHEN q.id IS NULL THEN 'missing_question'
                   WHEN t.id IS NULL THEN 'missing_turn'
                   ELSE 'cross_session'
               END AS reason
        FROM conversation_question_turns qt
        LEFT JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        LEFT JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND (q.id IS NULL OR t.id IS NULL OR q.session_id <> t.session_id)
        ORDER BY qt.question_id ASC, qt.turn_id ASC
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    if rows.is_empty() {
        return Ok(());
    }

    let detected_at = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    for row in &rows {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO conversation_question_turn_audits (
                tenant_id, record_kind, question_id, turn_id, reason, detected_at
            )
            VALUES (?1, 'session', ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(tenant_id)
        .bind(&row.question_id)
        .bind(&row.turn_id)
        .bind(&row.reason)
        .bind(&detected_at)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    }
    tx.commit().await.map_err(StoreError::external)
}
