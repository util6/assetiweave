use super::*;

pub(crate) async fn reproject_conversation_session_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    projection_version: Option<u32>,
) -> StoreResult<()> {
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(StoreError::external)?;

    let session = conversation_session_from_normalized(source, normalized, &now);

    upsert_conversation_session_sqlx_tx(&mut tx, tenant_id, &session).await?;

    for turn in &normalized.turns {
        if turn.user_text.trim().is_empty() {
            continue;
        }
        let stored_turn = conversation_turn_from_normalized(&session.id, turn, &now);
        upsert_conversation_turn_sqlx_tx(&mut tx, tenant_id, &stored_turn).await?;
        replace_conversation_parts_sqlx_tx(&mut tx, tenant_id, &stored_turn.id, &turn.parts)
            .await?;
    }

    prune_conversation_turns_sqlx_tx(&mut tx, tenant_id, &session.id, normalized).await?;
    ensure_question_groups_for_session_sqlx_tx(&mut tx, tenant_id, &session.id, &now).await?;
    rebuild_session_question_aggregates_sqlx_tx(&mut tx, tenant_id, &session.id, &now).await?;

    upsert_single_session_observation_clean_sqlx_tx(
        &mut tx,
        tenant_id,
        &source.id,
        "session",
        &session.external_id,
        session.source_fingerprint.as_deref().unwrap_or(&now),
        &now,
        adapter_content_hash,
        card_contract_version,
        payload_policy_version,
        projection_version,
    )
    .await?;

    tx.commit().await.map_err(StoreError::external)?;
    Ok(())
}

#[cfg(test)]
#[path = "reproject_tests.rs"]
mod tests;
