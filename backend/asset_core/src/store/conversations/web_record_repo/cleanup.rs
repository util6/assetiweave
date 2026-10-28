use super::*;

pub async fn delete_web_record_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<()> {
    delete_session_part_index_sqlx_tx(tx, tenant_id, session_id).await?;
    sqlx::query(
        r#"
        DELETE FROM web_record_parts
        WHERE tenant_id = ?1
          AND turn_id IN (
            SELECT id FROM web_record_turns
            WHERE tenant_id = ?1 AND session_id = ?2
        )
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query("DELETE FROM web_record_turns WHERE tenant_id = ?1 AND session_id = ?2")
        .bind(tenant_id)
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    sqlx::query(
        r#"
        DELETE FROM conversation_session_observations
        WHERE tenant_id = ?1 AND record_kind = 'web'
          AND external_id = (SELECT external_id FROM web_record_sessions WHERE tenant_id = ?1 AND id = ?2)
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query("DELETE FROM web_record_sessions WHERE tenant_id = ?1 AND id = ?2")
        .bind(tenant_id)
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

pub async fn clear_legacy_conversation_records_for_source_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    source_id: &str,
) -> StoreResult<()> {
    delete_source_part_index_sqlx_tx(tx, tenant_id, source_id, ConversationRecordKind::Session)
        .await?;
    sqlx::query(
        "DELETE FROM conversation_session_observations WHERE tenant_id = ?1 AND source_id = ?2 AND record_kind = 'session'",
    )
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query(
        r#"
        DELETE FROM conversation_parts
        WHERE tenant_id = ?1
          AND turn_id IN (
            SELECT t.id
            FROM conversation_turns t
            JOIN conversation_sessions s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
            WHERE s.tenant_id = ?1 AND s.source_id = ?2
        )
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query(
        r#"
        DELETE FROM conversation_turns
        WHERE tenant_id = ?1
          AND session_id IN (
            SELECT id FROM conversation_sessions
            WHERE tenant_id = ?1 AND source_id = ?2
          )
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query("DELETE FROM conversation_sessions WHERE tenant_id = ?1 AND source_id = ?2")
        .bind(tenant_id)
        .bind(source_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}
