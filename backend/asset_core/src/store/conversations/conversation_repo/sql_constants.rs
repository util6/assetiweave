use super::*;

pub(super) const LIST_CONVERSATION_ADAPTERS_SQL: &str = r#"
    SELECT id, name, kind, version, enabled, manifest_path, executable_path,
           content_hash, trusted_hash, trust_state, protocol_version,
           capabilities, input_kinds, card_contract_version, card_kinds_json,
           projection_version, created_at, updated_at
    FROM conversation_adapters
    WHERE tenant_id = ?1
    ORDER BY kind ASC, name ASC
    "#;

pub(super) const LOAD_CONVERSATION_ADAPTER_SQL: &str = r#"
    SELECT id, name, kind, version, enabled, manifest_path, executable_path,
           content_hash, trusted_hash, trust_state, protocol_version,
           capabilities, input_kinds, card_contract_version, card_kinds_json,
           projection_version, created_at, updated_at
    FROM conversation_adapters
    WHERE tenant_id = ?1 AND id = ?2
    "#;

pub(super) const UPSERT_CONVERSATION_ADAPTER_SQL: &str = r#"
    INSERT INTO conversation_adapters (
        tenant_id, id, name, kind, version, enabled, manifest_path, executable_path,
        content_hash, trusted_hash, trust_state, protocol_version,
        capabilities, input_kinds, card_contract_version, card_kinds_json,
        projection_version, created_at, updated_at
    )
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
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
        projection_version = excluded.projection_version,
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
    SELECT extension_id, capability_kind, target_id, display_name, active_version,
           distribution_type, install_dir, enabled, installation_status, runtime_status,
           error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
    FROM app_extensions
    WHERE capability_kind = 'conversation.adapter'
    ORDER BY display_name ASC, extension_id ASC
    "#;

pub(super) const LOAD_CONVERSATION_ADAPTER_PACKAGE_SQL: &str = r#"
    SELECT extension_id, capability_kind, target_id, display_name, active_version,
           distribution_type, install_dir, enabled, installation_status, runtime_status,
           error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
    FROM app_extensions
    WHERE capability_kind = 'conversation.adapter' AND extension_id = ?1
    "#;

pub(super) const LOAD_CONVERSATION_ADAPTER_PACKAGE_BY_ADAPTER_SQL: &str = r#"
    SELECT extension_id, capability_kind, target_id, display_name, active_version,
           distribution_type, install_dir, enabled, installation_status, runtime_status,
           error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
    FROM app_extensions
    WHERE capability_kind = 'conversation.adapter' AND target_id = ?1
    ORDER BY updated_at DESC, extension_id ASC
    LIMIT 1
    "#;

pub(super) const UPSERT_CONVERSATION_ADAPTER_PACKAGE_SQL: &str = r#"
    INSERT INTO app_extensions (
        extension_id, capability_kind, target_id, display_name, active_version,
        distribution_type, install_dir, enabled, installation_status, runtime_status,
        error_code, error_message, checked_at, installed_at, updated_at, capability_meta_json
    ) VALUES (
        ?1, 'conversation.adapter', ?2, ?3, ?4,
        ?5, ?6, ?7, ?8, ?9,
        ?10, ?11, ?12, ?13, ?14, ?15
    )
    ON CONFLICT(extension_id) DO UPDATE SET
        capability_kind = excluded.capability_kind,
        target_id = excluded.target_id,
        display_name = excluded.display_name,
        active_version = excluded.active_version,
        distribution_type = excluded.distribution_type,
        install_dir = excluded.install_dir,
        enabled = excluded.enabled,
        installation_status = excluded.installation_status,
        runtime_status = excluded.runtime_status,
        error_code = excluded.error_code,
        error_message = excluded.error_message,
        checked_at = excluded.checked_at,
        installed_at = excluded.installed_at,
        updated_at = excluded.updated_at,
        capability_meta_json = excluded.capability_meta_json
    "#;

pub(super) const DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL: &str =
    "DELETE FROM app_extensions WHERE capability_kind = 'conversation.adapter' AND extension_id = ?1";

pub(super) const LIST_CONVERSATION_SOURCES_SQL: &str = r#"
    SELECT id, adapter_id, name, kind, location, config_json, enabled,
           last_synced_at, last_sync_status, created_at, updated_at,
           synced_adapter_hash, synced_card_contract_version,
           synced_payload_policy_version, synced_projection_version
    FROM conversation_sources
    WHERE tenant_id = ?1
    ORDER BY adapter_id ASC, name ASC
    "#;

pub(super) const LOAD_CONVERSATION_SOURCE_SQL: &str = r#"
    SELECT id, adapter_id, name, kind, location, config_json, enabled,
           last_synced_at, last_sync_status, created_at, updated_at,
           synced_adapter_hash, synced_card_contract_version,
           synced_payload_policy_version, synced_projection_version
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
pub struct ConversationImportResult {
    pub source_id: String,
    pub adapter_id: String,
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_run_id: Option<String>,
    pub session_count: usize,
    pub skipped_session_count: usize,
    pub changed_session_count: usize,
    #[serde(default)]
    pub failed_session_count: usize,
    pub turn_count: usize,
    pub warning_count: usize,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub session_failures: Vec<crate::backend::domain::SessionSyncFailure>,
    #[serde(default)]
    pub session_warnings: Vec<crate::backend::domain::SessionSyncWarning>,
    #[serde(default)]
    pub status: crate::backend::domain::ConversationSyncStatus,
    #[serde(default)]
    pub succeeded_external_ids: Vec<String>,
}
