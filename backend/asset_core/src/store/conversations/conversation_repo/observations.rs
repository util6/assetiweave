use super::*;

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
    let source_contract: Option<(Option<String>, Option<i64>, Option<i64>, Option<i64>)> =
        sqlx::query_as(
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
        .map_err(StoreError::external)?;

    let contract_matches = if let Some((hash, card_ver, policy_ver, proj_ver)) = source_contract {
        let card_ver = card_ver.and_then(|v| u32::try_from(v).ok());
        let policy_ver = policy_ver.and_then(|v| u32::try_from(v).ok());
        let proj_ver = proj_ver.and_then(|v| u32::try_from(v).ok());

        hash.as_deref() == adapter_content_hash
            && card_ver == card_contract_version
            && policy_ver == Some(payload_policy_version)
            && proj_ver == projection_version
    } else {
        false
    };

    let kind_str = match record_kind {
        ConversationRecordKind::Session => "session",
        ConversationRecordKind::Web => "web",
    };

    if !contract_matches {
        sqlx::query(
            "DELETE FROM conversation_session_observations WHERE tenant_id = ?1 AND source_id = ?2 AND record_kind = ?3",
        )
        .bind(tenant_id)
        .bind(source_id)
        .bind(kind_str)
        .execute(pool)
        .await
        .map_err(StoreError::external)?;

        return Ok(std::collections::BTreeMap::new());
    }

    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT external_id, synced_version
        FROM conversation_session_observations
        WHERE tenant_id = ?1 AND source_id = ?2 AND record_kind = ?3
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(kind_str)
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
/// 失败的会话直接从水位表中删除对应行，确保下一轮增量同步必然重新拉取解析。
pub async fn record_conversation_session_failure_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
    record_kind: &str,
    external_id: &str,
    _observed_version: Option<&str>,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        DELETE FROM conversation_session_observations
        WHERE tenant_id = ?1 AND source_id = ?2 AND record_kind = ?3 AND external_id = ?4
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(record_kind)
    .bind(external_id)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;

    Ok(())
}

pub async fn upsert_single_session_observation_clean_sqlx_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tenant_id: &str,
    source_id: &str,
    record_kind: &str,
    external_id: &str,
    version_token: &str,
    _now: &str,
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

    sqlx::query(
        r#"
        INSERT INTO conversation_session_observations (
            tenant_id, source_id, record_kind, external_id, synced_version
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(tenant_id, source_id, record_kind, external_id) DO UPDATE SET
            synced_version = excluded.synced_version
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(record_kind)
    .bind(external_id)
    .bind(version_token)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    Ok(())
}

pub async fn persist_conversation_session_observations_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
    record_kind: ConversationRecordKind,
    session_descriptors: &[crate::backend::domain::conversations::ConversationSessionObservation],
    hydrated_external_ids: &std::collections::BTreeSet<String>,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    projection_version: Option<u32>,
) -> StoreResult<usize> {
    let kind_str = match record_kind {
        ConversationRecordKind::Session => "session",
        ConversationRecordKind::Web => "web",
    };

    let mut tx = crate::backend::store::begin_immediate(pool)
        .await
        .map_err(StoreError::external)?;

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
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;

    let mut written = 0;
    for desc in session_descriptors {
        if hydrated_external_ids.contains(&desc.external_id) {
            sqlx::query(
                r#"
                INSERT INTO conversation_session_observations (
                    tenant_id, source_id, record_kind, external_id, synced_version
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT(tenant_id, source_id, record_kind, external_id) DO UPDATE SET
                    synced_version = excluded.synced_version
                "#,
            )
            .bind(tenant_id)
            .bind(source_id)
            .bind(kind_str)
            .bind(&desc.external_id)
            .bind(&desc.version_token)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
            written += 1;
        }
    }

    tx.commit().await.map_err(StoreError::external)?;
    Ok(written)
}
