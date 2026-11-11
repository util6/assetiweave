use super::*;

pub async fn delete_web_record_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<()> {
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
    sqlx::query("DELETE FROM web_record_sessions WHERE tenant_id = ?1 AND id = ?2")
        .bind(tenant_id)
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    mark_search_index_behind_sqlx_tx(tx, tenant_id).await?;
    Ok(())
}

pub async fn clear_legacy_conversation_records_for_source_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    source_id: &str,
) -> StoreResult<()> {
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
    mark_search_index_behind_sqlx_tx(tx, tenant_id).await?;
    Ok(())
}

/// 事实被删除后，派生索引必然落后。
///
/// 这里只把代际标记为落后：检索会立刻切换到"读已提交事实"的正确回退，
/// 孤儿文档由下一次受控重建清理，绝不在事实事务里写索引。
async fn mark_search_index_behind_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
) -> StoreResult<()> {
    super::super::conversation_repo::bump_conversation_search_source_revision_sqlx_tx(
        &mut **tx, tenant_id,
    )
    .await?;
    Ok(())
}
