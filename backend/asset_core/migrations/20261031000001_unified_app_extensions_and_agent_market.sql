-- Milestone 3 (#162): Unified Extension Market Schema & Legacy Agent Table Purge
-- Implements ADR-0019 3-table unified model for app extensions, versions, and catalog releases.

-- 1. Create unified tables
CREATE TABLE app_extensions (
    extension_id TEXT NOT NULL PRIMARY KEY,
    capability_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    active_version TEXT NOT NULL,
    distribution_type TEXT NOT NULL CHECK (distribution_type IN ('binary', 'npx', 'archive', 'workspace')),
    install_dir TEXT,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    installation_status TEXT NOT NULL CHECK (installation_status IN ('ready', 'incompatible', 'broken')),
    runtime_status TEXT NOT NULL,
    error_code TEXT,
    error_message TEXT,
    checked_at TEXT,
    installed_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    capability_meta_json TEXT NOT NULL CHECK (json_valid(capability_meta_json))
);

CREATE UNIQUE INDEX idx_app_extensions_capability_target
    ON app_extensions (capability_kind, target_id);

CREATE INDEX idx_app_extensions_ready
    ON app_extensions (capability_kind, enabled, installation_status, runtime_status);

CREATE TABLE app_extension_versions (
    extension_id TEXT NOT NULL,
    version TEXT NOT NULL,
    install_dir TEXT NOT NULL,
    artifact_sha256 TEXT,
    content_hash TEXT NOT NULL,
    verification_status TEXT NOT NULL,
    installed_at TEXT NOT NULL,
    PRIMARY KEY (extension_id, version),
    FOREIGN KEY (extension_id)
        REFERENCES app_extensions(extension_id)
        ON DELETE CASCADE
);

CREATE INDEX idx_app_extension_versions_lookup
    ON app_extension_versions (extension_id, installed_at DESC);

CREATE TABLE app_extension_catalog_releases (
    catalog_source TEXT NOT NULL,
    extension_id TEXT NOT NULL,
    version TEXT NOT NULL,
    capability_kind TEXT NOT NULL,
    target_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    publisher TEXT NOT NULL,
    channel TEXT NOT NULL,
    released_at TEXT,
    artifact_url TEXT NOT NULL,
    artifact_size INTEGER,
    artifact_sha256 TEXT,
    release_meta_json TEXT NOT NULL CHECK (json_valid(release_meta_json)),
    fetched_at TEXT NOT NULL,
    PRIMARY KEY (catalog_source, extension_id, version)
);

CREATE INDEX idx_app_extension_catalog_releases_lookup
    ON app_extension_catalog_releases (capability_kind, extension_id, channel, released_at DESC);

-- 2. Migrate data from app_agent_installations into app_extensions
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
    agent_id AS extension_id,
    'agent' AS capability_kind,
    agent_id AS target_id,
    display_name,
    agent_version AS active_version,
    CASE
        WHEN distribution_type IN ('binary', 'npx', 'archive', 'workspace') THEN distribution_type
        ELSE 'binary'
    END AS distribution_type,
    install_dir,
    enabled,
    CASE
        WHEN protocol = 'native' OR distribution_type IN ('system', 'uvx') THEN 'incompatible'
        ELSE installation_status
    END AS installation_status,
    CASE
        WHEN protocol = 'native' OR distribution_type IN ('system', 'uvx') THEN 'failed'
        ELSE runtime_status
    END AS runtime_status,
    CASE
        WHEN protocol = 'native' OR distribution_type IN ('system', 'uvx') THEN 'agent_reinstall_required'
        ELSE runtime_error_code
    END AS error_code,
    CASE
        WHEN protocol = 'native' OR distribution_type IN ('system', 'uvx') THEN 'Agent protocol or distribution is deprecated. Please reinstall the agent.'
        ELSE runtime_error_message
    END AS error_message,
    runtime_checked_at AS checked_at,
    installed_at,
    updated_at,
    json_object(
        'installation_id', installation_id,
        'catalog_item_version', catalog_item_version,
        'protocol', protocol,
        'distribution_id', distribution_id,
        'ownership', ownership,
        'resolved_program', resolved_program,
        'args', json(args_json),
        'definition_json', json(definition_json),
        'integrity_json', CASE WHEN integrity_json IS NOT NULL AND json_valid(integrity_json) THEN json(integrity_json) ELSE NULL END,
        'source_registry', source_registry,
        'catalog_version', catalog_version,
        'protocol_status', protocol_status,
        'protocol_error_code', protocol_error_code,
        'protocol_error_message', protocol_error_message,
        'protocol_checked_at', protocol_checked_at,
        'model_status', model_status,
        'model_error_code', model_error_code,
        'model_checked_at', model_checked_at
    ) AS capability_meta_json
FROM app_agent_installations;

-- 3. Migrate data from app_conversation_adapter_packages into app_extensions
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

-- 4. Populate app_extension_versions
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
    agent_id AS extension_id,
    agent_version AS version,
    COALESCE(install_dir, '') AS install_dir,
    CASE
        WHEN integrity_json IS NOT NULL AND json_valid(integrity_json)
        THEN json_extract(integrity_json, '$.sha256')
        ELSE NULL
    END AS artifact_sha256,
    CASE
        WHEN integrity_json IS NOT NULL AND json_valid(integrity_json)
        THEN COALESCE(json_extract(integrity_json, '$.content_hash'), '')
        ELSE ''
    END AS content_hash,
    CASE
        WHEN protocol = 'native' OR distribution_type IN ('system', 'uvx') THEN 'incompatible'
        ELSE 'ready'
    END AS verification_status,
    installed_at
FROM app_agent_installations;

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
FROM app_conversation_adapter_package_versions;

-- 5. Populate app_extension_catalog_releases
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

-- 6. DROP the 5 legacy tables in safe dependency order
DROP TABLE IF EXISTS conversation_adapter_package_versions;
DROP TABLE IF EXISTS conversation_adapter_catalog_releases;
DROP TABLE IF EXISTS conversation_adapter_packages;
DROP TABLE IF EXISTS agent_installations;
DROP TABLE IF EXISTS app_agent_installations;
