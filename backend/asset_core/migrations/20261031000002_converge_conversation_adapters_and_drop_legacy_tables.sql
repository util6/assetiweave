-- Milestone 4 (#163): Conversation Adapter Convergence & Legacy Table Drop
-- Implements ADR-0019 final contraction migration:
-- 1. Catch-up sync from app_conversation_adapter_packages into app_extensions
-- 2. Catch-up sync from app_conversation_adapter_packages and versions into app_extension_versions
-- 3. Catch-up sync from app_conversation_adapter_catalog_releases into app_extension_catalog_releases
-- 4. Safely DROP remaining 3 legacy tables (child tables before parent tables)

-- 1. Catch-up sync from app_conversation_adapter_packages into app_extensions
INSERT OR REPLACE INTO app_extensions (
    extension_id,
    capability_kind,
    target_id,
    display_name,
    active_version,
    distribution_type,
    install_dir,
    enabled,
    installation_status,
    runtime_status,
    error_code,
    error_message,
    checked_at,
    installed_at,
    updated_at,
    capability_meta_json
)
SELECT
    package_id AS extension_id,
    'conversation.adapter' AS capability_kind,
    adapter_id AS target_id,
    name AS display_name,
    version AS active_version,
    CASE
        WHEN origin IN ('local_directory', 'dev_override') THEN 'workspace'
        ELSE 'archive'
    END AS distribution_type,
    install_dir,
    CASE WHEN runtime_ready = 1 THEN 1 ELSE 0 END AS enabled,
    CASE
        WHEN runtime_gate_status = 'core_incompatible' THEN 'incompatible'
        WHEN runtime_ready = 0 THEN 'broken'
        ELSE 'ready'
    END AS installation_status,
    CASE WHEN runtime_ready = 1 THEN 'ready' ELSE 'failed' END AS runtime_status,
    CASE WHEN runtime_ready = 0 THEN runtime_gate_status ELSE NULL END AS error_code,
    error_message,
    COALESCE(last_checked_at, runtime_validated_at, updated_at) AS checked_at,
    created_at AS installed_at,
    updated_at,
    json_object(
        'record_kind', record_kind,
        'manifest_path', manifest_path,
        'adapter_manifest_path', adapter_manifest_path,
        'runtime_protocol', runtime_protocol,
        'runtime_ready', runtime_ready,
        'installed_content_hash', installed_content_hash,
        'trusted_package_hash', trusted_package_hash,
        'origin', origin,
        'source_url', source_url,
        'git_ref', git_ref,
        'git_commit', git_commit,
        'catalog_url', catalog_url,
        'update_policy', update_policy,
        'latest_version', latest_version,
        'last_checked_at', last_checked_at,
        'runtime_gate_status', runtime_gate_status,
        'runtime_validated_at', runtime_validated_at
    ) AS capability_meta_json
FROM app_conversation_adapter_packages;

-- 2. Catch-up sync from app_conversation_adapter_packages and versions into app_extension_versions
INSERT OR IGNORE INTO app_extension_versions (
    extension_id,
    version,
    install_dir,
    artifact_sha256,
    content_hash,
    verification_status,
    installed_at
)
SELECT
    package_id AS extension_id,
    version,
    install_dir,
    trusted_package_hash AS artifact_sha256,
    COALESCE(installed_content_hash, '') AS content_hash,
    runtime_gate_status AS verification_status,
    created_at AS installed_at
FROM app_conversation_adapter_packages;

INSERT OR REPLACE INTO app_extension_versions (
    extension_id,
    version,
    install_dir,
    artifact_sha256,
    content_hash,
    verification_status,
    installed_at
)
SELECT
    package_id AS extension_id,
    version,
    install_dir,
    artifact_hash AS artifact_sha256,
    content_hash,
    runtime_gate_status AS verification_status,
    installed_at
FROM app_conversation_adapter_package_versions
WHERE EXISTS (
    SELECT 1 FROM app_extensions WHERE app_extensions.extension_id = app_conversation_adapter_package_versions.package_id
);

-- 3. Catch-up sync from app_conversation_adapter_catalog_releases into app_extension_catalog_releases
INSERT OR REPLACE INTO app_extension_catalog_releases (
    catalog_source,
    extension_id,
    version,
    capability_kind,
    target_id,
    display_name,
    publisher,
    channel,
    released_at,
    artifact_url,
    artifact_size,
    artifact_sha256,
    release_meta_json,
    fetched_at
)
SELECT
    catalog_url AS catalog_source,
    package_id AS extension_id,
    version,
    'conversation.adapter' AS capability_kind,
    adapter_id AS target_id,
    name AS display_name,
    publisher,
    channel,
    released_at,
    artifact_url,
    artifact_size,
    artifact_sha256,
    json_object(
        'core_compatibility', core_compatibility,
        'changelog_markdown', changelog_markdown,
        'breaking_change', breaking_change,
        'runtime_protocol', runtime_protocol,
        'record_kind', record_kind,
        'package_manifest_file', package_manifest_file,
        'adapter_manifest_file', adapter_manifest_file,
        'adapter_manifest_json', CASE WHEN adapter_manifest_json IS NOT NULL AND json_valid(adapter_manifest_json) THEN json(adapter_manifest_json) ELSE NULL END,
        'source_json', CASE WHEN source_json IS NOT NULL AND json_valid(source_json) THEN json(source_json) ELSE NULL END,
        'etag', etag
    ) AS release_meta_json,
    fetched_at
FROM app_conversation_adapter_catalog_releases;

-- 4. DROP the remaining 3 legacy conversation adapter tables in safe dependency order (children before parent)
DROP TABLE IF EXISTS app_conversation_adapter_package_versions;
DROP TABLE IF EXISTS app_conversation_adapter_catalog_releases;
DROP TABLE IF EXISTS app_conversation_adapter_packages;
