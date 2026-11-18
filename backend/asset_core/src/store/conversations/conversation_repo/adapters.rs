use super::*;

/// Seed the adapters discovered in the managed user directory.
///
/// #195：受管目录是用户可写的位置，磁盘内容本身不携带信任。这里只做两件事：
///
/// - 不存在行：以调用方给出的（未信任）状态插入；
/// - 已存在的「受管目录投影」行（`built_in` 历史行 / `untrusted` 行）：刷新观测字段
///   （版本、`content_hash`、清单与可执行路径等），**绝不覆盖** `trusted_hash`
///   与 `trust_state`，因此每轮引导都不会把磁盘当前内容重新签成受信任 hash。
///
/// 已经由晋升 / 安装 / 注册建立信任的行（`trusted` / `changed`）完全不动：它们的
/// 路径指向不可变快照，不能被可变工作区目录覆盖。
pub async fn seed_discovered_conversation_adapters_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapters: Vec<ConversationAdapter>,
) -> StoreResult<()> {
    for adapter in adapters {
        match load_conversation_adapter_sqlx(pool, tenant_id, &adapter.id).await? {
            Some(existing)
                if existing.trust_state != ConversationAdapterTrustState::BuiltIn
                    && existing.trust_state != ConversationAdapterTrustState::Untrusted => {}
            Some(existing) => {
                let observation = refresh_managed_adapter_observation(existing, adapter);
                upsert_conversation_adapter_sqlx(pool, tenant_id, &observation).await?;
            }
            None => upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter).await?,
        }
    }
    seed_builtin_conversation_sources_sqlx(pool, tenant_id).await?;
    Ok(())
}

/// Merge the freshly observed manifest fields onto an existing managed-directory
/// row while preserving everything that carries trust or user intent.
fn refresh_managed_adapter_observation(
    mut existing: ConversationAdapter,
    observed: ConversationAdapter,
) -> ConversationAdapter {
    existing.name = observed.name;
    existing.version = observed.version;
    existing.manifest_path = observed.manifest_path;
    existing.executable_path = observed.executable_path;
    existing.content_hash = observed.content_hash;
    existing.protocol_version = observed.protocol_version;
    existing.capabilities = observed.capabilities;
    existing.input_kinds = observed.input_kinds;
    existing.card_contract_version = observed.card_contract_version;
    existing.card_kinds = observed.card_kinds;
    existing.projection_version = observed.projection_version;
    existing.updated_at = observed.updated_at;
    // `enabled`, `trusted_hash`, `trust_state`, `created_at` stay untouched.
    existing
}

/// 只给「当前租户已投影存在适配器」的内置来源落库。
///
/// #196 之后二进制不再内嵌官方适配器包，全新安装的受管目录可以是空目录，
/// 此时一条适配器行都没有。指向不存在适配器的来源永远无法同步，所以来源行
/// 必须跟着适配器存在性走：适配器在场才物化对应来源。
///
/// 幂等：已存在的来源行永不重写，用户的启用/停用开关与自定义配置在每次
/// 引导、重复注册后都保持原样。返回本次新插入的行数。
pub async fn seed_builtin_conversation_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<usize> {
    let now = Utc::now().to_rfc3339();
    let mut inserted = 0;
    for source in builtin_sources(&now) {
        if load_conversation_adapter_sqlx(pool, tenant_id, &source.adapter_id)
            .await?
            .is_none()
        {
            continue;
        }
        if load_conversation_source_sqlx(pool, tenant_id, &source.id)
            .await?
            .is_none()
        {
            upsert_conversation_source_sqlx(pool, tenant_id, &source).await?;
            inserted += 1;
        }
    }
    Ok(inserted)
}

pub async fn list_conversation_adapters_sqlx(
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

pub async fn upsert_conversation_adapter_sqlx(
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

pub async fn set_app_conversation_adapter_projection_sqlx(
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
    tx.commit().await.map_err(StoreError::external)
}

pub async fn delete_conversation_adapter_registration_sqlx(
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
    tx.commit().await.map_err(StoreError::external)?;
    Ok(adapter)
}

pub async fn disable_builtin_conversation_adapter_sqlx(
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
    tx.commit().await.map_err(StoreError::external)?;

    adapter.enabled = false;
    adapter.updated_at = now;
    Ok(adapter)
}

pub async fn enable_conversation_sources_by_adapter_sqlx(
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

pub async fn load_conversation_adapter_sqlx(
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
