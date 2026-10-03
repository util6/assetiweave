use super::*;

pub(crate) async fn load_conversation_session_versions_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
    record_kind: ConversationRecordKind,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    projection_version: Option<u32>,
) -> StoreResult<std::collections::BTreeMap<String, String>> {
    let kind_str = match record_kind {
        ConversationRecordKind::Session => "session",
        ConversationRecordKind::Web => "web",
    };

    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT external_id, hydrated_version
        FROM conversation_session_observations
        WHERE tenant_id = ?1 AND source_id = ?2 AND record_kind = ?3
          AND dirty = 0
          AND COALESCE(hydrated_adapter_hash, '') = COALESCE(?4, '')
          AND COALESCE(hydrated_card_contract_version, 0) = COALESCE(?5, 0)
          AND COALESCE(hydrated_payload_policy_version, 0) = ?6
          AND COALESCE(hydrated_projection_version, 0) = COALESCE(?7, 0)
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(kind_str)
    .bind(adapter_content_hash)
    .bind(card_contract_version.map(i64::from))
    .bind(i64::from(payload_policy_version))
    .bind(projection_version.map(i64::from))
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    let mut map = std::collections::BTreeMap::new();
    for (ext_id, version) in rows {
        if let Some(v) = version {
            map.insert(ext_id, v);
        }
    }
    Ok(map)
}

pub(crate) async fn conversation_payload_policy_reparse_required_sqlx(
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

pub(crate) async fn mark_conversation_payload_policy_applied_sqlx(
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

pub(crate) async fn record_conversation_session_failure_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
    record_kind: &str,
    external_id: &str,
    observed_version: Option<&str>,
    error_code: &str,
    error_message: &str,
    error_stage: &str,
    retryable: bool,
) -> StoreResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let ver = observed_version.unwrap_or("");
    sqlx::query(
        r#"
        INSERT INTO conversation_session_observations (
            tenant_id, source_id, record_kind, external_id, observed_version,
            last_seen_at, source_presence, dirty,
            error_code, error_message, error_stage, retryable,
            attempt_count, last_attempt_at, last_failure_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'present', 1, ?7, ?8, ?9, ?10, 1, ?6, ?6)
        ON CONFLICT(tenant_id, source_id, record_kind, external_id) DO UPDATE SET
            observed_version = CASE
                WHEN excluded.observed_version != '' THEN excluded.observed_version
                ELSE conversation_session_observations.observed_version
            END,
            last_seen_at = excluded.last_seen_at,
            source_presence = 'present',
            dirty = 1,
            error_code = excluded.error_code,
            error_message = excluded.error_message,
            error_stage = excluded.error_stage,
            retryable = excluded.retryable,
            attempt_count = conversation_session_observations.attempt_count + 1,
            last_attempt_at = excluded.last_attempt_at,
            last_failure_at = excluded.last_failure_at
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(record_kind)
    .bind(external_id)
    .bind(ver)
    .bind(&now)
    .bind(error_code)
    .bind(error_message)
    .bind(error_stage)
    .bind(if retryable { 1 } else { 0 })
    .execute(pool)
    .await
    .map_err(StoreError::external)?;

    Ok(())
}

pub(crate) async fn upsert_single_session_observation_clean_sqlx_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tenant_id: &str,
    source_id: &str,
    record_kind: &str,
    external_id: &str,
    version_token: &str,
    now: &str,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    projection_version: Option<u32>,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_session_observations (
            tenant_id, source_id, record_kind, external_id, observed_version,
            hydrated_version, last_seen_at, source_presence, dirty,
            hydrated_adapter_hash, hydrated_card_contract_version,
            hydrated_payload_policy_version, hydrated_projection_version,
            error_code, error_message, error_stage, retryable,
            attempt_count, last_attempt_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, 'present', 0, ?7, ?8, ?9, ?10, NULL, NULL, NULL, NULL, 1, ?6)
        ON CONFLICT(tenant_id, source_id, record_kind, external_id) DO UPDATE SET
            observed_version = excluded.observed_version,
            hydrated_version = excluded.hydrated_version,
            last_seen_at = excluded.last_seen_at,
            source_presence = 'present',
            dirty = 0,
            hydrated_adapter_hash = excluded.hydrated_adapter_hash,
            hydrated_card_contract_version = excluded.hydrated_card_contract_version,
            hydrated_payload_policy_version = excluded.hydrated_payload_policy_version,
            hydrated_projection_version = excluded.hydrated_projection_version,
            error_code = NULL,
            error_message = NULL,
            error_stage = NULL,
            retryable = NULL,
            attempt_count = conversation_session_observations.attempt_count + 1,
            last_attempt_at = excluded.last_attempt_at
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(record_kind)
    .bind(external_id)
    .bind(version_token)
    .bind(now)
    .bind(adapter_content_hash)
    .bind(card_contract_version.map(i64::from))
    .bind(i64::from(payload_policy_version))
    .bind(projection_version.map(i64::from))
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    Ok(())
}

pub(crate) async fn persist_conversation_session_observations_sqlx(
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

    let mut tx = pool.begin().await.map_err(StoreError::external)?;

    let now = chrono::Utc::now().to_rfc3339();

    for desc in session_descriptors {
        let presence = "present";
        let hydrated_version = if hydrated_external_ids.contains(&desc.external_id) {
            Some(&desc.version_token)
        } else {
            None
        };

        if let Some(hv) = hydrated_version {
            sqlx::query(
                r#"
                INSERT INTO conversation_session_observations (
                    tenant_id, source_id, record_kind, external_id, observed_version,
                    hydrated_version, last_seen_at, source_presence, dirty,
                    hydrated_adapter_hash, hydrated_card_contract_version,
                    hydrated_payload_policy_version, hydrated_projection_version,
                    error_code, error_message, error_stage, retryable,
                    attempt_count, last_attempt_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10, ?11, ?12, NULL, NULL, NULL, NULL, 1, ?7)
                ON CONFLICT(tenant_id, source_id, record_kind, external_id) DO UPDATE SET
                    observed_version = excluded.observed_version,
                    hydrated_version = excluded.hydrated_version,
                    last_seen_at = excluded.last_seen_at,
                    source_presence = excluded.source_presence,
                    hydrated_adapter_hash = excluded.hydrated_adapter_hash,
                    hydrated_card_contract_version = excluded.hydrated_card_contract_version,
                    hydrated_payload_policy_version = excluded.hydrated_payload_policy_version,
                    hydrated_projection_version = excluded.hydrated_projection_version,
                    error_code = NULL,
                    error_message = NULL,
                    error_stage = NULL,
                    retryable = NULL,
                    dirty = 0
                "#,
            )
            .bind(tenant_id)
            .bind(source_id)
            .bind(kind_str)
            .bind(&desc.external_id)
            .bind(&desc.version_token)
            .bind(hv)
            .bind(&now)
            .bind(presence)
            .bind(adapter_content_hash)
            .bind(card_contract_version.map(i64::from))
            .bind(i64::from(payload_policy_version))
            .bind(projection_version.map(i64::from))
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
        } else {
            // Note: Keep dirty and error info intact if already dirty/failed!
            sqlx::query(
                r#"
                INSERT INTO conversation_session_observations (
                    tenant_id, source_id, record_kind, external_id, observed_version,
                    last_seen_at, source_presence, dirty, hydrated_adapter_hash,
                    hydrated_card_contract_version,
                    hydrated_payload_policy_version, hydrated_projection_version
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10, ?11)
                ON CONFLICT(tenant_id, source_id, record_kind, external_id) DO UPDATE SET
                    observed_version = excluded.observed_version,
                    last_seen_at = excluded.last_seen_at,
                    source_presence = excluded.source_presence,
                    hydrated_adapter_hash = excluded.hydrated_adapter_hash,
                    hydrated_card_contract_version = excluded.hydrated_card_contract_version,
                    hydrated_payload_policy_version = excluded.hydrated_payload_policy_version,
                    hydrated_projection_version = excluded.hydrated_projection_version
                "#,
            )
            .bind(tenant_id)
            .bind(source_id)
            .bind(kind_str)
            .bind(&desc.external_id)
            .bind(&desc.version_token)
            .bind(&now)
            .bind(presence)
            .bind(adapter_content_hash)
            .bind(card_contract_version.map(i64::from))
            .bind(i64::from(payload_policy_version))
            .bind(projection_version.map(i64::from))
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
        }
    }

    // Mark missing as absent
    if !session_descriptors.is_empty() {
        sqlx::query(
            r#"
            UPDATE conversation_session_observations
            SET source_presence = 'absent'
            WHERE tenant_id = ?1 AND source_id = ?2 AND record_kind = ?3 AND last_seen_at < ?4
            "#,
        )
        .bind(tenant_id)
        .bind(source_id)
        .bind(kind_str)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    }
    tx.commit().await.map_err(StoreError::external)?;
    Ok(session_descriptors.len())
}
