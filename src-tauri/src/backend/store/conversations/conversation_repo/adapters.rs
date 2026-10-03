use super::*;

pub(crate) async fn seed_prepared_builtin_conversation_adapters_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapters: Vec<ConversationAdapter>,
) -> StoreResult<()> {
    let now = Utc::now().to_rfc3339();
    for mut adapter in adapters {
        match load_conversation_adapter_sqlx(pool, tenant_id, &adapter.id).await? {
            Some(existing) if existing.trust_state != ConversationAdapterTrustState::BuiltIn => {}
            Some(existing) => {
                adapter.enabled = existing.enabled;
                upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter).await?;
            }
            None => upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter).await?,
        }
    }
    for source in builtin_sources(&now) {
        if load_conversation_source_sqlx(pool, tenant_id, &source.id)
            .await?
            .is_none()
        {
            upsert_conversation_source_sqlx(pool, tenant_id, &source).await?;
        }
    }
    Ok(())
}

pub(crate) async fn list_conversation_adapters_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Vec<ConversationAdapter>> {
    let rows = sqlx::query(LIST_CONVERSATION_ADAPTERS_SQL)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    rows.iter().map(map_sqlx_conversation_adapter).collect()
}

pub(crate) async fn upsert_conversation_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter: &ConversationAdapter,
) -> StoreResult<()> {
    upsert_conversation_adapter_with_executor(pool, tenant_id, adapter).await
}

pub(super) async fn upsert_conversation_adapter_with_executor<'e, E>(
    executor: E,
    tenant_id: &str,
    adapter: &ConversationAdapter,
) -> StoreResult<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query(UPSERT_CONVERSATION_ADAPTER_SQL)
        .bind(tenant_id)
        .bind(&adapter.id)
        .bind(&adapter.name)
        .bind(encode_enum(adapter.kind)?)
        .bind(&adapter.version)
        .bind(if adapter.enabled { 1 } else { 0 })
        .bind(&adapter.manifest_path)
        .bind(&adapter.executable_path)
        .bind(&adapter.content_hash)
        .bind(&adapter.trusted_hash)
        .bind(encode_enum(adapter.trust_state)?)
        .bind(adapter.protocol_version.map(i64::from))
        .bind(encode_json(&adapter.capabilities)?)
        .bind(encode_json(&adapter.input_kinds)?)
        .bind(adapter.card_contract_version.map(i64::from))
        .bind(encode_json(&adapter.card_kinds)?)
        .bind(adapter.projection_version.map(i64::from))
        .bind(&adapter.created_at)
        .bind(&adapter.updated_at)
        .execute(executor)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

pub(super) async fn upsert_conversation_adapter_for_all_tenants(
    tx: &mut Transaction<'_, Sqlite>,
    adapter: &ConversationAdapter,
) -> StoreResult<()> {
    let tenant_ids = sqlx::query_scalar::<_, String>("SELECT id FROM tenants ORDER BY id")
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    for tenant_id in tenant_ids {
        upsert_conversation_adapter_with_executor(&mut **tx, &tenant_id, adapter).await?;
    }
    Ok(())
}

pub(crate) async fn set_app_conversation_adapter_projection_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
    adapter: Option<&ConversationAdapter>,
) -> StoreResult<()> {
    if let Some(adapter) = adapter {
        return upsert_conversation_adapter_sqlx(pool, tenant_id, adapter).await;
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    sqlx::query(
        "DELETE FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2 AND trust_state != 'built_in'",
    )
    .bind(tenant_id)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query(DISABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL)
        .bind(&now)
        .bind(tenant_id)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND source_id IN (SELECT id FROM conversation_sources WHERE tenant_id = ?2 AND adapter_id = ?3)",
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    tx.commit().await.map_err(StoreError::external)
}

pub(crate) async fn delete_conversation_adapter_registration_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
    package_id: Option<&str>,
) -> StoreResult<Option<ConversationAdapter>> {
    let adapter = load_conversation_adapter_sqlx(pool, tenant_id, adapter_id).await?;
    if let Some(adapter) = adapter.as_ref() {
        if adapter.trust_state == ConversationAdapterTrustState::BuiltIn {
            return disable_builtin_conversation_adapter_sqlx(pool, tenant_id, adapter_id)
                .await
                .map(Some);
        }
        if adapter.kind != ConversationAdapterKind::External {
            return Err(StoreError::Validation(
                "only external conversation adapters can be unregistered".to_string(),
            ));
        }
    } else if package_id.is_none() {
        return Err(StoreError::NotFound(format!(
            "conversation adapter not found: {adapter_id}"
        )));
    }
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    if adapter.is_some() {
        if package_id.is_some() {
            sqlx::query("DELETE FROM conversation_adapters WHERE id = ?1")
                .bind(adapter_id)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::external)?;
        } else {
            sqlx::query(DELETE_CONVERSATION_ADAPTER_SQL)
                .bind(tenant_id)
                .bind(adapter_id)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::external)?;
        }
    }
    let now = Utc::now().to_rfc3339();
    if package_id.is_some() {
        sqlx::query(
            "UPDATE conversation_sources SET enabled = 0, updated_at = ?1 WHERE adapter_id = ?2",
        )
        .bind(&now)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    } else {
        sqlx::query(DISABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL)
            .bind(&now)
            .bind(tenant_id)
            .bind(adapter_id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
    }
    if let Some(package_id) = package_id {
        sqlx::query(DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL)
            .bind(package_id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
    }
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND source_id IN (SELECT id FROM conversation_sources WHERE tenant_id = ?2 AND adapter_id = ?3)",
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    tx.commit().await.map_err(StoreError::external)?;
    Ok(adapter)
}

pub(crate) async fn disable_builtin_conversation_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> StoreResult<ConversationAdapter> {
    let mut adapter = load_conversation_adapter_sqlx(pool, tenant_id, adapter_id)
        .await?
        .ok_or_else(|| {
            StoreError::external(format!("conversation adapter not found: {adapter_id}"))
        })?;
    if adapter.trust_state != ConversationAdapterTrustState::BuiltIn {
        return Err(StoreError::Validation(
            "only built-in conversation adapters use the disable workflow".to_string(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    sqlx::query(DISABLE_CONVERSATION_ADAPTER_SQL)
        .bind(&now)
        .bind(tenant_id)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    sqlx::query(DISABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL)
        .bind(&now)
        .bind(tenant_id)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND source_id IN (SELECT id FROM conversation_sources WHERE tenant_id = ?2 AND adapter_id = ?3)",
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    tx.commit().await.map_err(StoreError::external)?;

    adapter.enabled = false;
    adapter.updated_at = now;
    Ok(adapter)
}

pub(crate) async fn enable_conversation_sources_by_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> StoreResult<()> {
    sqlx::query(ENABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL)
        .bind(Utc::now().to_rfc3339())
        .bind(tenant_id)
        .bind(adapter_id)
        .execute(pool)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn load_conversation_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> StoreResult<Option<ConversationAdapter>> {
    sqlx::query(LOAD_CONVERSATION_ADAPTER_SQL)
        .bind(tenant_id)
        .bind(adapter_id)
        .fetch_optional(pool)
        .await
        .map_err(StoreError::external)?
        .as_ref()
        .map(map_sqlx_conversation_adapter)
        .transpose()
}
