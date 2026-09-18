use crate::backend::dto::{
    ConversationBlockDetail, ConversationBlockLocator, ConversationCardRenderer,
    ConversationContentNode, ConversationMutationResult, ConversationQuestionDetail,
    ConversationRecordKind, ConversationSearchCardType, ConversationSearchHit,
    ConversationSearchPage, ConversationSessionDetail, ConversationSessionListItem,
};
use crate::backend::events::DomainEvent;
use crate::backend::models::{
    conversation_turn_fingerprint, group_turn_ids_by_question, ConversationAdapter,
    ConversationAdapterCatalogRelease, ConversationAdapterKind, ConversationAdapterPackage,
    ConversationAdapterPackageOrigin, ConversationAdapterPackageVersion,
    ConversationAdapterRuntimeGateStatus, ConversationAdapterTrustState,
    ConversationCardKindDefinition, ConversationGroupingOrigin, ConversationPart,
    ConversationQuestion, ConversationQuestionTurn, ConversationSession, ConversationSource,
    ConversationSourceKind, ConversationSyncRun, ConversationSyncStatus, ConversationTurn,
    NormalizedConversationSession,
};
use crate::backend::projection::conversation_cards::ConversationCard;
use crate::backend::projection::conversation_content_nodes::{
    project_conversation_content_nodes, ConversationContentNodeCandidate,
};
use crate::backend::runtime::{AppError, AppResult};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteRow, AssertSqlSafe, Executor, FromRow, Sqlite, SqlitePool, Transaction};
use std::collections::{BTreeMap, BTreeSet};

use super::codec::{decode_enum, decode_json, encode_enum, encode_json};

pub(super) const CONVERSATION_IMPORT_BATCH_SIZE: usize = 8;

#[derive(Debug, Clone)]
pub(crate) struct RecentConversationSessionRecord {
    pub(crate) session: ConversationSessionListItem,
    pub(crate) last_activity_at: String,
    pub(crate) cwd: Option<String>,
    pub(crate) source_agent: String,
    pub(crate) recent_events: Vec<crate::backend::models::RecentMemoryEvent>,
}

const LIST_CONVERSATION_ADAPTERS_SQL: &str = r#"
    SELECT id, name, kind, version, enabled, manifest_path, executable_path,
           content_hash, trusted_hash, trust_state, protocol_version,
           capabilities, input_kinds, card_contract_version, card_kinds_json,
           created_at, updated_at
    FROM conversation_adapters
    WHERE tenant_id = ?1
    ORDER BY kind ASC, name ASC
    "#;

const LOAD_CONVERSATION_ADAPTER_SQL: &str = r#"
    SELECT id, name, kind, version, enabled, manifest_path, executable_path,
           content_hash, trusted_hash, trust_state, protocol_version,
           capabilities, input_kinds, card_contract_version, card_kinds_json,
           created_at, updated_at
    FROM conversation_adapters
    WHERE tenant_id = ?1 AND id = ?2
    "#;

const UPSERT_CONVERSATION_ADAPTER_SQL: &str = r#"
    INSERT INTO conversation_adapters (
        tenant_id, id, name, kind, version, enabled, manifest_path, executable_path,
        content_hash, trusted_hash, trust_state, protocol_version,
        capabilities, input_kinds, card_contract_version, card_kinds_json,
        created_at, updated_at
    )
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
    ON CONFLICT(tenant_id, id) DO UPDATE SET
        name = excluded.name,
        kind = excluded.kind,
        version = excluded.version,
        enabled = excluded.enabled,
        manifest_path = excluded.manifest_path,
        executable_path = excluded.executable_path,
        content_hash = excluded.content_hash,
        trusted_hash = excluded.trusted_hash,
        trust_state = excluded.trust_state,
        protocol_version = excluded.protocol_version,
        capabilities = excluded.capabilities,
        input_kinds = excluded.input_kinds,
        card_contract_version = excluded.card_contract_version,
        card_kinds_json = excluded.card_kinds_json,
        updated_at = excluded.updated_at
    "#;

const DELETE_CONVERSATION_ADAPTER_SQL: &str =
    "DELETE FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2";

const DISABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL: &str =
    "UPDATE conversation_sources SET enabled = 0, updated_at = ?1 WHERE tenant_id = ?2 AND adapter_id = ?3";
const DISABLE_CONVERSATION_ADAPTER_SQL: &str =
    "UPDATE conversation_adapters SET enabled = 0, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3";
const ENABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL: &str =
    "UPDATE conversation_sources SET enabled = 1, updated_at = ?1 WHERE tenant_id = ?2 AND adapter_id = ?3";

const LIST_CONVERSATION_ADAPTER_PACKAGES_SQL: &str = r#"
    SELECT package_id, adapter_id, name, version, record_kind, install_dir,
           manifest_path, adapter_manifest_path, runtime_protocol, runtime_ready,
           origin, source_url, git_ref, git_commit, catalog_url, update_policy,
           latest_version, last_checked_at, runtime_gate_status, runtime_validated_at,
           installed_content_hash, trusted_package_hash, error_message,
           created_at, updated_at
    FROM app_conversation_adapter_packages
    ORDER BY name ASC, package_id ASC
    "#;

const LOAD_CONVERSATION_ADAPTER_PACKAGE_SQL: &str = r#"
    SELECT package_id, adapter_id, name, version, record_kind, install_dir,
           manifest_path, adapter_manifest_path, runtime_protocol, runtime_ready,
           origin, source_url, git_ref, git_commit, catalog_url, update_policy,
           latest_version, last_checked_at, runtime_gate_status, runtime_validated_at,
           installed_content_hash, trusted_package_hash, error_message,
           created_at, updated_at
    FROM app_conversation_adapter_packages
    WHERE package_id = ?1
    "#;

const LOAD_CONVERSATION_ADAPTER_PACKAGE_BY_ADAPTER_SQL: &str = r#"
    SELECT package_id, adapter_id, name, version, record_kind, install_dir,
           manifest_path, adapter_manifest_path, runtime_protocol, runtime_ready,
           origin, source_url, git_ref, git_commit, catalog_url, update_policy,
           latest_version, last_checked_at, runtime_gate_status, runtime_validated_at,
           installed_content_hash, trusted_package_hash, error_message,
           created_at, updated_at
    FROM app_conversation_adapter_packages
    WHERE adapter_id = ?1
    ORDER BY updated_at DESC, package_id ASC
    LIMIT 1
    "#;

const UPSERT_CONVERSATION_ADAPTER_PACKAGE_SQL: &str = r#"
    INSERT INTO app_conversation_adapter_packages (
        package_id, adapter_id, name, version, record_kind, install_dir,
        manifest_path, adapter_manifest_path, runtime_protocol, runtime_ready,
        origin, source_url, git_ref, git_commit, catalog_url, update_policy,
        latest_version, last_checked_at, runtime_gate_status, runtime_validated_at,
        installed_content_hash, trusted_package_hash, error_message, created_at, updated_at
    )
    VALUES (
        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
        ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25
    )
    ON CONFLICT(package_id) DO UPDATE SET
        adapter_id = excluded.adapter_id,
        name = excluded.name,
        version = excluded.version,
        record_kind = excluded.record_kind,
        install_dir = excluded.install_dir,
        manifest_path = excluded.manifest_path,
        adapter_manifest_path = excluded.adapter_manifest_path,
        runtime_protocol = excluded.runtime_protocol,
        runtime_ready = excluded.runtime_ready,
        origin = excluded.origin,
        source_url = excluded.source_url,
        git_ref = excluded.git_ref,
        git_commit = excluded.git_commit,
        catalog_url = excluded.catalog_url,
        update_policy = excluded.update_policy,
        latest_version = excluded.latest_version,
        last_checked_at = excluded.last_checked_at,
        runtime_gate_status = excluded.runtime_gate_status,
        runtime_validated_at = excluded.runtime_validated_at,
        installed_content_hash = excluded.installed_content_hash,
        trusted_package_hash = excluded.trusted_package_hash,
        error_message = excluded.error_message,
        updated_at = excluded.updated_at
    "#;

const DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL: &str =
    "DELETE FROM app_conversation_adapter_packages WHERE package_id = ?1";

const LIST_CONVERSATION_SOURCES_SQL: &str = r#"
    SELECT id, adapter_id, name, kind, location, config_json, enabled,
           last_synced_at, last_sync_status, created_at, updated_at
    FROM conversation_sources
    WHERE tenant_id = ?1
    ORDER BY adapter_id ASC, name ASC
    "#;

const LOAD_CONVERSATION_SOURCE_SQL: &str = r#"
    SELECT id, adapter_id, name, kind, location, config_json, enabled,
           last_synced_at, last_sync_status, created_at, updated_at
    FROM conversation_sources
    WHERE tenant_id = ?1 AND id = ?2
    "#;

const UPSERT_CONVERSATION_SOURCE_SQL: &str = r#"
    INSERT INTO conversation_sources (
        tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
        last_synced_at, last_sync_status, created_at, updated_at
    )
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
    ON CONFLICT(tenant_id, id) DO UPDATE SET
        adapter_id = excluded.adapter_id,
        name = excluded.name,
        kind = excluded.kind,
        location = excluded.location,
        config_json = excluded.config_json,
        enabled = excluded.enabled,
        last_synced_at = excluded.last_synced_at,
        last_sync_status = excluded.last_sync_status,
        updated_at = excluded.updated_at
    "#;

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ConversationImportResult {
    pub(crate) source_id: String,
    pub(crate) adapter_id: String,
    pub(crate) dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sync_run_id: Option<String>,
    pub(crate) session_count: usize,
    pub(crate) skipped_session_count: usize,
    pub(crate) changed_session_count: usize,
    #[serde(default)]
    pub(crate) failed_session_count: usize,
    pub(crate) turn_count: usize,
    pub(crate) warning_count: usize,
    pub(crate) warnings: Vec<String>,
    #[serde(default)]
    pub(crate) session_failures: Vec<crate::backend::models::SessionSyncFailure>,
    #[serde(default)]
    pub(crate) session_warnings: Vec<crate::backend::models::SessionSyncWarning>,
    #[serde(default)]
    pub(crate) status: crate::backend::models::ConversationSyncStatus,
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct ConversationSyncDelta {
    pub(crate) sync_run_id: String,
    pub(crate) session_id: String,
    pub(crate) change_kind: String,
    pub(crate) observed_at: String,
}

pub(crate) async fn seed_prepared_builtin_conversation_adapters_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapters: Vec<ConversationAdapter>,
) -> AppResult<()> {
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

pub(crate) async fn migrate_legacy_conversation_adapter_hashes_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    for mut adapter in list_conversation_adapters_sqlx(pool, tenant_id).await? {
        if adapter.kind != ConversationAdapterKind::External {
            continue;
        }
        let Some(manifest_path) = adapter.manifest_path.clone() else {
            continue;
        };
        let Ok(validation) = crate::backend::conversations::validate_external_adapter(
            crate::backend::conversations::ExternalAdapterValidateParams { manifest_path },
        ) else {
            continue;
        };
        let Some(trusted_hash) = adapter.trusted_hash.as_deref() else {
            continue;
        };
        let content_hash = validation.content_hash.as_str();
        if trusted_hash == content_hash {
            if adapter.content_hash.as_deref() != Some(content_hash) {
                adapter.content_hash = Some(validation.content_hash);
                upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter).await?;
            }
            continue;
        }
        let legacy_executable_hash = validation.executable_hash.as_deref();
        let legacy_manifest_hash = validation.manifest_hash.as_str();
        if Some(trusted_hash) == legacy_executable_hash || trusted_hash == legacy_manifest_hash {
            adapter.content_hash = Some(validation.content_hash.clone());
            adapter.trusted_hash = Some(validation.content_hash);
            upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter).await?;
        }
    }
    Ok(())
}

pub(crate) async fn list_conversation_adapters_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<ConversationAdapter>> {
    let rows = sqlx::query(LIST_CONVERSATION_ADAPTERS_SQL)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.iter().map(map_sqlx_conversation_adapter).collect()
}

pub(crate) async fn upsert_conversation_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter: &ConversationAdapter,
) -> AppResult<()> {
    upsert_conversation_adapter_with_executor(pool, tenant_id, adapter).await
}

pub(crate) async fn normalize_conversation_paths_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    for adapter in list_conversation_adapters_sqlx(pool, tenant_id).await? {
        upsert_conversation_adapter_sqlx(pool, tenant_id, &adapter).await?;
    }
    for package in list_conversation_adapter_packages_sqlx(pool).await? {
        upsert_conversation_adapter_package_sqlx(pool, &package).await?;
    }

    #[derive(Debug, FromRow)]
    struct AdapterPackageInstallRow {
        package_id: String,
        version: String,
        install_dir: String,
    }

    let version_rows = sqlx::query_as::<_, AdapterPackageInstallRow>(
        r#"
        SELECT package_id, version, install_dir
        FROM app_conversation_adapter_package_versions
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    for row in version_rows {
        sqlx::query(
            r#"
            UPDATE app_conversation_adapter_package_versions
            SET install_dir = ?1
            WHERE package_id = ?2 AND version = ?3
            "#,
        )
        .bind(normalize_conversation_path(&row.install_dir)?)
        .bind(row.package_id)
        .bind(row.version)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
    }
    Ok(())
}

async fn upsert_conversation_adapter_with_executor<'e, E>(
    executor: E,
    tenant_id: &str,
    adapter: &ConversationAdapter,
) -> AppResult<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    let manifest_path = normalize_optional_conversation_path(adapter.manifest_path.as_deref())?;
    let executable_path = normalize_optional_conversation_path(adapter.executable_path.as_deref())?;
    sqlx::query(UPSERT_CONVERSATION_ADAPTER_SQL)
        .bind(tenant_id)
        .bind(&adapter.id)
        .bind(&adapter.name)
        .bind(encode_enum(adapter.kind)?)
        .bind(&adapter.version)
        .bind(if adapter.enabled { 1 } else { 0 })
        .bind(&manifest_path)
        .bind(&executable_path)
        .bind(&adapter.content_hash)
        .bind(&adapter.trusted_hash)
        .bind(encode_enum(adapter.trust_state)?)
        .bind(adapter.protocol_version.map(i64::from))
        .bind(encode_json(&adapter.capabilities)?)
        .bind(encode_json(&adapter.input_kinds)?)
        .bind(adapter.card_contract_version.map(i64::from))
        .bind(encode_json(&adapter.card_kinds)?)
        .bind(&adapter.created_at)
        .bind(&adapter.updated_at)
        .execute(executor)
        .await
        .map_err(AppError::external)?;
    Ok(())
}

async fn upsert_conversation_adapter_for_all_tenants(
    tx: &mut Transaction<'_, Sqlite>,
    adapter: &ConversationAdapter,
) -> AppResult<()> {
    let tenant_ids = sqlx::query_scalar::<_, String>("SELECT id FROM tenants ORDER BY id")
        .fetch_all(&mut **tx)
        .await
        .map_err(AppError::external)?;
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
) -> AppResult<()> {
    if let Some(adapter) = adapter {
        return upsert_conversation_adapter_sqlx(pool, tenant_id, adapter).await;
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    sqlx::query(
        "DELETE FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2 AND trust_state != 'built_in'",
    )
    .bind(tenant_id)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(DISABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL)
        .bind(&now)
        .bind(tenant_id)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND source_id IN (SELECT id FROM conversation_sources WHERE tenant_id = ?2 AND adapter_id = ?3)",
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)
}

pub(crate) async fn delete_conversation_adapter_registration_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
    package_id: Option<&str>,
) -> AppResult<Option<ConversationAdapter>> {
    let adapter = load_conversation_adapter_sqlx(pool, tenant_id, adapter_id).await?;
    if let Some(adapter) = adapter.as_ref() {
        if adapter.trust_state == ConversationAdapterTrustState::BuiltIn {
            return disable_builtin_conversation_adapter_sqlx(pool, tenant_id, adapter_id)
                .await
                .map(Some);
        }
        if adapter.kind != ConversationAdapterKind::External {
            return Err(AppError::Validation(
                "only external conversation adapters can be unregistered".to_string(),
            ));
        }
    } else if package_id.is_none() {
        return Err(AppError::NotFound(format!(
            "conversation adapter not found: {adapter_id}"
        )));
    }
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    if adapter.is_some() {
        if package_id.is_some() {
            sqlx::query("DELETE FROM conversation_adapters WHERE id = ?1")
                .bind(adapter_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;
        } else {
            sqlx::query(DELETE_CONVERSATION_ADAPTER_SQL)
                .bind(tenant_id)
                .bind(adapter_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::external)?;
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
        .map_err(AppError::external)?;
    } else {
        sqlx::query(DISABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL)
            .bind(&now)
            .bind(tenant_id)
            .bind(adapter_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
    }
    if let Some(package_id) = package_id {
        sqlx::query(DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL)
            .bind(package_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
    }
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND source_id IN (SELECT id FROM conversation_sources WHERE tenant_id = ?2 AND adapter_id = ?3)",
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;
    Ok(adapter)
}

pub(crate) async fn disable_builtin_conversation_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> AppResult<ConversationAdapter> {
    let mut adapter = load_conversation_adapter_sqlx(pool, tenant_id, adapter_id)
        .await?
        .ok_or_else(|| {
            AppError::external(format!("conversation adapter not found: {adapter_id}"))
        })?;
    if adapter.trust_state != ConversationAdapterTrustState::BuiltIn {
        return Err(AppError::Validation(
            "only built-in conversation adapters use the disable workflow".to_string(),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    sqlx::query(DISABLE_CONVERSATION_ADAPTER_SQL)
        .bind(&now)
        .bind(tenant_id)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    sqlx::query(DISABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL)
        .bind(&now)
        .bind(tenant_id)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND source_id IN (SELECT id FROM conversation_sources WHERE tenant_id = ?2 AND adapter_id = ?3)",
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;

    adapter.enabled = false;
    adapter.updated_at = now;
    Ok(adapter)
}

pub(crate) async fn enable_conversation_sources_by_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> AppResult<()> {
    sqlx::query(ENABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL)
        .bind(Utc::now().to_rfc3339())
        .bind(tenant_id)
        .bind(adapter_id)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn load_conversation_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> AppResult<Option<ConversationAdapter>> {
    sqlx::query(LOAD_CONVERSATION_ADAPTER_SQL)
        .bind(tenant_id)
        .bind(adapter_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::external)?
        .as_ref()
        .map(map_sqlx_conversation_adapter)
        .transpose()
}

pub(crate) async fn list_conversation_adapter_packages_sqlx(
    pool: &SqlitePool,
) -> AppResult<Vec<ConversationAdapterPackage>> {
    let rows = sqlx::query(LIST_CONVERSATION_ADAPTER_PACKAGES_SQL)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.iter()
        .map(map_sqlx_conversation_adapter_package)
        .collect()
}

pub(crate) async fn load_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    package_id: &str,
) -> AppResult<Option<ConversationAdapterPackage>> {
    sqlx::query(LOAD_CONVERSATION_ADAPTER_PACKAGE_SQL)
        .bind(package_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::external)?
        .as_ref()
        .map(map_sqlx_conversation_adapter_package)
        .transpose()
}

pub(crate) async fn load_conversation_adapter_package_by_adapter_sqlx(
    pool: &SqlitePool,
    adapter_id: &str,
) -> AppResult<Option<ConversationAdapterPackage>> {
    sqlx::query(LOAD_CONVERSATION_ADAPTER_PACKAGE_BY_ADAPTER_SQL)
        .bind(adapter_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::external)?
        .as_ref()
        .map(map_sqlx_conversation_adapter_package)
        .transpose()
}

pub(crate) async fn upsert_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    package: &ConversationAdapterPackage,
) -> AppResult<()> {
    upsert_conversation_adapter_package_with_executor(pool, package).await
}

async fn upsert_conversation_adapter_package_with_executor<'e, E>(
    executor: E,
    package: &ConversationAdapterPackage,
) -> AppResult<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    let install_dir = normalize_conversation_path(&package.install_dir)?;
    let manifest_path = normalize_conversation_path(&package.manifest_path)?;
    let adapter_manifest_path = normalize_conversation_path(&package.adapter_manifest_path)?;
    sqlx::query(UPSERT_CONVERSATION_ADAPTER_PACKAGE_SQL)
        .bind(&package.package_id)
        .bind(&package.adapter_id)
        .bind(&package.name)
        .bind(&package.version)
        .bind(encode_enum(package.record_kind)?)
        .bind(&install_dir)
        .bind(&manifest_path)
        .bind(&adapter_manifest_path)
        .bind(&package.runtime_protocol)
        .bind(if package.runtime_ready { 1 } else { 0 })
        .bind(encode_enum(package.origin)?)
        .bind(&package.source_url)
        .bind(&package.git_ref)
        .bind(&package.git_commit)
        .bind(&package.catalog_url)
        .bind(encode_enum(package.update_policy)?)
        .bind(&package.latest_version)
        .bind(&package.last_checked_at)
        .bind(encode_enum(package.runtime_gate_status)?)
        .bind(&package.runtime_validated_at)
        .bind(&package.installed_content_hash)
        .bind(&package.trusted_package_hash)
        .bind(&package.error_message)
        .bind(&package.created_at)
        .bind(&package.updated_at)
        .execute(executor)
        .await
        .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn activate_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
    version: &ConversationAdapterPackageVersion,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    #[derive(Debug, FromRow)]
    struct AdapterPackageVersionHashRow {
        artifact_hash: Option<String>,
        content_hash: String,
    }

    let existing = sqlx::query_as::<_, AdapterPackageVersionHashRow>(
        r#"
        SELECT artifact_hash, content_hash
        FROM app_conversation_adapter_package_versions
        WHERE package_id = ?1 AND version = ?2
        "#,
    )
    .bind(&version.package_id)
    .bind(&version.version)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if let Some(existing) = existing {
        if existing.artifact_hash != version.artifact_hash
            || existing.content_hash != version.content_hash
        {
            return Err(AppError::Conflict(format!(
                "conversation adapter package version is immutable: {}@{}",
                version.package_id, version.version
            )));
        }
    }

    upsert_conversation_adapter_for_all_tenants(&mut tx, adapter).await?;
    upsert_conversation_adapter_package_with_executor(&mut *tx, package).await?;
    sqlx::query(
        r#"
        INSERT INTO app_conversation_adapter_package_versions (
            package_id, version, install_dir, artifact_hash,
            content_hash, runtime_gate_status, installed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(package_id, version) DO UPDATE SET
            install_dir = excluded.install_dir,
            runtime_gate_status = excluded.runtime_gate_status
        "#,
    )
    .bind(&version.package_id)
    .bind(&version.version)
    .bind(normalize_conversation_path(&version.install_dir)?)
    .bind(&version.artifact_hash)
    .bind(&version.content_hash)
    .bind(encode_enum(version.runtime_gate_status)?)
    .bind(&version.installed_at)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)
}

pub(crate) async fn activate_conversation_adapter_workspace_sqlx(
    pool: &SqlitePool,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    upsert_conversation_adapter_for_all_tenants(&mut tx, adapter).await?;
    upsert_conversation_adapter_package_with_executor(&mut *tx, package).await?;
    sqlx::query("DELETE FROM app_conversation_adapter_package_versions WHERE package_id = ?1")
        .bind(&package.package_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)
}

pub(crate) async fn deactivate_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    package_id: &str,
    adapter_id: &str,
) -> AppResult<ConversationAdapterPackage> {
    let mut package = load_conversation_adapter_package_sqlx(pool, package_id)
        .await?
        .ok_or_else(|| {
            AppError::external(format!(
                "conversation adapter package not found: {package_id}"
            ))
        })?;
    if package.origin != ConversationAdapterPackageOrigin::ManagedRelease {
        return Err(AppError::Validation(
            "only managed conversation adapter packages can be uninstalled".to_string(),
        ));
    }
    if package.adapter_id != adapter_id {
        return Err(AppError::Validation(format!(
            "conversation adapter package {package_id} does not own adapter {adapter_id}"
        )));
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    sqlx::query("DELETE FROM conversation_adapters WHERE id = ?1")
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    sqlx::query(
        "UPDATE conversation_sources SET enabled = 0, updated_at = ?1 WHERE adapter_id = ?2",
    )
    .bind(&now)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE source_id IN (SELECT id FROM conversation_sources WHERE adapter_id = ?2)",
    )
    .bind(&now)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        r#"
        UPDATE app_conversation_adapter_packages
        SET runtime_ready = 0,
            runtime_gate_status = 'runtime_missing',
            runtime_validated_at = ?1,
            error_message = 'conversation adapter package is uninstalled',
            updated_at = ?1
        WHERE package_id = ?2
        "#,
    )
    .bind(&now)
    .bind(package_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;

    package.runtime_ready = false;
    package.runtime_gate_status = ConversationAdapterRuntimeGateStatus::RuntimeMissing;
    package.runtime_validated_at = Some(now.clone());
    package.error_message = Some("conversation adapter package is uninstalled".to_string());
    package.updated_at = now;
    Ok(package)
}

pub(crate) async fn upsert_conversation_adapter_catalog_release_sqlx(
    pool: &SqlitePool,
    release: &ConversationAdapterCatalogRelease,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO app_conversation_adapter_catalog_releases (
            catalog_url, package_id, version, channel, released_at,
            core_compatibility, artifact_url, artifact_size, artifact_sha256,
            changelog_markdown, breaking_change, runtime_protocol,
            adapter_manifest_json, etag, fetched_at, adapter_id, name, publisher,
            record_kind, package_manifest_file, adapter_manifest_file, source_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22
        )
        ON CONFLICT(catalog_url, package_id, version) DO UPDATE SET
            channel = excluded.channel,
            released_at = excluded.released_at,
            core_compatibility = excluded.core_compatibility,
            artifact_url = excluded.artifact_url,
            artifact_size = excluded.artifact_size,
            artifact_sha256 = excluded.artifact_sha256,
            changelog_markdown = excluded.changelog_markdown,
            breaking_change = excluded.breaking_change,
            runtime_protocol = excluded.runtime_protocol,
            adapter_manifest_json = excluded.adapter_manifest_json,
            etag = excluded.etag,
            fetched_at = excluded.fetched_at,
            adapter_id = excluded.adapter_id,
            name = excluded.name,
            publisher = excluded.publisher,
            record_kind = excluded.record_kind,
            package_manifest_file = excluded.package_manifest_file,
            adapter_manifest_file = excluded.adapter_manifest_file,
            source_json = excluded.source_json
        "#,
    )
    .bind(&release.catalog_url)
    .bind(&release.package_id)
    .bind(&release.version)
    .bind(encode_enum(release.channel)?)
    .bind(&release.released_at)
    .bind(&release.core_compatibility)
    .bind(&release.artifact_url)
    .bind(release.artifact_size)
    .bind(&release.artifact_sha256)
    .bind(&release.changelog_markdown)
    .bind(if release.breaking_change { 1 } else { 0 })
    .bind(&release.runtime_protocol)
    .bind(&release.adapter_manifest_json)
    .bind(&release.etag)
    .bind(&release.fetched_at)
    .bind(&release.adapter_id)
    .bind(&release.name)
    .bind(&release.publisher)
    .bind(encode_enum(release.record_kind)?)
    .bind(&release.package_manifest_file)
    .bind(&release.adapter_manifest_file)
    .bind(&release.source_json)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn list_conversation_adapter_catalog_releases_sqlx(
    pool: &SqlitePool,
    catalog_url: &str,
    package_id: Option<&str>,
) -> AppResult<Vec<ConversationAdapterCatalogRelease>> {
    let rows = sqlx::query(
        r#"
        SELECT catalog_url, package_id, adapter_id, name, publisher, version,
               channel, released_at, core_compatibility, artifact_url,
               artifact_size, artifact_sha256, changelog_markdown,
               breaking_change, runtime_protocol, record_kind,
               package_manifest_file, adapter_manifest_file,
               adapter_manifest_json, source_json, etag, fetched_at
        FROM app_conversation_adapter_catalog_releases
        WHERE catalog_url = ?1
          AND (?2 IS NULL OR package_id = ?2)
        ORDER BY package_id ASC, released_at DESC, version DESC
        "#,
    )
    .bind(catalog_url)
    .bind(package_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    rows.iter()
        .map(map_sqlx_conversation_adapter_catalog_release)
        .collect()
}

pub(crate) async fn list_conversation_adapter_package_versions_sqlx(
    pool: &SqlitePool,
    package_id: &str,
) -> AppResult<Vec<ConversationAdapterPackageVersion>> {
    #[derive(Debug, FromRow)]
    struct AdapterPackageVersionRow {
        package_id: String,
        version: String,
        install_dir: String,
        artifact_hash: Option<String>,
        content_hash: String,
        runtime_gate_status: String,
        installed_at: String,
    }

    impl AdapterPackageVersionRow {
        fn into_domain(self) -> AppResult<ConversationAdapterPackageVersion> {
            Ok(ConversationAdapterPackageVersion {
                package_id: self.package_id,
                version: self.version,
                install_dir: normalize_conversation_path(&self.install_dir)?,
                artifact_hash: self.artifact_hash,
                content_hash: self.content_hash,
                runtime_gate_status: decode_enum(self.runtime_gate_status)?,
                installed_at: self.installed_at,
            })
        }
    }

    let rows = sqlx::query_as::<_, AdapterPackageVersionRow>(
        r#"
        SELECT package_id, version, install_dir, artifact_hash, content_hash,
               runtime_gate_status, installed_at
        FROM app_conversation_adapter_package_versions
        WHERE package_id = ?1
        ORDER BY installed_at DESC, version DESC
        "#,
    )
    .bind(package_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    rows.into_iter()
        .map(AdapterPackageVersionRow::into_domain)
        .collect()
}

pub(crate) async fn delete_conversation_adapter_package_version_sqlx(
    pool: &SqlitePool,
    package_id: &str,
    version: &str,
    replacement_package: Option<&ConversationAdapterPackage>,
    delete_package: bool,
) -> AppResult<bool> {
    if replacement_package.is_some() && delete_package {
        return Err(AppError::Validation(
            "package version deletion cannot replace and delete the package record".to_string(),
        ));
    }
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let result = sqlx::query(
        "DELETE FROM app_conversation_adapter_package_versions WHERE package_id = ?1 AND version = ?2",
    )
    .bind(package_id)
    .bind(version)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if result.rows_affected() != 1 {
        tx.rollback().await.map_err(AppError::external)?;
        return Ok(false);
    }
    if let Some(package) = replacement_package {
        upsert_conversation_adapter_package_with_executor(&mut *tx, package).await?;
    } else if delete_package {
        sqlx::query(DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL)
            .bind(package_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
    }
    tx.commit().await.map_err(AppError::external)?;
    Ok(true)
}

#[derive(Debug, FromRow)]
struct ConversationAdapterCatalogReleaseRow {
    catalog_url: String,
    package_id: String,
    adapter_id: String,
    name: String,
    publisher: String,
    version: String,
    channel: String,
    released_at: Option<String>,
    core_compatibility: String,
    artifact_url: String,
    artifact_size: Option<i64>,
    artifact_sha256: String,
    changelog_markdown: String,
    breaking_change: i64,
    runtime_protocol: String,
    record_kind: String,
    package_manifest_file: String,
    adapter_manifest_file: String,
    adapter_manifest_json: Option<String>,
    source_json: Option<String>,
    etag: Option<String>,
    fetched_at: String,
}

impl ConversationAdapterCatalogReleaseRow {
    fn into_domain(self) -> AppResult<ConversationAdapterCatalogRelease> {
        Ok(ConversationAdapterCatalogRelease {
            catalog_url: self.catalog_url,
            package_id: self.package_id,
            adapter_id: self.adapter_id,
            name: self.name,
            publisher: self.publisher,
            version: self.version,
            channel: decode_enum(self.channel)?,
            released_at: self.released_at,
            core_compatibility: self.core_compatibility,
            artifact_url: self.artifact_url,
            artifact_size: self.artifact_size,
            artifact_sha256: self.artifact_sha256,
            changelog_markdown: self.changelog_markdown,
            breaking_change: self.breaking_change == 1,
            runtime_protocol: self.runtime_protocol,
            record_kind: decode_enum(self.record_kind)?,
            package_manifest_file: self.package_manifest_file,
            adapter_manifest_file: self.adapter_manifest_file,
            adapter_manifest_json: self.adapter_manifest_json,
            source_json: self.source_json,
            etag: self.etag,
            fetched_at: self.fetched_at,
        })
    }
}

fn map_sqlx_conversation_adapter_catalog_release(
    row: &SqliteRow,
) -> AppResult<ConversationAdapterCatalogRelease> {
    ConversationAdapterCatalogReleaseRow::from_row(row)
        .map_err(AppError::external)?
        .into_domain()
}

pub(crate) async fn has_running_conversation_sync_for_adapter_sqlx(
    pool: &SqlitePool,
    adapter_id: &str,
) -> AppResult<bool> {
    sqlx::query_scalar::<_, i64>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM conversation_sync_runs
            WHERE adapter_id = ?1 AND status = 'running'
        )
        "#,
    )
    .bind(adapter_id)
    .fetch_one(pool)
    .await
    .map(|value| value == 1)
    .map_err(AppError::external)
}

pub(crate) async fn list_conversation_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<ConversationSource>> {
    let rows = sqlx::query(LIST_CONVERSATION_SOURCES_SQL)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.iter().map(map_sqlx_conversation_source).collect()
}

pub(crate) async fn load_conversation_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> AppResult<Option<ConversationSource>> {
    sqlx::query(LOAD_CONVERSATION_SOURCE_SQL)
        .bind(tenant_id)
        .bind(source_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::external)?
        .as_ref()
        .map(map_sqlx_conversation_source)
        .transpose()
}

pub(crate) async fn upsert_conversation_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
) -> AppResult<()> {
    let mut source = source.clone();
    source.location = normalize_conversation_source_location(&source.location)?;
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
        .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn disable_conversation_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> AppResult<ConversationSource> {
    let mut source = load_conversation_source_sqlx(pool, tenant_id, source_id)
        .await?
        .ok_or_else(|| AppError::external(format!("conversation source not found: {source_id}")))?;
    source.enabled = false;
    source.updated_at = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    sqlx::query(
        "UPDATE conversation_sources SET enabled = 0, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3",
    )
    .bind(&source.updated_at)
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND source_id = ?3 AND status = 'active'",
    )
    .bind(&source.updated_at)
    .bind(tenant_id)
    .bind(source_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)?;
    Ok(source)
}

pub(crate) async fn import_conversation_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    dry_run: bool,
) -> AppResult<ConversationImportResult> {
    import_conversation_sessions_with_presence_sqlx(
        pool, tenant_id, source, sessions, None, dry_run,
    )
    .await
}

async fn import_conversation_sessions_with_presence_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    discovered_external_ids: Option<&BTreeSet<String>>,
    dry_run: bool,
) -> AppResult<ConversationImportResult> {
    import_conversation_sessions_with_control_sqlx(
        pool,
        tenant_id,
        source,
        sessions,
        discovered_external_ids,
        dry_run,
        None,
        &mut |_, _| {},
    )
    .await
}

fn ensure_sync_import_active(
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> AppResult<()> {
    if cancellation.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
        return Err(AppError::Cancelled(
            "conversation sync cancelled".to_string(),
        ));
    }
    Ok(())
}

pub(crate) async fn import_conversation_sessions_with_control_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    discovered_external_ids: Option<&BTreeSet<String>>,
    dry_run: bool,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
    on_progress: &mut impl FnMut(usize, usize),
) -> AppResult<ConversationImportResult> {
    ensure_sync_import_active(cancellation)?;
    on_progress(0, sessions.len());
    ensure_sync_import_active(cancellation)?;
    let turn_count = sessions.iter().map(|session| session.turns.len()).sum();
    if dry_run {
        on_progress(sessions.len(), sessions.len());
        ensure_sync_import_active(cancellation)?;
        return Ok(ConversationImportResult {
            source_id: source.id.clone(),
            adapter_id: source.adapter_id.clone(),
            dry_run: true,
            sync_run_id: None,
            session_count: sessions.len(),
            skipped_session_count: 0,
            changed_session_count: 0,
            failed_session_count: 0,
            turn_count,
            warning_count: 0,
            warnings: Vec::new(),
            session_failures: Vec::new(),
            session_warnings: Vec::new(),
            status: ConversationSyncStatus::Completed,
        });
    }

    audit_invalid_conversation_question_turns_sqlx(pool, tenant_id).await?;

    let now = Utc::now().to_rfc3339();
    let sync_run_id = stable_id("conversation-sync", &[&source.id, &now]);
    let mut warning_count = 0usize;
    let mut skipped_session_count = 0usize;
    let mut changed_session_count = 0usize;
    let warnings = Vec::new();
    let incoming_session_ids = discovered_external_ids
        .map(|external_ids| {
            external_ids
                .iter()
                .map(|external_id| stable_id("conversation-session", &[&source.id, external_id]))
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_else(|| {
            sessions
                .iter()
                .map(|session| {
                    stable_id("conversation-session", &[&source.id, &session.external_id])
                })
                .collect::<BTreeSet<_>>()
        });

    let mut completed_session_count = 0;
    for batch in sessions.chunks(CONVERSATION_IMPORT_BATCH_SIZE) {
        let mut tx = pool.begin().await.map_err(AppError::external)?;
        let mut batch_changed_session_ids = Vec::new();
        for normalized in batch {
            ensure_sync_import_active(cancellation)?;
            let session = conversation_session_from_normalized(source, normalized, &now);
            let change_kind =
                if conversation_session_exists_sqlx_tx(&mut tx, tenant_id, &session.id).await? {
                    "updated"
                } else {
                    "new"
                };
            if conversation_session_is_unchanged_sqlx_tx(&mut tx, tenant_id, &session, normalized)
                .await?
            {
                skipped_session_count += 1;
                completed_session_count += 1;
                on_progress(completed_session_count, sessions.len());
                continue;
            }
            sqlx::query(
                "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND session_id = ?3 AND status = 'active'",
            )
            .bind(&now)
            .bind(tenant_id)
            .bind(&session.id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
            upsert_conversation_session_sqlx_tx(&mut tx, tenant_id, &session).await?;
            for turn in &normalized.turns {
                ensure_sync_import_active(cancellation)?;
                if turn.user_text.trim().is_empty() {
                    warning_count += 1;
                    continue;
                }
                let stored_turn = conversation_turn_from_normalized(&session.id, turn, &now);
                upsert_conversation_turn_sqlx_tx(&mut tx, tenant_id, &stored_turn).await?;
                replace_conversation_parts_sqlx_tx(
                    &mut tx,
                    tenant_id,
                    &stored_turn.id,
                    &turn.parts,
                )
                .await?;
            }
            prune_conversation_turns_sqlx_tx(&mut tx, tenant_id, &session.id, normalized).await?;
            ensure_question_groups_for_session_sqlx_tx(&mut tx, tenant_id, &session.id, &now)
                .await?;
            rebuild_session_question_aggregates_sqlx_tx(&mut tx, tenant_id, &session.id, &now)
                .await?;
            insert_conversation_sync_delta_sqlx_tx(
                &mut tx,
                tenant_id,
                &sync_run_id,
                "session",
                &session.id,
                change_kind,
                &now,
            )
            .await?;
            changed_session_count += 1;
            batch_changed_session_ids.push(session.id);
            completed_session_count += 1;
            on_progress(completed_session_count, sessions.len());
        }
        ensure_sync_import_active(cancellation)?;
        let revision =
            super::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
        crate::backend::events::append_outbox_event_sqlx_tx(
            &mut tx,
            &DomainEvent::conversation_source_committed(
                tenant_id,
                &sync_run_id,
                &source.id,
                revision,
                batch_changed_session_ids,
            ),
        )
        .await?;
        ensure_sync_import_active(cancellation)?;
        tx.commit().await.map_err(AppError::external)?;
    }

    ensure_sync_import_active(cancellation)?;
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let missing_or_restored_session_ids = mark_missing_conversation_sessions_sqlx_tx(
        &mut tx,
        tenant_id,
        &source.id,
        &incoming_session_ids,
        &sync_run_id,
        &now,
    )
    .await?;
    sqlx::query(
        r#"
        UPDATE conversation_sources
        SET last_synced_at = ?1, last_sync_status = 'completed', updated_at = ?1
        WHERE tenant_id = ?2 AND id = ?3
        "#,
    )
    .bind(&now)
    .bind(tenant_id)
    .bind(&source.id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    insert_sync_run_sqlx_tx(
        &mut tx,
        tenant_id,
        &ConversationSyncRun {
            id: sync_run_id.clone(),
            source_id: Some(source.id.clone()),
            adapter_id: Some(source.adapter_id.clone()),
            status: ConversationSyncStatus::Completed,
            started_at: now.clone(),
            finished_at: Some(now.clone()),
            session_count: sessions.len() as i64,
            turn_count: turn_count as i64,
            warning_count: warning_count as i64,
            error_message: None,
        },
    )
    .await?;
    let revision =
        super::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
    crate::backend::events::append_outbox_event_sqlx_tx(
        &mut tx,
        &DomainEvent::conversation_source_committed(
            tenant_id,
            &sync_run_id,
            &source.id,
            revision,
            missing_or_restored_session_ids,
        ),
    )
    .await?;
    ensure_sync_import_active(cancellation)?;
    tx.commit().await.map_err(AppError::external)?;

    Ok(ConversationImportResult {
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        dry_run: false,
        sync_run_id: Some(sync_run_id),
        session_count: sessions.len(),
        skipped_session_count,
        changed_session_count,
        failed_session_count: 0,
        turn_count,
        warning_count,
        warnings,
        session_failures: Vec::new(),
        session_warnings: Vec::new(),
        status: ConversationSyncStatus::Completed,
    })
}

pub(crate) async fn import_conversation_sessions_advanced_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    discovered_external_ids: Option<&BTreeSet<String>>,
    descriptor_versions: Option<&BTreeMap<String, String>>,
    mut session_failures: Vec<crate::backend::models::SessionSyncFailure>,
    session_warnings: Vec<crate::backend::models::SessionSyncWarning>,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    dry_run: bool,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
    on_progress: &mut impl FnMut(usize, usize),
) -> AppResult<ConversationImportResult> {
    ensure_sync_import_active(cancellation)?;
    on_progress(0, sessions.len());
    ensure_sync_import_active(cancellation)?;
    let turn_count = sessions.iter().map(|session| session.turns.len()).sum();
    let initial_failed_count = session_failures.len();
    if dry_run {
        on_progress(sessions.len(), sessions.len());
        ensure_sync_import_active(cancellation)?;
        let status = if !session_failures.is_empty() {
            if !sessions.is_empty() {
                ConversationSyncStatus::PartialSuccess
            } else {
                ConversationSyncStatus::Failed
            }
        } else {
            ConversationSyncStatus::Completed
        };
        return Ok(ConversationImportResult {
            source_id: source.id.clone(),
            adapter_id: source.adapter_id.clone(),
            dry_run: true,
            sync_run_id: None,
            session_count: sessions.len() + initial_failed_count,
            skipped_session_count: 0,
            changed_session_count: 0,
            failed_session_count: initial_failed_count,
            turn_count,
            warning_count: session_warnings.len(),
            warnings: session_warnings.iter().map(|w| w.message.clone()).collect(),
            session_failures,
            session_warnings,
            status,
        });
    }

    audit_invalid_conversation_question_turns_sqlx(pool, tenant_id).await?;

    let now = Utc::now().to_rfc3339();
    let sync_run_id = stable_id("conversation-sync", &[&source.id, &now]);
    let mut skipped_session_count = 0usize;
    let mut changed_session_count = 0usize;

    // 先记录读取阶段失败的会话到 observation 表 (dirty = 1, presence = present)
    for failure in &session_failures {
        let observed_version = descriptor_versions.and_then(|map| {
            map.get(failure.session_external_id.as_str())
                .map(|s| s.as_str())
        });
        let _ = record_conversation_session_failure_sqlx(
            pool,
            tenant_id,
            &source.id,
            "session",
            &failure.session_external_id,
            observed_version,
            &failure.error_code,
            &failure.error_message,
            &failure.stage,
            failure.retryable,
        )
        .await;
    }

    let incoming_session_ids = discovered_external_ids
        .map(|external_ids| {
            external_ids
                .iter()
                .map(|external_id| stable_id("conversation-session", &[&source.id, external_id]))
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_else(|| {
            let mut ids = sessions
                .iter()
                .map(|session| {
                    stable_id("conversation-session", &[&source.id, &session.external_id])
                })
                .collect::<BTreeSet<_>>();
            for failure in &session_failures {
                ids.insert(stable_id(
                    "conversation-session",
                    &[&source.id, &failure.session_external_id],
                ));
            }
            ids
        });

    let mut completed_session_count = 0;
    let mut all_changed_session_ids = Vec::new();

    for normalized in sessions {
        ensure_sync_import_active(cancellation)?;
        let session = conversation_session_from_normalized(source, normalized, &now);
        let mut tx = match pool.begin().await {
            Ok(tx) => tx,
            Err(err) => {
                let sanitized =
                    crate::backend::models::sanitize_sync_error_message(&err.to_string());
                session_failures.push(crate::backend::models::SessionSyncFailure {
                    session_external_id: normalized.external_id.clone(),
                    stage: "storage".to_string(),
                    error_code: "transaction_begin_failed".to_string(),
                    error_message: sanitized.clone(),
                    retryable: true,
                });
                let _ = record_conversation_session_failure_sqlx(
                    pool,
                    tenant_id,
                    &source.id,
                    "session",
                    &normalized.external_id,
                    session.source_fingerprint.as_deref(),
                    "transaction_begin_failed",
                    &sanitized,
                    "storage",
                    true,
                )
                .await;
                completed_session_count += 1;
                on_progress(completed_session_count, sessions.len());
                continue;
            }
        };

        let session_import_res: AppResult<Option<String>> = async {
            let change_kind =
                if conversation_session_exists_sqlx_tx(&mut tx, tenant_id, &session.id).await? {
                    "updated"
                } else {
                    "new"
                };
            if conversation_session_is_unchanged_sqlx_tx(&mut tx, tenant_id, &session, normalized)
                .await?
            {
                // 未变更会话：推进 observation clean (dirty = 0)
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
                )
                .await?;
                tx.commit().await.map_err(AppError::external)?;
                return Ok(None);
            }
            sqlx::query(
                "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND session_id = ?3 AND status = 'active'",
            )
            .bind(&now)
            .bind(tenant_id)
            .bind(&session.id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
            upsert_conversation_session_sqlx_tx(&mut tx, tenant_id, &session).await?;
            for turn in &normalized.turns {
                if turn.user_text.trim().is_empty() {
                    continue;
                }
                let stored_turn = conversation_turn_from_normalized(&session.id, turn, &now);
                upsert_conversation_turn_sqlx_tx(&mut tx, tenant_id, &stored_turn).await?;
                replace_conversation_parts_sqlx_tx(
                    &mut tx,
                    tenant_id,
                    &stored_turn.id,
                    &turn.parts,
                )
                .await?;
            }
            prune_conversation_turns_sqlx_tx(&mut tx, tenant_id, &session.id, normalized).await?;
            ensure_question_groups_for_session_sqlx_tx(&mut tx, tenant_id, &session.id, &now)
                .await?;
            rebuild_session_question_aggregates_sqlx_tx(&mut tx, tenant_id, &session.id, &now)
                .await?;
            insert_conversation_sync_delta_sqlx_tx(
                &mut tx,
                tenant_id,
                &sync_run_id,
                "session",
                &session.id,
                change_kind,
                &now,
            )
            .await?;

            // 原子检查点：在同一事务中将 observation 标记为 clean
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
            )
            .await?;

            tx.commit().await.map_err(AppError::external)?;
            Ok(Some(session.id.clone()))
        }.await;

        match session_import_res {
            Ok(Some(changed_id)) => {
                changed_session_count += 1;
                all_changed_session_ids.push(changed_id);
            }
            Ok(None) => {
                skipped_session_count += 1;
            }
            Err(err) => {
                let err_str = err.to_string();
                let sanitized = crate::backend::models::sanitize_sync_error_message(&err_str);
                session_failures.push(crate::backend::models::SessionSyncFailure {
                    session_external_id: session.external_id.clone(),
                    stage: "storage".to_string(),
                    error_code: "storage_error".to_string(),
                    error_message: sanitized.clone(),
                    retryable: true,
                });
                let _ = record_conversation_session_failure_sqlx(
                    pool,
                    tenant_id,
                    &source.id,
                    "session",
                    &session.external_id,
                    session.source_fingerprint.as_deref(),
                    "storage_error",
                    &sanitized,
                    "storage",
                    true,
                )
                .await;
            }
        }
        completed_session_count += 1;
        on_progress(completed_session_count, sessions.len());
    }

    let is_cancelled = cancellation.is_some_and(tokio_util::sync::CancellationToken::is_cancelled);

    // Missing 对账（仅在非主动取消时进行，防止取消时误将未处理会话当成 missing）
    let missing_or_restored_session_ids = if !is_cancelled {
        let mut tx = pool.begin().await.map_err(AppError::external)?;
        let missing_ids = mark_missing_conversation_sessions_sqlx_tx(
            &mut tx,
            tenant_id,
            &source.id,
            &incoming_session_ids,
            &sync_run_id,
            &now,
        )
        .await?;
        tx.commit().await.map_err(AppError::external)?;
        missing_ids
    } else {
        Vec::new()
    };

    let total_failed = session_failures.len();
    let status = if is_cancelled {
        ConversationSyncStatus::Cancelled
    } else if total_failed > 0 {
        if changed_session_count > 0 || skipped_session_count > 0 {
            ConversationSyncStatus::PartialSuccess
        } else {
            ConversationSyncStatus::Failed
        }
    } else {
        ConversationSyncStatus::Completed
    };

    let status_str = match status {
        ConversationSyncStatus::Completed => "completed",
        ConversationSyncStatus::PartialSuccess => "partial_success",
        ConversationSyncStatus::Failed => "failed",
        ConversationSyncStatus::Cancelled => "cancelled",
        ConversationSyncStatus::Running => "running",
    };

    let error_summary = if total_failed > 0 {
        Some(format!("{total_failed} session(s) failed during sync"))
    } else {
        None
    };

    let mut final_tx = pool.begin().await.map_err(AppError::external)?;
    sqlx::query(
        r#"
        UPDATE conversation_sources
        SET last_synced_at = ?1, last_sync_status = ?2, updated_at = ?1
        WHERE tenant_id = ?3 AND id = ?4
        "#,
    )
    .bind(&now)
    .bind(status_str)
    .bind(tenant_id)
    .bind(&source.id)
    .execute(&mut *final_tx)
    .await
    .map_err(AppError::external)?;

    insert_sync_run_sqlx_tx(
        &mut final_tx,
        tenant_id,
        &ConversationSyncRun {
            id: sync_run_id.clone(),
            source_id: Some(source.id.clone()),
            adapter_id: Some(source.adapter_id.clone()),
            status,
            started_at: now.clone(),
            finished_at: Some(now.clone()),
            session_count: (sessions.len() + initial_failed_count) as i64,
            turn_count: turn_count as i64,
            warning_count: session_warnings.len() as i64,
            error_message: error_summary,
        },
    )
    .await?;

    if !all_changed_session_ids.is_empty() || !missing_or_restored_session_ids.is_empty() {
        let revision =
            super::bump_conversation_search_source_revision_sqlx_tx(&mut *final_tx, tenant_id)
                .await?;
        let mut affected = all_changed_session_ids;
        affected.extend(missing_or_restored_session_ids);
        crate::backend::events::append_outbox_event_sqlx_tx(
            &mut final_tx,
            &DomainEvent::conversation_source_committed(
                tenant_id,
                &sync_run_id,
                &source.id,
                revision,
                affected,
            ),
        )
        .await?;
    }
    final_tx.commit().await.map_err(AppError::external)?;

    let string_warnings = session_warnings.iter().map(|w| w.message.clone()).collect();
    Ok(ConversationImportResult {
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        dry_run: false,
        sync_run_id: Some(sync_run_id),
        session_count: sessions.len() + initial_failed_count,
        skipped_session_count,
        changed_session_count,
        failed_session_count: total_failed,
        turn_count,
        warning_count: session_warnings.len(),
        warnings: string_warnings,
        session_failures,
        session_warnings,
        status,
    })
}

#[derive(Debug, FromRow)]
struct ConversationSessionListItemRow {
    id: String,
    source_id: String,
    adapter_id: String,
    external_id: String,
    title: String,
    project_path: Option<String>,
    started_at: Option<String>,
    updated_at: Option<String>,
    source_locator: Option<String>,
    source_fingerprint: Option<String>,
    missing: i64,
    created_at: String,
    imported_at: String,
    question_count: i64,
    turn_count: i64,
    #[sqlx(default)]
    execution_origin: Option<String>,
    #[sqlx(default)]
    execution_purpose: Option<String>,
    #[sqlx(default)]
    user_visible: Option<i64>,
}

impl ConversationSessionListItemRow {
    fn into_item(self) -> AppResult<ConversationSessionListItem> {
        let question_count = usize::try_from(self.question_count)
            .map_err(|_| AppError::external("invalid conversation question count"))?;
        let turn_count = usize::try_from(self.turn_count)
            .map_err(|_| AppError::external("invalid conversation turn count"))?;
        let session = ConversationSession {
            id: self.id,
            source_id: self.source_id,
            adapter_id: self.adapter_id,
            external_id: self.external_id,
            title: self.title,
            project_path: self.project_path,
            started_at: self.started_at,
            updated_at: self.updated_at,
            source_locator: self.source_locator,
            source_fingerprint: self.source_fingerprint,
            missing: self.missing == 1,
            created_at: self.created_at,
            imported_at: self.imported_at,
            execution_origin: self.execution_origin.unwrap_or_else(|| "user".to_string()),
            execution_purpose: self.execution_purpose,
            user_visible: self.user_visible.map(|v| v != 0).unwrap_or(true),
        };
        Ok(ConversationSessionListItem {
            session,
            question_count,
            turn_count,
        })
    }
}

#[derive(Debug, FromRow)]
struct RecentConversationSessionRecordRow {
    id: String,
    source_id: String,
    adapter_id: String,
    external_id: String,
    title: String,
    project_path: Option<String>,
    started_at: Option<String>,
    updated_at: Option<String>,
    source_locator: Option<String>,
    source_fingerprint: Option<String>,
    missing: i64,
    created_at: String,
    imported_at: String,
    question_count: i64,
    turn_count: i64,
    last_activity_at: String,
    cwd: Option<String>,
    source_agent: String,
    #[sqlx(default)]
    execution_origin: Option<String>,
    #[sqlx(default)]
    execution_purpose: Option<String>,
    #[sqlx(default)]
    user_visible: Option<i64>,
}

impl RecentConversationSessionRecordRow {
    fn into_record(self) -> AppResult<RecentConversationSessionRecord> {
        let question_count = usize::try_from(self.question_count)
            .map_err(|_| AppError::external("invalid recent question count"))?;
        let turn_count = usize::try_from(self.turn_count)
            .map_err(|_| AppError::external("invalid recent turn count"))?;
        let session = ConversationSession {
            id: self.id,
            source_id: self.source_id,
            adapter_id: self.adapter_id,
            external_id: self.external_id,
            title: self.title,
            project_path: self.project_path,
            started_at: self.started_at,
            updated_at: self.updated_at,
            source_locator: self.source_locator,
            source_fingerprint: self.source_fingerprint,
            missing: self.missing == 1,
            created_at: self.created_at,
            imported_at: self.imported_at,
            execution_origin: self.execution_origin.unwrap_or_else(|| "user".to_string()),
            execution_purpose: self.execution_purpose,
            user_visible: self.user_visible.map(|v| v != 0).unwrap_or(true),
        };
        Ok(RecentConversationSessionRecord {
            session: ConversationSessionListItem {
                session,
                question_count,
                turn_count,
            },
            last_activity_at: self.last_activity_at,
            cwd: self.cwd,
            source_agent: self.source_agent,
            recent_events: Vec::new(),
        })
    }
}

pub(crate) async fn list_conversation_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> AppResult<Vec<ConversationSessionListItem>> {
    let needle = normalize_query(query);
    let id_needle = query.and_then(crate::backend::models::conversation_id_search_term);
    let rows = sqlx::query_as::<_, ConversationSessionListItemRow>(
        r#"
        SELECT s.id, s.source_id, s.adapter_id, s.external_id, s.title, s.project_path,
               s.started_at, s.updated_at, s.source_locator, s.source_fingerprint,
               s.missing, s.created_at, s.imported_at,
               s.execution_origin, s.execution_purpose, s.user_visible,
               (
                   SELECT COUNT(*)
                   FROM conversation_questions q
                   WHERE q.tenant_id = s.tenant_id AND q.session_id = s.id
               ) AS question_count,
               (
                   SELECT COUNT(*)
                   FROM conversation_turns t
                   WHERE t.tenant_id = s.tenant_id AND t.session_id = s.id
               ) AS turn_count
        FROM conversation_sessions s
        WHERE s.tenant_id = ?1
          AND s.user_visible = 1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND s.missing = 0
          AND (
              ?4 IS NULL
              OR instr(lower(s.title), ?4) > 0
              OR instr(lower(COALESCE(s.project_path, '')), ?4) > 0
              OR instr(lower(s.external_id), ?4) > 0
              OR (?5 IS NOT NULL AND instr(lower(s.id), ?5) > 0)
              OR EXISTS (
                SELECT 1
                  FROM conversation_question_fts f
                  WHERE f.tenant_id = s.tenant_id
                    AND f.session_id = s.id
                    AND instr(lower(
                        f.question_text || char(10) || f.answer_text || char(10) ||
                        f.code_text || char(10) || f.command_text
                    ), ?4) > 0
              )
          )
        ORDER BY COALESCE(s.updated_at, s.imported_at) DESC, s.title ASC
        LIMIT ?6 OFFSET ?7
        "#,
    )
    .bind(tenant_id)
    .bind(adapter_id)
    .bind(source_id)
    .bind(needle.as_deref())
    .bind(id_needle.as_deref())
    .bind(
        i64::try_from(limit)
            .map_err(|_| AppError::external(format!("invalid conversation limit: {limit}")))?,
    )
    .bind(
        i64::try_from(offset)
            .map_err(|_| AppError::external(format!("invalid conversation offset: {offset}")))?,
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    rows.into_iter()
        .map(ConversationSessionListItemRow::into_item)
        .collect()
}

const LIST_RECENT_CONVERSATION_SESSIONS_SQL: &str = r#"
        SELECT s.id, s.source_id, s.adapter_id, s.external_id, s.title, s.project_path,
               s.started_at, s.updated_at, s.source_locator, s.source_fingerprint,
               s.missing, s.created_at, s.imported_at,
               s.execution_origin, s.execution_purpose, s.user_visible,
               (
                   SELECT COUNT(*)
                   FROM conversation_questions q
                   WHERE q.tenant_id = s.tenant_id AND q.session_id = s.id
               ) AS question_count,
               (
                   SELECT COUNT(*)
                   FROM conversation_turns t
                   WHERE t.tenant_id = s.tenant_id AND t.session_id = s.id
               ) AS turn_count,
               s.updated_at AS last_activity_at,
               (
                   SELECT p.cwd
                   FROM conversation_turns t
                   CROSS JOIN conversation_parts p INDEXED BY idx_conversation_parts_tenant_turn
                   WHERE t.tenant_id = s.tenant_id
                     AND t.session_id = s.id
                     AND p.tenant_id = t.tenant_id
                     AND p.turn_id = t.id
                     AND p.cwd IS NOT NULL
                     AND trim(p.cwd) <> ''
                   ORDER BY COALESCE(t.ended_at, t.started_at) DESC,
                            t.turn_index DESC,
                            p.part_index DESC,
                            p.id DESC
                   LIMIT 1
               ) AS cwd,
               COALESCE(NULLIF(trim(a.name), ''), s.adapter_id) AS source_agent
        FROM conversation_sessions s
        JOIN conversation_sources source
          ON source.tenant_id = s.tenant_id
         AND source.id = s.source_id
         AND source.enabled = 1
        LEFT JOIN conversation_adapters a
          ON a.tenant_id = s.tenant_id AND a.id = s.adapter_id
        WHERE s.tenant_id = ?1
          AND s.user_visible = 1
          AND s.missing = 0
          AND (
                ?4 = ''
                OR s.project_path IS NULL
                OR (
                    s.project_path <> ?4
                    AND instr(s.project_path, ?4 || '/') <> 1
                )
              )
          AND s.updated_at IS NOT NULL
          AND CASE
                WHEN trim(s.updated_at) GLOB '[0-9]*'
                 AND trim(s.updated_at) NOT GLOB '*[^0-9]*'
                THEN datetime(
                    CASE WHEN length(trim(s.updated_at)) >= 12
                         THEN CAST(trim(s.updated_at) AS REAL) / 1000.0
                         ELSE CAST(trim(s.updated_at) AS REAL)
                    END,
                    'unixepoch'
                )
                ELSE datetime(s.updated_at)
              END >= datetime(?2)
          AND CASE
                WHEN trim(s.updated_at) GLOB '[0-9]*'
                 AND trim(s.updated_at) NOT GLOB '*[^0-9]*'
                THEN datetime(
                    CASE WHEN length(trim(s.updated_at)) >= 12
                         THEN CAST(trim(s.updated_at) AS REAL) / 1000.0
                         ELSE CAST(trim(s.updated_at) AS REAL)
                    END,
                    'unixepoch'
                )
                ELSE datetime(s.updated_at)
              END <= datetime(?3)
        ORDER BY CASE
                   WHEN trim(s.updated_at) GLOB '[0-9]*'
                    AND trim(s.updated_at) NOT GLOB '*[^0-9]*'
                   THEN datetime(
                       CASE WHEN length(trim(s.updated_at)) >= 12
                            THEN CAST(trim(s.updated_at) AS REAL) / 1000.0
                            ELSE CAST(trim(s.updated_at) AS REAL)
                       END,
                       'unixepoch'
                   )
                   ELSE datetime(s.updated_at)
                 END DESC,
                 s.id ASC
        "#;

pub(crate) async fn list_recent_conversation_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    cutoff: &str,
    now: &str,
    excluded_project_root: &str,
) -> AppResult<Vec<RecentConversationSessionRecord>> {
    let rows = sqlx::query_as::<_, RecentConversationSessionRecordRow>(
        LIST_RECENT_CONVERSATION_SESSIONS_SQL,
    )
    .bind(tenant_id)
    .bind(cutoff)
    .bind(now)
    .bind(excluded_project_root)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    rows.into_iter()
        .map(RecentConversationSessionRecordRow::into_record)
        .collect()
}

pub(crate) async fn load_conversation_session_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<ConversationSessionDetail> {
    let session_row = sqlx::query(
        r#"
        SELECT id, source_id, adapter_id, external_id, title, project_path,
               started_at, updated_at, source_locator, source_fingerprint,
               missing, created_at, imported_at
        FROM conversation_sessions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?
    .ok_or_else(|| AppError::external(format!("conversation session not found: {session_id}")))?;
    let session = map_sqlx_conversation_session(&session_row)?;
    let questions =
        load_conversation_question_details_for_session_sqlx(pool, tenant_id, session_id).await?;
    Ok(ConversationSessionDetail { session, questions })
}

pub(crate) async fn list_conversation_question_details_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
    query: Option<&str>,
    limit: usize,
    offset: usize,
) -> AppResult<Vec<ConversationQuestionDetail>> {
    let needle = normalize_query(query);
    let details =
        load_conversation_question_details_for_session_sqlx(pool, tenant_id, session_id).await?;
    Ok(details
        .into_iter()
        .filter(|detail| {
            needle.as_ref().is_none_or(|needle| {
                let question = &detail.question;
                std::iter::once(question.title.clone().unwrap_or_default())
                    .chain(detail.turns.iter().map(|turn| turn.user_text.clone()))
                    .chain(
                        detail
                            .projected_content_nodes
                            .iter()
                            .map(|node| node.content.clone()),
                    )
                    .collect::<Vec<_>>()
                    .join("\n")
                    .to_lowercase()
                    .contains(needle)
            })
        })
        .skip(offset)
        .take(limit)
        .collect())
}

pub(crate) async fn load_conversation_question_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_id: &str,
) -> AppResult<ConversationQuestionDetail> {
    let question_row = sqlx::query(
        r#"
        SELECT id, session_id, title, created_at, updated_at
        FROM conversation_questions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?
    .ok_or_else(|| AppError::external(format!("conversation question not found: {question_id}")))?;
    let question = map_sqlx_conversation_question(&question_row)?;
    let question_turns = load_question_turn_memberships_sqlx(pool, tenant_id, question_id).await?;

    let turn_rows = sqlx::query(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        WHERE qt.tenant_id = ?1
          AND qt.question_id = ?2
          AND q.session_id = t.session_id
        ORDER BY qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    let turns = turn_rows
        .iter()
        .map(map_sqlx_conversation_turn)
        .collect::<AppResult<Vec<_>>>()?;

    let part_rows = sqlx::query(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label
        FROM conversation_parts p
        JOIN conversation_question_turns qt ON qt.tenant_id = p.tenant_id AND qt.turn_id = p.turn_id
        JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        JOIN conversation_turns t
          ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
        WHERE qt.tenant_id = ?1
          AND qt.question_id = ?2
          AND q.session_id = t.session_id
        ORDER BY qt.turn_order ASC, p.part_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    let parts = part_rows
        .iter()
        .map(map_sqlx_conversation_part)
        .collect::<AppResult<Vec<_>>>()?;
    let (adapter_id, card_kinds) =
        load_conversation_card_projection_context_sqlx(pool, tenant_id, &question.session_id)
            .await?;
    let projected_content_nodes = project_question_content_nodes(
        &question.id,
        &question_turns,
        &parts,
        &adapter_id,
        &card_kinds,
    )?;
    Ok(ConversationQuestionDetail {
        question: project_question_title(question, &turns),
        question_turns,
        turns,
        parts,
        projected_content_nodes,
    })
}

pub(crate) async fn list_conversation_block_locators_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    question_id: &str,
) -> AppResult<Vec<ConversationBlockLocator>> {
    let tables = record_kind.tables();
    let session_id = sqlx::query_scalar::<_, String>(AssertSqlSafe(format!(
        "SELECT session_id FROM {} WHERE tenant_id = ?1 AND id = ?2",
        tables.questions
    )))
    .bind(tenant_id)
    .bind(question_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?
    .ok_or_else(|| AppError::external(format!("conversation question not found: {question_id}")))?;

    let turn_rows = sqlx::query(AssertSqlSafe(format!(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at
        FROM {question_turns} qt
        JOIN {turns} t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1 AND qt.question_id = ?2
        ORDER BY qt.turn_order ASC, t.turn_index ASC
        "#,
        question_turns = tables.question_turns,
        turns = tables.turns,
    )))
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    let turns = turn_rows
        .iter()
        .map(map_sqlx_conversation_turn)
        .collect::<AppResult<Vec<_>>>()?;

    let part_rows = sqlx::query(AssertSqlSafe(format!(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label
        FROM {question_turns} qt
        JOIN {parts} p ON p.tenant_id = qt.tenant_id AND p.turn_id = qt.turn_id
        WHERE qt.tenant_id = ?1 AND qt.question_id = ?2
        ORDER BY qt.turn_order ASC, p.part_index ASC
        "#,
        question_turns = tables.question_turns,
        parts = tables.parts,
    )))
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    let parts = part_rows
        .iter()
        .map(map_sqlx_conversation_part)
        .collect::<AppResult<Vec<_>>>()?;
    let (adapter_id, card_kinds) = load_conversation_card_projection_context_for_record_sqlx(
        pool,
        tenant_id,
        record_kind,
        &session_id,
    )
    .await?;
    let cards = parts.iter().try_fold(Vec::new(), |mut projected, part| {
        projected.extend(
            crate::backend::projection::conversation_cards::project_conversation_content_cards(
                part,
                &adapter_id,
                &card_kinds,
            )?,
        );
        Ok::<_, AppError>(projected)
    })?;
    let parts_by_id = parts
        .iter()
        .map(|part| (part.id.as_str(), part))
        .collect::<BTreeMap<_, _>>();

    let mut locators = Vec::with_capacity(turns.len() + cards.len());
    for turn in &turns {
        locators.push(conversation_question_block_locator(
            record_kind,
            &session_id,
            question_id,
            turn,
        ));
    }
    for card in &cards {
        if let Some(part) = parts_by_id.get(card.part_id.as_str()) {
            locators.push(conversation_card_block_locator(
                record_kind,
                &session_id,
                question_id,
                part,
                card,
            ));
        }
    }
    Ok(locators)
}

#[derive(Debug, FromRow)]
struct ConversationTurnWithQuestionRow {
    id: String,
    session_id: String,
    external_id: String,
    turn_index: i64,
    user_text: String,
    title: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    fingerprint: String,
    missing: i64,
    imported_at: String,
    question_id: String,
}

impl ConversationTurnWithQuestionRow {
    fn into_turn(self) -> (String, ConversationTurn) {
        let question_id = self.question_id;
        let turn = ConversationTurn {
            id: self.id,
            session_id: self.session_id,
            external_id: self.external_id,
            turn_index: self.turn_index,
            user_text: self.user_text,
            title: self.title,
            started_at: self.started_at,
            ended_at: self.ended_at,
            fingerprint: self.fingerprint,
            missing: self.missing == 1,
            imported_at: self.imported_at,
        };
        (question_id, turn)
    }
}

#[derive(Debug, FromRow)]
struct ConversationPartDetailRow {
    id: String,
    turn_id: String,
    part_index: i64,
    role: String,
    kind: String,
    text: Option<String>,
    language: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    status: Option<String>,
    exit_code: Option<i64>,
    metadata_json: Option<String>,
    content_card_json: Option<String>,
    translated_text: Option<String>,
    source_execution_id: Option<String>,
    command_label: Option<String>,
    question_id: String,
    session_id: String,
}

impl ConversationPartDetailRow {
    fn into_part(self) -> AppResult<(ConversationPart, String, String)> {
        let part = ConversationPart {
            id: self.id,
            turn_id: self.turn_id,
            part_index: self.part_index,
            role: decode_enum(self.role)?,
            kind: decode_enum(self.kind)?,
            text: self.text,
            language: self.language,
            command: self.command,
            cwd: self.cwd,
            status: self.status,
            exit_code: self.exit_code.map(|v| v as i32),
            command_label: self.command_label,
            source_execution_id: self.source_execution_id,
            content_card: self.content_card_json.map(decode_json).transpose()?,
            metadata_json: self.metadata_json,
            translated_text: self.translated_text,
        };
        Ok((part, self.question_id, self.session_id))
    }
}

pub(crate) async fn load_conversation_block_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    block_id: &str,
) -> AppResult<ConversationBlockDetail> {
    let tables = record_kind.tables();
    if let Some(turn_id) = block_id.strip_suffix("-question") {
        let row = sqlx::query_as::<_, ConversationTurnWithQuestionRow>(AssertSqlSafe(format!(
            r#"
            SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
                   t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at,
                   qt.question_id
            FROM {turns} t
            JOIN {question_turns} qt ON qt.tenant_id = t.tenant_id AND qt.turn_id = t.id
            WHERE t.tenant_id = ?1 AND t.id = ?2
            "#,
            turns = tables.turns,
            question_turns = tables.question_turns,
        )))
        .bind(tenant_id)
        .bind(turn_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::external)?
        .ok_or_else(|| {
            AppError::external(format!("conversation question block not found: {block_id}"))
        })?;
        let (question_id, turn) = row.into_turn();
        let locator =
            conversation_question_block_locator(record_kind, &turn.session_id, &question_id, &turn);
        return Ok(ConversationBlockDetail {
            locator,
            content: turn.user_text,
            translated_content: None,
        });
    }

    let part_id = conversation_part_id_for_block_id(block_id);
    let row = sqlx::query_as::<_, ConversationPartDetailRow>(AssertSqlSafe(format!(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label,
               qt.question_id, t.session_id
        FROM {parts} p
        JOIN {question_turns} qt ON qt.tenant_id = p.tenant_id AND qt.turn_id = p.turn_id
        JOIN {turns} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
        WHERE p.tenant_id = ?1 AND p.id = ?2
        "#,
        parts = tables.parts,
        question_turns = tables.question_turns,
        turns = tables.turns,
    )))
    .bind(tenant_id)
    .bind(part_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?
    .ok_or_else(|| {
        AppError::external(format!("conversation content block not found: {block_id}"))
    })?;
    let (part, question_id, session_id) = row.into_part()?;
    let (adapter_id, card_kinds) = load_conversation_card_projection_context_for_record_sqlx(
        pool,
        tenant_id,
        record_kind,
        &session_id,
    )
    .await?;
    let cards = crate::backend::projection::conversation_cards::project_conversation_content_cards(
        &part,
        &adapter_id,
        &card_kinds,
    )?;
    let card = cards
        .iter()
        .find(|card| card.node_id == block_id)
        .or_else(|| (block_id == part.id).then(|| cards.first()).flatten())
        .ok_or_else(|| {
            AppError::external(format!(
                "conversation block is not a readable content card: {block_id}"
            ))
        })?;
    let mut locator =
        conversation_card_block_locator(record_kind, &session_id, &question_id, &part, card);
    if block_id == part.id {
        locator.block_id = block_id.to_string();
    }
    Ok(ConversationBlockDetail {
        locator,
        content: card.body.clone(),
        translated_content: card.translated_body.clone(),
    })
}

async fn resolve_conversation_question_redirect_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> AppResult<String> {
    let mut current = question_id.to_string();
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(current.clone()) {
            return Err(AppError::Validation(format!(
                "conversation question redirect cycle: {question_id}"
            )));
        }
        let Some(target) = sqlx::query_scalar::<_, String>(
            "SELECT target_question_id FROM conversation_question_redirects WHERE tenant_id = ?1 AND source_question_id = ?2",
        )
        .bind(tenant_id)
        .bind(&current)
        .fetch_optional(&mut **tx)
        .await
        .map_err(AppError::external)? else {
            return Ok(current);
        };
        current = target;
    }
}

pub(crate) async fn merge_conversation_questions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_ids: &[String],
    dry_run: bool,
) -> AppResult<ConversationMutationResult> {
    if question_ids.len() < 2 {
        return Err(AppError::Validation(
            "at least two question ids are required".to_string(),
        ));
    }

    audit_invalid_conversation_question_turns_sqlx(pool, tenant_id).await?;
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let mut questions = Vec::with_capacity(question_ids.len());
    for question_id in question_ids {
        let resolved_question_id =
            resolve_conversation_question_redirect_sqlx_tx(&mut tx, tenant_id, question_id).await?;
        questions.push(
            load_conversation_question_sqlx_tx(&mut tx, tenant_id, &resolved_question_id)
                .await?
                .ok_or_else(|| {
                    AppError::external(format!("conversation question not found: {question_id}"))
                })?,
        );
    }
    let session_id = questions[0].session_id.clone();
    if questions
        .iter()
        .any(|question| question.session_id != session_id)
    {
        return Err(AppError::Validation(
            "questions must belong to the same session".to_string(),
        ));
    }
    let mut canonical_question_ids = Vec::with_capacity(questions.len());
    for question in &questions {
        if !canonical_question_ids.contains(&question.id) {
            canonical_question_ids.push(question.id.clone());
        }
    }
    if canonical_question_ids.len() < 2 {
        let session_id = questions[0].session_id.clone();
        tx.rollback().await.map_err(AppError::external)?;
        return Ok(ConversationMutationResult {
            dry_run,
            session_id,
            affected_question_ids: question_ids.to_vec(),
            questions: vec![
                load_conversation_question_detail_sqlx(pool, tenant_id, &canonical_question_ids[0])
                    .await?,
            ],
        });
    }
    reject_invalid_conversation_question_turns_sqlx_tx(&mut tx, tenant_id).await?;
    ensure_question_ids_are_adjacent_sqlx_tx(
        &mut tx,
        tenant_id,
        &session_id,
        &canonical_question_ids,
    )
    .await?;

    if dry_run {
        tx.rollback().await.map_err(AppError::external)?;
        let mut details = Vec::with_capacity(canonical_question_ids.len());
        for question_id in &canonical_question_ids {
            details
                .push(load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?);
        }
        return Ok(ConversationMutationResult {
            dry_run: true,
            session_id,
            affected_question_ids: question_ids.to_vec(),
            questions: details,
        });
    }

    let now = Utc::now().to_rfc3339();
    let survivor_id = canonical_question_ids[0].clone();
    sqlx::query(
        "UPDATE conversation_question_turns SET assignment_origin = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND question_id = ?4",
    )
    .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
    .bind(&now)
    .bind(tenant_id)
    .bind(&survivor_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    for question_id in &canonical_question_ids[1..] {
        let next_order =
            max_question_turn_order_sqlx_tx(&mut tx, tenant_id, &survivor_id).await? + 1;
        let turn_ids = load_question_turn_ids_sqlx_tx(&mut tx, tenant_id, question_id).await?;
        for (offset, turn_id) in turn_ids.iter().enumerate() {
            sqlx::query(
                r#"
                UPDATE conversation_question_turns
                SET question_id = ?1,
                    turn_order = ?2,
                    assignment_origin = ?3,
                    updated_at = ?4
                WHERE tenant_id = ?5 AND question_id = ?6 AND turn_id = ?7
                "#,
            )
            .bind(&survivor_id)
            .bind(next_order + offset as i64)
            .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
            .bind(&now)
            .bind(tenant_id)
            .bind(question_id)
            .bind(turn_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
        }
        sqlx::query(
            r#"
            UPDATE conversation_question_redirects
            SET target_question_id = ?1, updated_at = ?2
            WHERE tenant_id = ?3 AND target_question_id = ?4
            "#,
        )
        .bind(&survivor_id)
        .bind(&now)
        .bind(tenant_id)
        .bind(question_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
        sqlx::query(
            r#"
            INSERT INTO conversation_question_redirects (
                tenant_id, source_question_id, target_question_id,
                operation_kind, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, 'merge', ?4, ?4)
            ON CONFLICT(tenant_id, source_question_id) DO UPDATE SET
                target_question_id = excluded.target_question_id,
                operation_kind = excluded.operation_kind,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(question_id)
        .bind(&survivor_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    }
    for question_id in &canonical_question_ids[1..] {
        sqlx::query("DELETE FROM conversation_questions WHERE tenant_id = ?1 AND id = ?2")
            .bind(tenant_id)
            .bind(question_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
        sqlx::query(
            "DELETE FROM conversation_question_fts WHERE tenant_id = ?1 AND question_id = ?2",
        )
        .bind(tenant_id)
        .bind(question_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    }
    renumber_questions_for_session_sqlx_tx(&mut tx, tenant_id, &session_id).await?;
    rebuild_session_question_aggregates_sqlx_tx(&mut tx, tenant_id, &session_id, &now).await?;
    super::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
    tx.commit().await.map_err(AppError::external)?;

    Ok(ConversationMutationResult {
        dry_run: false,
        session_id,
        affected_question_ids: question_ids.to_vec(),
        questions: vec![
            load_conversation_question_detail_sqlx(pool, tenant_id, &survivor_id).await?,
        ],
    })
}

pub(crate) async fn split_conversation_question_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_id: &str,
    before_turn_id: &str,
    dry_run: bool,
) -> AppResult<ConversationMutationResult> {
    audit_invalid_conversation_question_turns_sqlx(pool, tenant_id).await?;
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let question = load_conversation_question_sqlx_tx(&mut tx, tenant_id, question_id)
        .await?
        .ok_or_else(|| {
            AppError::external(format!("conversation question not found: {question_id}"))
        })?;
    reject_invalid_conversation_question_turns_sqlx_tx(&mut tx, tenant_id).await?;
    let turns = load_question_turns_sqlx_tx(&mut tx, tenant_id, question_id).await?;
    let new_question_id = stable_id(
        "conversation-question",
        &["split", question_id, before_turn_id],
    );
    let split_index = turns.iter().position(|turn| turn.id == before_turn_id);
    let Some(split_index) = split_index else {
        let existing_question_id = sqlx::query_scalar::<_, String>(
            "SELECT question_id FROM conversation_question_turns WHERE tenant_id = ?1 AND turn_id = ?2",
        )
        .bind(tenant_id)
        .bind(before_turn_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::external)?;
        if existing_question_id.as_deref() == Some(new_question_id.as_str())
            && load_conversation_question_sqlx_tx(&mut tx, tenant_id, &new_question_id)
                .await?
                .is_some()
        {
            tx.rollback().await.map_err(AppError::external)?;
            return Ok(ConversationMutationResult {
                dry_run,
                session_id: question.session_id.clone(),
                affected_question_ids: vec![question_id.to_string(), new_question_id.clone()],
                questions: vec![
                    load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?,
                    load_conversation_question_detail_sqlx(pool, tenant_id, &new_question_id)
                        .await?,
                ],
            });
        }
        return Err(AppError::external(format!(
            "turn is not in question: {before_turn_id}"
        )));
    };
    if split_index == 0 {
        return Err(AppError::Validation(
            "split turn must not be the first turn in the question".to_string(),
        ));
    }

    if dry_run {
        tx.rollback().await.map_err(AppError::external)?;
        return Ok(ConversationMutationResult {
            dry_run: true,
            session_id: question.session_id,
            affected_question_ids: vec![question_id.to_string()],
            questions: vec![
                load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?,
            ],
        });
    }

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE conversation_question_turns SET assignment_origin = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND question_id = ?4",
    )
    .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
    .bind(&now)
    .bind(tenant_id)
    .bind(question_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        r#"
        INSERT INTO conversation_questions (
            tenant_id, id, session_id, title, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, NULL, ?4, ?4)
        "#,
    )
    .bind(tenant_id)
    .bind(&new_question_id)
    .bind(&question.session_id)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    for (order, turn) in turns.iter().skip(split_index).enumerate() {
        sqlx::query(
            r#"
            UPDATE conversation_question_turns
            SET question_id = ?1,
                turn_order = ?2,
                assignment_origin = ?3,
                updated_at = ?4
            WHERE tenant_id = ?5 AND question_id = ?6 AND turn_id = ?7
            "#,
        )
        .bind(&new_question_id)
        .bind(order as i64)
        .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
        .bind(&now)
        .bind(tenant_id)
        .bind(question_id)
        .bind(&turn.id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    }
    renumber_question_turns_sqlx_tx(&mut tx, tenant_id, question_id, &now).await?;
    renumber_questions_for_session_sqlx_tx(&mut tx, tenant_id, &question.session_id).await?;
    rebuild_session_question_aggregates_sqlx_tx(&mut tx, tenant_id, &question.session_id, &now)
        .await?;
    super::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
    tx.commit().await.map_err(AppError::external)?;

    Ok(ConversationMutationResult {
        dry_run: false,
        session_id: question.session_id,
        affected_question_ids: vec![question_id.to_string(), new_question_id.clone()],
        questions: vec![
            load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?,
            load_conversation_question_detail_sqlx(pool, tenant_id, &new_question_id).await?,
        ],
    })
}

pub(crate) async fn update_conversation_part_translation_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    part_id: &str,
    translated_text: &str,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let result = sqlx::query(
        r#"
        UPDATE conversation_parts
        SET translated_text = ?1
        WHERE tenant_id = ?2 AND id = ?3
        "#,
    )
    .bind(translated_text)
    .bind(tenant_id)
    .bind(part_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "conversation part not found: {part_id}"
        )));
    }

    super::bump_conversation_search_source_revision_sqlx_tx(&mut *tx, tenant_id).await?;
    tx.commit().await.map_err(AppError::external)?;
    Ok(())
}

async fn load_conversation_question_details_for_session_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<Vec<ConversationQuestionDetail>> {
    let (adapter_id, card_kinds) =
        load_conversation_card_projection_context_sqlx(pool, tenant_id, session_id).await?;
    let question_rows = sqlx::query(
        r#"
        SELECT id, session_id, title, created_at, updated_at
        FROM conversation_questions
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY COALESCE((
            SELECT MIN(t.turn_index)
            FROM conversation_question_turns qt_order
            JOIN conversation_turns t
              ON t.tenant_id = qt_order.tenant_id AND t.id = qt_order.turn_id
            WHERE qt_order.tenant_id = conversation_questions.tenant_id
              AND qt_order.question_id = conversation_questions.id
        ), 9223372036854775807) ASC, created_at ASC, id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    let questions = question_rows
        .iter()
        .map(map_sqlx_conversation_question)
        .collect::<AppResult<Vec<_>>>()?;

    let question_turn_rows = sqlx::query(
        r#"
        SELECT qt.question_id, qt.turn_id, qt.turn_order,
               qt.assignment_origin, qt.assigned_at, qt.updated_at
        FROM conversation_question_turns qt
        JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND q.session_id = ?2
          AND q.session_id = t.session_id
        ORDER BY COALESCE((SELECT MIN(t_order.turn_index) FROM conversation_question_turns qt_order JOIN conversation_turns t_order ON t_order.tenant_id = qt_order.tenant_id AND t_order.id = qt_order.turn_id WHERE qt_order.tenant_id = q.tenant_id AND qt_order.question_id = q.id), 9223372036854775807) ASC, qt.turn_order ASC, t.turn_index ASC,
                 qt.turn_id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    let mut question_turns_by_question = BTreeMap::<String, Vec<ConversationQuestionTurn>>::new();
    for row in &question_turn_rows {
        let membership = map_sqlx_conversation_question_turn(row)?;
        question_turns_by_question
            .entry(membership.question_id.clone())
            .or_default()
            .push(membership);
    }

    let turn_rows = sqlx::query_as::<_, ConversationTurnWithQuestionRow>(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at,
               qt.question_id
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        JOIN conversation_questions q ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        WHERE q.tenant_id = ?1
          AND q.session_id = ?2
          AND q.session_id = t.session_id
        ORDER BY COALESCE((SELECT MIN(t_order.turn_index) FROM conversation_question_turns qt_order JOIN conversation_turns t_order ON t_order.tenant_id = qt_order.tenant_id AND t_order.id = qt_order.turn_id WHERE qt_order.tenant_id = q.tenant_id AND qt_order.question_id = q.id), 9223372036854775807) ASC, qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    let mut turns_by_question = BTreeMap::<String, Vec<ConversationTurn>>::new();
    for row in turn_rows {
        let (question_id, turn) = row.into_turn();
        turns_by_question.entry(question_id).or_default().push(turn);
    }

    let part_rows = sqlx::query(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label
        FROM conversation_turns t INDEXED BY idx_conversation_turns_tenant_session
        JOIN conversation_parts p INDEXED BY idx_conversation_parts_tenant_turn
          ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
        WHERE t.tenant_id = ?1 AND t.session_id = ?2
        ORDER BY t.turn_index ASC, p.part_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    let mut parts_by_turn = BTreeMap::<String, Vec<ConversationPart>>::new();
    for row in &part_rows {
        let part = map_sqlx_conversation_part(row)?;
        parts_by_turn
            .entry(part.turn_id.clone())
            .or_default()
            .push(part);
    }

    let mut details = Vec::with_capacity(questions.len());
    for question in questions {
        let question_turns = question_turns_by_question
            .remove(&question.id)
            .unwrap_or_default();
        let turns = turns_by_question.remove(&question.id).unwrap_or_default();
        let mut parts = Vec::new();
        for turn in &turns {
            parts.extend(parts_by_turn.remove(&turn.id).unwrap_or_default());
        }
        let projected_content_nodes = project_question_content_nodes(
            &question.id,
            &question_turns,
            &parts,
            &adapter_id,
            &card_kinds,
        )?;
        details.push(ConversationQuestionDetail {
            question: project_question_title(question, &turns),
            question_turns,
            turns,
            parts,
            projected_content_nodes,
        });
    }
    Ok(details)
}

async fn load_question_turn_memberships_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_id: &str,
) -> AppResult<Vec<ConversationQuestionTurn>> {
    let rows = sqlx::query(
        r#"
        SELECT qt.question_id, qt.turn_id, qt.turn_order,
               qt.assignment_origin, qt.assigned_at, qt.updated_at
        FROM conversation_question_turns qt
        JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND qt.question_id = ?2
          AND q.session_id = t.session_id
        ORDER BY qt.turn_order ASC, t.turn_index ASC, qt.turn_id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    rows.iter()
        .map(map_sqlx_conversation_question_turn)
        .collect()
}

pub(crate) fn project_question_title(
    mut question: ConversationQuestion,
    turns: &[ConversationTurn],
) -> ConversationQuestion {
    if question
        .title
        .as_deref()
        .is_none_or(|title| title.trim().is_empty())
    {
        if let Some(turn) = turns.iter().find(|turn| !turn.user_text.trim().is_empty()) {
            question.title = Some(first_line(&turn.user_text));
        }
    }
    question
}

async fn load_conversation_card_projection_context_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<(String, Vec<ConversationCardKindDefinition>)> {
    load_conversation_card_projection_context_for_record_sqlx(
        pool,
        tenant_id,
        ConversationRecordKind::Session,
        session_id,
    )
    .await
}

async fn load_conversation_card_projection_context_for_record_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    session_id: &str,
) -> AppResult<(String, Vec<ConversationCardKindDefinition>)> {
    let tables = record_kind.tables();
    let adapter_id = sqlx::query_scalar::<_, String>(AssertSqlSafe(format!(
        "SELECT adapter_id FROM {} WHERE tenant_id = ?1 AND id = ?2",
        tables.sessions
    )))
    .bind(tenant_id)
    .bind(session_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::external)?;
    let card_kinds_json = sqlx::query_scalar::<_, String>(
        "SELECT card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&adapter_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?
    .unwrap_or_else(|| "[]".to_string());
    Ok((adapter_id, decode_json(card_kinds_json)?))
}

fn conversation_question_block_locator(
    record_kind: ConversationRecordKind,
    session_id: &str,
    question_id: &str,
    turn: &ConversationTurn,
) -> ConversationBlockLocator {
    ConversationBlockLocator {
        record_kind: conversation_record_kind_label(record_kind).to_string(),
        session_id: session_id.to_string(),
        question_id: question_id.to_string(),
        turn_id: turn.id.clone(),
        block_id: format!("{}-question", turn.id),
        part_id: None,
        kind: "question".to_string(),
        semantic_role: None,
        renderer: ConversationCardRenderer::Plain,
        role: crate::backend::models::ConversationPartRole::User,
        content_length: turn.user_text.chars().count(),
        language: None,
        cwd: None,
        status: None,
        exit_code: None,
    }
}

fn conversation_card_block_locator(
    record_kind: ConversationRecordKind,
    session_id: &str,
    question_id: &str,
    part: &ConversationPart,
    card: &ConversationCard,
) -> ConversationBlockLocator {
    ConversationBlockLocator {
        record_kind: conversation_record_kind_label(record_kind).to_string(),
        session_id: session_id.to_string(),
        question_id: question_id.to_string(),
        turn_id: part.turn_id.clone(),
        block_id: card.node_id.clone(),
        part_id: Some(part.id.clone()),
        kind: card.kind.clone(),
        semantic_role: card.semantic_role.clone().or_else(|| {
            card.kind
                .rsplit_once('.')
                .map(|(_, value)| value.to_string())
        }),
        renderer: card.renderer,
        role: card.role.clone(),
        content_length: card.body.chars().count(),
        language: card.language.clone(),
        cwd: card.cwd.clone(),
        status: card.status.clone(),
        exit_code: card.exit_code,
    }
}

fn conversation_part_id_for_block_id(block_id: &str) -> &str {
    block_id
        .rsplit_once("-node-")
        .filter(|(_, order)| !order.is_empty() && order.chars().all(|value| value.is_ascii_digit()))
        .map(|(part_id, _)| part_id)
        .unwrap_or(block_id)
}

fn conversation_record_kind_label(record_kind: ConversationRecordKind) -> &'static str {
    match record_kind {
        ConversationRecordKind::Session => "session",
        ConversationRecordKind::Web => "web",
    }
}

pub(super) fn project_question_content_nodes(
    question_id: &str,
    question_turns: &[ConversationQuestionTurn],
    parts: &[ConversationPart],
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
) -> AppResult<Vec<ConversationContentNode>> {
    Ok(project_conversation_content_nodes(
        question_id,
        question_turns,
        parts,
        |part| {
            crate::backend::projection::conversation_cards::project_conversation_content_cards(
                part, adapter_id, card_kinds,
            )
            .map(|cards| {
                cards
                    .into_iter()
                    .map(ConversationContentNodeCandidate::from)
                    .collect()
            })
        },
    )?)
}

pub(crate) async fn load_recent_conversation_sync_deltas_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    source_id: Option<&str>,
    adapter_id: Option<&str>,
    recent_run_limit: usize,
) -> AppResult<Vec<ConversationSyncDelta>> {
    let record_kind = match record_kind {
        ConversationRecordKind::Session => "session",
        ConversationRecordKind::Web => "web",
    };
    let run_limit = i64::try_from(recent_run_limit.clamp(1, 20)).map_err(|_| {
        AppError::external("invalid recent conversation sync run limit".to_string())
    })?;
    sqlx::query_as::<_, ConversationSyncDelta>(
        r#"
        WITH recent_runs AS (
            SELECT r.id
            FROM conversation_sync_runs r
            JOIN conversation_sync_deltas d
              ON d.tenant_id = r.tenant_id AND d.sync_run_id = r.id
            WHERE r.tenant_id = ?1
              AND r.status = 'completed'
              AND d.record_kind = ?2
              AND (?3 IS NULL OR r.source_id = ?3)
              AND (?4 IS NULL OR r.adapter_id = ?4)
            GROUP BY r.id
            ORDER BY MAX(d.observed_at) DESC, r.id DESC
            LIMIT ?5
        )
        SELECT d.sync_run_id, d.session_id, d.change_kind, d.observed_at
        FROM conversation_sync_deltas d
        JOIN recent_runs r ON r.id = d.sync_run_id
        WHERE d.tenant_id = ?1 AND d.record_kind = ?2
        ORDER BY d.observed_at DESC, d.sync_run_id DESC, d.session_id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(record_kind)
    .bind(source_id)
    .bind(adapter_id)
    .bind(run_limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn search_conversation_cards_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    project_path: Option<&str>,
    query: &str,
    content_types: &[ConversationSearchCardType],
    semantic_roles: &[String],
    include_questions: bool,
    include_cards: bool,
    since: Option<&str>,
    until: Option<&str>,
    timeline: bool,
    limit: usize,
    offset: usize,
    allowed_session_ids: Option<&BTreeSet<String>>,
) -> AppResult<ConversationSearchPage> {
    let needle = normalize_query(Some(query))
        .ok_or_else(|| AppError::external("conversation search query is required".to_string()))?;
    let id_fragment = crate::backend::models::conversation_id_search_term(query)
        .map(|value| crate::backend::models::conversation_id_fragment(&value));
    let project_path = normalize_project_path(project_path);
    let since = parse_search_time_bound(since, SearchTimeBound::Since)?;
    let until = parse_search_time_bound(until, SearchTimeBound::Until)?;
    let allowed_types = content_types.iter().cloned().collect::<BTreeSet<_>>();
    let allowed_semantic_roles = semantic_roles.iter().cloned().collect::<BTreeSet<_>>();
    let all_types = BTreeSet::new();
    let adapter_card_kinds = load_search_adapter_card_kinds_sqlx(pool, tenant_id).await?;
    let tables = record_kind.tables();
    let id_matched_session_ids = if let Some(fragment) = id_fragment.as_deref() {
        let session_ids =
            load_search_session_ids_by_id_fragment_sqlx(pool, tenant_id, tables, fragment).await?;
        if session_ids.is_empty() {
            return Ok(ConversationSearchPage {
                total_count: 0,
                hits: Vec::new(),
            });
        }
        Some(session_ids)
    } else {
        None
    };
    let selected_session_ids = match (id_matched_session_ids, allowed_session_ids) {
        (Some(id_matched), Some(allowed)) => id_matched
            .intersection(allowed)
            .cloned()
            .collect::<BTreeSet<_>>(),
        (Some(id_matched), None) => id_matched,
        (None, Some(allowed)) => allowed.clone(),
        (None, None) => BTreeSet::new(),
    };
    if (id_fragment.is_some() || allowed_session_ids.is_some()) && selected_session_ids.is_empty() {
        return Ok(ConversationSearchPage {
            total_count: 0,
            hits: Vec::new(),
        });
    }
    let session_ids_json = if id_fragment.is_some() || allowed_session_ids.is_some() {
        Some(serde_json::to_string(&selected_session_ids).map_err(AppError::external)?)
    } else {
        None
    };
    let mut sessions = load_search_sessions_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        session_ids_json.as_deref(),
    )
    .await?;
    if timeline {
        sessions.sort_by(|left, right| {
            conversation_session_search_time(&left.session)
                .cmp(&conversation_session_search_time(&right.session))
                .then_with(|| left.session.title.cmp(&right.session.title))
        });
    }
    let mut questions_by_session = load_search_questions_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        session_ids_json.as_deref(),
    )
    .await?;
    let mut turns_by_question = load_search_turns_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        session_ids_json.as_deref(),
    )
    .await?;
    let mut parts_by_turn = load_search_parts_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        session_ids_json.as_deref(),
    )
    .await?;
    let mut hits = Vec::new();

    for session_item in sessions {
        let session = &session_item.session;
        if let Some(project_path) = project_path.as_deref() {
            let session_project = normalize_project_path(session.project_path.as_deref());
            if session_project.as_deref() != Some(project_path) {
                continue;
            }
        }
        if since.is_some() || until.is_some() {
            let Some(session_time) = conversation_session_search_time(session) else {
                continue;
            };
            if let Some(since) = since.as_ref() {
                if &session_time < since {
                    continue;
                }
            }
            if let Some(until) = until.as_ref() {
                if &session_time > until {
                    continue;
                }
            }
        }

        for (question_index, question) in questions_by_session
            .remove(&session.id)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            let question_turns = turns_by_question.remove(&question.id).unwrap_or_default();
            let question_title = search_question_title_from_turns(&question, &question_turns);
            for turn in question_turns {
                let question_block_id = format!("{}-question", turn.id);
                if include_questions {
                    push_search_hit_if_matching(
                        &mut hits,
                        &needle,
                        &all_types,
                        &session_item,
                        &question,
                        question_index as i64,
                        &question_title,
                        Some(turn.id.clone()),
                        None,
                        question_block_id.clone(),
                        ConversationSearchCardType::question(),
                        &turn.user_text,
                        id_fragment.as_deref(),
                        &[&session.id, &question.id, &turn.id, &question_block_id],
                    );
                }

                if !include_cards {
                    continue;
                }
                for part in parts_by_turn.remove(&turn.id).unwrap_or_default() {
                    for entry in search_entries_for_part(
                        &part,
                        &session.adapter_id,
                        adapter_card_kinds
                            .get(&session.adapter_id)
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                    ) {
                        if !allowed_semantic_roles.is_empty()
                            && entry
                                .semantic_role
                                .as_ref()
                                .is_none_or(|role| !allowed_semantic_roles.contains(role))
                        {
                            continue;
                        }
                        let entry_block_id = entry.block_id.clone();
                        push_search_hit_if_matching(
                            &mut hits,
                            &needle,
                            &allowed_types,
                            &session_item,
                            &question,
                            question_index as i64,
                            &question_title,
                            Some(turn.id.clone()),
                            Some(part.id.clone()),
                            entry.block_id,
                            entry.card_type,
                            &entry.text,
                            id_fragment.as_deref(),
                            &[
                                &session.id,
                                &question.id,
                                &turn.id,
                                &part.id,
                                &entry_block_id,
                            ],
                        );
                    }
                }
            }
        }
    }

    let total_count = hits.len();
    Ok(ConversationSearchPage {
        total_count,
        hits: hits.into_iter().skip(offset).take(limit).collect(),
    })
}

pub(crate) async fn hydrate_conversation_search_matches_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    query: &str,
    matches: crate::backend::search::conversation::ConversationSearchMatches,
) -> AppResult<ConversationSearchPage> {
    let tables = record_kind.tables();
    let adapter_card_kinds = load_search_adapter_card_kinds_sqlx(pool, tenant_id).await?;
    let session_ids = matches
        .hits
        .iter()
        .map(|matched| matched.session_id.as_str())
        .collect::<BTreeSet<_>>();
    let session_ids_json = serde_json::to_string(&session_ids).map_err(AppError::external)?;
    let sessions = load_search_sessions_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        Some(&session_ids_json),
    )
    .await?
    .into_iter()
    .map(|item| (item.session.id.clone(), item))
    .collect::<BTreeMap<_, _>>();
    let question_groups = load_search_questions_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        Some(&session_ids_json),
    )
    .await?;
    let question_indices = question_groups
        .values()
        .flat_map(|questions| questions.iter().enumerate())
        .map(|(index, question)| (question.id.clone(), index as i64))
        .collect::<BTreeMap<_, _>>();
    let questions = question_groups
        .into_values()
        .flatten()
        .map(|item| (item.id.clone(), item))
        .collect::<BTreeMap<_, _>>();
    let turns_by_question = load_search_turns_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        Some(&session_ids_json),
    )
    .await?;
    let turns = turns_by_question
        .values()
        .flatten()
        .cloned()
        .map(|item| (item.id.clone(), item))
        .collect::<BTreeMap<_, _>>();
    let parts = load_search_parts_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        Some(&session_ids_json),
    )
    .await?
    .into_values()
    .flatten()
    .map(|item| (item.id.clone(), item))
    .collect::<BTreeMap<_, _>>();
    let needle = normalize_query(Some(query))
        .ok_or_else(|| AppError::external("conversation search query is required".to_string()))?;
    let id_fragment = crate::backend::models::conversation_id_search_term(query)
        .map(|value| crate::backend::models::conversation_id_fragment(&value));
    let mut hits = Vec::with_capacity(matches.hits.len());

    for matched in matches.hits {
        let matched_by_id = id_fragment.as_deref().is_some_and(|fragment| {
            [
                matched.session_id.as_str(),
                matched.question_id.as_str(),
                matched.turn_id.as_str(),
                matched.part_id.as_str(),
                matched.document_id.as_str(),
            ]
            .into_iter()
            .any(|value| crate::backend::models::conversation_id_fragment(value) == fragment)
        });
        let session = sessions.get(&matched.session_id).ok_or_else(|| {
            AppError::external("conversation search index hydration missed a session".to_string())
        })?;
        let question = questions.get(&matched.question_id).ok_or_else(|| {
            AppError::external("conversation search index hydration missed a question".to_string())
        })?;
        let question_title = search_question_title_from_turns(
            question,
            turns_by_question
                .get(&question.id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
        let (part_id, text) = if matched.card_type == "question" {
            let turn = turns.get(&matched.turn_id).ok_or_else(|| {
                AppError::external("conversation search index hydration missed a turn".to_string())
            })?;
            (None, turn.user_text.clone())
        } else {
            let part = parts.get(&matched.part_id).ok_or_else(|| {
                AppError::external("conversation search index hydration missed a part".to_string())
            })?;
            let cards =
                crate::backend::projection::conversation_cards::project_conversation_content_cards(
                    part,
                    &session.session.adapter_id,
                    adapter_card_kinds
                        .get(&session.session.adapter_id)
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                )?;
            let card = cards
                .iter()
                .find(|card| card.node_id == matched.document_id)
                .ok_or_else(|| {
                    AppError::external(
                        "conversation search index hydration missed a projected card".to_string(),
                    )
                })?;
            if card.kind != matched.card_type || card.part_id != part.id {
                return Err(AppError::Validation(
                    "conversation search index hydration found stale card metadata".to_string(),
                ));
            }
            (Some(part.id.clone()), card.body.clone())
        };
        let card_type = content_card_type_value(&matched.card_type)
            .or_else(|| {
                (matched.card_type == "question").then_some(ConversationSearchCardType::question())
            })
            .ok_or_else(|| {
                AppError::external(
                    "conversation search index returned an invalid card type".to_string(),
                )
            })?;
        hits.push(ConversationSearchHit {
            session: session.clone(),
            question_id: question.id.clone(),
            question_index: question_indices.get(&question.id).copied().unwrap_or(0),
            question_title,
            turn_id: Some(matched.turn_id),
            part_id,
            block_id: matched.document_id,
            card_type,
            snippet: if matched_by_id {
                leading_search_snippet(&text)
            } else {
                search_snippet(&text, &needle)
            },
            score: matched.score,
            incremental: None,
            highlight_segments: if matched_by_id {
                None
            } else {
                search_highlight_segments(&text, &needle)
            },
        });
    }
    Ok(ConversationSearchPage {
        total_count: matches.total_count,
        hits,
    })
}

fn builtin_sources(now: &str) -> Vec<ConversationSource> {
    vec![
        ConversationSource {
            id: "codex-live".to_string(),
            adapter_id: "codex".to_string(),
            name: "Codex local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.codex".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationSource {
            id: "claude-code-live".to_string(),
            adapter_id: "claude-code".to_string(),
            name: "Claude Code local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.claude/projects".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationSource {
            id: "opencode-live".to_string(),
            adapter_id: "opencode".to_string(),
            name: "OpenCode local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.local/share/opencode/opencode.db".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
    ]
}

#[derive(Debug, FromRow)]
struct ConversationAdapterRow {
    id: String,
    name: String,
    kind: String,
    version: String,
    enabled: i64,
    manifest_path: Option<String>,
    executable_path: Option<String>,
    content_hash: Option<String>,
    trusted_hash: Option<String>,
    trust_state: String,
    protocol_version: Option<i64>,
    capabilities: String,
    input_kinds: String,
    card_contract_version: Option<i64>,
    card_kinds_json: String,
    created_at: String,
    updated_at: String,
}

impl ConversationAdapterRow {
    fn into_domain(self) -> AppResult<ConversationAdapter> {
        let protocol_version = self
            .protocol_version
            .map(|value| {
                u32::try_from(value)
                    .map_err(|_| AppError::external(format!("invalid protocol_version: {value}")))
            })
            .transpose()?;
        let card_contract_version = self
            .card_contract_version
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    AppError::external(format!("invalid card_contract_version: {value}"))
                })
            })
            .transpose()?;
        Ok(ConversationAdapter {
            id: self.id,
            name: self.name,
            kind: decode_enum(self.kind)?,
            version: self.version,
            enabled: self.enabled == 1,
            manifest_path: normalize_optional_conversation_path(self.manifest_path.as_deref())?,
            executable_path: normalize_optional_conversation_path(self.executable_path.as_deref())?,
            content_hash: self.content_hash,
            trusted_hash: self.trusted_hash,
            trust_state: decode_enum(self.trust_state)?,
            protocol_version,
            capabilities: decode_json(self.capabilities)?,
            input_kinds: decode_json(self.input_kinds)?,
            card_contract_version,
            card_kinds: decode_json(self.card_kinds_json)?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn map_sqlx_conversation_adapter(row: &SqliteRow) -> AppResult<ConversationAdapter> {
    ConversationAdapterRow::from_row(row)
        .map_err(AppError::external)?
        .into_domain()
}

#[derive(Debug, FromRow)]
struct ConversationAdapterPackageRow {
    package_id: String,
    adapter_id: String,
    name: String,
    version: String,
    record_kind: String,
    install_dir: String,
    manifest_path: String,
    adapter_manifest_path: String,
    runtime_protocol: String,
    runtime_ready: i64,
    origin: String,
    source_url: Option<String>,
    git_ref: Option<String>,
    git_commit: Option<String>,
    catalog_url: Option<String>,
    update_policy: String,
    latest_version: Option<String>,
    last_checked_at: Option<String>,
    runtime_gate_status: String,
    runtime_validated_at: Option<String>,
    installed_content_hash: Option<String>,
    trusted_package_hash: Option<String>,
    error_message: Option<String>,
    created_at: String,
    updated_at: String,
}

impl ConversationAdapterPackageRow {
    fn into_domain(self) -> AppResult<ConversationAdapterPackage> {
        Ok(ConversationAdapterPackage {
            package_id: self.package_id,
            adapter_id: self.adapter_id,
            name: self.name,
            version: self.version,
            record_kind: decode_enum(self.record_kind)?,
            install_dir: normalize_conversation_path(&self.install_dir)?,
            manifest_path: normalize_conversation_path(&self.manifest_path)?,
            adapter_manifest_path: normalize_conversation_path(&self.adapter_manifest_path)?,
            runtime_protocol: self.runtime_protocol,
            runtime_ready: self.runtime_ready == 1,
            origin: decode_enum(self.origin)?,
            source_url: self.source_url,
            git_ref: self.git_ref,
            git_commit: self.git_commit,
            catalog_url: self.catalog_url,
            update_policy: decode_enum(self.update_policy)?,
            latest_version: self.latest_version,
            last_checked_at: self.last_checked_at,
            runtime_gate_status: decode_enum(self.runtime_gate_status)?,
            runtime_validated_at: self.runtime_validated_at,
            installed_content_hash: self.installed_content_hash,
            trusted_package_hash: self.trusted_package_hash,
            error_message: self.error_message,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn map_sqlx_conversation_adapter_package(row: &SqliteRow) -> AppResult<ConversationAdapterPackage> {
    ConversationAdapterPackageRow::from_row(row)
        .map_err(AppError::external)?
        .into_domain()
}

#[derive(Debug, FromRow)]
struct ConversationSourceRow {
    id: String,
    adapter_id: String,
    name: String,
    kind: String,
    location: String,
    config_json: Option<String>,
    enabled: i64,
    last_synced_at: Option<String>,
    last_sync_status: Option<String>,
    created_at: String,
    updated_at: String,
}

impl ConversationSourceRow {
    fn into_domain(self) -> AppResult<ConversationSource> {
        Ok(ConversationSource {
            id: self.id,
            adapter_id: self.adapter_id,
            name: self.name,
            kind: decode_enum(self.kind)?,
            location: normalize_conversation_source_location(&self.location)?,
            config_json: self.config_json,
            enabled: self.enabled == 1,
            last_synced_at: self.last_synced_at,
            last_sync_status: self.last_sync_status,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn map_sqlx_conversation_source(row: &SqliteRow) -> AppResult<ConversationSource> {
    ConversationSourceRow::from_row(row)
        .map_err(AppError::external)?
        .into_domain()
}

fn normalize_conversation_source_location(location: &str) -> AppResult<String> {
    if location.contains("://") {
        return Ok(location.to_string());
    }
    Ok(crate::backend::path_utils::normalize_path_for_storage(
        location,
    )?)
}

fn normalize_conversation_path(path: &str) -> AppResult<String> {
    Ok(crate::backend::path_utils::normalize_path_for_storage(
        path,
    )?)
}

fn normalize_optional_conversation_path(path: Option<&str>) -> AppResult<Option<String>> {
    path.map(normalize_conversation_path).transpose()
}

#[derive(Debug, FromRow)]
struct ConversationSessionRow {
    id: String,
    source_id: String,
    adapter_id: String,
    external_id: String,
    title: String,
    project_path: Option<String>,
    started_at: Option<String>,
    updated_at: Option<String>,
    source_locator: Option<String>,
    source_fingerprint: Option<String>,
    missing: i64,
    created_at: String,
    imported_at: String,
    #[sqlx(default)]
    execution_origin: Option<String>,
    #[sqlx(default)]
    execution_purpose: Option<String>,
    #[sqlx(default)]
    user_visible: Option<i64>,
}

impl ConversationSessionRow {
    fn into_domain(self) -> ConversationSession {
        ConversationSession {
            id: self.id,
            source_id: self.source_id,
            adapter_id: self.adapter_id,
            external_id: self.external_id,
            title: self.title,
            project_path: self.project_path,
            started_at: self.started_at,
            updated_at: self.updated_at,
            source_locator: self.source_locator,
            source_fingerprint: self.source_fingerprint,
            missing: self.missing == 1,
            created_at: self.created_at,
            imported_at: self.imported_at,
            execution_origin: self.execution_origin.unwrap_or_else(|| "user".to_string()),
            execution_purpose: self.execution_purpose,
            user_visible: self.user_visible.map(|v| v != 0).unwrap_or(true),
        }
    }
}

pub(super) fn map_sqlx_conversation_session(row: &SqliteRow) -> AppResult<ConversationSession> {
    Ok(ConversationSessionRow::from_row(row)
        .map_err(AppError::external)?
        .into_domain())
}

#[derive(Debug, FromRow)]
struct ConversationTurnRow {
    id: String,
    session_id: String,
    external_id: String,
    turn_index: i64,
    user_text: String,
    title: Option<String>,
    started_at: Option<String>,
    ended_at: Option<String>,
    fingerprint: String,
    missing: i64,
    imported_at: String,
}

impl ConversationTurnRow {
    fn into_domain(self) -> ConversationTurn {
        ConversationTurn {
            id: self.id,
            session_id: self.session_id,
            external_id: self.external_id,
            turn_index: self.turn_index,
            user_text: self.user_text,
            title: self.title,
            started_at: self.started_at,
            ended_at: self.ended_at,
            fingerprint: self.fingerprint,
            missing: self.missing == 1,
            imported_at: self.imported_at,
        }
    }
}

pub(super) fn map_sqlx_conversation_turn(row: &SqliteRow) -> AppResult<ConversationTurn> {
    Ok(ConversationTurnRow::from_row(row)
        .map_err(AppError::external)?
        .into_domain())
}

#[derive(Debug, FromRow)]
struct ConversationPartRow {
    id: String,
    turn_id: String,
    part_index: i64,
    role: String,
    kind: String,
    text: Option<String>,
    language: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    status: Option<String>,
    exit_code: Option<i64>,
    metadata_json: Option<String>,
    command_label: Option<String>,
    source_execution_id: Option<String>,
    content_card_json: Option<String>,
    translated_text: Option<String>,
}

impl ConversationPartRow {
    fn into_domain(self) -> AppResult<ConversationPart> {
        Ok(ConversationPart {
            id: self.id,
            turn_id: self.turn_id,
            part_index: self.part_index,
            role: decode_enum(self.role)?,
            kind: decode_enum(self.kind)?,
            text: self.text,
            language: self.language,
            command: self.command,
            cwd: self.cwd,
            status: self.status,
            exit_code: self.exit_code.map(|v| v as i32),
            command_label: self.command_label,
            source_execution_id: self.source_execution_id,
            content_card: self.content_card_json.map(decode_json).transpose()?,
            metadata_json: self.metadata_json,
            translated_text: self.translated_text,
        })
    }
}

pub(super) fn map_sqlx_conversation_part(row: &SqliteRow) -> AppResult<ConversationPart> {
    ConversationPartRow::from_row(row)
        .map_err(AppError::external)?
        .into_domain()
}

#[derive(Debug, FromRow)]
struct ConversationQuestionRow {
    id: String,
    session_id: String,
    title: Option<String>,
    created_at: String,
    updated_at: String,
}

impl ConversationQuestionRow {
    fn into_domain(self) -> ConversationQuestion {
        ConversationQuestion {
            id: self.id,
            session_id: self.session_id,
            title: self.title,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

pub(super) fn map_sqlx_conversation_question(row: &SqliteRow) -> AppResult<ConversationQuestion> {
    Ok(ConversationQuestionRow::from_row(row)
        .map_err(AppError::external)?
        .into_domain())
}

#[derive(Debug, FromRow)]
struct ConversationQuestionTurnRow {
    question_id: String,
    turn_id: String,
    turn_order: i64,
    assignment_origin: String,
    assigned_at: String,
    updated_at: String,
}

impl ConversationQuestionTurnRow {
    fn into_domain(self) -> AppResult<ConversationQuestionTurn> {
        Ok(ConversationQuestionTurn {
            question_id: self.question_id,
            turn_id: self.turn_id,
            turn_order: self.turn_order,
            assignment_origin: decode_enum(self.assignment_origin)?,
            assigned_at: self.assigned_at,
            updated_at: self.updated_at,
        })
    }
}

pub(super) fn map_sqlx_conversation_question_turn(
    row: &SqliteRow,
) -> AppResult<ConversationQuestionTurn> {
    ConversationQuestionTurnRow::from_row(row)
        .map_err(AppError::external)?
        .into_domain()
}

fn conversation_session_from_normalized(
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    now: &str,
) -> ConversationSession {
    let is_agent_workspace = normalized
        .project_path
        .as_deref()
        .map(|p| p.contains("agent-executions"))
        .unwrap_or(false);
    let execution_origin = normalized.execution_origin.clone().unwrap_or_else(|| {
        if is_agent_workspace {
            "internal_agent".to_string()
        } else {
            "user".to_string()
        }
    });
    let execution_purpose = normalized.execution_purpose.clone();
    let user_visible = normalized
        .user_visible
        .unwrap_or_else(|| execution_origin == "user" || execution_origin == "recall");

    ConversationSession {
        id: stable_id(
            "conversation-session",
            &[&source.id, &normalized.external_id],
        ),
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        external_id: normalized.external_id.clone(),
        title: normalized
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Untitled session")
            .to_string(),
        project_path: normalized.project_path.clone(),
        started_at: normalized.started_at.clone(),
        updated_at: normalized.updated_at.clone(),
        source_locator: normalized.source_locator.clone(),
        source_fingerprint: normalized.source_fingerprint.clone(),
        missing: false,
        created_at: now.to_string(),
        imported_at: now.to_string(),
        execution_origin,
        execution_purpose,
        user_visible,
    }
}

fn conversation_turn_from_normalized(
    session_id: &str,
    normalized: &crate::backend::models::NormalizedConversationTurn,
    now: &str,
) -> ConversationTurn {
    ConversationTurn {
        id: stable_id("conversation-turn", &[session_id, &normalized.external_id]),
        session_id: session_id.to_string(),
        external_id: normalized.external_id.clone(),
        turn_index: normalized.turn_index,
        user_text: normalized.user_text.trim().to_string(),
        title: normalized.title.clone(),
        started_at: normalized.started_at.clone(),
        ended_at: normalized.ended_at.clone(),
        fingerprint: conversation_turn_fingerprint(normalized),
        missing: false,
        imported_at: now.to_string(),
    }
}

async fn conversation_session_is_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
    normalized: &NormalizedConversationSession,
) -> AppResult<bool> {
    let Some(source_fingerprint) = session.source_fingerprint.as_deref() else {
        return Ok(false);
    };
    #[derive(Debug, FromRow)]
    struct ConversationSessionUnchangedCheckRow {
        title: String,
        project_path: Option<String>,
        started_at: Option<String>,
        updated_at: Option<String>,
        source_locator: Option<String>,
        source_fingerprint: Option<String>,
        missing: i64,
    }

    let Some(row) = sqlx::query_as::<_, ConversationSessionUnchangedCheckRow>(
        r#"
        SELECT title, project_path, started_at, updated_at, source_locator,
               source_fingerprint, missing
        FROM conversation_sessions
        WHERE tenant_id = ?1 AND source_id = ?2 AND external_id = ?3
        "#,
    )
    .bind(tenant_id)
    .bind(&session.source_id)
    .bind(&session.external_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::external)?
    else {
        return Ok(false);
    };

    Ok(row.title == session.title
        && row.project_path == session.project_path
        && row.started_at == session.started_at
        && row.updated_at == session.updated_at
        && row.source_locator == session.source_locator
        && row.source_fingerprint.as_deref() == Some(source_fingerprint)
        && row.missing == 0
        && conversation_session_turns_are_unchanged_sqlx_tx(tx, tenant_id, &session.id, normalized)
            .await?)
}

async fn conversation_session_exists_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<bool> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM conversation_sessions WHERE tenant_id = ?1 AND id = ?2)",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(exists != 0)
}

async fn conversation_session_turns_are_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    normalized: &NormalizedConversationSession,
) -> AppResult<bool> {
    #[derive(Debug, FromRow)]
    struct ConversationTurnUnchangedCheckRow {
        external_id: String,
        fingerprint: String,
        missing: i64,
    }

    let rows = sqlx::query_as::<_, ConversationTurnUnchangedCheckRow>(
        r#"
        SELECT external_id, fingerprint, missing
        FROM conversation_turns
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY turn_index ASC, external_id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    if rows.len() != normalized.turns.len() {
        return Ok(false);
    }
    for (row, turn) in rows.iter().zip(&normalized.turns) {
        if row.external_id != turn.external_id
            || row.fingerprint != conversation_turn_fingerprint(turn)
            || row.missing != 0
        {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn upsert_conversation_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title, project_path, started_at,
            updated_at, source_locator, source_fingerprint, missing, created_at, imported_at,
            execution_origin, execution_purpose, user_visible
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        ON CONFLICT(tenant_id, source_id, external_id) DO UPDATE SET
            adapter_id = excluded.adapter_id,
            title = excluded.title,
            project_path = excluded.project_path,
            started_at = excluded.started_at,
            updated_at = excluded.updated_at,
            source_locator = excluded.source_locator,
            source_fingerprint = excluded.source_fingerprint,
            missing = 0,
            imported_at = excluded.imported_at,
            execution_origin = excluded.execution_origin,
            execution_purpose = excluded.execution_purpose,
            user_visible = excluded.user_visible
        "#,
    )
    .bind(tenant_id)
    .bind(&session.id)
    .bind(&session.source_id)
    .bind(&session.adapter_id)
    .bind(&session.external_id)
    .bind(&session.title)
    .bind(&session.project_path)
    .bind(&session.started_at)
    .bind(&session.updated_at)
    .bind(&session.source_locator)
    .bind(&session.source_fingerprint)
    .bind(if session.missing { 1_i64 } else { 0_i64 })
    .bind(&session.created_at)
    .bind(&session.imported_at)
    .bind(&session.execution_origin)
    .bind(&session.execution_purpose)
    .bind(if session.user_visible { 1_i64 } else { 0_i64 })
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

async fn upsert_conversation_turn_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn: &ConversationTurn,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_turns (
            tenant_id, id, session_id, external_id, turn_index, user_text, title, started_at,
            ended_at, fingerprint, missing, imported_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        ON CONFLICT(tenant_id, session_id, external_id) DO UPDATE SET
            turn_index = excluded.turn_index,
            user_text = excluded.user_text,
            title = excluded.title,
            started_at = excluded.started_at,
            ended_at = excluded.ended_at,
            fingerprint = excluded.fingerprint,
            missing = 0,
            imported_at = excluded.imported_at
        "#,
    )
    .bind(tenant_id)
    .bind(&turn.id)
    .bind(&turn.session_id)
    .bind(&turn.external_id)
    .bind(turn.turn_index)
    .bind(&turn.user_text)
    .bind(&turn.title)
    .bind(&turn.started_at)
    .bind(&turn.ended_at)
    .bind(&turn.fingerprint)
    .bind(if turn.missing { 1_i64 } else { 0_i64 })
    .bind(&turn.imported_at)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

async fn replace_conversation_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
    parts: &[crate::backend::models::NormalizedConversationPart],
) -> AppResult<()> {
    let existing_ids = sqlx::query_scalar::<_, String>(
        "SELECT id FROM conversation_parts WHERE tenant_id = ?1 AND turn_id = ?2",
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    let mut incoming_ids = BTreeSet::new();
    for (index, part) in parts.iter().enumerate() {
        let part_id = stable_id("conversation-part", &[turn_id, &index.to_string()]);
        incoming_ids.insert(part_id.clone());
        let content_card_json = part.content_card.as_ref().map(encode_json).transpose()?;
        sqlx::query(
            r#"
            INSERT INTO conversation_parts (
                tenant_id, id, turn_id, part_index, role, kind, text, language, command,
                cwd, status, exit_code, command_label, metadata_json, content_card_json, source_execution_id
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(tenant_id, id) DO UPDATE SET
                turn_id = excluded.turn_id,
                part_index = excluded.part_index,
                role = excluded.role,
                kind = excluded.kind,
                text = excluded.text,
                language = excluded.language,
                command = excluded.command,
                cwd = excluded.cwd,
                status = excluded.status,
                exit_code = excluded.exit_code,
                command_label = excluded.command_label,
                metadata_json = excluded.metadata_json,
                content_card_json = excluded.content_card_json,
                source_execution_id = excluded.source_execution_id,
                translated_text = CASE
                    WHEN COALESCE(conversation_parts.text, '') = COALESCE(excluded.text, '')
                     AND COALESCE(conversation_parts.command, '') = COALESCE(excluded.command, '')
                    THEN conversation_parts.translated_text
                    ELSE NULL
                END
            "#,
        )
        .bind(tenant_id)
        .bind(part_id)
        .bind(turn_id)
        .bind(index as i64)
        .bind(encode_enum(part.role)?)
        .bind(encode_enum(part.kind)?)
        .bind(&part.text)
        .bind(&part.language)
        .bind(&part.command)
        .bind(&part.cwd)
        .bind(&part.status)
        .bind(part.exit_code)
        .bind(&part.command_label)
        .bind(&part.metadata_json)
        .bind(content_card_json)
        .bind(&part.source_execution_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
    }
    for stale_id in existing_ids
        .into_iter()
        .filter(|id| !incoming_ids.contains(id))
    {
        sqlx::query("DELETE FROM conversation_parts WHERE tenant_id = ?1 AND id = ?2")
            .bind(tenant_id)
            .bind(stale_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
    }
    Ok(())
}

async fn mark_missing_conversation_sessions_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    source_id: &str,
    incoming_session_ids: &BTreeSet<String>,
    sync_run_id: &str,
    now: &str,
) -> AppResult<Vec<String>> {
    let mut changed_session_ids = Vec::new();
    let existing_sessions = sqlx::query_as::<_, (String, i64)>(
        "SELECT id, missing FROM conversation_sessions WHERE tenant_id = ?1 AND source_id = ?2",
    )
    .bind(tenant_id)
    .bind(source_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    for (session_id, missing) in existing_sessions {
        if incoming_session_ids.contains(&session_id) {
            if missing == 0 {
                continue;
            }
            sqlx::query(
                r#"
                UPDATE conversation_sessions
                SET missing = 0
                WHERE tenant_id = ?1 AND id = ?2
                "#,
            )
            .bind(tenant_id)
            .bind(&session_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
            insert_conversation_sync_delta_sqlx_tx(
                tx,
                tenant_id,
                sync_run_id,
                "session",
                &session_id,
                "restored",
                now,
            )
            .await?;
            changed_session_ids.push(session_id.clone());
            continue;
        }
        if missing != 0 {
            continue;
        }
        sqlx::query(
            r#"
            UPDATE conversation_sessions
            SET missing = 1, imported_at = ?1
            WHERE tenant_id = ?2 AND id = ?3
            "#,
        )
        .bind(now)
        .bind(tenant_id)
        .bind(&session_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
        sqlx::query(
            "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE tenant_id = ?2 AND session_id = ?3 AND status = 'active'",
        )
        .bind(now)
        .bind(tenant_id)
        .bind(&session_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
        insert_conversation_sync_delta_sqlx_tx(
            tx,
            tenant_id,
            sync_run_id,
            "session",
            &session_id,
            "missing",
            now,
        )
        .await?;
        changed_session_ids.push(session_id);
    }
    Ok(changed_session_ids)
}

async fn prune_conversation_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    normalized: &NormalizedConversationSession,
) -> AppResult<()> {
    let retained_turn_ids = normalized
        .turns
        .iter()
        .filter(|turn| !turn.user_text.trim().is_empty())
        .map(|turn| stable_id("conversation-turn", &[session_id, &turn.external_id]))
        .collect::<BTreeSet<_>>();
    let turn_ids = sqlx::query_scalar::<_, String>(
        "SELECT id FROM conversation_turns WHERE tenant_id = ?1 AND session_id = ?2",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    let stale_turn_ids = turn_ids
        .into_iter()
        .filter(|turn_id| !retained_turn_ids.contains(turn_id))
        .collect::<Vec<_>>();
    if stale_turn_ids.is_empty() {
        return Ok(());
    }

    for turn_id in &stale_turn_ids {
        sqlx::query("DELETE FROM conversation_parts WHERE tenant_id = ?1 AND turn_id = ?2")
            .bind(tenant_id)
            .bind(turn_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
        sqlx::query(
            "DELETE FROM conversation_question_turns WHERE tenant_id = ?1 AND turn_id = ?2",
        )
        .bind(tenant_id)
        .bind(turn_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
        sqlx::query("DELETE FROM conversation_turns WHERE tenant_id = ?1 AND id = ?2")
            .bind(tenant_id)
            .bind(turn_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
    }
    sqlx::query(
        r#"
        DELETE FROM conversation_question_fts
        WHERE tenant_id = ?1
          AND question_id IN (
            SELECT q.id
            FROM conversation_questions q
            LEFT JOIN conversation_question_turns qt
              ON qt.tenant_id = q.tenant_id AND qt.question_id = q.id
            WHERE q.tenant_id = ?1 AND q.session_id = ?2
            GROUP BY q.id
            HAVING COUNT(qt.turn_id) = 0
        )
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        r#"
        DELETE FROM conversation_questions
        WHERE tenant_id = ?1 AND session_id = ?2
          AND id NOT IN (
              SELECT DISTINCT question_id
              FROM conversation_question_turns
              WHERE tenant_id = ?1
          )
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    renumber_questions_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    Ok(())
}

async fn ensure_question_groups_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    now: &str,
) -> AppResult<()> {
    reject_invalid_conversation_question_turns_sqlx_tx(tx, tenant_id).await?;
    let turns = load_session_turns_sqlx_tx(tx, tenant_id, session_id).await?;
    if turns.is_empty() {
        return Ok(());
    }

    let manual_fenced_turn_ids = sqlx::query_scalar::<_, String>(
        r#"
        SELECT qt.turn_id
        FROM conversation_question_turns qt
        JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND t.session_id = ?2
          AND qt.assignment_origin = 'manual'
        ORDER BY t.turn_index ASC, t.id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?
    .into_iter()
    .collect::<BTreeSet<_>>();

    // Automatic rows are a derived relationship. Rebuilding only these rows
    // makes full, repeated full, and equivalent incremental imports converge to
    // the same stable question IDs while preserving all manual rows.
    sqlx::query(
        r#"
        DELETE FROM conversation_question_turns
        WHERE tenant_id = ?1
          AND assignment_origin <> 'manual'
          AND turn_id IN (
              SELECT id FROM conversation_turns
              WHERE tenant_id = ?1 AND session_id = ?2
          )
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;

    let mut automatic_segment = Vec::new();
    let mut automatic_segments = Vec::new();
    for turn in &turns {
        if manual_fenced_turn_ids.contains(&turn.id) {
            if !automatic_segment.is_empty() {
                automatic_segments.push(std::mem::take(&mut automatic_segment));
            }
        } else {
            automatic_segment.push((turn.id.clone(), turn.user_text.clone()));
        }
    }
    if !automatic_segment.is_empty() {
        automatic_segments.push(automatic_segment);
    }

    for segment in automatic_segments {
        for group in group_turn_ids_by_question(segment) {
            let first_turn_id = group.turn_ids.first().ok_or_else(|| {
                AppError::external("empty conversation question group".to_string())
            })?;
            let question_id = stable_id("conversation-question", &[session_id, first_turn_id]);
            match load_conversation_question_sqlx_tx(tx, tenant_id, &question_id).await? {
                Some(question) if question.session_id != session_id => {
                    return Err(AppError::Validation(format!(
                        "question id belongs to another session: {question_id}"
                    )));
                }
                Some(_) => {}
                None => {
                    sqlx::query(
                        r#"
                        INSERT INTO conversation_questions (
                            tenant_id, id, session_id, title, created_at, updated_at
                        )
                        VALUES (?1, ?2, ?3, NULL, ?4, ?4)
                        "#,
                    )
                    .bind(tenant_id)
                    .bind(&question_id)
                    .bind(session_id)
                    .bind(now)
                    .execute(&mut **tx)
                    .await
                    .map_err(AppError::external)?;
                }
            }

            for (turn_order, turn_id) in group.turn_ids.iter().enumerate() {
                ensure_question_turn_scope_sqlx_tx(tx, tenant_id, &question_id, turn_id).await?;
                sqlx::query(
                    r#"
                    INSERT INTO conversation_question_turns (
                        tenant_id, question_id, turn_id, turn_order,
                        assignment_origin, assigned_at, updated_at
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                    "#,
                )
                .bind(tenant_id)
                .bind(&question_id)
                .bind(turn_id)
                .bind(turn_order as i64)
                .bind(encode_enum(group.origin)?)
                .bind(now)
                .execute(&mut **tx)
                .await
                .map_err(AppError::external)?;
            }
        }
    }

    sqlx::query(
        r#"
        DELETE FROM conversation_question_fts
        WHERE tenant_id = ?1
          AND question_id IN (
              SELECT q.id
              FROM conversation_questions q
              LEFT JOIN conversation_question_turns qt
                ON qt.tenant_id = q.tenant_id AND qt.question_id = q.id
              WHERE q.tenant_id = ?1 AND q.session_id = ?2
              GROUP BY q.id
              HAVING COUNT(qt.turn_id) = 0
          )
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        r#"
        DELETE FROM conversation_questions
        WHERE tenant_id = ?1 AND session_id = ?2
          AND NOT EXISTS (
              SELECT 1
              FROM conversation_question_turns qt
              WHERE qt.tenant_id = conversation_questions.tenant_id
                AND qt.question_id = conversation_questions.id
          )
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    renumber_questions_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    Ok(())
}

async fn ensure_question_turn_scope_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
    turn_id: &str,
) -> AppResult<()> {
    let same_session = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT 1
        FROM conversation_questions q
        JOIN conversation_turns t
          ON t.tenant_id = q.tenant_id
         AND t.session_id = q.session_id
         AND t.id = ?3
        WHERE q.tenant_id = ?1 AND q.id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .bind(turn_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::external)?;
    if same_session.is_none() {
        return Err(AppError::Validation(format!(
            "question turn membership must use the same tenant and session: question={question_id}, turn={turn_id}"
        )));
    }
    Ok(())
}

#[derive(Debug, FromRow)]
struct InvalidConversationQuestionTurnRow {
    question_id: String,
    turn_id: String,
    reason: String,
}

async fn reject_invalid_conversation_question_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
) -> AppResult<()> {
    let rows = sqlx::query_as::<_, InvalidConversationQuestionTurnRow>(
        r#"
        SELECT qt.question_id, qt.turn_id,
               CASE
                   WHEN q.id IS NULL THEN 'missing_question'
                   WHEN t.id IS NULL THEN 'missing_turn'
                   ELSE 'cross_session'
               END AS reason
        FROM conversation_question_turns qt
        LEFT JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        LEFT JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND (q.id IS NULL OR t.id IS NULL OR q.session_id <> t.session_id)
        ORDER BY qt.question_id ASC, qt.turn_id ASC
        "#,
    )
    .bind(tenant_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    if let Some(first) = rows.first() {
        return Err(AppError::Validation(format!(
            "invalid question turn membership ({}): question={}, turn={}",
            first.reason, first.question_id, first.turn_id
        )));
    }
    Ok(())
}

async fn audit_invalid_conversation_question_turns_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    let rows = sqlx::query_as::<_, InvalidConversationQuestionTurnRow>(
        r#"
        SELECT qt.question_id, qt.turn_id,
               CASE
                   WHEN q.id IS NULL THEN 'missing_question'
                   WHEN t.id IS NULL THEN 'missing_turn'
                   ELSE 'cross_session'
               END AS reason
        FROM conversation_question_turns qt
        LEFT JOIN conversation_questions q
          ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
        LEFT JOIN conversation_turns t
          ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1
          AND (q.id IS NULL OR t.id IS NULL OR q.session_id <> t.session_id)
        ORDER BY qt.question_id ASC, qt.turn_id ASC
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    if rows.is_empty() {
        return Ok(());
    }

    let detected_at = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    for row in &rows {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO conversation_question_turn_audits (
                tenant_id, record_kind, question_id, turn_id, reason, detected_at
            )
            VALUES (?1, 'session', ?2, ?3, ?4, ?5)
            "#,
        )
        .bind(tenant_id)
        .bind(&row.question_id)
        .bind(&row.turn_id)
        .bind(&row.reason)
        .bind(&detected_at)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    }
    tx.commit().await.map_err(AppError::external)
}

async fn rebuild_session_question_aggregates_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    now: &str,
) -> AppResult<()> {
    let question_ids = question_ids_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    for question_id in question_ids {
        rebuild_question_aggregate_sqlx_tx(tx, tenant_id, &question_id, now).await?;
    }
    Ok(())
}

async fn rebuild_question_aggregate_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
    now: &str,
) -> AppResult<()> {
    // FTS remains an independently rebuildable projection. Question Detail reads
    // only membership and Turn-Part source facts.
    let turns = load_question_turns_sqlx_tx(tx, tenant_id, question_id).await?;
    let mut question_text = Vec::new();
    let mut answer_text = Vec::new();
    let mut code_text = Vec::new();
    let mut command_text = Vec::new();
    let adapter_id = sqlx::query_scalar::<_, String>(
        r#"
        SELECT s.adapter_id
        FROM conversation_questions q
        JOIN conversation_sessions s
          ON s.tenant_id = q.tenant_id AND s.id = q.session_id
        WHERE q.tenant_id = ?1 AND q.id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::external)?;
    let card_kinds_json = sqlx::query_scalar::<_, String>(
        "SELECT card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(&adapter_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::external)?
    .unwrap_or_else(|| "[]".to_string());
    let card_kinds: Vec<ConversationCardKindDefinition> = decode_json(card_kinds_json)?;

    for turn in &turns {
        question_text.push(turn.user_text.clone());
        for part in load_turn_parts_sqlx_tx(tx, tenant_id, &turn.id).await? {
            append_projected_cards_to_question_aggregate(
                &part,
                &adapter_id,
                &card_kinds,
                &mut answer_text,
                &mut code_text,
                &mut command_text,
            )?;
        }
    }

    let question_text = question_text.join("\n\n");
    let answer_text = answer_text.join("\n\n");
    let code_text = code_text.join("\n\n");
    let command_text = command_text.join("\n\n");
    let title = first_line(&question_text);

    sqlx::query(
        "UPDATE conversation_questions SET title = COALESCE(NULLIF(title, ''), ?1), updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4",
    )
    .bind(&title)
    .bind(now)
    .bind(tenant_id)
    .bind(question_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    let session_id: String = sqlx::query_scalar::<_, String>(
        "SELECT session_id FROM conversation_questions WHERE tenant_id = ?1 AND id = ?2",
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::external)?;
    sqlx::query("DELETE FROM conversation_question_fts WHERE tenant_id = ?1 AND question_id = ?2")
        .bind(tenant_id)
        .bind(question_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
    sqlx::query(
        r#"
        INSERT INTO conversation_question_fts (
            tenant_id, question_id, session_id, question_text, answer_text, code_text, command_text
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .bind(&session_id)
    .bind(&question_text)
    .bind(&answer_text)
    .bind(&code_text)
    .bind(&command_text)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

async fn insert_sync_run_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    run: &ConversationSyncRun,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_sync_runs (
            tenant_id, id, source_id, adapter_id, status, started_at, finished_at,
            session_count, turn_count, warning_count, error_message
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
    )
    .bind(tenant_id)
    .bind(&run.id)
    .bind(&run.source_id)
    .bind(&run.adapter_id)
    .bind(encode_enum(run.status)?)
    .bind(&run.started_at)
    .bind(&run.finished_at)
    .bind(run.session_count)
    .bind(run.turn_count)
    .bind(run.warning_count)
    .bind(&run.error_message)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn insert_conversation_sync_delta_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    sync_run_id: &str,
    record_kind: &str,
    session_id: &str,
    change_kind: &str,
    observed_at: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_sync_deltas (
            tenant_id, sync_run_id, record_kind, session_id, change_kind, observed_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
    )
    .bind(tenant_id)
    .bind(sync_run_id)
    .bind(record_kind)
    .bind(session_id)
    .bind(change_kind)
    .bind(observed_at)
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

async fn load_session_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<Vec<ConversationTurn>> {
    let rows = sqlx::query(
        r#"
        SELECT id, session_id, external_id, turn_index, user_text, title,
               started_at, ended_at, fingerprint, missing, imported_at
        FROM conversation_turns
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY turn_index ASC, id ASC, imported_at ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    rows.iter().map(map_sqlx_conversation_turn).collect()
}

async fn load_question_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> AppResult<Vec<ConversationTurn>> {
    let rows = sqlx::query(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE qt.tenant_id = ?1 AND qt.question_id = ?2
        ORDER BY qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    rows.iter().map(map_sqlx_conversation_turn).collect()
}

async fn load_turn_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
) -> AppResult<Vec<ConversationPart>> {
    let rows = sqlx::query(
        r#"
        SELECT id, turn_id, part_index, role, kind, text, language, command,
               cwd, status, exit_code, metadata_json, content_card_json, translated_text,
               source_execution_id, command_label
        FROM conversation_parts
        WHERE tenant_id = ?1 AND turn_id = ?2
        ORDER BY part_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    rows.iter().map(map_sqlx_conversation_part).collect()
}

async fn max_question_turn_order_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> AppResult<i64> {
    let max_order: Option<i64> = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(turn_order) FROM conversation_question_turns WHERE tenant_id = ?1 AND question_id = ?2",
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(max_order.unwrap_or(-1))
}

async fn load_conversation_question_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> AppResult<Option<ConversationQuestion>> {
    sqlx::query(
        r#"
        SELECT id, session_id, title, created_at, updated_at
        FROM conversation_questions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::external)?
    .as_ref()
    .map(map_sqlx_conversation_question)
    .transpose()
}

async fn question_ids_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT q.id
        FROM conversation_questions q
        WHERE q.tenant_id = ?1 AND q.session_id = ?2
        ORDER BY COALESCE((SELECT MIN(t.turn_index)
                          FROM conversation_question_turns qt
                          JOIN conversation_turns t
                            ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
                          WHERE qt.tenant_id = q.tenant_id AND qt.question_id = q.id),
                         9223372036854775807),
                 q.created_at ASC,
                 q.id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)
}

async fn load_question_turn_ids_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> AppResult<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT turn_id
        FROM conversation_question_turns
        WHERE tenant_id = ?1 AND question_id = ?2
        ORDER BY turn_order ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)
}

async fn renumber_question_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
    updated_at: &str,
) -> AppResult<()> {
    let turn_ids = load_question_turn_ids_sqlx_tx(tx, tenant_id, question_id).await?;
    for (index, turn_id) in turn_ids.iter().enumerate() {
        sqlx::query(
            r#"
            UPDATE conversation_question_turns
            SET turn_order = ?1, updated_at = ?2
            WHERE tenant_id = ?3 AND question_id = ?4 AND turn_id = ?5
            "#,
        )
        .bind(index as i64)
        .bind(updated_at)
        .bind(tenant_id)
        .bind(question_id)
        .bind(turn_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::external)?;
    }
    Ok(())
}

async fn ensure_question_ids_are_adjacent_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    question_ids: &[String],
) -> AppResult<()> {
    let ordered = question_ids_for_session_sqlx_tx(tx, tenant_id, session_id).await?;
    let selected = question_ids.iter().collect::<BTreeSet<_>>();
    let positions = ordered
        .iter()
        .enumerate()
        .filter_map(|(index, id)| selected.contains(id).then_some(index))
        .collect::<Vec<_>>();
    if positions.len() != question_ids.len() {
        return Err(AppError::Validation(
            "all questions must exist in the session".to_string(),
        ));
    }
    if positions
        .windows(2)
        .any(|window| window[1] != window[0] + 1)
    {
        return Err(AppError::Validation(
            "questions must be adjacent".to_string(),
        ));
    }
    if positions
        .iter()
        .map(|index| &ordered[*index])
        .zip(question_ids.iter())
        .any(|(actual, requested)| actual != requested)
    {
        return Err(AppError::Validation(
            "question ids must be supplied in session order".to_string(),
        ));
    }
    Ok(())
}

async fn renumber_questions_for_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<()> {
    let _ = (tx, tenant_id, session_id);
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct ConversationRecordTables {
    sessions: &'static str,
    session_project_path_expr: &'static str,
    turns: &'static str,
    parts: &'static str,
    questions: &'static str,
    question_turns: &'static str,
    session_id_prefix: &'static str,
    question_id_prefix: &'static str,
    turn_id_prefix: &'static str,
    part_id_prefix: &'static str,
}

impl ConversationRecordKind {
    fn tables(self) -> ConversationRecordTables {
        match self {
            ConversationRecordKind::Session => ConversationRecordTables {
                sessions: "conversation_sessions",
                session_project_path_expr: "s.project_path",
                turns: "conversation_turns",
                parts: "conversation_parts",
                questions: "conversation_questions",
                question_turns: "conversation_question_turns",
                session_id_prefix: "conversation-session-",
                question_id_prefix: "conversation-question-",
                turn_id_prefix: "conversation-turn-",
                part_id_prefix: "conversation-part-",
            },
            ConversationRecordKind::Web => ConversationRecordTables {
                sessions: "web_record_sessions",
                session_project_path_expr: "NULL",
                turns: "web_record_turns",
                parts: "web_record_parts",
                questions: "web_record_questions",
                question_turns: "web_record_question_turns",
                session_id_prefix: "web-record-session-",
                question_id_prefix: "web-record-question-",
                turn_id_prefix: "web-record-turn-",
                part_id_prefix: "web-record-part-",
            },
        }
    }
}

async fn load_search_session_ids_by_id_fragment_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    fragment: &str,
) -> AppResult<BTreeSet<String>> {
    let (session_lower, session_upper) =
        conversation_id_fragment_range(tables.session_id_prefix, fragment);
    let (question_lower, question_upper) =
        conversation_id_fragment_range(tables.question_id_prefix, fragment);
    let (turn_lower, turn_upper) = conversation_id_fragment_range(tables.turn_id_prefix, fragment);
    let (part_lower, part_upper) = conversation_id_fragment_range(tables.part_id_prefix, fragment);
    let query = format!(
        r#"
        SELECT session_id FROM (
            SELECT s.id AS session_id
            FROM {sessions} s
            WHERE s.tenant_id = ?1 AND s.missing = 0 AND s.id >= ?2 AND s.id < ?3
            UNION
            SELECT q.session_id
            FROM {questions} q
            JOIN {sessions} s ON s.tenant_id = q.tenant_id AND s.id = q.session_id
            WHERE q.tenant_id = ?1 AND s.missing = 0 AND q.id >= ?4 AND q.id < ?5
            UNION
            SELECT t.session_id
            FROM {turns} t
            JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
            WHERE t.tenant_id = ?1 AND s.missing = 0 AND t.id >= ?6 AND t.id < ?7
            UNION
            SELECT t.session_id
            FROM {parts} p
            JOIN {turns} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
            JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
            WHERE p.tenant_id = ?1 AND s.missing = 0 AND p.id >= ?8 AND p.id < ?9
        )
        "#,
        sessions = tables.sessions,
        questions = tables.questions,
        turns = tables.turns,
        parts = tables.parts,
    );
    let rows = sqlx::query_scalar::<_, String>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(session_lower)
        .bind(session_upper)
        .bind(question_lower)
        .bind(question_upper)
        .bind(turn_lower)
        .bind(turn_upper)
        .bind(part_lower)
        .bind(part_upper)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    Ok(rows.into_iter().collect())
}

fn conversation_id_fragment_range(prefix: &str, fragment: &str) -> (String, String) {
    let lower = format!("{prefix}{}", fragment.trim().to_ascii_lowercase());
    // Stable IDs continue with lowercase hex, so `g` is the exclusive prefix-range sentinel.
    let upper = format!("{lower}g");
    (lower, upper)
}

pub(crate) async fn list_conversation_sessions_by_id_fragment_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    query: &str,
    limit: usize,
    offset: usize,
) -> AppResult<Vec<ConversationSessionListItem>> {
    if query.trim().len() != 8 {
        return Err(AppError::Validation(
            "conversation short ID must be exactly 8 hexadecimal characters".to_string(),
        ));
    }
    let search_term =
        crate::backend::models::conversation_id_search_term(query).ok_or_else(|| {
            AppError::external("conversation short ID must be exactly 8 hexadecimal characters")
        })?;
    let fragment = crate::backend::models::conversation_id_fragment(&search_term);
    let tables = record_kind.tables();
    let session_ids =
        load_search_session_ids_by_id_fragment_sqlx(pool, tenant_id, tables, &fragment).await?;
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }
    let session_ids_json = serde_json::to_string(&session_ids).map_err(AppError::external)?;
    let sessions = load_search_sessions_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        Some(&session_ids_json),
    )
    .await?;
    Ok(sessions.into_iter().skip(offset).take(limit).collect())
}

async fn load_search_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    session_ids_json: Option<&str>,
) -> AppResult<Vec<ConversationSessionListItem>> {
    let query = format!(
        r#"
        SELECT s.id, s.source_id, s.adapter_id, s.external_id, s.title, {project_path_expr} AS project_path,
               s.started_at, s.updated_at, s.source_locator, s.source_fingerprint,
               s.missing, s.created_at, s.imported_at,
               (
                   SELECT COUNT(*)
                   FROM {questions} q
                   WHERE q.tenant_id = s.tenant_id AND q.session_id = s.id
               ) AS question_count,
               (
                   SELECT COUNT(*)
                   FROM {turns} t
                   WHERE t.tenant_id = s.tenant_id AND t.session_id = s.id
               ) AS turn_count
        FROM {sessions} s
        WHERE s.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY COALESCE(s.updated_at, s.imported_at) DESC, s.title ASC
        "#,
        sessions = tables.sessions,
        project_path_expr = tables.session_project_path_expr,
        questions = tables.questions,
        turns = tables.turns,
    );
    let rows = sqlx::query_as::<_, ConversationSessionListItemRow>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.into_iter()
        .map(ConversationSessionListItemRow::into_item)
        .collect()
}

async fn load_search_questions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    session_ids_json: Option<&str>,
) -> AppResult<BTreeMap<String, Vec<ConversationQuestion>>> {
    let query = format!(
        r#"
        SELECT q.id, q.session_id, q.title,
               q.created_at, q.updated_at
        FROM {questions} q
        JOIN {sessions} s ON s.tenant_id = q.tenant_id AND s.id = q.session_id
        WHERE q.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY q.session_id ASC,
                 COALESCE((SELECT MIN(t.turn_index)
                           FROM {question_turns} qt_order
                           JOIN {turns} t ON t.tenant_id = qt_order.tenant_id AND t.id = qt_order.turn_id
                           WHERE qt_order.tenant_id = q.tenant_id AND qt_order.question_id = q.id), 9223372036854775807),
                 q.created_at ASC, q.id ASC
        "#,
        questions = tables.questions,
        sessions = tables.sessions,
        question_turns = tables.question_turns,
        turns = tables.turns,
    );
    let rows = sqlx::query(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    let mut questions_by_session = BTreeMap::<String, Vec<ConversationQuestion>>::new();
    for row in &rows {
        let question = map_sqlx_conversation_question(row)?;
        questions_by_session
            .entry(question.session_id.clone())
            .or_default()
            .push(question);
    }
    Ok(questions_by_session)
}

async fn load_search_turns_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    session_ids_json: Option<&str>,
) -> AppResult<BTreeMap<String, Vec<ConversationTurn>>> {
    let query = format!(
        r#"
        SELECT t.id, t.session_id, t.external_id, t.turn_index, t.user_text, t.title,
               t.started_at, t.ended_at, t.fingerprint, t.missing, t.imported_at,
               qt.question_id
        FROM {turns} t
        JOIN {question_turns} qt ON qt.tenant_id = t.tenant_id AND qt.turn_id = t.id
        JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE t.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY qt.question_id ASC, qt.turn_order ASC, t.turn_index ASC
        "#,
        turns = tables.turns,
        question_turns = tables.question_turns,
        sessions = tables.sessions,
    );
    let rows = sqlx::query_as::<_, ConversationTurnWithQuestionRow>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    let mut turns_by_question = BTreeMap::<String, Vec<ConversationTurn>>::new();
    for row in rows {
        let (question_id, turn) = row.into_turn();
        turns_by_question.entry(question_id).or_default().push(turn);
    }
    Ok(turns_by_question)
}

async fn load_search_parts_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: ConversationRecordTables,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    session_ids_json: Option<&str>,
) -> AppResult<BTreeMap<String, Vec<ConversationPart>>> {
    let query = format!(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label
        FROM {parts} p
        JOIN {turns} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
        JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE p.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY p.turn_id ASC, p.part_index ASC
        "#,
        parts = tables.parts,
        turns = tables.turns,
        sessions = tables.sessions,
    );
    let rows = sqlx::query(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    let mut parts_by_turn = BTreeMap::<String, Vec<ConversationPart>>::new();
    for row in &rows {
        let part = map_sqlx_conversation_part(row)?;
        parts_by_turn
            .entry(part.turn_id.clone())
            .or_default()
            .push(part);
    }
    Ok(parts_by_turn)
}

struct ConversationSearchEntry {
    card_type: ConversationSearchCardType,
    block_id: String,
    text: String,
    semantic_role: Option<String>,
}

pub(super) fn append_projected_cards_to_question_aggregate(
    part: &ConversationPart,
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
    answer_text: &mut Vec<String>,
    code_text: &mut Vec<String>,
    command_text: &mut Vec<String>,
) -> AppResult<()> {
    let cards = crate::backend::projection::conversation_cards::project_conversation_content_cards(
        part, adapter_id, card_kinds,
    )?;
    for card in cards {
        let semantic_role = card
            .semantic_role
            .as_deref()
            .or_else(|| card.kind.rsplit_once('.').map(|(_, value)| value))
            .unwrap_or(card.kind.as_str());
        match semantic_role {
            "answer" => answer_text.push(card.body),
            "code" => code_text.push(card.body),
            "command" => command_text.push(card.body),
            _ => {}
        }
    }
    Ok(())
}

fn content_card_type_value(value: &str) -> Option<ConversationSearchCardType> {
    crate::backend::projection::conversation_cards::is_valid_card_kind(value)
        .then(|| ConversationSearchCardType::new(value))
}

#[derive(Clone, Copy)]
enum SearchTimeBound {
    Since,
    Until,
}

fn parse_search_time_bound(
    value: Option<&str>,
    bound: SearchTimeBound,
) -> AppResult<Option<DateTime<Utc>>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Ok(Some(parsed.with_timezone(&Utc)));
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        let time = match bound {
            SearchTimeBound::Since => NaiveTime::from_hms_opt(0, 0, 0),
            SearchTimeBound::Until => NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999),
        }
        .expect("valid search time bound");
        return Ok(Some(DateTime::from_naive_utc_and_offset(
            date.and_time(time),
            Utc,
        )));
    }
    Err(AppError::Validation(format!(
        "invalid conversation search time {value:?}; use RFC3339 or YYYY-MM-DD"
    )))
}

fn conversation_session_search_time(session: &ConversationSession) -> Option<DateTime<Utc>> {
    session
        .started_at
        .as_deref()
        .and_then(crate::backend::models::parse_conversation_timestamp)
        .or_else(|| {
            session
                .updated_at
                .as_deref()
                .and_then(crate::backend::models::parse_conversation_timestamp)
        })
        .or_else(|| parse_rfc3339_utc(&session.imported_at))
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

#[allow(clippy::too_many_arguments)]
fn push_search_hit_if_matching(
    hits: &mut Vec<ConversationSearchHit>,
    needle: &str,
    allowed_types: &BTreeSet<ConversationSearchCardType>,
    session: &ConversationSessionListItem,
    question: &ConversationQuestion,
    question_index: i64,
    question_title: &str,
    turn_id: Option<String>,
    part_id: Option<String>,
    block_id: String,
    card_type: ConversationSearchCardType,
    text: &str,
    id_fragment: Option<&str>,
    related_ids: &[&str],
) {
    if !allowed_types.is_empty() && !allowed_types.contains(&card_type) {
        return;
    }
    let matched_by_id = id_fragment.is_some_and(|fragment| {
        related_ids
            .iter()
            .any(|value| crate::backend::models::conversation_id_fragment(value) == fragment)
    });
    if !matched_by_id && !text.to_lowercase().contains(needle) {
        return;
    }

    hits.push(ConversationSearchHit {
        session: session.clone(),
        question_id: question.id.clone(),
        question_index,
        question_title: question_title.to_string(),
        turn_id,
        part_id,
        block_id,
        card_type,
        snippet: if matched_by_id {
            leading_search_snippet(text)
        } else {
            search_snippet(text, needle)
        },
        score: if matched_by_id {
            12_000
        } else {
            match_count(text, needle) * 100
        },
        incremental: None,
        highlight_segments: None,
    });
}

fn search_highlight_segments(
    text: &str,
    needle: &str,
) -> Option<Vec<crate::backend::dto::ConversationSearchHighlightSegment>> {
    let start = text.find(needle).or_else(|| {
        (text.is_ascii() && needle.is_ascii())
            .then(|| text.to_ascii_lowercase().find(needle))
            .flatten()
    })?;
    let end = start + needle.len();
    if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return None;
    }
    let mut segments = Vec::new();
    if start > 0 {
        segments.push(crate::backend::dto::ConversationSearchHighlightSegment {
            text: text[..start].to_string(),
            matched: false,
        });
    }
    segments.push(crate::backend::dto::ConversationSearchHighlightSegment {
        text: text[start..end].to_string(),
        matched: true,
    });
    if end < text.len() {
        segments.push(crate::backend::dto::ConversationSearchHighlightSegment {
            text: text[end..].to_string(),
            matched: false,
        });
    }
    Some(segments)
}

fn search_entries_for_part(
    part: &ConversationPart,
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
) -> Vec<ConversationSearchEntry> {
    crate::backend::projection::conversation_cards::project_conversation_content_cards(
        part, adapter_id, card_kinds,
    )
    .unwrap_or_default()
    .into_iter()
    .map(|card| {
        let semantic_role = card
            .semantic_role
            .clone()
            .or_else(|| {
                card_kinds
                    .iter()
                    .find(|definition| definition.id == card.kind)
                    .and_then(|definition| definition.semantic_role.clone())
            })
            .or_else(|| {
                card.kind
                    .rsplit_once('.')
                    .map(|(_, value)| value.to_string())
            });
        ConversationSearchEntry {
            card_type: ConversationSearchCardType::new(card.kind),
            block_id: card.node_id,
            text: card.body,
            semantic_role,
        }
    })
    .collect()
}

#[derive(Debug, FromRow)]
struct AdapterCardKindsRow {
    id: String,
    card_kinds_json: String,
}

async fn load_search_adapter_card_kinds_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<BTreeMap<String, Vec<ConversationCardKindDefinition>>> {
    let rows = sqlx::query_as::<_, AdapterCardKindsRow>(
        "SELECT id, card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    rows.into_iter()
        .map(|row| {
            let definitions = decode_json(row.card_kinds_json)?;
            Ok((row.id, definitions))
        })
        .collect()
}

fn search_snippet(text: &str, needle: &str) -> String {
    let normalized_text = text.to_lowercase();
    let match_start = normalized_text
        .find(needle)
        .map(|index| normalized_text[..index].chars().count())
        .unwrap_or(0);
    let chars = text.chars().collect::<Vec<_>>();
    let start = match_start.saturating_sub(64);
    let end = (match_start + needle.chars().count() + 96).min(chars.len());
    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < chars.len() { "..." } else { "" };
    compact_whitespace(&format!(
        "{prefix}{}{suffix}",
        chars[start..end].iter().collect::<String>()
    ))
}

fn leading_search_snippet(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let end = 104.min(chars.len());
    let suffix = if end < chars.len() { "..." } else { "" };
    compact_whitespace(&format!(
        "{}{suffix}",
        chars[..end].iter().collect::<String>()
    ))
}

fn match_count(text: &str, needle: &str) -> usize {
    text.to_lowercase().matches(needle).count().max(1)
}

fn compact_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

fn normalize_project_path(project_path: Option<&str>) -> Option<String> {
    project_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("Untitled question");
    let trimmed = line.trim();
    if trimmed.chars().count() > 96 {
        trimmed.chars().take(96).collect()
    } else {
        trimmed.to_string()
    }
}

fn search_question_title_from_turns(
    question: &ConversationQuestion,
    turns: &[ConversationTurn],
) -> String {
    question
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .or_else(|| {
            turns
                .iter()
                .map(|turn| turn.user_text.as_str())
                .find(|text| !text.trim().is_empty())
                .map(first_line)
        })
        .unwrap_or_else(|| "Untitled question".to_string())
}

fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    format!("{prefix}-{:x}", hasher.finalize())
}

pub(crate) async fn resolve_conversation_session_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    prefix_or_id: &str,
) -> AppResult<String> {
    if prefix_or_id.len() >= 36 {
        return Ok(prefix_or_id.to_string());
    }
    let clean_prefix = prefix_or_id
        .strip_prefix("conversation-session-")
        .unwrap_or(prefix_or_id);
    let like_pattern_verbatim = format!("{}%", prefix_or_id);
    let like_pattern_domain = format!("conversation-session-{}%", clean_prefix);

    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND (id LIKE ?2 OR id LIKE ?3) LIMIT 11",
    )
    .bind(tenant_id)
    .bind(&like_pattern_verbatim)
    .bind(&like_pattern_domain)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    if rows.is_empty() {
        return Err(AppError::NotFound(format!(
            "no session matches prefix {:?}",
            prefix_or_id
        )));
    }
    if rows.len() > 1 {
        let max_display = std::cmp::min(rows.len(), 5);
        let examples = rows[..max_display].join(", ");
        let indicator = if rows.len() > 5 { "..." } else { "" };
        return Err(AppError::Conflict(format!(
            "ambiguous prefix {:?}: {} sessions match (e.g. {}{})",
            prefix_or_id,
            if rows.len() > 10 {
                "10+".to_string()
            } else {
                rows.len().to_string()
            },
            examples,
            indicator
        )));
    }
    Ok(rows[0].clone())
}

pub(crate) async fn resolve_conversation_question_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    prefix_or_id: &str,
) -> AppResult<String> {
    if prefix_or_id.len() >= 36 {
        return Ok(prefix_or_id.to_string());
    }
    let clean_prefix = prefix_or_id
        .strip_prefix("conversation-question-")
        .unwrap_or(prefix_or_id);
    let like_pattern_verbatim = format!("{}%", prefix_or_id);
    let like_pattern_domain = format!("conversation-question-{}%", clean_prefix);

    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM conversation_questions WHERE tenant_id = ?1 AND (id LIKE ?2 OR id LIKE ?3) LIMIT 11",
    )
    .bind(tenant_id)
    .bind(&like_pattern_verbatim)
    .bind(&like_pattern_domain)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    if rows.is_empty() {
        return Err(AppError::NotFound(format!(
            "no question matches prefix {:?}",
            prefix_or_id
        )));
    }
    if rows.len() > 1 {
        let max_display = std::cmp::min(rows.len(), 5);
        let examples = rows[..max_display].join(", ");
        let indicator = if rows.len() > 5 { "..." } else { "" };
        return Err(AppError::Conflict(format!(
            "ambiguous prefix {:?}: {} questions match (e.g. {}{})",
            prefix_or_id,
            if rows.len() > 10 {
                "10+".to_string()
            } else {
                rows.len().to_string()
            },
            examples,
            indicator
        )));
    }
    Ok(rows[0].clone())
}

pub(crate) async fn resolve_conversation_turn_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    prefix_or_id: &str,
) -> AppResult<String> {
    if prefix_or_id.len() >= 36 {
        return Ok(prefix_or_id.to_string());
    }
    let clean_prefix = prefix_or_id
        .strip_prefix("conversation-turn-")
        .unwrap_or(prefix_or_id);
    let like_pattern_verbatim = format!("{}%", prefix_or_id);
    let like_pattern_domain = format!("conversation-turn-{}%", clean_prefix);

    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM conversation_turns WHERE tenant_id = ?1 AND (id LIKE ?2 OR id LIKE ?3) LIMIT 11",
    )
    .bind(tenant_id)
    .bind(&like_pattern_verbatim)
    .bind(&like_pattern_domain)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    if rows.is_empty() {
        return Err(AppError::NotFound(format!(
            "no turn matches prefix {:?}",
            prefix_or_id
        )));
    }
    if rows.len() > 1 {
        let max_display = std::cmp::min(rows.len(), 5);
        let examples = rows[..max_display].join(", ");
        let indicator = if rows.len() > 5 { "..." } else { "" };
        return Err(AppError::Conflict(format!(
            "ambiguous prefix {:?}: {} turns match (e.g. {}{})",
            prefix_or_id,
            if rows.len() > 10 {
                "10+".to_string()
            } else {
                rows.len().to_string()
            },
            examples,
            indicator
        )));
    }
    Ok(rows[0].clone())
}

pub(crate) async fn resolve_conversation_part_id_prefix_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    prefix_or_id: &str,
) -> AppResult<String> {
    if prefix_or_id.len() >= 36 {
        return Ok(prefix_or_id.to_string());
    }
    let clean_prefix = prefix_or_id
        .strip_prefix("conversation-part-")
        .unwrap_or(prefix_or_id);
    let like_pattern_verbatim = format!("{}%", prefix_or_id);
    let like_pattern_domain = format!("conversation-part-{}%", clean_prefix);

    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM conversation_parts WHERE tenant_id = ?1 AND (id LIKE ?2 OR id LIKE ?3) LIMIT 11",
    )
    .bind(tenant_id)
    .bind(&like_pattern_verbatim)
    .bind(&like_pattern_domain)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    if rows.is_empty() {
        return Err(AppError::NotFound(format!(
            "no part matches prefix {:?}",
            prefix_or_id
        )));
    }
    if rows.len() > 1 {
        let max_display = std::cmp::min(rows.len(), 5);
        let examples = rows[..max_display].join(", ");
        let indicator = if rows.len() > 5 { "..." } else { "" };
        return Err(AppError::Conflict(format!(
            "ambiguous prefix {:?}: {} parts match (e.g. {}{})",
            prefix_or_id,
            if rows.len() > 10 {
                "10+".to_string()
            } else {
                rows.len().to_string()
            },
            examples,
            indicator
        )));
    }
    Ok(rows[0].clone())
}

pub(crate) async fn load_conversation_session_versions_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
    record_kind: crate::backend::dto::ConversationRecordKind,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
) -> AppResult<std::collections::BTreeMap<String, String>> {
    let kind_str = match record_kind {
        crate::backend::dto::ConversationRecordKind::Session => "session",
        crate::backend::dto::ConversationRecordKind::Web => "web",
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
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(kind_str)
    .bind(adapter_content_hash)
    .bind(card_contract_version.map(i64::from))
    .bind(i64::from(payload_policy_version))
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

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
) -> AppResult<bool> {
    let applied_version = sqlx::query_scalar::<_, i64>(
        "SELECT applied_version FROM conversation_payload_policy_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;
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
    .map_err(AppError::external)?;
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
    .map_err(AppError::external)?;
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
) -> AppResult<()> {
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
    .map_err(AppError::external)?;
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
) -> AppResult<()> {
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
    .map_err(AppError::external)?;

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
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_session_observations (
            tenant_id, source_id, record_kind, external_id, observed_version,
            hydrated_version, last_seen_at, source_presence, dirty,
            hydrated_adapter_hash, hydrated_card_contract_version,
            hydrated_payload_policy_version,
            error_code, error_message, error_stage, retryable,
            attempt_count, last_attempt_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, 'present', 0, ?7, ?8, ?9, NULL, NULL, NULL, NULL, 1, ?6)
        ON CONFLICT(tenant_id, source_id, record_kind, external_id) DO UPDATE SET
            observed_version = excluded.observed_version,
            hydrated_version = excluded.hydrated_version,
            last_seen_at = excluded.last_seen_at,
            source_presence = 'present',
            dirty = 0,
            hydrated_adapter_hash = excluded.hydrated_adapter_hash,
            hydrated_card_contract_version = excluded.hydrated_card_contract_version,
            hydrated_payload_policy_version = excluded.hydrated_payload_policy_version,
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
    .execute(&mut **tx)
    .await
    .map_err(AppError::external)?;

    Ok(())
}

pub(crate) async fn persist_conversation_session_observations_sqlx(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    source_id: &str,
    record_kind: crate::backend::dto::ConversationRecordKind,
    session_descriptors: &[crate::backend::conversations::ConversationSessionDescriptor],
    hydrated_external_ids: &std::collections::BTreeSet<String>,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
) -> AppResult<usize> {
    let kind_str = match record_kind {
        crate::backend::dto::ConversationRecordKind::Session => "session",
        crate::backend::dto::ConversationRecordKind::Web => "web",
    };

    let mut tx = pool.begin().await.map_err(AppError::external)?;

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
                    hydrated_payload_policy_version,
                    error_code, error_message, error_stage, retryable,
                    attempt_count, last_attempt_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10, ?11, NULL, NULL, NULL, NULL, 1, ?7)
                ON CONFLICT(tenant_id, source_id, record_kind, external_id) DO UPDATE SET
                    observed_version = excluded.observed_version,
                    hydrated_version = excluded.hydrated_version,
                    last_seen_at = excluded.last_seen_at,
                    source_presence = excluded.source_presence,
                    hydrated_adapter_hash = excluded.hydrated_adapter_hash,
                    hydrated_card_contract_version = excluded.hydrated_card_contract_version,
                    hydrated_payload_policy_version = excluded.hydrated_payload_policy_version,
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
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
        } else {
            // Note: Keep dirty and error info intact if already dirty/failed!
            sqlx::query(
                r#"
                INSERT INTO conversation_session_observations (
                    tenant_id, source_id, record_kind, external_id, observed_version,
                    last_seen_at, source_presence, dirty, hydrated_adapter_hash,
                    hydrated_card_contract_version,
                    hydrated_payload_policy_version
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10)
                ON CONFLICT(tenant_id, source_id, record_kind, external_id) DO UPDATE SET
                    observed_version = excluded.observed_version,
                    last_seen_at = excluded.last_seen_at,
                    source_presence = excluded.source_presence,
                    hydrated_adapter_hash = excluded.hydrated_adapter_hash,
                    hydrated_card_contract_version = excluded.hydrated_card_contract_version,
                    hydrated_payload_policy_version = excluded.hydrated_payload_policy_version
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
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
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
        .map_err(AppError::external)?;
    }
    tx.commit().await.map_err(AppError::external)?;
    Ok(session_descriptors.len())
}

#[cfg(test)]
#[path = "conversation_repo_tests.rs"]
mod tests;
