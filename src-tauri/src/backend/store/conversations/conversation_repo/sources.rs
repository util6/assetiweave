use super::*;

pub(crate) async fn list_conversation_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Vec<ConversationSource>> {
    let rows = sqlx::query(LIST_CONVERSATION_SOURCES_SQL)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    rows.iter().map(map_sqlx_conversation_source).collect()
}

pub(crate) async fn load_conversation_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> StoreResult<Option<ConversationSource>> {
    sqlx::query(LOAD_CONVERSATION_SOURCE_SQL)
        .bind(tenant_id)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(StoreError::external)?
        .as_ref()
        .map(map_sqlx_conversation_source)
        .transpose()
}

pub(crate) async fn upsert_conversation_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
) -> StoreResult<()> {
    sqlx::query(UPSERT_CONVERSATION_SOURCE_SQL)
        .bind(tenant_id)
        .bind(&source.id)
        .bind(&source.adapter_id)
        .bind(&source.name)
        .bind(encode_enum(source.kind)?)
        .bind(&source.location)
        .bind(&source.config_json)
        .bind(if source.enabled { 1 } else { 0 })
        .bind(&source.last_synced_at)
        .bind(&source.last_sync_status)
        .bind(&source.created_at)
        .bind(&source.updated_at)
        .execute(pool)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn disable_conversation_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> StoreResult<ConversationSource> {
    let mut source = load_conversation_source_sqlx(pool, tenant_id, source_id)
        .await?
        .ok_or_else(|| {
            StoreError::external(format!("conversation source not found: {source_id}"))
        })?;
    source.enabled = false;
    source.updated_at = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    sqlx::query(
        "UPDATE conversation_sources SET enabled = 0, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3",
    )
    .bind(&source.updated_at)
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND source_id = ?3 AND status = 'active'",
    )
    .bind(&source.updated_at)
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    tx.commit().await.map_err(StoreError::external)?;
    Ok(source)
}
