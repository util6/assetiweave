use crate::backend::dto::{
    ConversationBlockDetail, ConversationBlockLocator, ConversationCardRenderer,
    ConversationContentNode, ConversationMutationResult, ConversationQuestionDetail,
    ConversationRecordKind, ConversationSearchCardType, ConversationSearchHit,
    ConversationSearchPage, ConversationSessionDetail, ConversationSessionListItem,
    LegacyConversationContentNode,
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
use crate::backend::projection::conversation_content_nodes::{
    project_conversation_content_nodes, ConversationContentNodeCandidate,
};
use crate::backend::runtime::{AppError, AppResult};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::SqliteRow, AssertSqlSafe, Executor, Row as SqlxRow, Sqlite, SqlitePool, Transaction,
};
use std::collections::{BTreeMap, BTreeSet};

use super::codec::{decode_enum, decode_json, encode_enum, encode_json};

pub(super) const CONVERSATION_IMPORT_BATCH_SIZE: usize = 8;

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
    FROM conversation_adapter_packages
    WHERE tenant_id = ?1
    ORDER BY name ASC, package_id ASC
    "#;

const LOAD_CONVERSATION_ADAPTER_PACKAGE_SQL: &str = r#"
    SELECT package_id, adapter_id, name, version, record_kind, install_dir,
           manifest_path, adapter_manifest_path, runtime_protocol, runtime_ready,
           origin, source_url, git_ref, git_commit, catalog_url, update_policy,
           latest_version, last_checked_at, runtime_gate_status, runtime_validated_at,
           installed_content_hash, trusted_package_hash, error_message,
           created_at, updated_at
    FROM conversation_adapter_packages
    WHERE tenant_id = ?1 AND package_id = ?2
    "#;

const LOAD_CONVERSATION_ADAPTER_PACKAGE_BY_ADAPTER_SQL: &str = r#"
    SELECT package_id, adapter_id, name, version, record_kind, install_dir,
           manifest_path, adapter_manifest_path, runtime_protocol, runtime_ready,
           origin, source_url, git_ref, git_commit, catalog_url, update_policy,
           latest_version, last_checked_at, runtime_gate_status, runtime_validated_at,
           installed_content_hash, trusted_package_hash, error_message,
           created_at, updated_at
    FROM conversation_adapter_packages
    WHERE tenant_id = ?1 AND adapter_id = ?2
    ORDER BY updated_at DESC, package_id ASC
    LIMIT 1
    "#;

const UPSERT_CONVERSATION_ADAPTER_PACKAGE_SQL: &str = r#"
    INSERT INTO conversation_adapter_packages (
        tenant_id, package_id, adapter_id, name, version, record_kind, install_dir,
        manifest_path, adapter_manifest_path, runtime_protocol, runtime_ready,
        origin, source_url, git_ref, git_commit, catalog_url, update_policy,
        latest_version, last_checked_at, runtime_gate_status, runtime_validated_at,
        installed_content_hash, trusted_package_hash, error_message, created_at, updated_at
    )
    VALUES (
        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
        ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26
    )
    ON CONFLICT(tenant_id, package_id) DO UPDATE SET
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
    "DELETE FROM conversation_adapter_packages WHERE tenant_id = ?1 AND package_id = ?2";

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
    pub(crate) turn_count: usize,
    pub(crate) warning_count: usize,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone)]
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
    for package in list_conversation_adapter_packages_sqlx(pool, tenant_id).await? {
        upsert_conversation_adapter_package_sqlx(pool, tenant_id, &package).await?;
    }

    let version_rows = sqlx::query(
        r#"
        SELECT package_id, version, install_dir
        FROM conversation_adapter_package_versions
        WHERE tenant_id = ?1
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    for row in version_rows {
        let package_id: String = row.try_get(0).map_err(AppError::external)?;
        let version: String = row.try_get(1).map_err(AppError::external)?;
        let install_dir: String = row.try_get(2).map_err(AppError::external)?;
        sqlx::query(
            r#"
            UPDATE conversation_adapter_package_versions
            SET install_dir = ?1
            WHERE tenant_id = ?2 AND package_id = ?3 AND version = ?4
            "#,
        )
        .bind(normalize_conversation_path(&install_dir)?)
        .bind(tenant_id)
        .bind(package_id)
        .bind(version)
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

#[cfg(test)]
pub(crate) async fn delete_conversation_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> AppResult<ConversationAdapter> {
    Ok(
        delete_conversation_adapter_registration_sqlx(pool, tenant_id, adapter_id, None)
            .await?
            .ok_or_else(|| {
                AppError::external({ format!("conversation adapter not found: {adapter_id}") })
            })?,
    )
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
        sqlx::query(DELETE_CONVERSATION_ADAPTER_SQL)
            .bind(tenant_id)
            .bind(adapter_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
    }
    sqlx::query(DISABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL)
        .bind(Utc::now().to_rfc3339())
        .bind(tenant_id)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?;
    if let Some(package_id) = package_id {
        sqlx::query(DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL)
            .bind(tenant_id)
            .bind(package_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
    }
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
            AppError::external({ format!("conversation adapter not found: {adapter_id}") })
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
    tenant_id: &str,
) -> AppResult<Vec<ConversationAdapterPackage>> {
    let rows = sqlx::query(LIST_CONVERSATION_ADAPTER_PACKAGES_SQL)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.iter()
        .map(map_sqlx_conversation_adapter_package)
        .collect()
}

pub(crate) async fn load_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    package_id: &str,
) -> AppResult<Option<ConversationAdapterPackage>> {
    sqlx::query(LOAD_CONVERSATION_ADAPTER_PACKAGE_SQL)
        .bind(tenant_id)
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
    tenant_id: &str,
    adapter_id: &str,
) -> AppResult<Option<ConversationAdapterPackage>> {
    sqlx::query(LOAD_CONVERSATION_ADAPTER_PACKAGE_BY_ADAPTER_SQL)
        .bind(tenant_id)
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
    tenant_id: &str,
    package: &ConversationAdapterPackage,
) -> AppResult<()> {
    upsert_conversation_adapter_package_with_executor(pool, tenant_id, package).await
}

async fn upsert_conversation_adapter_package_with_executor<'e, E>(
    executor: E,
    tenant_id: &str,
    package: &ConversationAdapterPackage,
) -> AppResult<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    let install_dir = normalize_conversation_path(&package.install_dir)?;
    let manifest_path = normalize_conversation_path(&package.manifest_path)?;
    let adapter_manifest_path = normalize_conversation_path(&package.adapter_manifest_path)?;
    sqlx::query(UPSERT_CONVERSATION_ADAPTER_PACKAGE_SQL)
        .bind(tenant_id)
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
    tenant_id: &str,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
    version: &ConversationAdapterPackageVersion,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    let existing = sqlx::query(
        r#"
        SELECT artifact_hash, content_hash
        FROM conversation_adapter_package_versions
        WHERE tenant_id = ?1 AND package_id = ?2 AND version = ?3
        "#,
    )
    .bind(tenant_id)
    .bind(&version.package_id)
    .bind(&version.version)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::external)?;
    if let Some(existing) = existing {
        let artifact_hash: Option<String> = existing.try_get(0).map_err(AppError::external)?;
        let content_hash: String = existing.try_get(1).map_err(AppError::external)?;
        if artifact_hash != version.artifact_hash || content_hash != version.content_hash {
            return Err(AppError::Conflict(format!(
                "conversation adapter package version is immutable: {}@{}",
                version.package_id, version.version
            )));
        }
    }

    upsert_conversation_adapter_with_executor(&mut *tx, tenant_id, adapter).await?;
    upsert_conversation_adapter_package_with_executor(&mut *tx, tenant_id, package).await?;
    sqlx::query(
        r#"
        INSERT INTO conversation_adapter_package_versions (
            tenant_id, package_id, version, install_dir, artifact_hash,
            content_hash, runtime_gate_status, installed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(tenant_id, package_id, version) DO UPDATE SET
            install_dir = excluded.install_dir,
            runtime_gate_status = excluded.runtime_gate_status
        "#,
    )
    .bind(tenant_id)
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
    tenant_id: &str,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    upsert_conversation_adapter_with_executor(&mut *tx, tenant_id, adapter).await?;
    upsert_conversation_adapter_package_with_executor(&mut *tx, tenant_id, package).await?;
    sqlx::query(
        "DELETE FROM conversation_adapter_package_versions WHERE tenant_id = ?1 AND package_id = ?2",
    )
    .bind(tenant_id)
    .bind(&package.package_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
    tx.commit().await.map_err(AppError::external)
}

pub(crate) async fn deactivate_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    package_id: &str,
    adapter_id: &str,
) -> AppResult<ConversationAdapterPackage> {
    let mut package = load_conversation_adapter_package_sqlx(pool, tenant_id, package_id)
        .await?
        .ok_or_else(|| {
            AppError::external({ format!("conversation adapter package not found: {package_id}") })
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
    sqlx::query(DELETE_CONVERSATION_ADAPTER_SQL)
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
        r#"
        UPDATE conversation_adapter_packages
        SET runtime_ready = 0,
            runtime_gate_status = 'runtime_missing',
            runtime_validated_at = ?1,
            error_message = 'conversation adapter package is uninstalled',
            updated_at = ?1
        WHERE tenant_id = ?2 AND package_id = ?3
        "#,
    )
    .bind(&now)
    .bind(tenant_id)
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
    tenant_id: &str,
    release: &ConversationAdapterCatalogRelease,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_adapter_catalog_releases (
            tenant_id, catalog_url, package_id, version, channel, released_at,
            core_compatibility, artifact_url, artifact_size, artifact_sha256,
            changelog_markdown, breaking_change, runtime_protocol,
            adapter_manifest_json, etag, fetched_at, adapter_id, name, publisher,
            record_kind, package_manifest_file, adapter_manifest_file, source_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
            ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
        )
        ON CONFLICT(tenant_id, catalog_url, package_id, version) DO UPDATE SET
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
    .bind(tenant_id)
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
    tenant_id: &str,
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
        FROM conversation_adapter_catalog_releases
        WHERE tenant_id = ?1 AND catalog_url = ?2
          AND (?3 IS NULL OR package_id = ?3)
        ORDER BY package_id ASC, released_at DESC, version DESC
        "#,
    )
    .bind(tenant_id)
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
    tenant_id: &str,
    package_id: &str,
) -> AppResult<Vec<ConversationAdapterPackageVersion>> {
    let rows = sqlx::query(
        r#"
        SELECT package_id, version, install_dir, artifact_hash, content_hash,
               runtime_gate_status, installed_at
        FROM conversation_adapter_package_versions
        WHERE tenant_id = ?1 AND package_id = ?2
        ORDER BY installed_at DESC, version DESC
        "#,
    )
    .bind(tenant_id)
    .bind(package_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    rows.iter()
        .map(|row| {
            Ok(ConversationAdapterPackageVersion {
                package_id: row.try_get(0).map_err(AppError::external)?,
                version: row.try_get(1).map_err(AppError::external)?,
                install_dir: normalize_conversation_path(
                    &row.try_get::<String, _>(2).map_err(AppError::external)?,
                )?,
                artifact_hash: row.try_get(3).map_err(AppError::external)?,
                content_hash: row.try_get(4).map_err(AppError::external)?,
                runtime_gate_status: decode_enum(
                    row.try_get::<String, _>(5).map_err(AppError::external)?,
                )?,
                installed_at: row.try_get(6).map_err(AppError::external)?,
            })
        })
        .collect()
}

pub(crate) async fn delete_conversation_adapter_package_version_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
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
        "DELETE FROM conversation_adapter_package_versions WHERE tenant_id = ?1 AND package_id = ?2 AND version = ?3",
    )
    .bind(tenant_id)
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
        upsert_conversation_adapter_package_with_executor(&mut *tx, tenant_id, package).await?;
    } else if delete_package {
        sqlx::query(DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL)
            .bind(tenant_id)
            .bind(package_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::external)?;
    }
    tx.commit().await.map_err(AppError::external)?;
    Ok(true)
}

fn map_sqlx_conversation_adapter_catalog_release(
    row: &SqliteRow,
) -> AppResult<ConversationAdapterCatalogRelease> {
    Ok(ConversationAdapterCatalogRelease {
        catalog_url: row.try_get(0).map_err(AppError::external)?,
        package_id: row.try_get(1).map_err(AppError::external)?,
        adapter_id: row.try_get(2).map_err(AppError::external)?,
        name: row.try_get(3).map_err(AppError::external)?,
        publisher: row.try_get(4).map_err(AppError::external)?,
        version: row.try_get(5).map_err(AppError::external)?,
        channel: decode_enum(row.try_get::<String, _>(6).map_err(AppError::external)?)?,
        released_at: row.try_get(7).map_err(AppError::external)?,
        core_compatibility: row.try_get(8).map_err(AppError::external)?,
        artifact_url: row.try_get(9).map_err(AppError::external)?,
        artifact_size: row.try_get(10).map_err(AppError::external)?,
        artifact_sha256: row.try_get(11).map_err(AppError::external)?,
        changelog_markdown: row.try_get(12).map_err(AppError::external)?,
        breaking_change: row.try_get::<i64, _>(13).map_err(AppError::external)? == 1,
        runtime_protocol: row.try_get(14).map_err(AppError::external)?,
        record_kind: decode_enum(row.try_get::<String, _>(15).map_err(AppError::external)?)?,
        package_manifest_file: row.try_get(16).map_err(AppError::external)?,
        adapter_manifest_file: row.try_get(17).map_err(AppError::external)?,
        adapter_manifest_json: row.try_get(18).map_err(AppError::external)?,
        source_json: row.try_get(19).map_err(AppError::external)?,
        etag: row.try_get(20).map_err(AppError::external)?,
        fetched_at: row.try_get(21).map_err(AppError::external)?,
    })
}

#[cfg(test)]
pub(crate) async fn delete_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    package_id: &str,
) -> AppResult<Option<ConversationAdapterPackage>> {
    let package = load_conversation_adapter_package_sqlx(pool, tenant_id, package_id).await?;
    sqlx::query(DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL)
        .bind(tenant_id)
        .bind(package_id)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
    Ok(package)
}

pub(crate) async fn has_running_conversation_sync_for_adapter_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    adapter_id: &str,
) -> AppResult<bool> {
    sqlx::query_scalar::<_, i64>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM conversation_sync_runs
            WHERE tenant_id = ?1 AND adapter_id = ?2 AND status = 'running'
        )
        "#,
    )
    .bind(tenant_id)
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
        .ok_or_else(|| {
            AppError::external({ format!("conversation source not found: {source_id}") })
        })?;
    source.enabled = false;
    source.updated_at = Utc::now().to_rfc3339();
    upsert_conversation_source_sqlx(pool, tenant_id, &source).await?;
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

pub(crate) async fn import_incremental_conversation_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    sessions: &[NormalizedConversationSession],
    discovered_external_ids: &BTreeSet<String>,
    dry_run: bool,
) -> AppResult<ConversationImportResult> {
    import_conversation_sessions_with_presence_sqlx(
        pool,
        tenant_id,
        source,
        sessions,
        Some(discovered_external_ids),
        dry_run,
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
    let turn_count = sessions.iter().map(|session| session.turns.len()).sum();
    if dry_run {
        return Ok(ConversationImportResult {
            source_id: source.id.clone(),
            adapter_id: source.adapter_id.clone(),
            dry_run: true,
            sync_run_id: None,
            session_count: sessions.len(),
            skipped_session_count: 0,
            changed_session_count: 0,
            turn_count,
            warning_count: 0,
            warnings: Vec::new(),
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

    for batch in sessions.chunks(CONVERSATION_IMPORT_BATCH_SIZE) {
        let mut tx = pool.begin().await.map_err(AppError::external)?;
        let mut batch_changed_session_ids = Vec::new();
        for normalized in batch {
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
                continue;
            }
            upsert_conversation_session_sqlx_tx(&mut tx, tenant_id, &session).await?;
            for turn in &normalized.turns {
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
        }
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
        tx.commit().await.map_err(AppError::external)?;
    }

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
    tx.commit().await.map_err(AppError::external)?;

    Ok(ConversationImportResult {
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        dry_run: false,
        sync_run_id: Some(sync_run_id),
        session_count: sessions.len(),
        skipped_session_count,
        changed_session_count,
        turn_count,
        warning_count,
        warnings,
    })
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
    let rows = sqlx::query(
        r#"
        SELECT s.id, s.source_id, s.adapter_id, s.external_id, s.title, s.project_path,
               s.started_at, s.updated_at, s.source_locator, s.source_fingerprint,
               s.missing, s.created_at, s.imported_at,
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
                  FROM conversation_questions q
                  WHERE q.tenant_id = s.tenant_id
                    AND q.session_id = s.id
                    AND (
                        instr(lower(q.question_text), ?4) > 0
                        OR instr(lower(q.answer_text), ?4) > 0
                        OR instr(lower(q.code_text), ?4) > 0
                        OR instr(lower(q.command_text), ?4) > 0
                    )
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
            .map_err(|_| AppError::external({ format!("invalid conversation limit: {limit}") }))?,
    )
    .bind(
        i64::try_from(offset).map_err(|_| {
            AppError::external({ format!("invalid conversation offset: {offset}") })
        })?,
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    rows.iter()
        .map(|row| {
            let question_count =
                usize::try_from(row.try_get::<i64, _>(13).map_err(AppError::external)?)
                    .map_err(|_| AppError::external("invalid conversation question count"))?;
            let turn_count =
                usize::try_from(row.try_get::<i64, _>(14).map_err(AppError::external)?)
                    .map_err(|_| AppError::external("invalid conversation turn count"))?;
            Ok(ConversationSessionListItem {
                session: map_sqlx_conversation_session(row)?,
                question_count,
                turn_count,
            })
        })
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
    .ok_or_else(|| {
        AppError::external({ format!("conversation session not found: {session_id}") })
    })?;
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
                format!(
                    "{}\n{}\n{}\n{}",
                    question.question_text,
                    question.answer_text,
                    question.code_text,
                    question.command_text
                )
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
        SELECT id, session_id, question_index, title, question_text, answer_text,
               code_text, command_text, grouping_origin, created_at, updated_at
        FROM conversation_questions
        WHERE tenant_id = ?1 AND id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?
    .ok_or_else(|| {
        AppError::external({ format!("conversation question not found: {question_id}") })
    })?;
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
    let (cards, legacy_content_nodes) =
        project_conversation_legacy_cards_and_nodes(&parts, &adapter_id, &card_kinds)?;
    let projected_content_nodes = project_question_content_nodes(
        &question.id,
        &question_turns,
        &parts,
        &adapter_id,
        &card_kinds,
    )?;
    Ok(ConversationQuestionDetail {
        question: project_question_compatibility_fields(question, &question_turns, &turns, &parts),
        question_turns,
        turns,
        parts,
        cards,
        legacy_content_nodes,
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
    let question_row = sqlx::query(AssertSqlSafe(format!(
        "SELECT session_id FROM {} WHERE tenant_id = ?1 AND id = ?2",
        tables.questions
    )))
    .bind(tenant_id)
    .bind(question_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?
    .ok_or_else(|| {
        AppError::external({ format!("conversation question not found: {question_id}") })
    })?;
    let session_id: String = question_row.try_get(0).map_err(AppError::external)?;

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
    let cards = project_conversation_cards(&parts, &adapter_id, &card_kinds)?;
    let cards_by_part = cards
        .into_iter()
        .map(|card| (card.part_id.clone(), card))
        .collect::<BTreeMap<_, _>>();

    let mut locators = Vec::with_capacity(turns.len() + cards_by_part.len());
    for turn in &turns {
        locators.push(conversation_question_block_locator(
            record_kind,
            &session_id,
            question_id,
            turn,
        ));
    }
    for part in &parts {
        if let Some(card) = cards_by_part.get(&part.id) {
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

pub(crate) async fn load_conversation_block_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    block_id: &str,
) -> AppResult<ConversationBlockDetail> {
    let tables = record_kind.tables();
    if let Some(turn_id) = block_id.strip_suffix("-question") {
        let row = sqlx::query(AssertSqlSafe(format!(
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
            AppError::external({ format!("conversation question block not found: {block_id}") })
        })?;
        let turn = map_sqlx_conversation_turn(&row)?;
        let question_id: String = row.try_get(11).map_err(AppError::external)?;
        let locator =
            conversation_question_block_locator(record_kind, &turn.session_id, &question_id, &turn);
        return Ok(ConversationBlockDetail {
            locator,
            content: turn.user_text,
            translated_content: None,
        });
    }

    let row = sqlx::query(AssertSqlSafe(format!(
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
    .bind(block_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?
    .ok_or_else(|| {
        AppError::external({ format!("conversation content block not found: {block_id}") })
    })?;
    let part = map_sqlx_conversation_part(&row)?;
    let question_id: String = row.try_get(16).map_err(AppError::external)?;
    let session_id: String = row.try_get(17).map_err(AppError::external)?;
    let (adapter_id, card_kinds) = load_conversation_card_projection_context_for_record_sqlx(
        pool,
        tenant_id,
        record_kind,
        &session_id,
    )
    .await?;
    let card = project_conversation_cards(&[part.clone()], &adapter_id, &card_kinds)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            AppError::external({
                format!("conversation block is not a readable content card: {block_id}")
            })
        })?;
    let locator =
        conversation_card_block_locator(record_kind, &session_id, &question_id, &part, &card);
    Ok(ConversationBlockDetail {
        locator,
        content: card.body,
        translated_content: card.translated_body,
    })
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
        questions.push(
            load_conversation_question_sqlx_tx(&mut tx, tenant_id, question_id)
                .await?
                .ok_or_else(|| {
                    AppError::external({
                        format!("conversation question not found: {question_id}")
                    })
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
    reject_invalid_conversation_question_turns_sqlx_tx(&mut tx, tenant_id).await?;
    ensure_question_ids_are_adjacent_sqlx_tx(&mut tx, tenant_id, &session_id, question_ids).await?;

    if dry_run {
        tx.rollback().await.map_err(AppError::external)?;
        let mut details = Vec::with_capacity(question_ids.len());
        for question_id in question_ids {
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
    let survivor_id = question_ids[0].clone();
    for question_id in &question_ids[1..] {
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
    sqlx::query(
        "UPDATE conversation_questions SET grouping_origin = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4",
    )
    .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
    .bind(&now)
    .bind(tenant_id)
    .bind(&survivor_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
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
            AppError::external({ format!("conversation question not found: {question_id}") })
        })?;
    reject_invalid_conversation_question_turns_sqlx_tx(&mut tx, tenant_id).await?;
    let turns = load_question_turns_sqlx_tx(&mut tx, tenant_id, question_id).await?;
    let split_index = turns
        .iter()
        .position(|turn| turn.id == before_turn_id)
        .ok_or_else(|| {
            AppError::external({ format!("turn is not in question: {before_turn_id}") })
        })?;
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
    let new_question_id = stable_id(
        "conversation-question",
        &[question_id, before_turn_id, &now],
    );
    sqlx::query(
        r#"
        INSERT INTO conversation_questions (
            tenant_id, id, session_id, question_index, title, question_text, answer_text,
            code_text, command_text, grouping_origin, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, NULL, '', '', '', '', ?5, ?6, ?6)
        "#,
    )
    .bind(tenant_id)
    .bind(&new_question_id)
    .bind(&question.session_id)
    .bind(question.question_index + 1)
    .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
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
    sqlx::query(
        "UPDATE conversation_questions SET grouping_origin = ?1, updated_at = ?2 WHERE tenant_id = ?3 AND id = ?4",
    )
    .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
    .bind(&now)
    .bind(tenant_id)
    .bind(question_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::external)?;
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
        SELECT id, session_id, question_index, title, question_text, answer_text,
               code_text, command_text, grouping_origin, created_at, updated_at
        FROM conversation_questions
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY question_index ASC
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
        ORDER BY q.question_index ASC, qt.turn_order ASC, t.turn_index ASC,
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

    let turn_rows = sqlx::query(
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
        ORDER BY q.question_index ASC, qt.turn_order ASC, t.turn_index ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;
    let mut turns_by_question = BTreeMap::<String, Vec<ConversationTurn>>::new();
    for row in &turn_rows {
        let question_id = row.try_get::<String, _>(11).map_err(AppError::external)?;
        turns_by_question
            .entry(question_id)
            .or_default()
            .push(map_sqlx_conversation_turn(row)?);
    }

    let part_rows = sqlx::query(
        r#"
        SELECT p.id, p.turn_id, p.part_index, p.role, p.kind, p.text, p.language,
               p.command, p.cwd, p.status, p.exit_code, p.metadata_json,
               p.content_card_json, p.translated_text, p.source_execution_id, p.command_label
        FROM conversation_parts p
        JOIN conversation_turns t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
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
        let (cards, legacy_content_nodes) =
            project_conversation_legacy_cards_and_nodes(&parts, &adapter_id, &card_kinds)?;
        let projected_content_nodes = project_question_content_nodes(
            &question.id,
            &question_turns,
            &parts,
            &adapter_id,
            &card_kinds,
        )?;
        details.push(ConversationQuestionDetail {
            question: project_question_compatibility_fields(
                question,
                &question_turns,
                &turns,
                &parts,
            ),
            question_turns,
            turns,
            parts,
            cards,
            legacy_content_nodes,
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

pub(crate) fn project_question_compatibility_fields(
    mut question: ConversationQuestion,
    question_turns: &[ConversationQuestionTurn],
    turns: &[ConversationTurn],
    parts: &[ConversationPart],
) -> ConversationQuestion {
    let turns_by_id = turns
        .iter()
        .map(|turn| (turn.id.as_str(), turn))
        .collect::<BTreeMap<_, _>>();
    let question_text = question_turns
        .iter()
        .filter_map(|membership| turns_by_id.get(membership.turn_id.as_str()))
        .map(|turn| turn.user_text.as_str())
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut answer_text = Vec::new();
    let mut code_text = Vec::new();
    let mut command_text = Vec::new();
    for part in parts {
        append_declared_card_to_question_aggregate(
            part,
            &mut answer_text,
            &mut code_text,
            &mut command_text,
        );
    }

    question.question_text = question_text;
    question.answer_text = answer_text.join("\n\n");
    question.code_text = code_text.join("\n\n");
    question.command_text = command_text.join("\n\n");
    if question
        .title
        .as_deref()
        .is_none_or(|title| title.trim().is_empty())
    {
        question.title = Some(first_line(&question.question_text));
    }
    // `grouping_origin` remains in the compatibility Question shape until the
    // later contract cleanup. Membership rows are the authority for new code.
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
    card: &crate::backend::dto::ConversationCard,
) -> ConversationBlockLocator {
    ConversationBlockLocator {
        record_kind: conversation_record_kind_label(record_kind).to_string(),
        session_id: session_id.to_string(),
        question_id: question_id.to_string(),
        turn_id: part.turn_id.clone(),
        block_id: part.id.clone(),
        part_id: Some(part.id.clone()),
        kind: card.kind.clone(),
        semantic_role: card.semantic_role.clone(),
        renderer: card.renderer,
        role: card.role.clone(),
        content_length: card.body.chars().count(),
        language: card.language.clone(),
        cwd: card.cwd.clone(),
        status: card.status.clone(),
        exit_code: card.exit_code,
    }
}

fn conversation_record_kind_label(record_kind: ConversationRecordKind) -> &'static str {
    match record_kind {
        ConversationRecordKind::Session => "session",
        ConversationRecordKind::Web => "web",
    }
}

pub(super) fn project_conversation_cards(
    parts: &[ConversationPart],
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
) -> AppResult<Vec<crate::backend::dto::ConversationCard>> {
    project_conversation_legacy_cards_and_nodes(parts, adapter_id, card_kinds)
        .map(|(cards, _)| cards)
}

pub(super) fn project_conversation_legacy_cards_and_nodes(
    parts: &[ConversationPart],
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
) -> AppResult<(
    Vec<crate::backend::dto::ConversationCard>,
    Vec<LegacyConversationContentNode>,
)> {
    let mut cards = Vec::new();
    let mut content_nodes = Vec::new();
    let mut execution_node_indices = BTreeMap::<(String, String), usize>::new();
    for part in parts {
        if let Some(card) =
            crate::backend::projection::conversation_cards::project_conversation_content_card(
                part, adapter_id, card_kinds,
            )
            .map_err(AppError::external)?
        {
            let card_index = cards.len();
            let execution_role = card
                .semantic_role
                .as_deref()
                .or_else(|| card.kind.rsplit('.').next())
                .filter(|role| matches!(*role, "command" | "result"))
                .map(str::to_string);
            let source_execution_id = part
                .source_execution_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            cards.push(card);

            let (Some(source_execution_id), Some(execution_role)) =
                (source_execution_id, execution_role)
            else {
                content_nodes.push(LegacyConversationContentNode::Card {
                    turn_id: part.turn_id.clone(),
                    card_index,
                });
                continue;
            };

            let key = (part.turn_id.clone(), source_execution_id.to_string());
            if execution_role == "command" {
                let node_index = content_nodes.len();
                content_nodes.push(LegacyConversationContentNode::Execution {
                    turn_id: part.turn_id.clone(),
                    source_execution_id: source_execution_id.to_string(),
                    command_card_index: Some(card_index),
                    result_card_indices: Vec::new(),
                });
                execution_node_indices.insert(key, node_index);
                continue;
            }

            if let Some(node_index) = execution_node_indices.get(&key).copied() {
                if let LegacyConversationContentNode::Execution {
                    result_card_indices,
                    ..
                } = &mut content_nodes[node_index]
                {
                    if execution_role == "result" {
                        result_card_indices.push(card_index);
                        continue;
                    }
                }
            }

            let node_index = content_nodes.len();
            content_nodes.push(LegacyConversationContentNode::Execution {
                turn_id: part.turn_id.clone(),
                source_execution_id: source_execution_id.to_string(),
                command_card_index: (execution_role == "command").then_some(card_index),
                result_card_indices: if execution_role == "result" {
                    vec![card_index]
                } else {
                    Vec::new()
                },
            });
            execution_node_indices.insert(key, node_index);
        }
    }
    let content_nodes = content_nodes
        .into_iter()
        .map(|node| match node {
            LegacyConversationContentNode::Execution {
                turn_id,
                command_card_index: Some(card_index),
                result_card_indices,
                ..
            } if result_card_indices.is_empty() => LegacyConversationContentNode::Card {
                turn_id,
                card_index,
            },
            node => node,
        })
        .collect();
    Ok((cards, content_nodes))
}

pub(super) fn project_question_content_nodes(
    question_id: &str,
    question_turns: &[ConversationQuestionTurn],
    parts: &[ConversationPart],
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
) -> AppResult<Vec<ConversationContentNode>> {
    project_conversation_content_nodes(question_id, question_turns, parts, |part| {
        let card =
            crate::backend::projection::conversation_cards::project_conversation_content_card(
                part, adapter_id, card_kinds,
            )?;
        Ok(card
            .map(ConversationContentNodeCandidate::from)
            .into_iter()
            .collect())
    })
    .map_err(AppError::external)
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
        AppError::external({ "invalid recent conversation sync run limit".to_string() })
    })?;
    let rows = sqlx::query(
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
    .map_err(AppError::external)?;
    rows.iter()
        .map(|row| {
            Ok(ConversationSyncDelta {
                sync_run_id: row.try_get(0).map_err(AppError::external)?,
                session_id: row.try_get(1).map_err(AppError::external)?,
                change_kind: row.try_get(2).map_err(AppError::external)?,
                observed_at: row.try_get(3).map_err(AppError::external)?,
            })
        })
        .collect()
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
    let needle = normalize_query(Some(query)).ok_or_else(|| {
        AppError::external({ "conversation search query is required".to_string() })
    })?;
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

        for question in questions_by_session.remove(&session.id).unwrap_or_default() {
            let question_title = question
                .title
                .clone()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| first_line(&question.question_text));
            for turn in turns_by_question.remove(&question.id).unwrap_or_default() {
                let question_block_id = format!("{}-question", turn.id);
                if include_questions {
                    push_search_hit_if_matching(
                        &mut hits,
                        &needle,
                        &all_types,
                        &session_item,
                        &question,
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
    let questions = load_search_questions_sqlx(
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
    let turns = load_search_turns_sqlx(
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
    let needle = normalize_query(Some(query)).ok_or_else(|| {
        AppError::external({ "conversation search query is required".to_string() })
    })?;
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
            AppError::external({
                "conversation search index hydration missed a session".to_string()
            })
        })?;
        let question = questions.get(&matched.question_id).ok_or_else(|| {
            AppError::external({
                "conversation search index hydration missed a question".to_string()
            })
        })?;
        let question_title = question
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| first_line(&question.question_text));
        let (part_id, text) = if matched.card_type == "question" {
            let turn = turns.get(&matched.turn_id).ok_or_else(|| {
                AppError::external({
                    "conversation search index hydration missed a turn".to_string()
                })
            })?;
            (None, turn.user_text.clone())
        } else {
            let part = parts.get(&matched.part_id).ok_or_else(|| {
                AppError::external({
                    "conversation search index hydration missed a part".to_string()
                })
            })?;
            let card = resolved_content_card_for_part(part).ok_or_else(|| {
                AppError::external({
                    "conversation search index hydration missed a declared card".to_string()
                })
            })?;
            if card.kind != matched.card_type || part.id != matched.document_id {
                return Err(AppError::Validation(
                    "conversation search index hydration found stale card metadata".to_string(),
                ));
            }
            (Some(part.id.clone()), card.body)
        };
        let card_type = content_card_type_value(&matched.card_type)
            .or_else(|| {
                (matched.card_type == "question").then_some(ConversationSearchCardType::question())
            })
            .ok_or_else(|| {
                AppError::external({
                    "conversation search index returned an invalid card type".to_string()
                })
            })?;
        hits.push(ConversationSearchHit {
            session: session.clone(),
            question_id: question.id.clone(),
            question_index: question.question_index,
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

fn map_sqlx_conversation_adapter(row: &SqliteRow) -> AppResult<ConversationAdapter> {
    let protocol_version = row
        .try_get::<Option<i64>, _>(10)
        .map_err(AppError::external)?
        .map(|value| {
            u32::try_from(value)
                .map_err(|_| AppError::external({ format!("invalid protocol_version: {value}") }))
        })
        .transpose()?;
    Ok(ConversationAdapter {
        id: row.try_get(0).map_err(AppError::external)?,
        name: row.try_get(1).map_err(AppError::external)?,
        kind: decode_enum(row.try_get::<String, _>(2).map_err(AppError::external)?)?,
        version: row.try_get(3).map_err(AppError::external)?,
        enabled: row.try_get::<i64, _>(4).map_err(AppError::external)? == 1,
        manifest_path: normalize_optional_conversation_path(
            row.try_get::<Option<String>, _>(5)
                .map_err(AppError::external)?
                .as_deref(),
        )?,
        executable_path: normalize_optional_conversation_path(
            row.try_get::<Option<String>, _>(6)
                .map_err(AppError::external)?
                .as_deref(),
        )?,
        content_hash: row.try_get(7).map_err(AppError::external)?,
        trusted_hash: row.try_get(8).map_err(AppError::external)?,
        trust_state: decode_enum(row.try_get::<String, _>(9).map_err(AppError::external)?)?,
        protocol_version,
        capabilities: decode_json(row.try_get::<String, _>(11).map_err(AppError::external)?)?,
        input_kinds: decode_json(row.try_get::<String, _>(12).map_err(AppError::external)?)?,
        card_contract_version: row
            .try_get::<Option<i64>, _>(13)
            .map_err(AppError::external)?
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    AppError::external({ format!("invalid card_contract_version: {value}") })
                })
            })
            .transpose()?,
        card_kinds: decode_json(row.try_get::<String, _>(14).map_err(AppError::external)?)?,
        created_at: row.try_get(15).map_err(AppError::external)?,
        updated_at: row.try_get(16).map_err(AppError::external)?,
    })
}

fn map_sqlx_conversation_adapter_package(row: &SqliteRow) -> AppResult<ConversationAdapterPackage> {
    let install_dir: String = row.try_get(5).map_err(AppError::external)?;
    let manifest_path: String = row.try_get(6).map_err(AppError::external)?;
    let adapter_manifest_path: String = row.try_get(7).map_err(AppError::external)?;
    Ok(ConversationAdapterPackage {
        package_id: row.try_get(0).map_err(AppError::external)?,
        adapter_id: row.try_get(1).map_err(AppError::external)?,
        name: row.try_get(2).map_err(AppError::external)?,
        version: row.try_get(3).map_err(AppError::external)?,
        record_kind: decode_enum(row.try_get::<String, _>(4).map_err(AppError::external)?)?,
        install_dir: normalize_conversation_path(&install_dir)?,
        manifest_path: normalize_conversation_path(&manifest_path)?,
        adapter_manifest_path: normalize_conversation_path(&adapter_manifest_path)?,
        runtime_protocol: row.try_get(8).map_err(AppError::external)?,
        runtime_ready: row.try_get::<i64, _>(9).map_err(AppError::external)? == 1,
        origin: decode_enum(row.try_get::<String, _>(10).map_err(AppError::external)?)?,
        source_url: row.try_get(11).map_err(AppError::external)?,
        git_ref: row.try_get(12).map_err(AppError::external)?,
        git_commit: row.try_get(13).map_err(AppError::external)?,
        catalog_url: row.try_get(14).map_err(AppError::external)?,
        update_policy: decode_enum(row.try_get::<String, _>(15).map_err(AppError::external)?)?,
        latest_version: row.try_get(16).map_err(AppError::external)?,
        last_checked_at: row.try_get(17).map_err(AppError::external)?,
        runtime_gate_status: decode_enum(
            row.try_get::<String, _>(18).map_err(AppError::external)?,
        )?,
        runtime_validated_at: row.try_get(19).map_err(AppError::external)?,
        installed_content_hash: row.try_get(20).map_err(AppError::external)?,
        trusted_package_hash: row.try_get(21).map_err(AppError::external)?,
        error_message: row.try_get(22).map_err(AppError::external)?,
        created_at: row.try_get(23).map_err(AppError::external)?,
        updated_at: row.try_get(24).map_err(AppError::external)?,
    })
}

fn map_sqlx_conversation_source(row: &SqliteRow) -> AppResult<ConversationSource> {
    let location: String = row.try_get(4).map_err(AppError::external)?;
    Ok(ConversationSource {
        id: row.try_get(0).map_err(AppError::external)?,
        adapter_id: row.try_get(1).map_err(AppError::external)?,
        name: row.try_get(2).map_err(AppError::external)?,
        kind: decode_enum(row.try_get::<String, _>(3).map_err(AppError::external)?)?,
        location: normalize_conversation_source_location(&location)?,
        config_json: row.try_get(5).map_err(AppError::external)?,
        enabled: row.try_get::<i64, _>(6).map_err(AppError::external)? == 1,
        last_synced_at: row.try_get(7).map_err(AppError::external)?,
        last_sync_status: row.try_get(8).map_err(AppError::external)?,
        created_at: row.try_get(9).map_err(AppError::external)?,
        updated_at: row.try_get(10).map_err(AppError::external)?,
    })
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

pub(super) fn map_sqlx_conversation_session(row: &SqliteRow) -> AppResult<ConversationSession> {
    Ok(ConversationSession {
        id: row.try_get(0).map_err(AppError::external)?,
        source_id: row.try_get(1).map_err(AppError::external)?,
        adapter_id: row.try_get(2).map_err(AppError::external)?,
        external_id: row.try_get(3).map_err(AppError::external)?,
        title: row.try_get(4).map_err(AppError::external)?,
        project_path: row.try_get(5).map_err(AppError::external)?,
        started_at: row.try_get(6).map_err(AppError::external)?,
        updated_at: row.try_get(7).map_err(AppError::external)?,
        source_locator: row.try_get(8).map_err(AppError::external)?,
        source_fingerprint: row.try_get(9).map_err(AppError::external)?,
        missing: row.try_get::<i64, _>(10).map_err(AppError::external)? == 1,
        created_at: row.try_get(11).map_err(AppError::external)?,
        imported_at: row.try_get(12).map_err(AppError::external)?,
    })
}

pub(super) fn map_sqlx_conversation_turn(row: &SqliteRow) -> AppResult<ConversationTurn> {
    Ok(ConversationTurn {
        id: row.try_get(0).map_err(AppError::external)?,
        session_id: row.try_get(1).map_err(AppError::external)?,
        external_id: row.try_get(2).map_err(AppError::external)?,
        turn_index: row.try_get(3).map_err(AppError::external)?,
        user_text: row.try_get(4).map_err(AppError::external)?,
        title: row.try_get(5).map_err(AppError::external)?,
        started_at: row.try_get(6).map_err(AppError::external)?,
        ended_at: row.try_get(7).map_err(AppError::external)?,
        fingerprint: row.try_get(8).map_err(AppError::external)?,
        missing: row.try_get::<i64, _>(9).map_err(AppError::external)? == 1,
        imported_at: row.try_get(10).map_err(AppError::external)?,
    })
}

pub(super) fn map_sqlx_conversation_part(row: &SqliteRow) -> AppResult<ConversationPart> {
    Ok(ConversationPart {
        id: row.try_get(0).map_err(AppError::external)?,
        turn_id: row.try_get(1).map_err(AppError::external)?,
        part_index: row.try_get(2).map_err(AppError::external)?,
        role: decode_enum(row.try_get::<String, _>(3).map_err(AppError::external)?)?,
        kind: decode_enum(row.try_get::<String, _>(4).map_err(AppError::external)?)?,
        text: row.try_get(5).map_err(AppError::external)?,
        language: row.try_get(6).map_err(AppError::external)?,
        command: row.try_get(7).map_err(AppError::external)?,
        cwd: row.try_get(8).map_err(AppError::external)?,
        status: row.try_get(9).map_err(AppError::external)?,
        exit_code: row.try_get(10).map_err(AppError::external)?,
        metadata_json: row.try_get(11).map_err(AppError::external)?,
        command_label: row.try_get("command_label").map_err(AppError::external)?,
        source_execution_id: row
            .try_get("source_execution_id")
            .map_err(AppError::external)?,
        content_card: row
            .try_get::<Option<String>, _>(12)
            .map_err(AppError::external)?
            .map(decode_json)
            .transpose()?,
        translated_text: row.try_get(13).map_err(AppError::external)?,
    })
}

pub(super) fn map_sqlx_conversation_question(row: &SqliteRow) -> AppResult<ConversationQuestion> {
    Ok(ConversationQuestion {
        id: row.try_get(0).map_err(AppError::external)?,
        session_id: row.try_get(1).map_err(AppError::external)?,
        question_index: row.try_get(2).map_err(AppError::external)?,
        title: row.try_get(3).map_err(AppError::external)?,
        question_text: row.try_get(4).map_err(AppError::external)?,
        answer_text: row.try_get(5).map_err(AppError::external)?,
        code_text: row.try_get(6).map_err(AppError::external)?,
        command_text: row.try_get(7).map_err(AppError::external)?,
        grouping_origin: decode_enum(row.try_get::<String, _>(8).map_err(AppError::external)?)?,
        created_at: row.try_get(9).map_err(AppError::external)?,
        updated_at: row.try_get(10).map_err(AppError::external)?,
    })
}

pub(super) fn map_sqlx_conversation_question_turn(
    row: &SqliteRow,
) -> AppResult<ConversationQuestionTurn> {
    Ok(ConversationQuestionTurn {
        question_id: row.try_get(0).map_err(AppError::external)?,
        turn_id: row.try_get(1).map_err(AppError::external)?,
        turn_order: row.try_get(2).map_err(AppError::external)?,
        assignment_origin: decode_enum(row.try_get::<String, _>(3).map_err(AppError::external)?)?,
        assigned_at: row.try_get(4).map_err(AppError::external)?,
        updated_at: row.try_get(5).map_err(AppError::external)?,
    })
}

fn conversation_session_from_normalized(
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    now: &str,
) -> ConversationSession {
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
    let Some(row) = sqlx::query(
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

    let title: String = row.try_get(0).map_err(AppError::external)?;
    let project_path: Option<String> = row.try_get(1).map_err(AppError::external)?;
    let started_at: Option<String> = row.try_get(2).map_err(AppError::external)?;
    let updated_at: Option<String> = row.try_get(3).map_err(AppError::external)?;
    let source_locator: Option<String> = row.try_get(4).map_err(AppError::external)?;
    let existing_fingerprint: Option<String> = row.try_get(5).map_err(AppError::external)?;
    let missing: i64 = row.try_get(6).map_err(AppError::external)?;

    Ok(title == session.title
        && project_path == session.project_path
        && started_at == session.started_at
        && updated_at == session.updated_at
        && source_locator == session.source_locator
        && existing_fingerprint.as_deref() == Some(source_fingerprint)
        && missing == 0
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
    let rows = sqlx::query(
        r#"
        SELECT external_id, fingerprint, missing
        FROM conversation_turns
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY turn_index ASC
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
        let external_id: String = row.try_get(0).map_err(AppError::external)?;
        let fingerprint: String = row.try_get(1).map_err(AppError::external)?;
        let missing: i64 = row.try_get(2).map_err(AppError::external)?;
        if external_id != turn.external_id
            || fingerprint != conversation_turn_fingerprint(turn)
            || missing != 0
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
            updated_at, source_locator, source_fingerprint, missing, created_at, imported_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        ON CONFLICT(tenant_id, source_id, external_id) DO UPDATE SET
            adapter_id = excluded.adapter_id,
            title = excluded.title,
            project_path = excluded.project_path,
            started_at = excluded.started_at,
            updated_at = excluded.updated_at,
            source_locator = excluded.source_locator,
            source_fingerprint = excluded.source_fingerprint,
            missing = 0,
            imported_at = excluded.imported_at
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
    let rows =
        sqlx::query("SELECT id FROM conversation_turns WHERE tenant_id = ?1 AND session_id = ?2")
            .bind(tenant_id)
            .bind(session_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(AppError::external)?;
    let stale_turn_ids = rows
        .iter()
        .map(|row| row.try_get::<String, _>(0))
        .collect::<Result<Vec<_>, _>>()
        .map_err(AppError::external)?
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
    let existing_memberships =
        load_turn_question_memberships_sqlx_tx(tx, tenant_id, session_id).await?;
    let missing_turns = turns
        .iter()
        .filter(|turn| !existing_memberships.contains_key(&turn.id))
        .map(|turn| (turn.id.clone(), turn.user_text.clone()))
        .collect::<Vec<_>>();
    if missing_turns.is_empty() {
        return Ok(());
    }

    for group in group_turn_ids_by_question(missing_turns) {
        let first_turn_id = group.turn_ids.first().ok_or_else(|| {
            AppError::external({ "empty conversation question group".to_string() })
        })?;
        let previous_question_id =
            previous_question_id_for_turn_sqlx_tx(tx, tenant_id, session_id, first_turn_id).await?;
        let question_id = if group.origin == ConversationGroupingOrigin::AutoMerged {
            previous_question_id
                .unwrap_or_else(|| stable_id("conversation-question", &[session_id, first_turn_id]))
        } else {
            stable_id("conversation-question", &[session_id, first_turn_id])
        };
        if load_conversation_question_sqlx_tx(tx, tenant_id, &question_id)
            .await?
            .is_none()
        {
            let question_index = next_question_index_sqlx_tx(tx, tenant_id, session_id).await?;
            sqlx::query(
                r#"
                INSERT INTO conversation_questions (
                    tenant_id, id, session_id, question_index, title, question_text, answer_text,
                    code_text, command_text, grouping_origin, created_at, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, NULL, '', '', '', '', ?5, ?6, ?6)
                "#,
            )
            .bind(tenant_id)
            .bind(&question_id)
            .bind(session_id)
            .bind(question_index)
            .bind(encode_enum(group.origin)?)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
        }
        let start_order = max_question_turn_order_sqlx_tx(tx, tenant_id, &question_id).await? + 1;
        for (offset, turn_id) in group.turn_ids.iter().enumerate() {
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
            .bind(start_order + offset as i64)
            .bind(encode_enum(group.origin)?)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
        }
    }
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

async fn reject_invalid_conversation_question_turns_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
) -> AppResult<()> {
    let rows = sqlx::query(
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
    if rows.is_empty() {
        return Ok(());
    }

    let question_id: String = rows[0].try_get(0).map_err(AppError::external)?;
    let turn_id: String = rows[0].try_get(1).map_err(AppError::external)?;
    let reason: String = rows[0].try_get(2).map_err(AppError::external)?;
    Err(AppError::Validation(format!(
        "invalid question turn membership ({reason}): question={question_id}, turn={turn_id}"
    )))
}

async fn audit_invalid_conversation_question_turns_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    let rows = sqlx::query(
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
        .bind(row.try_get::<String, _>(0).map_err(AppError::external)?)
        .bind(row.try_get::<String, _>(1).map_err(AppError::external)?)
        .bind(row.try_get::<String, _>(2).map_err(AppError::external)?)
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
    // Keep the legacy snapshot and FTS row synchronized as a compatibility cache. Question Detail
    // reads project from membership and Turn-Part facts through the seam above.
    let turns = load_question_turns_sqlx_tx(tx, tenant_id, question_id).await?;
    let mut question_text = Vec::new();
    let mut answer_text = Vec::new();
    let mut code_text = Vec::new();
    let mut command_text = Vec::new();

    for turn in &turns {
        question_text.push(turn.user_text.clone());
        for part in load_turn_parts_sqlx_tx(tx, tenant_id, &turn.id).await? {
            append_declared_card_to_question_aggregate(
                &part,
                &mut answer_text,
                &mut code_text,
                &mut command_text,
            );
        }
    }

    let question_text = question_text.join("\n\n");
    let answer_text = answer_text.join("\n\n");
    let code_text = code_text.join("\n\n");
    let command_text = command_text.join("\n\n");
    let title = first_line(&question_text);

    sqlx::query(
        r#"
        UPDATE conversation_questions
        SET title = COALESCE(NULLIF(title, ''), ?1),
            question_text = ?2,
            answer_text = ?3,
            code_text = ?4,
            command_text = ?5,
            updated_at = ?6
        WHERE tenant_id = ?7 AND id = ?8
        "#,
    )
    .bind(&title)
    .bind(&question_text)
    .bind(&answer_text)
    .bind(&code_text)
    .bind(&command_text)
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
        ORDER BY turn_index ASC, imported_at ASC
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

async fn load_turn_question_memberships_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<BTreeMap<String, String>> {
    let rows = sqlx::query(
        r#"
        SELECT qt.turn_id, qt.question_id
        FROM conversation_question_turns qt
        JOIN conversation_turns t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE t.tenant_id = ?1 AND t.session_id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;
    let mut memberships = BTreeMap::new();
    for row in rows {
        memberships.insert(
            row.try_get(0).map_err(AppError::external)?,
            row.try_get(1).map_err(AppError::external)?,
        );
    }
    Ok(memberships)
}

async fn previous_question_id_for_turn_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    turn_id: &str,
) -> AppResult<Option<String>> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT qt.question_id
        FROM conversation_turns current
        JOIN conversation_turns previous
          ON previous.tenant_id = current.tenant_id
         AND previous.session_id = current.session_id
         AND previous.turn_index < current.turn_index
        JOIN conversation_question_turns qt
          ON qt.tenant_id = previous.tenant_id AND qt.turn_id = previous.id
        WHERE current.tenant_id = ?1 AND current.session_id = ?2 AND current.id = ?3
        ORDER BY previous.turn_index DESC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .bind(turn_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::external)
}

async fn next_question_index_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> AppResult<i64> {
    let max_index: Option<i64> = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(question_index) FROM conversation_questions WHERE tenant_id = ?1 AND session_id = ?2",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::external)?;
    Ok(max_index.unwrap_or(-1) + 1)
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
        SELECT id, session_id, question_index, title, question_text, answer_text,
               code_text, command_text, grouping_origin, created_at, updated_at
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
        ORDER BY q.question_index ASC
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
    let question_ids = sqlx::query_scalar::<_, String>(
        r#"
        SELECT q.id
        FROM conversation_questions q
        JOIN conversation_question_turns qt
          ON qt.tenant_id = q.tenant_id AND qt.question_id = q.id
        JOIN conversation_turns t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
        WHERE q.tenant_id = ?1 AND q.session_id = ?2
        GROUP BY q.id
        ORDER BY MIN(t.turn_index) ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::external)?;

    for (index, question_id) in question_ids.iter().enumerate() {
        sqlx::query(
            "UPDATE conversation_questions SET question_index = ?1 WHERE tenant_id = ?2 AND id = ?3",
        )
            .bind(1_000_000i64 + index as i64)
            .bind(tenant_id)
            .bind(question_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
    }
    for (index, question_id) in question_ids.iter().enumerate() {
        sqlx::query(
            "UPDATE conversation_questions SET question_index = ?1 WHERE tenant_id = ?2 AND id = ?3",
        )
            .bind(index as i64)
            .bind(tenant_id)
            .bind(question_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::external)?;
    }
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
    let rows = sqlx::query(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.iter()
        .map(|row| {
            let question_count = usize::try_from(
                row.try_get::<i64, _>(13).map_err(AppError::external)?,
            )
            .map_err(|_| {
                AppError::external({ "invalid conversation search question count".to_string() })
            })?;
            let turn_count =
                usize::try_from(row.try_get::<i64, _>(14).map_err(AppError::external)?)
                    .map_err(|_| AppError::external("invalid conversation search turn count"))?;
            Ok(ConversationSessionListItem {
                session: map_sqlx_conversation_session(row)?,
                question_count,
                turn_count,
            })
        })
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
        SELECT q.id, q.session_id, q.question_index, q.title, q.question_text,
               q.answer_text, q.code_text, q.command_text, q.grouping_origin,
               q.created_at, q.updated_at
        FROM {questions} q
        JOIN {sessions} s ON s.tenant_id = q.tenant_id AND s.id = q.session_id
        WHERE q.tenant_id = ?1
          AND (?2 IS NULL OR s.adapter_id = ?2)
          AND (?3 IS NULL OR s.source_id = ?3)
          AND (?4 IS NULL OR s.id IN (SELECT value FROM json_each(?4)))
        ORDER BY q.session_id ASC, q.question_index ASC
        "#,
        questions = tables.questions,
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
    let rows = sqlx::query(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(source_id)
        .bind(session_ids_json)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    let mut turns_by_question = BTreeMap::<String, Vec<ConversationTurn>>::new();
    for row in &rows {
        let question_id = row.try_get::<String, _>(11).map_err(AppError::external)?;
        turns_by_question
            .entry(question_id)
            .or_default()
            .push(map_sqlx_conversation_turn(row)?);
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

pub(super) fn append_declared_card_to_question_aggregate(
    part: &ConversationPart,
    answer_text: &mut Vec<String>,
    code_text: &mut Vec<String>,
    command_text: &mut Vec<String>,
) {
    let Some(card) = resolved_content_card_for_part(part) else {
        return;
    };
    let semantic_role = card
        .semantic_role
        .as_deref()
        .or_else(|| card.kind.rsplit_once('.').map(|(_, value)| value))
        .unwrap_or(card.kind.as_str());
    match semantic_role {
        "answer" => answer_text.push(card.body),
        "code" => code_text.push(card.body),
        "command" => command_text.push(card.body),
        // Tool, result, file changes and adapter-specific cards are kept on their
        // original Part/Card only; question aggregates must not duplicate them.
        _ => {}
    }
}

fn resolved_content_card_for_part(
    part: &ConversationPart,
) -> Option<crate::backend::projection::conversation_cards::ResolvedConversationContentCard> {
    crate::backend::projection::conversation_cards::resolve_historical_content_card(
        crate::backend::projection::conversation_cards::ConversationCardProjectionSource {
            content_card: part.content_card.as_ref(),
            metadata_json: part.metadata_json.as_deref(),
            text: part.text.as_deref(),
            language: part.language.as_deref(),
            command: part.command.as_deref(),
            cwd: part.cwd.as_deref(),
            status: part.status.as_deref(),
            exit_code: part.exit_code,
        },
    )
    .ok()?
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
        .and_then(parse_rfc3339_utc)
        .or_else(|| session.updated_at.as_deref().and_then(parse_rfc3339_utc))
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
        question_index: question.question_index,
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
    card_kinds: &[ConversationCardKindDefinition],
) -> Vec<ConversationSearchEntry> {
    resolved_content_card_for_part(part)
        .map(|card| {
            let semantic_role = card.semantic_role.clone().or_else(|| {
                card_kinds
                    .iter()
                    .find(|definition| definition.id == card.kind)
                    .and_then(|definition| definition.semantic_role.clone())
            });
            ConversationSearchEntry {
                card_type: ConversationSearchCardType::new(card.kind),
                block_id: part.id.clone(),
                text: card.body,
                semantic_role,
            }
        })
        .into_iter()
        .collect()
}

async fn load_search_adapter_card_kinds_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<BTreeMap<String, Vec<ConversationCardKindDefinition>>> {
    let rows =
        sqlx::query("SELECT id, card_kinds_json FROM conversation_adapters WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_all(pool)
            .await
            .map_err(AppError::external)?;
    rows.iter()
        .map(|row| {
            let adapter_id = row.try_get(0).map_err(AppError::external)?;
            let definitions =
                decode_json(row.try_get::<String, _>(1).map_err(AppError::external)?)?;
            Ok((adapter_id, definitions))
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

fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    format!("{prefix}-{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{
        ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
        ConversationAdapterRuntimeGateStatus, ConversationCardKindDefinition,
        ConversationContentCardDescriptor, ConversationPackageUpdatePolicy, ConversationPartKind,
        ConversationPartRole,
        NormalizedConversationPart, NormalizedConversationTurn,
    };
    use crate::backend::store::Database;
    use uuid::Uuid;

    const TEST_TENANT_ID: &str = "default";

    #[test]
    fn question_aggregate_excludes_tool_result_and_diff_bodies() {
        let base = |kind: ConversationPartKind, card_kind: &str, text: &str| ConversationPart {
            id: format!("part-{card_kind}"),
            turn_id: "turn-1".to_string(),
            part_index: 0,
            role: crate::backend::models::ConversationPartRole::Tool,
            kind,
            text: Some(text.to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: format!("fixture.{card_kind}"),
                renderer: Some(if card_kind == "diff" { "diff" } else { "plain" }.to_string()),
            }),
            metadata_json: None,
            translated_text: None,
        };
        let answer = base(ConversationPartKind::Text, "answer", "answer");
        let result = base(ConversationPartKind::Tool, "result", "duplicated result");
        let diff = base(
            ConversationPartKind::FileChange,
            "result",
            "duplicated diff",
        );
        let mut answer_text = Vec::new();
        let mut code_text = Vec::new();
        let mut command_text = Vec::new();

        append_declared_card_to_question_aggregate(
            &answer,
            &mut answer_text,
            &mut code_text,
            &mut command_text,
        );
        append_declared_card_to_question_aggregate(
            &result,
            &mut answer_text,
            &mut code_text,
            &mut command_text,
        );
        append_declared_card_to_question_aggregate(
            &diff,
            &mut answer_text,
            &mut code_text,
            &mut command_text,
        );

        assert_eq!(answer_text, vec!["answer"]);
        assert!(code_text.is_empty());
        assert!(command_text.is_empty());
    }

    #[test]
    fn conversation_content_nodes_group_interleaved_results_by_source_id() {
        let card_kinds = [
            ConversationCardKindDefinition {
                id: "fixture.command".to_string(),
                semantic_role: Some("command".to_string()),
                label: "Command".to_string(),
                default_renderer: "command".to_string(),
                allowed_renderers: vec!["command".to_string()],
                icon_hint: None,
            },
            ConversationCardKindDefinition {
                id: "fixture.result".to_string(),
                semantic_role: Some("result".to_string()),
                label: "Result".to_string(),
                default_renderer: "terminal_output".to_string(),
                allowed_renderers: vec!["terminal_output".to_string()],
                icon_hint: None,
            },
        ];
        let part = |id: &str,
                    part_index: i64,
                    source_execution_id: &str,
                    command: bool,
                    command_label: Option<&str>| {
            ConversationPart {
                id: id.to_string(),
                turn_id: "turn-1".to_string(),
                part_index,
                role: ConversationPartRole::Tool,
                kind: if command {
                    ConversationPartKind::Command
                } else {
                    ConversationPartKind::Tool
                },
                text: (!command).then(|| format!("result-{id}")),
                language: None,
                command: command.then(|| format!("command-{id}")),
                cwd: None,
                status: None,
                exit_code: None,
                command_label: command_label.map(str::to_string),
                source_execution_id: Some(source_execution_id.to_string()),
                content_card: Some(ConversationContentCardDescriptor {
                    schema_version: 1,
                    kind: if command {
                        "fixture.command".to_string()
                    } else {
                        "fixture.result".to_string()
                    },
                    renderer: Some(if command {
                        "command".to_string()
                    } else {
                        "terminal_output".to_string()
                    }),
                }),
                metadata_json: None,
                translated_text: None,
            }
        };
        let parts = vec![
            part("command-a", 0, "call-a", true, None),
            part("command-a-2", 1, "call-a", true, Some("DEBUG")),
            part("command-b", 2, "call-b", true, None),
            part("result-b", 3, "call-b", false, None),
            part("result-a", 4, "call-a", false, None),
        ];

        let (cards, nodes) =
            project_conversation_legacy_cards_and_nodes(&parts, "fixture", &card_kinds)
                .expect("project execution nodes");

        assert_eq!(cards.len(), 5);
        assert_eq!(cards[0].card_id, "command-a");
        assert_eq!(cards[1].card_id, "command-a-2");
        assert_eq!(cards[1].command_label.as_deref(), Some("DEBUG"));
        assert_eq!(
            nodes,
            vec![
                LegacyConversationContentNode::Card {
                    turn_id: "turn-1".to_string(),
                    card_index: 0,
                },
                LegacyConversationContentNode::Execution {
                    turn_id: "turn-1".to_string(),
                    source_execution_id: "call-a".to_string(),
                    command_card_index: Some(1),
                    result_card_indices: vec![4],
                },
                LegacyConversationContentNode::Execution {
                    turn_id: "turn-1".to_string(),
                    source_execution_id: "call-b".to_string(),
                    command_card_index: Some(2),
                    result_card_indices: vec![3],
                },
            ]
        );
    }

    #[test]
    fn conversation_content_nodes_keep_file_changes_outside_execution_results() {
        let card_kinds = [
            ConversationCardKindDefinition {
                id: "fixture.command".to_string(),
                semantic_role: Some("command".to_string()),
                label: "Command".to_string(),
                default_renderer: "command".to_string(),
                allowed_renderers: vec!["command".to_string()],
                icon_hint: None,
            },
            ConversationCardKindDefinition {
                id: "fixture.file-change".to_string(),
                semantic_role: Some("file-change".to_string()),
                label: "文件更改".to_string(),
                default_renderer: "diff".to_string(),
                allowed_renderers: vec!["diff".to_string()],
                icon_hint: None,
            },
        ];
        let parts = vec![
            ConversationPart {
                id: "command".to_string(),
                turn_id: "turn-1".to_string(),
                part_index: 0,
                role: ConversationPartRole::Tool,
                kind: ConversationPartKind::Command,
                text: None,
                language: None,
                command: Some("apply patch".to_string()),
                cwd: None,
                status: Some("completed".to_string()),
                exit_code: Some(0),
                command_label: None,
                source_execution_id: Some("call-1".to_string()),
                content_card: Some(ConversationContentCardDescriptor {
                    schema_version: 1,
                    kind: "fixture.command".to_string(),
                    renderer: Some("command".to_string()),
                }),
                metadata_json: None,
                translated_text: None,
            },
            ConversationPart {
                id: "file-change".to_string(),
                turn_id: "turn-1".to_string(),
                part_index: 1,
                role: ConversationPartRole::Tool,
                kind: ConversationPartKind::FileChange,
                text: Some("--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n".to_string()),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: Some("call-1".to_string()),
                content_card: Some(ConversationContentCardDescriptor {
                    schema_version: 1,
                    kind: "fixture.file-change".to_string(),
                    renderer: Some("diff".to_string()),
                }),
                metadata_json: None,
                translated_text: None,
            },
        ];

        let (cards, nodes) =
            project_conversation_legacy_cards_and_nodes(&parts, "fixture", &card_kinds)
                .expect("project standalone file change node");

        assert_eq!(cards.len(), 2);
        assert_eq!(cards[1].kind, "fixture.file-change");
        assert_eq!(cards[1].semantic_role.as_deref(), Some("file-change"));
        assert_eq!(
            nodes,
            vec![
                LegacyConversationContentNode::Card {
                    turn_id: "turn-1".to_string(),
                    card_index: 0,
                },
                LegacyConversationContentNode::Card {
                    turn_id: "turn-1".to_string(),
                    card_index: 1,
                },
            ]
        );
    }

    #[test]
    fn sqlx_conversation_metadata_round_trips_and_disables_sources() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-metadata-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let builtin_adapter = test_conversation_adapter(
            "metadata-builtin",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::BuiltIn,
        );
        let external_adapter = test_conversation_adapter(
            "metadata-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&external_adapter.id);

        let (
            adapters,
            loaded_adapter,
            sources,
            loaded_source,
            disabled_source,
            disabled_builtin,
            deleted_adapter,
            source_after_adapter_delete,
            missing_adapter,
        ) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &builtin_adapter)
                    .await?;
                upsert_conversation_adapter_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &external_adapter,
                )
                .await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;

                let adapters =
                    list_conversation_adapters_sqlx(database.pool(), TEST_TENANT_ID).await?;
                let loaded_adapter = load_conversation_adapter_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &external_adapter.id,
                )
                .await?;
                let sources =
                    list_conversation_sources_sqlx(database.pool(), TEST_TENANT_ID).await?;
                let loaded_source =
                    load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id)
                        .await?;
                let disabled_source =
                    disable_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id)
                        .await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                let disabled_builtin = delete_conversation_adapter_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &builtin_adapter.id,
                )
                .await?;
                let deleted_adapter = delete_conversation_adapter_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &external_adapter.id,
                )
                .await?;
                let source_after_adapter_delete =
                    load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id)
                        .await?
                        .expect("source is retained after adapter delete");
                let missing_adapter = load_conversation_adapter_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &external_adapter.id,
                )
                .await?;

                Ok::<_, AppError>((
                    adapters,
                    loaded_adapter,
                    sources,
                    loaded_source,
                    disabled_source,
                    disabled_builtin,
                    deleted_adapter,
                    source_after_adapter_delete,
                    missing_adapter,
                ))
            })
            .expect("query SQLx conversation metadata repo");

        assert!(adapters.iter().any(|adapter| adapter == &external_adapter));
        assert_eq!(loaded_adapter.as_ref(), Some(&external_adapter));
        assert!(sources.iter().any(|candidate| candidate == &source));
        assert_eq!(loaded_source.as_ref(), Some(&source));
        assert_eq!(disabled_source.id, source.id);
        assert!(!disabled_source.enabled);
        assert_eq!(disabled_builtin.id, builtin_adapter.id);
        assert!(!disabled_builtin.enabled);
        assert_eq!(deleted_adapter, external_adapter);
        assert_eq!(source_after_adapter_delete.id, source.id);
        assert!(!source_after_adapter_delete.enabled);
        assert!(missing_adapter.is_none());

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    #[cfg(unix)]
    fn sqlx_conversation_adapter_packages_round_trip_by_package_and_adapter() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-package-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let install_dir = dirs::config_dir()
            .expect("config directory")
            .join("assetiweave")
            .join("conversation-adapters")
            .join("packages")
            .join("codex-session")
            .join("current");
        let package = ConversationAdapterPackage {
            package_id: "codex-session".to_string(),
            adapter_id: "codex".to_string(),
            name: "Codex Session Parser".to_string(),
            version: "1.0.0".to_string(),
            record_kind: ConversationAdapterPackageRecordKind::Session,
            install_dir: install_dir.to_string_lossy().to_string(),
            manifest_path: install_dir
                .join("conversation-adapter-package.json")
                .to_string_lossy()
                .to_string(),
            adapter_manifest_path: install_dir
                .join("conversation-adapter.json")
                .to_string_lossy()
                .to_string(),
            runtime_protocol: "stdio-ndjson-v1".to_string(),
            runtime_ready: true,
            origin: ConversationAdapterPackageOrigin::ManagedRelease,
            source_url: Some("https://github.com/util6/assetiweave".to_string()),
            git_ref: Some("refs/tags/conversation-adapter-packages-main".to_string()),
            git_commit: Some("abc123".to_string()),
            catalog_url: Some("https://example.com/index.json".to_string()),
            update_policy: ConversationPackageUpdatePolicy::Manual,
            latest_version: Some("1.1.0".to_string()),
            last_checked_at: Some("2026-07-04T01:00:00Z".to_string()),
            runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
            runtime_validated_at: Some("2026-07-04T02:00:00Z".to_string()),
            installed_content_hash: Some("package-hash".to_string()),
            trusted_package_hash: Some("package-hash".to_string()),
            error_message: None,
            created_at: "2026-07-04T00:00:00Z".to_string(),
            updated_at: "2026-07-04T00:00:00Z".to_string(),
        };

        let (listed, by_package, by_adapter, deleted, missing) = database
            .block_on(async {
                upsert_conversation_adapter_package_sqlx(database.pool(), TEST_TENANT_ID, &package)
                    .await?;
                let listed =
                    list_conversation_adapter_packages_sqlx(database.pool(), TEST_TENANT_ID)
                        .await?;
                let by_package = load_conversation_adapter_package_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &package.package_id,
                )
                .await?;
                let by_adapter = load_conversation_adapter_package_by_adapter_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &package.adapter_id,
                )
                .await?;
                let deleted = delete_conversation_adapter_package_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &package.package_id,
                )
                .await?;
                let missing = load_conversation_adapter_package_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &package.package_id,
                )
                .await?;
                Ok::<_, AppError>((listed, by_package, by_adapter, deleted, missing))
            })
            .expect("round trip package");

        let mut stored_package = package;
        stored_package.install_dir =
            "@config/assetiweave/conversation-adapters/packages/codex-session/current".to_string();
        stored_package.manifest_path = format!(
            "{}/conversation-adapter-package.json",
            stored_package.install_dir
        );
        stored_package.adapter_manifest_path =
            format!("{}/conversation-adapter.json", stored_package.install_dir);
        assert_eq!(listed, vec![stored_package.clone()]);
        assert_eq!(by_package, Some(stored_package.clone()));
        assert_eq!(by_adapter, Some(stored_package.clone()));
        assert_eq!(deleted, Some(stored_package));
        assert!(missing.is_none());

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    #[cfg(unix)]
    fn conversation_adapter_runtime_paths_normalize_to_config_anchor() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-adapter-config-path-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let mut adapter = test_conversation_adapter(
            "portable-adapter",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let adapter_dir = dirs::config_dir()
            .expect("config directory")
            .join("assetiweave")
            .join("conversation-adapters")
            .join("portable-adapter");
        adapter.manifest_path = Some(
            adapter_dir
                .join("conversation-adapter.json")
                .to_string_lossy()
                .to_string(),
        );
        adapter.executable_path = Some(
            adapter_dir
                .join("adapter.mjs")
                .to_string_lossy()
                .to_string(),
        );

        let loaded = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter.id).await
            })
            .expect("round trip adapter")
            .expect("stored adapter");

        assert_eq!(
            loaded.manifest_path.as_deref(),
            Some("@config/assetiweave/conversation-adapters/portable-adapter/conversation-adapter.json")
        );
        assert_eq!(
            loaded.executable_path.as_deref(),
            Some("@config/assetiweave/conversation-adapters/portable-adapter/adapter.mjs")
        );
        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn managed_package_uninstall_disables_runtime_but_preserves_package_versions_and_sources() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-package-uninstall-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "uninstall-adapter",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);
        let package = ConversationAdapterPackage {
            package_id: "com.util6.uninstall-adapter".to_string(),
            adapter_id: adapter.id.clone(),
            name: "Uninstall Adapter".to_string(),
            version: "1.0.0".to_string(),
            record_kind: ConversationAdapterPackageRecordKind::Session,
            install_dir: "/tmp/packages/com.util6.uninstall-adapter/versions/1.0.0".to_string(),
            manifest_path: "/tmp/packages/com.util6.uninstall-adapter/versions/1.0.0/conversation-adapter-package.json".to_string(),
            adapter_manifest_path: "/tmp/packages/com.util6.uninstall-adapter/versions/1.0.0/conversation-adapter.json".to_string(),
            runtime_protocol: "stdio-ndjson-v1".to_string(),
            runtime_ready: true,
            origin: ConversationAdapterPackageOrigin::ManagedRelease,
            source_url: None,
            git_ref: None,
            git_commit: None,
            catalog_url: None,
            update_policy: ConversationPackageUpdatePolicy::Manual,
            latest_version: Some("1.0.0".to_string()),
            last_checked_at: None,
            runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
            runtime_validated_at: None,
            installed_content_hash: Some("hash-v1".to_string()),
            trusted_package_hash: Some("hash-v1".to_string()),
            error_message: None,
            created_at: "2026-07-17T00:00:00Z".to_string(),
            updated_at: "2026-07-17T00:00:00Z".to_string(),
        };
        let version = ConversationAdapterPackageVersion {
            package_id: package.package_id.clone(),
            version: package.version.clone(),
            install_dir: package.install_dir.clone(),
            artifact_hash: Some("artifact-v1".to_string()),
            content_hash: "hash-v1".to_string(),
            runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
            installed_at: package.created_at.clone(),
        };

        let (uninstalled, remaining_versions, retained_source, missing_adapter) = database
            .block_on(async {
                activate_conversation_adapter_package_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &adapter,
                    &package,
                    &version,
                )
                .await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                let uninstalled = deactivate_conversation_adapter_package_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &package.package_id,
                    &adapter.id,
                )
                .await?;
                let remaining_versions = list_conversation_adapter_package_versions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &package.package_id,
                )
                .await?;
                let retained_source =
                    load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id)
                        .await?
                        .expect("source remains after uninstall");
                let missing_adapter =
                    load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter.id)
                        .await?;
                Ok::<_, AppError>((
                    uninstalled,
                    remaining_versions,
                    retained_source,
                    missing_adapter,
                ))
            })
            .expect("uninstall managed package runtime");

        assert!(!uninstalled.runtime_ready);
        assert_eq!(
            uninstalled.runtime_gate_status,
            ConversationAdapterRuntimeGateStatus::RuntimeMissing
        );
        assert_eq!(remaining_versions, vec![version]);
        assert!(!retained_source.enabled);
        assert!(missing_adapter.is_none());

        let (deleted_version, missing_package, versions_after_delete, source_after_delete) =
            database
                .block_on(async {
                    let deleted_version = delete_conversation_adapter_package_version_sqlx(
                        database.pool(),
                        TEST_TENANT_ID,
                        &package.package_id,
                        &package.version,
                        None,
                        true,
                    )
                    .await?;
                    let missing_package = load_conversation_adapter_package_sqlx(
                        database.pool(),
                        TEST_TENANT_ID,
                        &package.package_id,
                    )
                    .await?;
                    let versions_after_delete = list_conversation_adapter_package_versions_sqlx(
                        database.pool(),
                        TEST_TENANT_ID,
                        &package.package_id,
                    )
                    .await?;
                    let source_after_delete =
                        load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id)
                            .await?
                            .expect("source remains after deleting package files");
                    Ok::<_, AppError>((
                        deleted_version,
                        missing_package,
                        versions_after_delete,
                        source_after_delete,
                    ))
                })
                .expect("delete the final uninstalled package version");
        assert!(deleted_version);
        assert!(missing_package.is_none());
        assert!(versions_after_delete.is_empty());
        assert!(!source_after_delete.enabled);

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn package_activation_rolls_back_adapter_and_active_version_when_version_insert_fails() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-package-activation-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let original_adapter = test_conversation_adapter(
            "activation-adapter",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let package = ConversationAdapterPackage {
            package_id: "com.util6.activation".to_string(),
            adapter_id: original_adapter.id.clone(),
            name: "Activation Adapter".to_string(),
            version: "1.0.0".to_string(),
            record_kind: ConversationAdapterPackageRecordKind::Session,
            install_dir: "/tmp/packages/com.util6.activation/versions/1.0.0".to_string(),
            manifest_path: "/tmp/packages/com.util6.activation/versions/1.0.0/conversation-adapter-package.json".to_string(),
            adapter_manifest_path: "/tmp/packages/com.util6.activation/versions/1.0.0/conversation-adapter.json".to_string(),
            runtime_protocol: "stdio-ndjson-v1".to_string(),
            runtime_ready: true,
            origin: ConversationAdapterPackageOrigin::ManagedRelease,
            source_url: None,
            git_ref: None,
            git_commit: None,
            catalog_url: None,
            update_policy: ConversationPackageUpdatePolicy::Manual,
            latest_version: Some("1.0.0".to_string()),
            last_checked_at: None,
            runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
            runtime_validated_at: None,
            installed_content_hash: Some("hash-v1".to_string()),
            trusted_package_hash: Some("hash-v1".to_string()),
            error_message: None,
            created_at: "2026-07-15T00:00:00Z".to_string(),
            updated_at: "2026-07-15T00:00:00Z".to_string(),
        };
        let version = ConversationAdapterPackageVersion {
            package_id: package.package_id.clone(),
            version: package.version.clone(),
            install_dir: package.install_dir.clone(),
            artifact_hash: Some("hash-v1".to_string()),
            content_hash: "hash-v1".to_string(),
            runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
            installed_at: package.created_at.clone(),
        };
        database
            .block_on(activate_conversation_adapter_package_sqlx(
                database.pool(),
                TEST_TENANT_ID,
                &original_adapter,
                &package,
                &version,
            ))
            .expect("activate original package");

        let mut candidate_adapter = original_adapter.clone();
        candidate_adapter.version = "2.0.0".to_string();
        let mut candidate_package = package.clone();
        candidate_package.version = "2.0.0".to_string();
        candidate_package.install_dir =
            "/tmp/packages/com.util6.activation/versions/2.0.0".to_string();
        let invalid_version = ConversationAdapterPackageVersion {
            package_id: "com.util6.missing-parent".to_string(),
            version: "2.0.0".to_string(),
            install_dir: candidate_package.install_dir.clone(),
            artifact_hash: Some("hash-v2".to_string()),
            content_hash: "hash-v2".to_string(),
            runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
            installed_at: "2026-07-15T01:00:00Z".to_string(),
        };
        database
            .block_on(activate_conversation_adapter_package_sqlx(
                database.pool(),
                TEST_TENANT_ID,
                &candidate_adapter,
                &candidate_package,
                &invalid_version,
            ))
            .expect_err("foreign-key failure should roll back activation");

        let (stored_adapter, stored_package) = database
            .block_on(async {
                Ok::<_, AppError>((
                    load_conversation_adapter_sqlx(
                        database.pool(),
                        TEST_TENANT_ID,
                        &original_adapter.id,
                    )
                    .await?,
                    load_conversation_adapter_package_sqlx(
                        database.pool(),
                        TEST_TENANT_ID,
                        &package.package_id,
                    )
                    .await?,
                ))
            })
            .expect("reload active package");
        assert_eq!(stored_adapter, Some(original_adapter));
        assert_eq!(stored_package, Some(package));

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn seeding_builtin_conversation_adapters_preserves_user_registered_adapter() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-adapter-seed-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let mut market_adapter = test_conversation_adapter(
            "codex",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        market_adapter.manifest_path =
            Some("/tmp/assetiweave-market/codex-session/conversation-adapter.json".to_string());
        market_adapter.executable_path =
            Some("/tmp/assetiweave-market/codex-session/adapter.mjs".to_string());

        let loaded = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &market_adapter)
                    .await?;
                seed_prepared_builtin_conversation_adapters_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    vec![test_conversation_adapter(
                        "codex",
                        ConversationAdapterKind::External,
                        ConversationAdapterTrustState::BuiltIn,
                    )],
                )
                .await?;
                load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, "codex").await
            })
            .expect("seed built-in conversation adapters")
            .expect("codex adapter");

        assert_eq!(loaded.trust_state, ConversationAdapterTrustState::Trusted);
        assert_eq!(loaded.manifest_path, market_adapter.manifest_path);
        assert_eq!(loaded.executable_path, market_adapter.executable_path);

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn disabling_builtin_adapter_disables_runtime_and_sources() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-adapter-disable-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");

        let builtin_adapter = test_conversation_adapter(
            "disable-builtin",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::BuiltIn,
        );
        let source = test_conversation_source(&builtin_adapter.id);
        let (adapter, source) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &builtin_adapter)
                    .await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                disable_builtin_conversation_adapter_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &builtin_adapter.id,
                )
                .await?;
                let adapter = load_conversation_adapter_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &builtin_adapter.id,
                )
                .await?
                .expect("built-in adapter retained");
                let source =
                    load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id)
                        .await?
                        .expect("source retained");
                Ok::<_, AppError>((adapter, source))
            })
            .expect("disable built-in adapter");

        assert!(!adapter.enabled);
        assert!(!source.enabled);

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_import_preserves_manual_grouping_across_resync() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-import-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "import-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);

        let (initial_question_count, initial_first_question_turn_count, detail) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session("v1")],
                    false,
                )
                .await?;
                let sessions = list_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    None,
                    20,
                    0,
                )
                .await?;
                let detail = load_conversation_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &sessions[0].session.id,
                )
                .await?;
                let initial_question_count = detail.questions.len();
                let initial_first_question_turn_count = detail.questions[0].turns.len();
                let question_ids = detail
                    .questions
                    .iter()
                    .map(|question| question.question.id.clone())
                    .collect::<Vec<_>>();
                merge_conversation_questions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &question_ids,
                    false,
                )
                .await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session("v2")],
                    false,
                )
                .await?;
                let detail = load_conversation_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &sessions[0].session.id,
                )
                .await?;
                Ok::<_, AppError>((
                    initial_question_count,
                    initial_first_question_turn_count,
                    detail,
                ))
            })
            .expect("import and merge through SQLx");

        assert_eq!(initial_question_count, 2);
        assert_eq!(initial_first_question_turn_count, 2);
        assert_eq!(detail.questions.len(), 1);
        assert_eq!(detail.questions[0].turns.len(), 3);
        assert_eq!(
            detail.questions[0].question.grouping_origin,
            ConversationGroupingOrigin::Manual
        );

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_question_detail_projects_from_question_turn_membership_and_turn_facts() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-question-projection-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "question-projection-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);

        let detail = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session("v1")],
                    false,
                )
                .await?;
                let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
                sqlx::query(
                    r#"
                    UPDATE conversation_questions
                    SET question_text = 'stale snapshot',
                        answer_text = 'stale answer',
                        code_text = 'stale code',
                        command_text = 'stale command'
                    WHERE tenant_id = ?1 AND session_id = ?2
                    "#,
                )
                .bind(TEST_TENANT_ID)
                .bind(&session_id)
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                load_conversation_session_detail_sqlx(database.pool(), TEST_TENANT_ID, &session_id)
                    .await
            })
            .expect("load projected question detail");

        let first_question = &detail.questions[0];
        assert_eq!(
            first_question.question.question_text,
            "How does sync work?\n\n继续"
        );
        assert_eq!(
            first_question.question.answer_text,
            "answer for t1\n\nanswer for t2"
        );
        assert_eq!(first_question.question_turns.len(), 2);
        assert_eq!(
            first_question
                .question_turns
                .iter()
                .map(|membership| membership.turn_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                first_question.turns[0].id.as_str(),
                first_question.turns[1].id.as_str()
            ]
        );
        assert_eq!(
            first_question.question_turns[0].assignment_origin,
            ConversationGroupingOrigin::AutoMerged
        );
        assert_eq!(first_question.question_turns[0].turn_order, 0);
        assert!(!first_question.question_turns[0].assigned_at.is_empty());
        assert!(!first_question.question_turns[0].updated_at.is_empty());

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_rejects_cross_session_question_turn_membership_before_new_write() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-question-membership-scope-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "question-membership-scope-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);
        let mut second_session = fixture_session("v1");
        second_session.external_id = "session-2".to_string();

        let error = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session("v1"), second_session],
                    false,
                )
                .await?;

                let first_session_id =
                    stable_id("conversation-session", &[&source.id, "session-1"]);
                let second_session_id =
                    stable_id("conversation-session", &[&source.id, "session-2"]);
                let first_turn_id = stable_id("conversation-turn", &[&first_session_id, "t1"]);
                let second_turn_id = stable_id("conversation-turn", &[&second_session_id, "t1"]);
                let first_question_id = stable_id(
                    "conversation-question",
                    &[&first_session_id, &first_turn_id],
                );
                sqlx::query(
                    "DELETE FROM conversation_question_turns WHERE tenant_id = ?1 AND turn_id = ?2",
                )
                .bind(TEST_TENANT_ID)
                .bind(&second_turn_id)
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                sqlx::query(
                    r#"
                    INSERT INTO conversation_question_turns (
                        tenant_id, question_id, turn_id, turn_order,
                        assignment_origin, assigned_at, updated_at
                    )
                    VALUES (?1, ?2, ?3, 0, 'imported', ?4, ?4)
                    "#,
                )
                .bind(TEST_TENANT_ID)
                .bind(&first_question_id)
                .bind(&second_turn_id)
                .bind("2026-08-25T00:00:00Z")
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;

                let result = import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session("v1")],
                    false,
                )
                .await;
                Ok::<_, AppError>(
                    result.expect_err("cross-session membership must block the write"),
                )
            })
            .expect("validate cross-session membership");

        assert!(error.to_string().contains("cross_session"));
        let audit_count = database
            .block_on(async {
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM conversation_question_turn_audits WHERE tenant_id = ?1 AND record_kind = 'session' AND reason = 'cross_session'",
                )
                .bind(TEST_TENANT_ID)
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)
            })
            .expect("persist cross-session audit");
        assert_eq!(audit_count, 1);
        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_import_skips_unchanged_fingerprinted_sessions() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-import-skip-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "import-skip-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);
        let mut session = fixture_session("v1");
        session.source_fingerprint = Some("unchanged".to_string());

        let imported_at = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter)
                    .await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[session.clone()],
                    false,
                )
                .await?;
                sqlx::query(
                    "UPDATE conversation_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
                )
                .bind(&source.id)
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[session],
                    false,
                )
                .await?;
                sqlx::query_scalar::<_, String>(
                    "SELECT imported_at FROM conversation_sessions WHERE source_id = ?1",
                )
                .bind(&source.id)
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)
            })
            .expect("import unchanged fingerprinted session through SQLx");

        assert_eq!(imported_at, "preserved");

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_import_rewrites_session_when_normalized_parts_change() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-import-refresh-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "import-refresh-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);
        let mut old_session = fixture_session("v1");
        old_session.source_fingerprint = Some("same-source".to_string());
        old_session.turns[0].parts[0].metadata_json = None;
        let mut refreshed_session = fixture_session("v1");
        refreshed_session.source_fingerprint = Some("same-source".to_string());
        refreshed_session.turns[0].parts[0].metadata_json =
            Some(r#"{"content_card":{"type":"answer","format":"markdown"}}"#.to_string());

        let (result, imported_at, metadata_json, part_ids_before, part_ids_after) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter)
                    .await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[old_session],
                    false,
                )
                .await?;
                let part_ids_before = sqlx::query_as::<_, (String, i64)>(
                    r#"
                    SELECT p.id, p.part_index
                    FROM conversation_parts p
                    JOIN conversation_turns t ON t.id = p.turn_id
                    JOIN conversation_sessions s ON s.id = t.session_id
                    WHERE s.source_id = ?1
                    ORDER BY t.turn_index ASC, p.part_index ASC
                    "#,
                )
                .bind(&source.id)
                .fetch_all(database.pool())
                .await
                .map_err(AppError::external)?;
                sqlx::query(
                    "UPDATE conversation_sessions SET imported_at = 'preserved' WHERE source_id = ?1",
                )
                .bind(&source.id)
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                let result = import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[refreshed_session],
                    false,
                )
                .await?;
                let imported_at = sqlx::query_scalar::<_, String>(
                    "SELECT imported_at FROM conversation_sessions WHERE source_id = ?1",
                )
                .bind(&source.id)
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                let metadata_json = sqlx::query_scalar::<_, Option<String>>(
                    r#"
                    SELECT p.metadata_json
                    FROM conversation_parts p
                    JOIN conversation_turns t ON t.id = p.turn_id
                    JOIN conversation_sessions s ON s.id = t.session_id
                    WHERE s.source_id = ?1
                    ORDER BY p.part_index ASC
                    LIMIT 1
                    "#,
                )
                .bind(&source.id)
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                let part_ids_after = sqlx::query_as::<_, (String, i64)>(
                    r#"
                    SELECT p.id, p.part_index
                    FROM conversation_parts p
                    JOIN conversation_turns t ON t.id = p.turn_id
                    JOIN conversation_sessions s ON s.id = t.session_id
                    WHERE s.source_id = ?1
                    ORDER BY t.turn_index ASC, p.part_index ASC
                    "#,
                )
                .bind(&source.id)
                .fetch_all(database.pool())
                .await
                .map_err(AppError::external)?;
                Ok::<_, AppError>((
                    result,
                    imported_at,
                    metadata_json,
                    part_ids_before,
                    part_ids_after,
                ))
            })
            .expect("refresh normalized parts through SQLx");

        assert_eq!(result.skipped_session_count, 0);
        assert_ne!(imported_at, "preserved");
        assert_eq!(part_ids_after, part_ids_before);
        assert!(!part_ids_after.is_empty());
        assert!(metadata_json
            .as_deref()
            .unwrap_or("")
            .contains(r#""content_card""#));

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_import_prunes_turns_removed_by_external_adapter() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-import-prune-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "import-prune-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);
        let mut pruned_session = fixture_session("v2");
        pruned_session.turns.truncate(1);
        pruned_session.source_fingerprint = Some("pruned-source".to_string());

        let (detail, stale_parts) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session("v2")],
                    false,
                )
                .await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[pruned_session],
                    false,
                )
                .await?;
                let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
                let detail = load_conversation_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &session_id,
                )
                .await?;
                let stale_parts = sqlx::query_scalar::<_, i64>(
                    r#"
                    SELECT COUNT(*)
                    FROM conversation_parts p
                    JOIN conversation_turns t ON t.id = p.turn_id
                    WHERE t.session_id = ?1
                      AND t.external_id IN ('t2', 't3')
                    "#,
                )
                .bind(&session_id)
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                Ok::<_, AppError>((detail, stale_parts))
            })
            .expect("prune stale turns through SQLx");

        assert_eq!(detail.questions.len(), 1);
        assert_eq!(detail.questions[0].turns.len(), 1);
        assert_eq!(detail.questions[0].turns[0].external_id, "t1");
        assert_eq!(detail.questions[0].parts.len(), 2);
        assert_eq!(stale_parts, 0);

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_import_marks_sessions_missing_when_external_adapter_omits_them() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-import-missing-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "import-missing-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);
        let current_session = fixture_session("v1");
        let mut removed_session = fixture_session("v1");
        removed_session.external_id = "removed-session".to_string();
        removed_session.title = Some("Removed fixture".to_string());

        let (listed, missing_count) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter)
                    .await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[current_session.clone(), removed_session],
                    false,
                )
                .await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[current_session],
                    false,
                )
                .await?;
                let listed = list_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    None,
                    20,
                    0,
                )
                .await?;
                let missing_count = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM conversation_sessions WHERE source_id = ?1 AND missing = 1",
                )
                .bind(&source.id)
                .fetch_one(database.pool())
                .await
                .map_err(AppError::external)?;
                Ok::<_, AppError>((listed, missing_count))
            })
            .expect("mark omitted sessions missing through SQLx");

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].session.external_id, "session-1");
        assert_eq!(missing_count, 1);

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_incremental_import_preserves_and_recovers_discovered_sessions() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-import-incremental-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "import-incremental-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);
        let current_session = fixture_session("v1");
        let mut retained_session = fixture_session("v1");
        retained_session.external_id = "retained-session".to_string();
        retained_session.title = Some("Retained fixture".to_string());
        let discovered_external_ids = BTreeSet::from([
            current_session.external_id.clone(),
            retained_session.external_id.clone(),
        ]);

        let listed = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[current_session, retained_session],
                    false,
                )
                .await?;

                // Reproduce the damaged state created by the old incremental path.
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[],
                    false,
                )
                .await?;

                import_incremental_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[],
                    &discovered_external_ids,
                    false,
                )
                .await?;
                list_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    None,
                    20,
                    0,
                )
                .await
            })
            .expect("preserve discovered sessions during incremental import");

        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|item| !item.session.missing));

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_conversation_reads_and_filters_questions() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-read-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "read-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);
        let mut session = fixture_session("v2");
        session.turns[2].parts.push(NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Command,
            text: None,
            language: None,
            command: Some("assetiweave-cli conversation session export".to_string()),
            cwd: Some("/tmp/project".to_string()),
            status: Some("completed".to_string()),
            exit_code: Some(0),
            command_label: None,
            source_execution_id: Some("call-export".to_string()),
            content_card: None,
            metadata_json: content_card_metadata("command"),
        });
        session.turns[2].parts.push(NormalizedConversationPart {
            role: ConversationPartRole::Tool,
            kind: ConversationPartKind::Tool,
            text: Some("tests passed".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: Some("completed".to_string()),
            exit_code: Some(0),
            command_label: None,
            source_execution_id: Some("call-export".to_string()),
            content_card: None,
            metadata_json: content_card_metadata("result"),
        });

        let (sessions, detail, filtered_questions, question) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[session],
                    false,
                )
                .await?;
                let sessions = list_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    Some("answer for t3"),
                    20,
                    0,
                )
                .await?;
                let detail = load_conversation_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &sessions[0].session.id,
                )
                .await?;
                let filtered_questions = list_conversation_question_details_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &sessions[0].session.id,
                    Some("answer for t3"),
                    20,
                    0,
                )
                .await?;
                let question = load_conversation_question_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &filtered_questions[0].question.id,
                )
                .await?;
                Ok::<_, AppError>((sessions, detail, filtered_questions, question))
            })
            .expect("read conversations through SQLx");

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].question_count, 2);
        assert_eq!(sessions[0].turn_count, 3);
        assert_eq!(detail.questions.len(), 2);
        assert_eq!(filtered_questions.len(), 1);
        assert_eq!(filtered_questions[0].question.question_text, "Export it");
        assert_eq!(question.turns.len(), 1);
        assert_eq!(question.parts.len(), 3);
        assert_eq!(
            question.parts[1].source_execution_id.as_deref(),
            Some("call-export")
        );
        assert_eq!(
            question.parts[2].source_execution_id.as_deref(),
            Some("call-export")
        );
        let serialized = serde_json::to_value(&question).expect("serialize question detail");
        let nodes = serialized["content_nodes"]
            .as_array()
            .expect("content nodes");
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0]["type"], "card");
        assert_eq!(nodes[0]["card_index"], 0);
        assert_eq!(nodes[1]["type"], "execution");
        assert_eq!(nodes[1]["source_execution_id"], "call-export");
        assert_eq!(nodes[1]["command_card_index"], serde_json::json!(1));
        assert_eq!(nodes[1]["result_card_indices"], serde_json::json!([2]));
        let projected_nodes = serialized["projected_content_nodes"]
            .as_array()
            .expect("projected content nodes");
        assert_eq!(projected_nodes.len(), 3);
        assert_eq!(projected_nodes[0]["question_id"], question.question.id);
        assert_eq!(projected_nodes[0]["turn_id"], question.turns[0].id);
        assert_eq!(projected_nodes[0]["part_id"], question.parts[0].id);
        assert_eq!(projected_nodes[0]["node_order"], 0);
        assert_eq!(
            projected_nodes[0]["locator"]["part_id"],
            question.parts[0].id
        );
        assert_eq!(projected_nodes[1]["source_execution_id"], "call-export");
        assert_eq!(projected_nodes[2]["source_execution_id"], "call-export");

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_conversation_lists_sessions_by_display_id_fragment() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-id-fragment-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "id-fragment-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);

        let (fragment_matches, direct_fragment_matches, full_matches) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session("v1")],
                    false,
                )
                .await?;
                let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
                let fragment = crate::backend::models::conversation_id_fragment(&session_id);
                let collision_id = format!("conversation-session-{fragment}{}", "0".repeat(56));
                sqlx::query(
                    r#"
                    INSERT INTO conversation_sessions (
                        tenant_id, id, source_id, adapter_id, external_id, title, project_path,
                        started_at, updated_at, source_locator, source_fingerprint, missing,
                        created_at, imported_at
                    )
                    SELECT tenant_id, ?1, source_id, adapter_id, 'fragment-collision',
                           'Fragment collision', project_path, started_at, updated_at,
                           source_locator, source_fingerprint, missing, created_at, imported_at
                    FROM conversation_sessions
                    WHERE tenant_id = ?2 AND id = ?3
                    "#,
                )
                .bind(&collision_id)
                .bind(TEST_TENANT_ID)
                .bind(&session_id)
                .execute(database.pool())
                .await
                .map_err(AppError::external)?;
                let fragment_matches = list_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    Some(&fragment),
                    20,
                    0,
                )
                .await?;
                let direct_fragment_matches = list_conversation_sessions_by_id_fragment_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    ConversationRecordKind::Session,
                    None,
                    Some(&source.id),
                    &fragment,
                    20,
                    0,
                )
                .await?;
                let full_matches = list_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    Some(&session_id),
                    20,
                    0,
                )
                .await?;
                Ok::<_, AppError>((fragment_matches, direct_fragment_matches, full_matches))
            })
            .expect("list conversation sessions by display id fragment");

        assert_eq!(fragment_matches.len(), 2);
        assert_eq!(direct_fragment_matches.len(), 2);
        assert_eq!(full_matches.len(), 1);
        assert_eq!(full_matches[0].session.external_id, "session-1");

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_merge_and_split_conversation_questions_preserve_grouping() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-mutation-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "mutation-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);

        let detail = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session("v1")],
                    false,
                )
                .await?;
                let detail = load_conversation_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &stable_id("conversation-session", &[&source.id, "session-1"]),
                )
                .await?;
                let question_ids = detail
                    .questions
                    .iter()
                    .map(|question| question.question.id.clone())
                    .collect::<Vec<_>>();
                let dry_run = merge_conversation_questions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &question_ids,
                    true,
                )
                .await?;
                assert!(dry_run.dry_run);
                assert_eq!(
                    load_conversation_session_detail_sqlx(
                        database.pool(),
                        TEST_TENANT_ID,
                        &detail.session.id,
                    )
                    .await?
                    .questions
                    .len(),
                    2
                );
                let merged = merge_conversation_questions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &question_ids,
                    false,
                )
                .await?;
                let first_turn_error = split_conversation_question_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &merged.questions[0].question.id,
                    &merged.questions[0].turns[0].id,
                    false,
                )
                .await
                .expect_err("split at first turn should fail");
                assert!(first_turn_error.contains("must not be the first turn"));
                let split_turn_id = merged.questions[0].turns[2].id.clone();
                split_conversation_question_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &merged.questions[0].question.id,
                    &split_turn_id,
                    false,
                )
                .await?;
                load_conversation_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &detail.session.id,
                )
                .await
            })
            .expect("merge and split through SQLx");

        assert_eq!(detail.questions.len(), 2);
        assert_eq!(detail.questions[0].turns.len(), 2);
        assert_eq!(detail.questions[1].turns.len(), 1);
        assert!(detail.questions.iter().all(|question| {
            question.question.grouping_origin == ConversationGroupingOrigin::Manual
        }));
        assert_eq!(
            detail.questions[0].question.question_text,
            "How does sync work?\n\n继续"
        );
        assert_eq!(detail.questions[1].question.question_text, "Export it");

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_searches_session_and_web_conversation_cards() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-search-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let session_adapter = test_conversation_adapter(
            "search-session-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let web_adapter = test_conversation_adapter(
            "search-web-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let session_source = test_conversation_source(&session_adapter.id);
        let mut web_source = test_conversation_source(&web_adapter.id);
        web_source.id = "search-web-source".to_string();
        let mut session = fixture_session("v1");
        session.started_at = Some("2026-03-02T10:00:00Z".to_string());
        let mut web_session = fixture_session("v1");
        web_session.external_id = "web-session".to_string();
        web_session.started_at = Some("2026-04-02T10:00:00Z".to_string());

        let (session_page, web_page, fragment_page, fragment_session_ids) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &session_adapter)
                    .await?;
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &web_adapter)
                    .await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &session_source)
                    .await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &web_source)
                    .await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &session_source,
                    &[session],
                    false,
                )
                .await?;
                super::super::web_record_repo::import_web_record_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &web_source,
                    &[web_session],
                    false,
                )
                .await?;
                let session_page = search_conversation_cards_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    ConversationRecordKind::Session,
                    Some(&session_adapter.id),
                    Some(&session_source.id),
                    Some("/tmp/project"),
                    "answer for t1",
                    &[ConversationSearchCardType::answer()],
                    &[],
                    false,
                    true,
                    Some("2026-03-01"),
                    Some("2026-03-31"),
                    true,
                    20,
                    0,
                    None,
                )
                .await?;
                let web_page = search_conversation_cards_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    ConversationRecordKind::Web,
                    Some(&web_adapter.id),
                    Some(&web_source.id),
                    None,
                    "answer for t3",
                    &[ConversationSearchCardType::answer()],
                    &[],
                    false,
                    true,
                    None,
                    None,
                    false,
                    20,
                    0,
                    None,
                )
                .await?;
                let session_id =
                    stable_id("conversation-session", &[&session_source.id, "session-1"]);
                let fragment = crate::backend::models::conversation_id_fragment(&session_id);
                let fragment_page = search_conversation_cards_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    ConversationRecordKind::Session,
                    Some(&session_adapter.id),
                    Some(&session_source.id),
                    Some("/tmp/project"),
                    &fragment,
                    &[],
                    &[],
                    false,
                    true,
                    None,
                    None,
                    false,
                    20,
                    0,
                    None,
                )
                .await?;
                let fragment_session_ids = load_search_session_ids_by_id_fragment_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    ConversationRecordKind::Session.tables(),
                    &fragment,
                )
                .await?;
                Ok::<_, AppError>((session_page, web_page, fragment_page, fragment_session_ids))
            })
            .expect("search session and web records through SQLx");

        assert_eq!(session_page.total_count, 1);
        assert_eq!(
            session_page.hits[0].session.session.source_id,
            session_source.id
        );
        assert_eq!(
            session_page.hits[0].card_type,
            ConversationSearchCardType::answer()
        );
        assert_eq!(web_page.total_count, 1);
        assert_eq!(web_page.hits[0].session.session.source_id, web_source.id);
        assert_eq!(
            web_page.hits[0].card_type,
            ConversationSearchCardType::answer()
        );
        assert!(fragment_page.total_count > 0);
        assert_eq!(fragment_session_ids.len(), 1);
        assert!(fragment_page.hits.iter().all(|hit| hit.session.session.id
            == stable_id("conversation-session", &[&session_source.id, "session-1"])));
        assert!(fragment_page
            .hits
            .iter()
            .all(|hit| hit.highlight_segments.is_none()));

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_search_and_aggregates_only_declared_content_cards() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-declared-cards-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "declared-card-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);
        let mut undeclared_turn = fixture_turn("undeclared", 0, "First question");
        undeclared_turn.parts[0].text = Some("undeclared answer needle".to_string());
        undeclared_turn.parts[0].metadata_json = None;
        let mut declared_turn = fixture_turn("declared", 1, "Second question");
        declared_turn.parts[0].text = Some("declared answer needle".to_string());
        declared_turn.parts[0].metadata_json = content_card_metadata("answer");
        let session = NormalizedConversationSession {
            external_id: "declared-card-session".to_string(),
            title: Some("Declared card fixture".to_string()),
            project_path: Some("/tmp/project".to_string()),
            started_at: None,
            updated_at: None,
            source_locator: None,
            source_fingerprint: None,
            turns: vec![undeclared_turn, declared_turn],
        };

        let (detail, undeclared_list, undeclared_page, declared_page) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[session],
                    false,
                )
                .await?;
                let sessions = list_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    None,
                    20,
                    0,
                )
                .await?;
                let detail = load_conversation_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &sessions[0].session.id,
                )
                .await?;
                let undeclared_list = list_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    None,
                    Some(&source.id),
                    Some("undeclared answer"),
                    20,
                    0,
                )
                .await?;
                let undeclared_page = search_conversation_cards_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    ConversationRecordKind::Session,
                    Some(&adapter.id),
                    Some(&source.id),
                    None,
                    "undeclared answer",
                    &[ConversationSearchCardType::answer()],
                    &[],
                    false,
                    true,
                    None,
                    None,
                    false,
                    20,
                    0,
                    None,
                )
                .await?;
                let declared_page = search_conversation_cards_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    ConversationRecordKind::Session,
                    Some(&adapter.id),
                    Some(&source.id),
                    None,
                    "declared answer",
                    &[ConversationSearchCardType::answer()],
                    &[],
                    false,
                    true,
                    None,
                    None,
                    false,
                    20,
                    0,
                    None,
                )
                .await?;
                Ok::<_, AppError>((detail, undeclared_list, undeclared_page, declared_page))
            })
            .expect("search declared content cards through SQLx");

        assert_eq!(detail.questions[0].question.answer_text, "");
        assert_eq!(
            detail.questions[1].question.answer_text,
            "declared answer needle"
        );
        assert!(undeclared_list.is_empty());
        assert_eq!(undeclared_page.total_count, 0);
        assert_eq!(declared_page.total_count, 1);

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_conversation_records_are_isolated_by_tenant() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-tenant-isolation-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let tenant_alpha = "tenant-alpha";
        let tenant_beta = "tenant-beta";
        let adapter = test_conversation_adapter(
            "tenant-isolation-adapter",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);
        let mut alpha_session = fixture_session("v1");
        alpha_session.turns[0].parts[0].text = Some("alpha tenant answer".to_string());
        let mut beta_session = fixture_session("v1");
        beta_session.turns[0].parts[0].text = Some("beta tenant answer".to_string());

        let (session_id, alpha_detail, beta_detail, alpha_page, beta_page) = database
            .block_on(async {
                for tenant_id in [tenant_alpha, tenant_beta] {
                    upsert_conversation_adapter_sqlx(database.pool(), tenant_id, &adapter).await?;
                    upsert_conversation_source_sqlx(database.pool(), tenant_id, &source).await?;
                }
                import_conversation_sessions_sqlx(
                    database.pool(),
                    tenant_alpha,
                    &source,
                    &[alpha_session],
                    false,
                )
                .await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    tenant_beta,
                    &source,
                    &[beta_session],
                    false,
                )
                .await?;

                let alpha_sessions = list_conversation_sessions_sqlx(
                    database.pool(),
                    tenant_alpha,
                    None,
                    Some(&source.id),
                    Some("alpha tenant"),
                    20,
                    0,
                )
                .await?;
                let beta_sessions = list_conversation_sessions_sqlx(
                    database.pool(),
                    tenant_beta,
                    None,
                    Some(&source.id),
                    Some("beta tenant"),
                    20,
                    0,
                )
                .await?;
                let session_id = alpha_sessions[0].session.id.clone();
                assert_eq!(beta_sessions[0].session.id, session_id);
                let alpha_detail = load_conversation_session_detail_sqlx(
                    database.pool(),
                    tenant_alpha,
                    &session_id,
                )
                .await?;
                let beta_detail = load_conversation_session_detail_sqlx(
                    database.pool(),
                    tenant_beta,
                    &session_id,
                )
                .await?;
                let alpha_page = search_conversation_cards_sqlx(
                    database.pool(),
                    tenant_alpha,
                    ConversationRecordKind::Session,
                    Some(&adapter.id),
                    Some(&source.id),
                    None,
                    "beta tenant answer",
                    &[ConversationSearchCardType::answer()],
                    &[],
                    false,
                    true,
                    None,
                    None,
                    false,
                    20,
                    0,
                    None,
                )
                .await?;
                let beta_page = search_conversation_cards_sqlx(
                    database.pool(),
                    tenant_beta,
                    ConversationRecordKind::Session,
                    Some(&adapter.id),
                    Some(&source.id),
                    None,
                    "alpha tenant answer",
                    &[ConversationSearchCardType::answer()],
                    &[],
                    false,
                    true,
                    None,
                    None,
                    false,
                    20,
                    0,
                    None,
                )
                .await?;
                Ok::<_, AppError>((session_id, alpha_detail, beta_detail, alpha_page, beta_page))
            })
            .expect("isolate conversation records by tenant");

        assert_eq!(alpha_detail.session.id, session_id);
        assert_eq!(beta_detail.session.id, session_id);
        assert!(alpha_detail.questions[0]
            .question
            .answer_text
            .contains("alpha tenant answer"));
        assert!(!alpha_detail.questions[0]
            .question
            .answer_text
            .contains("beta tenant answer"));
        assert!(beta_detail.questions[0]
            .question
            .answer_text
            .contains("beta tenant answer"));
        assert_eq!(alpha_page.total_count, 0);
        assert_eq!(beta_page.total_count, 0);

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_round_trips_structured_cards_and_preserves_translation_on_reclassification() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-card-persistence-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let mut adapter = test_conversation_adapter(
            "fixture-cards",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        adapter.card_contract_version = Some(1);
        adapter.card_kinds = vec![
            ConversationCardKindDefinition {
                id: "fixture-cards.reasoning".to_string(),
                semantic_role: Some("reasoning".to_string()),
                label: "Reasoning".to_string(),
                default_renderer: "markdown".to_string(),
                allowed_renderers: vec!["markdown".to_string()],
                icon_hint: Some("brain".to_string()),
            },
            ConversationCardKindDefinition {
                id: "fixture-cards.analysis".to_string(),
                semantic_role: Some("reasoning".to_string()),
                label: "Analysis".to_string(),
                default_renderer: "markdown".to_string(),
                allowed_renderers: vec!["markdown".to_string()],
                icon_hint: None,
            },
        ];
        let source = test_conversation_source(&adapter.id);
        let mut first = fixture_session("v1");
        first.turns[0].parts[0].content_card = Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "fixture-cards.reasoning".to_string(),
            renderer: Some("markdown".to_string()),
        });
        first.turns[0].parts[0].command_label = Some("DEBUG".to_string());
        let mut second = first.clone();
        second.turns[0].parts[0].content_card.as_mut().unwrap().kind =
            "fixture-cards.analysis".to_string();

        let (stored_adapter, part_id, detail) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[first],
                    false,
                )
                .await?;
                let session_id = stable_id("conversation-session", &[&source.id, "session-1"]);
                let initial = load_conversation_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &session_id,
                )
                .await?;
                let part_id = initial.questions[0].parts[0].id.clone();
                update_conversation_part_translation_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &part_id,
                    "译文",
                )
                .await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[second],
                    false,
                )
                .await?;
                let detail = load_conversation_session_detail_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &session_id,
                )
                .await?;
                let stored_adapter =
                    load_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter.id)
                        .await?
                        .expect("stored adapter");
                Ok::<_, AppError>((stored_adapter, part_id, detail))
            })
            .expect("round trip structured card");

        let part = &detail.questions[0].parts[0];
        assert_eq!(part.id, part_id);
        assert_eq!(part.translated_text.as_deref(), Some("译文"));
        assert_eq!(part.command_label.as_deref(), Some("DEBUG"));
        assert_eq!(
            part.content_card.as_ref().map(|card| card.kind.as_str()),
            Some("fixture-cards.analysis")
        );
        let projected = detail.questions[0]
            .cards
            .iter()
            .find(|card| card.card_id == part_id)
            .expect("projected structured card");
        assert_eq!(projected.semantic_role.as_deref(), Some("reasoning"));
        assert_eq!(projected.command_label.as_deref(), Some("DEBUG"));
        assert_eq!(stored_adapter.card_contract_version, Some(1));
        assert_eq!(stored_adapter.card_kinds, adapter.card_kinds);

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_adapter_or_card_contract_change_invalidates_incremental_hydration_versions() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-card-hydration-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let descriptors = vec![
            crate::backend::conversations::ConversationSessionDescriptor {
                external_id: "session-1".to_string(),
                updated_at: None,
                source_locator: None,
                version_token: "source-v1".to_string(),
            },
        ];
        let hydrated = BTreeSet::from(["session-1".to_string()]);

        let (same, adapter_changed, contract_changed, payload_policy_changed) = database
            .block_on(async {
                persist_conversation_session_observations_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    "source-1",
                    ConversationRecordKind::Session,
                    &descriptors,
                    &hydrated,
                    Some("adapter-hash-v1"),
                    Some(1),
                    crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                )
                .await?;
                let same = load_conversation_session_versions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    "source-1",
                    ConversationRecordKind::Session,
                    Some("adapter-hash-v1"),
                    Some(1),
                    crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                )
                .await?;
                let adapter_changed = load_conversation_session_versions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    "source-1",
                    ConversationRecordKind::Session,
                    Some("adapter-hash-v2"),
                    Some(1),
                    crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                )
                .await?;
                let contract_changed = load_conversation_session_versions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    "source-1",
                    ConversationRecordKind::Session,
                    Some("adapter-hash-v1"),
                    Some(2),
                    crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                )
                .await?;
                let payload_policy_changed = load_conversation_session_versions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    "source-1",
                    ConversationRecordKind::Session,
                    Some("adapter-hash-v1"),
                    Some(1),
                    crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION + 1,
                )
                .await?;
                Ok::<_, AppError>((
                    same,
                    adapter_changed,
                    contract_changed,
                    payload_policy_changed,
                ))
            })
            .expect("compare hydration identity");

        assert_eq!(same.get("session-1").map(String::as_str), Some("source-v1"));
        assert!(adapter_changed.is_empty());
        assert!(contract_changed.is_empty());
        assert!(payload_policy_changed.is_empty());

        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn sqlx_payload_policy_state_requests_one_reparse_for_existing_records() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-payload-policy-state-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = Database::open(&db_path).expect("open database");
        let adapter = test_conversation_adapter(
            "payload-policy-state-external",
            ConversationAdapterKind::External,
            ConversationAdapterTrustState::Trusted,
        );
        let source = test_conversation_source(&adapter.id);

        let (required_before, required_after) = database
            .block_on(async {
                upsert_conversation_adapter_sqlx(database.pool(), TEST_TENANT_ID, &adapter).await?;
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                import_conversation_sessions_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    &source,
                    &[fixture_session("v1")],
                    false,
                )
                .await?;
                let required_before = conversation_payload_policy_reparse_required_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                )
                .await?;
                mark_conversation_payload_policy_applied_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                )
                .await?;
                let required_after = conversation_payload_policy_reparse_required_sqlx(
                    database.pool(),
                    TEST_TENANT_ID,
                    crate::backend::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION,
                )
                .await?;
                Ok::<_, AppError>((required_before, required_after))
            })
            .expect("track payload policy reparse state");

        assert!(required_before);
        assert!(!required_after);

        drop(database);
        cleanup_database(&db_path);
    }

    fn fixture_session(version: &str) -> NormalizedConversationSession {
        let mut turns = vec![
            fixture_turn("t1", 0, "How does sync work?"),
            fixture_turn("t2", 1, "继续"),
            fixture_turn("t3", 2, "Export it"),
        ];
        if version == "v2" {
            turns[0].parts.push(NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::CodeBlock,
                text: Some("cargo test".to_string()),
                language: Some("sh".to_string()),
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: content_card_metadata("code"),
            });
        }
        NormalizedConversationSession {
            external_id: "session-1".to_string(),
            title: Some("Fixture".to_string()),
            project_path: Some("/tmp/project".to_string()),
            started_at: None,
            updated_at: None,
            source_locator: None,
            source_fingerprint: None,
            turns,
        }
    }

    fn fixture_turn(id: &str, index: i64, user_text: &str) -> NormalizedConversationTurn {
        NormalizedConversationTurn {
            external_id: id.to_string(),
            turn_index: index,
            user_text: user_text.to_string(),
            title: None,
            started_at: None,
            ended_at: None,
            parts: vec![NormalizedConversationPart {
                role: ConversationPartRole::Assistant,
                kind: ConversationPartKind::Text,
                text: Some(format!("answer for {id}")),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
                command_label: None,
                source_execution_id: None,
                content_card: None,
                metadata_json: content_card_metadata("answer"),
            }],
        }
    }

    fn content_card_metadata(card_type: &str) -> Option<String> {
        Some(format!(
            r#"{{"content_card":{{"type":"{card_type}","format":"markdown"}}}}"#
        ))
    }

    fn test_conversation_adapter(
        id: &str,
        kind: ConversationAdapterKind,
        trust_state: ConversationAdapterTrustState,
    ) -> ConversationAdapter {
        ConversationAdapter {
            id: id.to_string(),
            name: id.to_string(),
            kind,
            version: "1.0.0".to_string(),
            enabled: true,
            manifest_path: Some(format!("/tmp/{id}/manifest.json")),
            executable_path: Some(format!("/tmp/{id}/adapter")),
            content_hash: Some(format!("{id}-hash")),
            trusted_hash: Some(format!("{id}-hash")),
            trust_state,
            protocol_version: Some(1),
            capabilities: vec!["read".to_string()],
            input_kinds: vec![ConversationSourceKind::Directory],
            card_contract_version: None,
            card_kinds: Vec::new(),
            created_at: "2026-06-19T00:00:00Z".to_string(),
            updated_at: "2026-06-19T00:00:00Z".to_string(),
        }
    }

    fn test_conversation_source(adapter_id: &str) -> ConversationSource {
        ConversationSource {
            id: format!("{adapter_id}-source"),
            adapter_id: adapter_id.to_string(),
            name: format!("{adapter_id} source"),
            kind: ConversationSourceKind::Directory,
            location: format!("/tmp/{adapter_id}/sessions"),
            config_json: Some("{\"mode\":\"test\"}".to_string()),
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: "2026-06-19T00:00:00Z".to_string(),
            updated_at: "2026-06-19T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn conversation_source_locations_normalize_absolute_home_paths() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-conversation-source-home-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");
        let mut source = test_conversation_source("codex");
        source.location = dirs::home_dir()
            .expect("home directory")
            .join(".codex")
            .to_string_lossy()
            .to_string();

        let loaded = database
            .block_on(async {
                upsert_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source).await?;
                load_conversation_source_sqlx(database.pool(), TEST_TENANT_ID, &source.id).await
            })
            .expect("round trip conversation source")
            .expect("stored source");

        assert_eq!(loaded.location, "~/.codex");
        drop(database);
        cleanup_database(&db_path);
    }

    fn cleanup_database(db_path: &std::path::Path) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
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
                    hydrated_payload_policy_version
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10, ?11)
                ON CONFLICT(tenant_id, source_id, record_kind, external_id) DO UPDATE SET
                    observed_version = excluded.observed_version,
                    hydrated_version = excluded.hydrated_version,
                    last_seen_at = excluded.last_seen_at,
                    source_presence = excluded.source_presence,
                    hydrated_adapter_hash = excluded.hydrated_adapter_hash,
                    hydrated_card_contract_version = excluded.hydrated_card_contract_version,
                    hydrated_payload_policy_version = excluded.hydrated_payload_policy_version,
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
