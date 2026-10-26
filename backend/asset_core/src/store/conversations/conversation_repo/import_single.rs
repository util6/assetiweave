use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn import_single_session_sqlx_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tenant_id: &str,
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    session: &ConversationSession,
    now: &str,
    sync_run_id: &str,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    projection_version: Option<u32>,
) -> StoreResult<Option<String>> {
    if is_droppable_internal_session(normalized, session) {
        return Ok(None);
    }
    let change_kind =
        if conversation_session_exists_sqlx_tx(&mut *tx, tenant_id, &session.id).await? {
            "updated"
        } else {
            "new"
        };
    if conversation_session_is_unchanged_sqlx_tx(&mut *tx, tenant_id, session, normalized).await? {
        upsert_single_session_observation_clean_sqlx_tx(
            &mut *tx,
            tenant_id,
            &source.id,
            "session",
            &session.external_id,
            session.source_fingerprint.as_deref().unwrap_or(now),
            now,
            adapter_content_hash,
            card_contract_version,
            payload_policy_version,
            projection_version,
        )
        .await?;
        return Ok(None);
    }
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND session_id = ?3 AND status = 'active'",
    )
    .bind(now)
    .bind(tenant_id)
    .bind(&session.id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    upsert_conversation_session_sqlx_tx(&mut *tx, tenant_id, session).await?;
    for turn in &normalized.turns {
        if turn.user_text.trim().is_empty() {
            continue;
        }
        let (stored_turn, triad_parts) = prepare_turn_for_import_sqlx_tx(
            &mut *tx,
            tenant_id,
            &session.id,
            &session.adapter_id,
            turn,
            now,
        )
        .await?;
        upsert_conversation_turn_sqlx_tx(&mut *tx, tenant_id, &stored_turn).await?;
        replace_conversation_parts_sqlx_tx(&mut *tx, tenant_id, &stored_turn.id, &triad_parts)
            .await?;
    }
    let last_cwd = derive_session_last_cwd(&normalized.turns);
    update_conversation_session_last_cwd_sqlx_tx(
        &mut *tx,
        tenant_id,
        &session.id,
        last_cwd.as_deref(),
    )
    .await?;
    prune_conversation_turns_sqlx_tx(&mut *tx, tenant_id, &session.id, normalized).await?;
    insert_conversation_sync_delta_sqlx_tx(
        &mut *tx,
        tenant_id,
        sync_run_id,
        "session",
        &session.id,
        change_kind,
        now,
    )
    .await?;

    upsert_single_session_observation_clean_sqlx_tx(
        &mut *tx,
        tenant_id,
        &source.id,
        "session",
        &session.external_id,
        session.source_fingerprint.as_deref().unwrap_or(now),
        now,
        adapter_content_hash,
        card_contract_version,
        payload_policy_version,
        projection_version,
    )
    .await?;

    Ok(Some(session.id.clone()))
}
