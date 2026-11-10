//! 会话增量同步的**水位装配**：源级契约的回签与按源装载版本指纹。
//!
//! 历史包袱说明：本模块曾承载 `conversation_session_observations` 逐会话水位行，
//! 该表已在 #182 下线。现在“水位”只剩两段——一个源一行的同步契约，
//! 以及会话记录自身的 `source_fingerprint`；本模块不再产生任何删除写入。

use super::*;

/// 数据源级的同步契约。四个版本字段描述「上一轮同步是用什么解析契约完成的」，
/// 它们只用于观测与载荷策略闸门，不再以「删除会话水位」的方式表达失效。
#[derive(Debug, sqlx::FromRow)]
struct SourceSyncContractRow {
    synced_adapter_hash: Option<String>,
    synced_card_contract_version: Option<i64>,
    synced_payload_policy_version: Option<i64>,
    synced_projection_version: Option<i64>,
}

async fn load_source_sync_contract_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> StoreResult<Option<SourceSyncContractRow>> {
    sqlx::query_as::<_, SourceSyncContractRow>(
        r#"
        SELECT synced_adapter_hash, synced_card_contract_version,
               synced_payload_policy_version, synced_projection_version
        FROM conversation_sources
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)
}

/// 装载某数据源已完成同步的会话版本指纹，供增量差集剔除未变更会话。
///
/// 水位不再单独持有 per-session 行：每会话版本由会话记录自身的
/// `source_fingerprint` 承载，源级契约只保留「载荷策略版本」这一条必须全量
/// 重解析的闸门。适配器内容哈希、卡片契约与投影版本的变化都不再作废水位，
/// 而是交由每条会话的版本令牌自证内容是否变化。
///
/// 本函数不产生任何删除写入：契约失配只让本轮差集退化，绝不销毁既有证据。
pub async fn load_conversation_session_versions_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
    record_kind: ConversationRecordKind,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    projection_version: Option<u32>,
) -> StoreResult<std::collections::BTreeMap<String, String>> {
    let Some(contract) = load_source_sync_contract_sqlx(pool, tenant_id, source_id).await? else {
        // 源尚未建立契约（首次同步）：差集为空，本轮全量。
        return Ok(std::collections::BTreeMap::new());
    };

    if contract.synced_payload_policy_version != Some(i64::from(payload_policy_version)) {
        // 载荷策略升级会改变解析产物的语义，而源文件指纹不变，
        // 因此这是唯一必须整源重解析的合法入口。
        tracing::info!(
            action = "conversation.sync.payload_policy_changed",
            source_id = %source_id,
            stored = ?contract.synced_payload_policy_version,
            current = payload_policy_version,
            "Payload policy version changed; reparsing the whole source"
        );
        return Ok(std::collections::BTreeMap::new());
    }

    if contract.synced_adapter_hash.as_deref() != adapter_content_hash
        || contract.synced_card_contract_version != card_contract_version.map(i64::from)
        || contract.synced_projection_version != projection_version.map(i64::from)
    {
        // 适配器包升级不再触发整源重解析：版本令牌已内含卡片模式版本与文件指纹，
        // 真正变化的会话会在差集中自然浮现。
        tracing::debug!(
            action = "conversation.sync.adapter_contract_drift",
            source_id = %source_id,
            "Adapter contract drifted; session fingerprints stay authoritative"
        );
    }

    let rows: Vec<(String, String)> = match record_kind {
        ConversationRecordKind::Session => sqlx::query_as(
            r#"
            SELECT external_id, source_fingerprint
            FROM conversation_sessions
            WHERE tenant_id = ?1 AND source_id = ?2
              AND missing = 0
              AND source_fingerprint IS NOT NULL
              AND trim(source_fingerprint) <> ''
            "#,
        ),
        ConversationRecordKind::Web => sqlx::query_as(
            r#"
            SELECT external_id, source_fingerprint
            FROM web_record_sessions
            WHERE tenant_id = ?1 AND source_id = ?2
              AND missing = 0
              AND source_fingerprint IS NOT NULL
              AND trim(source_fingerprint) <> ''
            "#,
        ),
    }
    .bind(tenant_id)
    .bind(source_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    Ok(rows.into_iter().collect())
}

pub async fn conversation_payload_policy_reparse_required_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    payload_policy_version: u32,
) -> StoreResult<bool> {
    let applied_version = sqlx::query_scalar::<_, i64>(
        "SELECT applied_version FROM conversation_payload_policy_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;
    if applied_version.is_some_and(|version| version >= i64::from(payload_policy_version)) {
        return Ok(false);
    }

    let stored_session_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM conversation_sessions records
        JOIN conversation_sources sources
          ON sources.tenant_id = records.tenant_id AND sources.id = records.source_id
        WHERE records.tenant_id = ?1 AND records.missing = 0 AND sources.enabled = 1
        "#,
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(StoreError::external)?;
    let stored_web_record_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM web_record_sessions records
        JOIN conversation_sources sources
          ON sources.tenant_id = records.tenant_id AND sources.id = records.source_id
        WHERE records.tenant_id = ?1 AND records.missing = 0 AND sources.enabled = 1
        "#,
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(StoreError::external)?;
    if stored_session_count + stored_web_record_count > 0 {
        return Ok(true);
    }

    mark_conversation_payload_policy_applied_sqlx(pool, tenant_id, payload_policy_version).await?;
    Ok(false)
}

pub async fn mark_conversation_payload_policy_applied_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    payload_policy_version: u32,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_payload_policy_state (tenant_id, applied_version, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(tenant_id) DO UPDATE SET
            applied_version = MAX(conversation_payload_policy_state.applied_version, excluded.applied_version),
            updated_at = excluded.updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(i64::from(payload_policy_version))
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

/// 标记单条会话本轮解析失败。
///
/// 失效信号落在会话记录自身：清空版本指纹，使下一轮差集必然重新纳入该会话。
/// 不再通过删除独立水位行来表达重试意图。
pub async fn record_conversation_session_failure_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
    record_kind: &str,
    external_id: &str,
) -> StoreResult<()> {
    let statement = if record_kind == "web" {
        "UPDATE web_record_sessions SET source_fingerprint = NULL \
         WHERE tenant_id = ?1 AND source_id = ?2 AND external_id = ?3"
    } else {
        "UPDATE conversation_sessions SET source_fingerprint = NULL \
         WHERE tenant_id = ?1 AND source_id = ?2 AND external_id = ?3"
    };
    sqlx::query(statement)
        .bind(tenant_id)
        .bind(source_id)
        .bind(external_id)
        .execute(pool)
        .await
        .map_err(StoreError::external)?;

    Ok(())
}

/// 回签数据源的同步契约：一行一个源，记录本轮同步所依据的契约版本。
#[allow(clippy::too_many_arguments)]
pub async fn stamp_source_sync_contract_sqlx_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tenant_id: &str,
    source_id: &str,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    projection_version: Option<u32>,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        UPDATE conversation_sources
        SET synced_adapter_hash = ?1,
            synced_card_contract_version = ?2,
            synced_payload_policy_version = ?3,
            synced_projection_version = ?4
        WHERE tenant_id = ?5 AND id = ?6
        "#,
    )
    .bind(adapter_content_hash)
    .bind(card_contract_version.map(i64::from))
    .bind(i64::from(payload_policy_version))
    .bind(projection_version.map(i64::from))
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn stamp_source_sync_contract_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    projection_version: Option<u32>,
) -> StoreResult<()> {
    let mut tx = crate::backend::store::begin_immediate(pool)
        .await
        .map_err(StoreError::external)?;
    stamp_source_sync_contract_sqlx_tx(
        &mut tx,
        tenant_id,
        source_id,
        adapter_content_hash,
        card_contract_version,
        payload_policy_version,
        projection_version,
    )
    .await?;
    tx.commit().await.map_err(StoreError::external)?;
    Ok(())
}
