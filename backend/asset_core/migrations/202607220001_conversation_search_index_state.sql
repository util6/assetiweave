CREATE TABLE conversation_search_index_state (
    tenant_id TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    index_instance_id TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    tokenizer_version TEXT NOT NULL,
    source_revision INTEGER NOT NULL DEFAULT 0,
    indexed_revision INTEGER,
    active_generation TEXT,
    health TEXT NOT NULL DEFAULT 'missing'
        CHECK (health IN ('missing', 'ready', 'stale', 'failed', 'disabled')),
    document_count INTEGER NOT NULL DEFAULT 0,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    last_built_at TEXT,
    last_error TEXT,
    lease_owner TEXT,
    lease_expires_at TEXT,
    updated_at TEXT NOT NULL
);
