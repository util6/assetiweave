PRAGMA foreign_keys = OFF;

CREATE TABLE web_record_sessions_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    external_id TEXT NOT NULL,
    title TEXT NOT NULL,
    started_at TEXT,
    updated_at TEXT,
    source_locator TEXT,
    source_fingerprint TEXT,
    missing INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE(tenant_id, source_id, external_id)
);

INSERT INTO web_record_sessions_new (
    tenant_id, id, source_id, adapter_id, external_id, title,
    started_at, updated_at, source_locator, source_fingerprint, missing,
    created_at, imported_at
)
SELECT
    tenant_id, id, source_id, adapter_id, external_id, title,
    started_at, updated_at, source_locator, source_fingerprint, missing,
    created_at, imported_at
FROM web_record_sessions;

DROP TABLE web_record_sessions;
ALTER TABLE web_record_sessions_new RENAME TO web_record_sessions;

CREATE INDEX idx_web_record_sessions_tenant_source
ON web_record_sessions(tenant_id, source_id, missing);
CREATE INDEX idx_web_record_sessions_tenant_adapter
ON web_record_sessions(tenant_id, adapter_id);

PRAGMA foreign_keys = ON;
