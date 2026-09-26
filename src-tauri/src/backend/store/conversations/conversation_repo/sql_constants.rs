use super::*;

pub(super) const LIST_CONVERSATION_ADAPTERS_SQL: &str = r#"
    SELECT id, name, kind, version, enabled, manifest_path, executable_path,
           content_hash, trusted_hash, trust_state, protocol_version,
           capabilities, input_kinds, card_contract_version, card_kinds_json,
           created_at, updated_at
    FROM conversation_adapters
    WHERE tenant_id = ?1
    ORDER BY kind ASC, name ASC
    "#;

pub(super) const LOAD_CONVERSATION_ADAPTER_SQL: &str = r#"
    SELECT id, name, kind, version, enabled, manifest_path, executable_path,
           content_hash, trusted_hash, trust_state, protocol_version,
           capabilities, input_kinds, card_contract_version, card_kinds_json,
           created_at, updated_at
    FROM conversation_adapters
    WHERE tenant_id = ?1 AND id = ?2
    "#;

pub(super) const UPSERT_CONVERSATION_ADAPTER_SQL: &str = r#"
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

pub(super) const DELETE_CONVERSATION_ADAPTER_SQL: &str =
    "DELETE FROM conversation_adapters WHERE tenant_id = ?1 AND id = ?2";

pub(super) const DISABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL: &str =
    "UPDATE conversation_sources SET enabled = 0, updated_at = ?1 WHERE tenant_id = ?2 AND adapter_id = ?3";
pub(super) const DISABLE_CONVERSATION_ADAPTER_SQL: &str =
    "UPDATE conversation_adapters SET enabled = 0, updated_at = ?1 WHERE tenant_id = ?2 AND id = ?3";
pub(super) const ENABLE_CONVERSATION_SOURCES_BY_ADAPTER_SQL: &str =
    "UPDATE conversation_sources SET enabled = 1, updated_at = ?1 WHERE tenant_id = ?2 AND adapter_id = ?3";

pub(super) const LIST_CONVERSATION_ADAPTER_PACKAGES_SQL: &str = r#"
    SELECT package_id, adapter_id, name, version, record_kind, install_dir,
           manifest_path, adapter_manifest_path, runtime_protocol, runtime_ready,
           origin, source_url, git_ref, git_commit, catalog_url, update_policy,
           latest_version, last_checked_at, runtime_gate_status, runtime_validated_at,
           installed_content_hash, trusted_package_hash, error_message,
           created_at, updated_at
    FROM app_conversation_adapter_packages
    ORDER BY name ASC, package_id ASC
    "#;

pub(super) const LOAD_CONVERSATION_ADAPTER_PACKAGE_SQL: &str = r#"
    SELECT package_id, adapter_id, name, version, record_kind, install_dir,
           manifest_path, adapter_manifest_path, runtime_protocol, runtime_ready,
           origin, source_url, git_ref, git_commit, catalog_url, update_policy,
           latest_version, last_checked_at, runtime_gate_status, runtime_validated_at,
           installed_content_hash, trusted_package_hash, error_message,
           created_at, updated_at
    FROM app_conversation_adapter_packages
    WHERE package_id = ?1
    "#;

pub(super) const LOAD_CONVERSATION_ADAPTER_PACKAGE_BY_ADAPTER_SQL: &str = r#"
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

pub(super) const UPSERT_CONVERSATION_ADAPTER_PACKAGE_SQL: &str = r#"
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

pub(super) const DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL: &str =
    "DELETE FROM app_conversation_adapter_packages WHERE package_id = ?1";

pub(super) const LIST_CONVERSATION_SOURCES_SQL: &str = r#"
    SELECT id, adapter_id, name, kind, location, config_json, enabled,
           last_synced_at, last_sync_status, created_at, updated_at
    FROM conversation_sources
    WHERE tenant_id = ?1
    ORDER BY adapter_id ASC, name ASC
    "#;

pub(super) const LOAD_CONVERSATION_SOURCE_SQL: &str = r#"
    SELECT id, adapter_id, name, kind, location, config_json, enabled,
           last_synced_at, last_sync_status, created_at, updated_at
    FROM conversation_sources
    WHERE tenant_id = ?1 AND id = ?2
    "#;

pub(super) const UPSERT_CONVERSATION_SOURCE_SQL: &str = r#"
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
    pub(crate) session_failures: Vec<crate::backend::domain::SessionSyncFailure>,
    #[serde(default)]
    pub(crate) session_warnings: Vec<crate::backend::domain::SessionSyncWarning>,
    #[serde(default)]
    pub(crate) status: crate::backend::domain::ConversationSyncStatus,
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct ConversationSyncDelta {
    pub(crate) sync_run_id: String,
    pub(crate) session_id: String,
    pub(crate) change_kind: String,
    pub(crate) observed_at: String,
}
