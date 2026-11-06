use super::*;
use crate::backend::store::conversations::snapshot::{
    prepare_session_snapshot, replace_session_snapshot_sqlx_tx,
};

pub async fn reproject_conversation_session_sqlx(
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
    let prepared = prepare_session_snapshot(
        ConversationRecordKind::Session,
        &session.id,
        &session.adapter_id,
        session.started_at.as_deref(),
        session.updated_at.as_deref(),
        &normalized.turns,
        &now,
    );

    upsert_conversation_session_sqlx_tx(&mut tx, tenant_id, &session).await?;
    replace_session_snapshot_sqlx_tx(
        &mut tx,
        tenant_id,
        ConversationRecordKind::Session,
        &session,
        &prepared,
    )
    .await?;

    let last_cwd = derive_session_last_cwd(&normalized.turns);
    update_conversation_session_last_cwd_sqlx_tx(
        &mut tx,
        tenant_id,
        &session.id,
        last_cwd.as_deref(),
    )
    .await?;

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
