-- ACP Agents and downloaded conversation adapter packages are part of the
-- application environment. Their lifecycle must not fork when the active
-- tenant changes; only assets and conversation data remain tenant-scoped.

CREATE TABLE app_agent_installations (
  agent_id TEXT NOT NULL PRIMARY KEY,
  installation_id TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  catalog_item_version TEXT NOT NULL,
  agent_version TEXT NOT NULL,
  protocol TEXT NOT NULL CHECK (protocol IN ('acp', 'native')),
  distribution_id TEXT NOT NULL,
  distribution_type TEXT NOT NULL CHECK (distribution_type IN ('system', 'binary', 'npx', 'uvx')),
  ownership TEXT NOT NULL CHECK (ownership IN ('system', 'managed')),
  install_dir TEXT,
  resolved_program TEXT NOT NULL,
  args_json TEXT NOT NULL,
  definition_json TEXT NOT NULL,
  integrity_json TEXT,
  source_registry TEXT NOT NULL,
  catalog_version TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  installation_status TEXT NOT NULL CHECK (installation_status IN ('ready', 'incompatible', 'broken')),
  runtime_status TEXT NOT NULL CHECK (runtime_status IN ('unchecked', 'ready', 'runtime_missing', 'entry_missing', 'failed')),
  runtime_error_code TEXT,
  runtime_error_message TEXT,
  runtime_checked_at TEXT,
  protocol_status TEXT NOT NULL CHECK (protocol_status IN ('unchecked', 'ready', 'auth_required', 'failed', 'unsupported')),
  protocol_error_code TEXT,
  protocol_error_message TEXT,
  protocol_checked_at TEXT,
  model_status TEXT CHECK (model_status IS NULL OR model_status IN ('unchecked', 'ready', 'failed', 'unsupported')),
  model_error_code TEXT,
  model_checked_at TEXT,
  installed_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  CHECK ((ownership = 'system' AND install_dir IS NULL) OR (ownership = 'managed' AND install_dir IS NOT NULL))
);

CREATE INDEX idx_app_agent_installations_ready
  ON app_agent_installations (enabled, installation_status, protocol_status);

INSERT INTO app_agent_installations (
  agent_id, installation_id, display_name, catalog_item_version, agent_version,
  protocol, distribution_id, distribution_type, ownership, install_dir,
  resolved_program, args_json, definition_json, integrity_json, source_registry,
  catalog_version, enabled, installation_status, runtime_status,
  runtime_error_code, runtime_error_message, runtime_checked_at,
  protocol_status, protocol_error_code, protocol_error_message,
  protocol_checked_at, model_status, model_error_code, model_checked_at,
  installed_at, updated_at
)
SELECT
  source.agent_id, source.installation_id, source.display_name,
  source.catalog_item_version, source.agent_version, source.protocol,
  source.distribution_id, source.distribution_type, source.ownership,
  source.install_dir, source.resolved_program, source.args_json,
  source.definition_json, source.integrity_json, source.source_registry,
  source.catalog_version, source.enabled, source.installation_status,
  source.runtime_status, source.runtime_error_code, source.runtime_error_message,
  source.runtime_checked_at, source.protocol_status, source.protocol_error_code,
  source.protocol_error_message, source.protocol_checked_at, source.model_status,
  source.model_error_code, source.model_checked_at, source.installed_at,
  source.updated_at
FROM agent_installations AS source
WHERE source.rowid = (
  SELECT candidate.rowid
  FROM agent_installations AS candidate
  WHERE candidate.agent_id = source.agent_id
  ORDER BY candidate.updated_at DESC, candidate.installed_at DESC, candidate.rowid DESC
  LIMIT 1
);

CREATE TABLE app_conversation_adapter_packages (
  package_id TEXT NOT NULL PRIMARY KEY,
  adapter_id TEXT NOT NULL,
  name TEXT NOT NULL,
  version TEXT NOT NULL,
  record_kind TEXT NOT NULL,
  install_dir TEXT NOT NULL,
  manifest_path TEXT NOT NULL,
  adapter_manifest_path TEXT NOT NULL,
  runtime_protocol TEXT NOT NULL,
  runtime_ready INTEGER NOT NULL,
  installed_content_hash TEXT,
  trusted_package_hash TEXT,
  error_message TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  origin TEXT NOT NULL DEFAULT 'managed_release',
  source_url TEXT,
  git_ref TEXT,
  git_commit TEXT,
  catalog_url TEXT,
  update_policy TEXT NOT NULL DEFAULT 'manual',
  latest_version TEXT,
  last_checked_at TEXT,
  runtime_gate_status TEXT NOT NULL DEFAULT 'ready',
  runtime_validated_at TEXT
);

CREATE INDEX idx_app_conversation_adapter_packages_ready
  ON app_conversation_adapter_packages (runtime_ready, updated_at);

INSERT INTO app_conversation_adapter_packages (
  package_id, adapter_id, name, version, record_kind, install_dir,
  manifest_path, adapter_manifest_path, runtime_protocol, runtime_ready,
  installed_content_hash, trusted_package_hash, error_message, created_at,
  updated_at, origin, source_url, git_ref, git_commit, catalog_url,
  update_policy, latest_version, last_checked_at, runtime_gate_status,
  runtime_validated_at
)
SELECT
  source.package_id, source.adapter_id, source.name, source.version,
  source.record_kind, source.install_dir, source.manifest_path,
  source.adapter_manifest_path, source.runtime_protocol, source.runtime_ready,
  source.installed_content_hash, source.trusted_package_hash,
  source.error_message, source.created_at, source.updated_at, source.origin,
  source.source_url, source.git_ref, source.git_commit, source.catalog_url,
  source.update_policy, source.latest_version, source.last_checked_at,
  source.runtime_gate_status, source.runtime_validated_at
FROM conversation_adapter_packages AS source
WHERE source.rowid = (
  SELECT candidate.rowid
  FROM conversation_adapter_packages AS candidate
  WHERE candidate.package_id = source.package_id
  ORDER BY candidate.updated_at DESC, candidate.created_at DESC, candidate.rowid DESC
  LIMIT 1
);

CREATE TABLE app_conversation_adapter_package_versions (
  package_id TEXT NOT NULL,
  version TEXT NOT NULL,
  install_dir TEXT NOT NULL,
  artifact_hash TEXT,
  content_hash TEXT NOT NULL,
  runtime_gate_status TEXT NOT NULL,
  installed_at TEXT NOT NULL,
  PRIMARY KEY (package_id, version),
  FOREIGN KEY (package_id)
    REFERENCES app_conversation_adapter_packages(package_id)
    ON DELETE CASCADE
);

CREATE INDEX idx_app_conversation_adapter_versions_lookup
  ON app_conversation_adapter_package_versions(package_id, installed_at DESC);

INSERT INTO app_conversation_adapter_package_versions (
  package_id, version, install_dir, artifact_hash, content_hash,
  runtime_gate_status, installed_at
)
SELECT
  source.package_id, source.version, source.install_dir, source.artifact_hash,
  source.content_hash, source.runtime_gate_status, source.installed_at
FROM conversation_adapter_package_versions AS source
WHERE EXISTS (
  SELECT 1 FROM app_conversation_adapter_packages AS package
  WHERE package.package_id = source.package_id
)
AND source.rowid = (
  SELECT candidate.rowid
  FROM conversation_adapter_package_versions AS candidate
  WHERE candidate.package_id = source.package_id
    AND candidate.version = source.version
  ORDER BY candidate.installed_at DESC, candidate.rowid DESC
  LIMIT 1
);

CREATE TABLE app_conversation_adapter_catalog_releases (
  catalog_url TEXT NOT NULL,
  package_id TEXT NOT NULL,
  version TEXT NOT NULL,
  channel TEXT NOT NULL,
  released_at TEXT,
  core_compatibility TEXT NOT NULL,
  artifact_url TEXT NOT NULL,
  artifact_size INTEGER,
  artifact_sha256 TEXT NOT NULL,
  changelog_markdown TEXT NOT NULL,
  breaking_change INTEGER NOT NULL DEFAULT 0,
  runtime_protocol TEXT NOT NULL,
  adapter_manifest_json TEXT,
  etag TEXT,
  fetched_at TEXT NOT NULL,
  adapter_id TEXT NOT NULL DEFAULT '',
  name TEXT NOT NULL DEFAULT '',
  publisher TEXT NOT NULL DEFAULT '',
  record_kind TEXT NOT NULL DEFAULT 'session',
  package_manifest_file TEXT NOT NULL DEFAULT 'conversation-adapter-package.json',
  adapter_manifest_file TEXT NOT NULL DEFAULT 'conversation-adapter.json',
  source_json TEXT,
  PRIMARY KEY (catalog_url, package_id, version)
);

CREATE INDEX idx_app_conversation_adapter_catalog_releases_lookup
  ON app_conversation_adapter_catalog_releases(package_id, channel, released_at DESC);

INSERT INTO app_conversation_adapter_catalog_releases (
  catalog_url, package_id, version, channel, released_at, core_compatibility,
  artifact_url, artifact_size, artifact_sha256, changelog_markdown,
  breaking_change, runtime_protocol, adapter_manifest_json, etag, fetched_at,
  adapter_id, name, publisher, record_kind, package_manifest_file,
  adapter_manifest_file, source_json
)
SELECT
  source.catalog_url, source.package_id, source.version, source.channel,
  source.released_at, source.core_compatibility, source.artifact_url,
  source.artifact_size, source.artifact_sha256, source.changelog_markdown,
  source.breaking_change, source.runtime_protocol, source.adapter_manifest_json,
  source.etag, source.fetched_at, source.adapter_id, source.name,
  source.publisher, source.record_kind, source.package_manifest_file,
  source.adapter_manifest_file, source.source_json
FROM conversation_adapter_catalog_releases AS source
WHERE source.rowid = (
  SELECT candidate.rowid
  FROM conversation_adapter_catalog_releases AS candidate
  WHERE candidate.catalog_url = source.catalog_url
    AND candidate.package_id = source.package_id
    AND candidate.version = source.version
  ORDER BY candidate.fetched_at DESC, candidate.rowid DESC
  LIMIT 1
);
