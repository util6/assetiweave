CREATE TABLE IF NOT EXISTS conversation_adapter_packages (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    package_id TEXT NOT NULL,
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
    PRIMARY KEY (tenant_id, package_id)
);

CREATE INDEX IF NOT EXISTS idx_conversation_adapter_packages_tenant_adapter
ON conversation_adapter_packages(tenant_id, adapter_id);

CREATE INDEX IF NOT EXISTS idx_conversation_adapter_packages_tenant_ready
ON conversation_adapter_packages(tenant_id, runtime_ready, updated_at);
