use super::*;

pub(super) async fn mark_missing_conversation_sessions_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    source_id: &str,
    incoming_session_ids: &BTreeSet<String>,
    sync_run_id: &str,
    now: &str,
) -> StoreResult<Vec<String>> {
    let mut changed_session_ids = Vec::new();
    let existing_sessions = sqlx::query_as::<_, (String, i64)>(
        "SELECT id, missing FROM conversation_sessions WHERE tenant_id = ?1 AND source_id = ?2",
    )
    .bind(tenant_id)
    .bind(source_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    for (session_id, missing) in existing_sessions {
        if incoming_session_ids.contains(&session_id) {
            if missing == 0 {
                continue;
            }
            sqlx::query(
                r#"
                UPDATE conversation_sessions
                SET missing = 0
                WHERE tenant_id = ?1 AND id = ?2
                "#,
            )
            .bind(tenant_id)
            .bind(&session_id)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
            insert_conversation_sync_delta_sqlx_tx(
                tx,
                tenant_id,
                sync_run_id,
                "session",
                &session_id,
                "restored",
                now,
            )
            .await?;
            changed_session_ids.push(session_id.clone());
            continue;
        }
        if missing != 0 {
            continue;
        }
        sqlx::query(
            r#"
            UPDATE conversation_sessions
            SET missing = 1, imported_at = ?1
            WHERE tenant_id = ?2 AND id = ?3
            "#,
        )
        .bind(now)
        .bind(tenant_id)
        .bind(&session_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
        sqlx::query(
            "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND session_id = ?3 AND status = 'active'",
        )
        .bind(now)
        .bind(tenant_id)
        .bind(&session_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
        insert_conversation_sync_delta_sqlx_tx(
            tx,
            tenant_id,
            sync_run_id,
            "session",
            &session_id,
            "missing",
            now,
        )
        .await?;
        changed_session_ids.push(session_id);
    }
    Ok(changed_session_ids)
}

pub(super) async fn prune_conversation_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    normalized: &NormalizedConversationSession,
) -> StoreResult<()> {
    let retained_turn_ids = normalized
        .turns
        .iter()
        .filter(|turn| !turn.user_text.trim().is_empty())
        .map(|turn| stable_id("conversation-turn", &[session_id, &turn.external_id]))
        .collect::<BTreeSet<_>>();
    let turn_ids = sqlx::query_scalar::<_, String>(
        "SELECT id FROM conversation_turns WHERE tenant_id = ?1 AND session_id = ?2",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    let stale_turn_ids = turn_ids
        .into_iter()
        .filter(|turn_id| !retained_turn_ids.contains(turn_id))
        .collect::<Vec<_>>();
    if stale_turn_ids.is_empty() {
        return Ok(());
    }

    for turn_id in &stale_turn_ids {
        sqlx::query("DELETE FROM conversation_parts WHERE tenant_id = ?1 AND turn_id = ?2")
            .bind(tenant_id)
            .bind(turn_id)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
        sqlx::query(
            "DELETE FROM conversation_question_turns WHERE tenant_id = ?1 AND turn_id = ?2",
        )
        .bind(tenant_id)
        .bind(turn_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
        sqlx::query("DELETE FROM conversation_turns WHERE tenant_id = ?1 AND id = ?2")
            .bind(tenant_id)
            .bind(turn_id)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
    }
    sqlx::query(
        r#"
        DELETE FROM conversation_question_fts
        WHERE tenant_id = ?1
          AND question_id IN (
            SELECT q.id
            FROM conversation_questions q
            LEFT JOIN conversation_question_turns qt
              ON qt.tenant_id = q.tenant_id AND qt.question_id = q.id
            WHERE q.tenant_id = ?1 AND q.session_id = ?2
            GROUP BY q.id
            HAVING COUNT(qt.turn_id) = 0
        )
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query(
        r#"
        DELETE FROM conversation_questions
        WHERE tenant_id = ?1 AND session_id = ?2
          AND id NOT IN (
              SELECT DISTINCT question_id
              FROM conversation_question_turns
              WHERE tenant_id = ?1
          )
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    renumber_questions_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    Ok(())
}
