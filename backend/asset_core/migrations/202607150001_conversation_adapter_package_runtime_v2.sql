ALTER TABLE conversation_adapter_packages
ADD COLUMN origin TEXT NOT NULL DEFAULT 'managed_release';

ALTER TABLE conversation_adapter_packages
ADD COLUMN source_url TEXT;

ALTER TABLE conversation_adapter_packages
ADD COLUMN git_ref TEXT;

ALTER TABLE conversation_adapter_packages
ADD COLUMN git_commit TEXT;

ALTER TABLE conversation_adapter_packages
ADD COLUMN catalog_url TEXT;

ALTER TABLE conversation_adapter_packages
ADD COLUMN update_policy TEXT NOT NULL DEFAULT 'manual';

ALTER TABLE conversation_adapter_packages
ADD COLUMN latest_version TEXT;

ALTER TABLE conversation_adapter_packages
ADD COLUMN last_checked_at TEXT;

ALTER TABLE conversation_adapter_packages
ADD COLUMN runtime_gate_status TEXT NOT NULL DEFAULT 'ready';

ALTER TABLE conversation_adapter_packages
ADD COLUMN runtime_validated_at TEXT;

UPDATE conversation_adapter_packages
SET latest_version = version,
    runtime_gate_status = CASE WHEN runtime_ready = 1 THEN 'ready' ELSE 'runtime_missing' END;

CREATE TABLE conversation_adapter_package_versions (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    package_id TEXT NOT NULL,
    version TEXT NOT NULL,
    install_dir TEXT NOT NULL,
    artifact_hash TEXT,
    content_hash TEXT NOT NULL,
    runtime_gate_status TEXT NOT NULL,
    installed_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, package_id, version),
    FOREIGN KEY (tenant_id, package_id)
        REFERENCES conversation_adapter_packages(tenant_id, package_id)
        ON DELETE CASCADE
);

INSERT INTO conversation_adapter_package_versions (
    tenant_id, package_id, version, install_dir, artifact_hash, content_hash,
    runtime_gate_status, installed_at
)
SELECT tenant_id, package_id, version, install_dir, trusted_package_hash,
       COALESCE(installed_content_hash, ''), runtime_gate_status, created_at
FROM conversation_adapter_packages;

CREATE TABLE conversation_adapter_catalog_releases (
    tenant_id TEXT NOT NULL DEFAULT 'default',
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
    PRIMARY KEY (tenant_id, catalog_url, package_id, version)
);

CREATE INDEX idx_conversation_adapter_versions_active_lookup
ON conversation_adapter_package_versions(tenant_id, package_id, installed_at DESC);

CREATE INDEX idx_conversation_adapter_catalog_releases_lookup
ON conversation_adapter_catalog_releases(tenant_id, package_id, channel, released_at DESC);
