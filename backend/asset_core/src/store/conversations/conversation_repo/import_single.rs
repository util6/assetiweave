use super::*;
use crate::backend::store::conversations::snapshot::{
    prepare_session_snapshot, replace_session_snapshot_sqlx_tx,
};

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
    // 载荷解析策略版本发生上移时，源文件指纹不会变化，仅靠会话级 unchanged 判定会永久跳过存量会话。
    // 该标记只在本轮有效：源级版本尚未与当前策略版本对齐时为 true，强制重读一次。
    force_reparse: bool,
) -> StoreResult<Option<String>> {
    if is_droppable_internal_session(normalized, session) {
        return Ok(None);
    }
    if !force_reparse
        && conversation_session_is_unchanged_sqlx_tx(&mut *tx, tenant_id, session).await?
    {
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

    let prepared = prepare_session_snapshot(
        ConversationRecordKind::Session,
        &source.id,
        &session.id,
        &session.adapter_id,
        normalized.execution_origin.as_deref(),
        session.started_at.as_deref(),
        session.updated_at.as_deref(),
        &normalized.turns,
        now,
    );

    upsert_conversation_session_sqlx_tx(&mut *tx, tenant_id, session).await?;
    replace_session_snapshot_sqlx_tx(
        &mut *tx,
        tenant_id,
        ConversationRecordKind::Session,
        session,
        &prepared,
    )
    .await?;

    let last_cwd = derive_session_last_cwd(&normalized.turns);
    update_conversation_session_last_cwd_sqlx_tx(
        &mut *tx,
        tenant_id,
        &session.id,
        last_cwd.as_deref(),
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
