use super::*;

pub(super) async fn rebuild_session_question_aggregates_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    now: &str,
) -> StoreResult<()> {
    let question_ids = question_ids_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    for question_id in question_ids {
        rebuild_question_aggregate_sqlx_tx(tx, tenant_id, &question_id, now).await?;
    }
    Ok(())
}

pub(super) async fn rebuild_question_aggregate_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
    now: &str,
) -> StoreResult<()> {
    // FTS remains an independently rebuildable projection. Question Detail reads
    // only membership and Turn-Part source facts.
    let turns = load_question_turns_sqlx_tx(tx, tenant_id, question_id).await?;
    let mut question_text = Vec::new();
    let mut answer_text = Vec::new();
    let mut code_text = Vec::new();
    let mut command_text = Vec::new();
    let adapter_id = sqlx::query_scalar::<_, String>(
        r#"
        SELECT s.adapter_id
        FROM conversation_questions q
        JOIN conversation_sessions s
          ON s.tenant_id = q.tenant_id AND s.id = q.session_id
        WHERE q.tenant_id = ?1 AND q.id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    let card_kinds_json = sqlx::query_scalar::<_, String>(
        "SELECT card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&adapter_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?
    .unwrap_or_else(|| "[]".to_string());
    let card_kinds: Vec<ConversationCardKindDefinition> = decode_json(card_kinds_json)?;

    for turn in &turns {
        question_text.push(turn.user_text.clone());
        for part in load_turn_parts_sqlx_tx(tx, tenant_id, &turn.id).await? {
            append_projected_cards_to_question_aggregate(
                &part,
                &adapter_id,
                &card_kinds,
                &mut answer_text,
                &mut code_text,
                &mut command_text,
            )?;
        }
    }

    let question_text = question_text.join("\n\n");
    let answer_text = answer_text.join("\n\n");
    let code_text = code_text.join("\n\n");
    let command_text = command_text.join("\n\n");
    let title = first_line(&question_text);

    sqlx::query(
        "UPDATE conversation_questions SET title = COALESCE(NULLIF(title, ''), ?1), updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4",
    )
    .bind(&title)
    .bind(now)
    .bind(tenant_id)
    .bind(question_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    let session_id: String = sqlx::query_scalar::<_, String>(
        "SELECT session_id FROM conversation_questions WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query("DELETE FROM conversation_question_fts WHERE tenant_id = ?1 AND question_id = ?2")
        .bind(tenant_id)
        .bind(question_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    sqlx::query(
        r#"
        INSERT INTO conversation_question_fts (
            tenant_id, question_id, session_id, question_text, answer_text, code_text, command_text
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .bind(&session_id)
    .bind(&question_text)
    .bind(&answer_text)
    .bind(&code_text)
    .bind(&command_text)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(super) async fn insert_sync_run_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    run: &ConversationSyncRun,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_sync_runs (
            tenant_id, id, source_id, adapter_id, status, started_at, finished_at,
            session_count, turn_count, warning_count, error_message
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
    )
    .bind(tenant_id)
    .bind(&run.id)
    .bind(&run.source_id)
    .bind(&run.adapter_id)
    .bind(encode_enum(run.status)?)
    .bind(&run.started_at)
    .bind(&run.finished_at)
    .bind(run.session_count)
    .bind(run.turn_count)
    .bind(run.warning_count)
    .bind(&run.error_message)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn insert_conversation_sync_delta_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    sync_run_id: &str,
    record_kind: &str,
    session_id: &str,
    change_kind: &str,
    observed_at: &str,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_sync_deltas (
            tenant_id, sync_run_id, record_kind, session_id, change_kind, observed_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(tenant_id)
    .bind(sync_run_id)
    .bind(record_kind)
    .bind(session_id)
    .bind(change_kind)
    .bind(observed_at)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(super) async fn load_session_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Vec<ConversationTurn>> {
    let rows = sqlx::query(
        r#"
        SELECT id, session_id, external_id, turn_index, user_text, title,
               started_at, ended_at, fingerprint, missing, imported_at, model
        FROM conversation_turns
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY turn_index ASC, id ASC, imported_at ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    rows.iter().map(map_sqlx_conversation_turn).collect()
}

pub(super) async fn load_question_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<Vec<ConversationTurn>> {
    let rows = sqlx::query(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at, t.model
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1 AND qt.question_id = ?2
        ORDER BY qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    rows.iter().map(map_sqlx_conversation_turn).collect()
}

pub(super) async fn load_turn_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
) -> StoreResult<Vec<ConversationPart>> {
    let rows = sqlx::query(
        r#"
        SELECT id, turn_id, part_index, role, kind, text, language, command,
               cwd, status, exit_code, metadata_json, content_card_json, translated_text,
               source_execution_id, command_label
        FROM conversation_parts
        WHERE tenant_id = ?1 AND turn_id = ?2
        ORDER BY part_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    rows.iter().map(map_sqlx_conversation_part).collect()
}

pub(super) async fn max_question_turn_order_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<i64> {
    let max_order: Option<i64> = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(turn_order) FROM conversation_question_turns WHERE tenant_id = ?1 AND question_id = ?2",
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(max_order.unwrap_or(-1))
}

pub(super) async fn load_conversation_question_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<Option<ConversationQuestion>> {
    sqlx::query(
        r#"
        SELECT id, session_id, title, created_at, updated_at
        FROM conversation_questions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?
    .as_ref()
    .map(map_sqlx_conversation_question)
    .transpose()
}

pub(super) async fn question_ids_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT q.id
        FROM conversation_questions q
        WHERE q.tenant_id = ?1 AND q.session_id = ?2
        ORDER BY COALESCE((SELECT MIN(t.turn_index)
                          FROM conversation_question_turns qt
                          JOIN conversation_turns t
                            ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
                          WHERE qt.tenant_id = q.tenant_id AND qt.question_id = q.id),
                         9223372036854775807),
                 q.created_at ASC,
                 q.id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)
}

pub(super) async fn load_question_turn_ids_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT turn_id
        FROM conversation_question_turns
        WHERE tenant_id = ?1 AND question_id = ?2
        ORDER BY turn_order ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)
}

pub(super) async fn renumber_question_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
    updated_at: &str,
) -> StoreResult<()> {
    let turn_ids = load_question_turn_ids_sqlx_tx(tx, tenant_id, question_id).await?;
    for (index, turn_id) in turn_ids.iter().enumerate() {
        sqlx::query(
            r#"
            UPDATE conversation_question_turns
            SET turn_order = ?1, updated_at = ?2
            WHERE tenant_id = ?3 AND question_id = ?4 AND turn_id = ?5
            "#,
        )
        .bind(index as i64)
        .bind(updated_at)
        .bind(tenant_id)
        .bind(question_id)
        .bind(turn_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }
    Ok(())
}

pub(super) async fn ensure_question_ids_are_adjacent_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    question_ids: &[String],
) -> StoreResult<()> {
    let ordered = question_ids_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    let selected = question_ids.iter().collect::<BTreeSet<_>>();
    let positions = ordered
        .iter()
        .enumerate()
        .filter_map(|(index, id)| selected.contains(id).then_some(index))
        .collect::<Vec<_>>();
    if positions.len() != question_ids.len() {
        return Err(StoreError::Validation(
            "all questions must exist in the session".to_string(),
        ));
    }
    if positions
        .windows(2)
        .any(|window| window[1] != window[0] + 1)
    {
        return Err(StoreError::Validation(
            "questions must be adjacent".to_string(),
        ));
    }
    if positions
        .iter()
        .map(|index| &ordered[*index])
        .zip(question_ids.iter())
        .any(|(actual, requested)| actual != requested)
    {
        return Err(StoreError::Validation(
            "question ids must be supplied in session order".to_string(),
        ));
    }
    Ok(())
}

pub(super) async fn renumber_questions_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<()> {
    let _ = (tx, tenant_id, session_id);
    Ok(())
}
