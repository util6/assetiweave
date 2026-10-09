CREATE TABLE domain_event_outbox (
    seq            INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id       TEXT NOT NULL UNIQUE,
    tenant_id      TEXT NOT NULL,
    event_type     TEXT NOT NULL,
    source_id      TEXT,
    revision_start INTEGER,
    revision_end   INTEGER,
    payload        TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_outbox_tenant_seq
ON domain_event_outbox(tenant_id, seq);

CREATE TABLE domain_event_consumer_offsets (
    consumer_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    last_seq INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (consumer_id, tenant_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

DROP INDEX IF EXISTS idx_conversation_sync_deltas_recent;
DROP INDEX IF EXISTS idx_conversation_sync_deltas_session;

CREATE TABLE conversation_sync_deltas_v2 (
    tenant_id TEXT NOT NULL,
    sync_run_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('session', 'web')),
    session_id TEXT NOT NULL,
    change_kind TEXT NOT NULL CHECK (change_kind IN ('new', 'updated', 'missing', 'restored')),
    observed_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, sync_run_id, record_kind, session_id)
);

INSERT INTO conversation_sync_deltas_v2 (
    tenant_id, sync_run_id, record_kind, session_id, change_kind, observed_at
)
SELECT tenant_id, sync_run_id, record_kind, session_id, change_kind, observed_at
FROM conversation_sync_deltas;

DROP TABLE conversation_sync_deltas;
ALTER TABLE conversation_sync_deltas_v2 RENAME TO conversation_sync_deltas;

CREATE INDEX idx_conversation_sync_deltas_recent
ON conversation_sync_deltas(tenant_id, record_kind, observed_at DESC);

CREATE INDEX idx_conversation_sync_deltas_session
ON conversation_sync_deltas(tenant_id, record_kind, session_id);

CREATE TABLE memory_evidence_staleness (
    tenant_id TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    record_kind TEXT NOT NULL,
    source_id TEXT,
    session_id TEXT NOT NULL,
    stale_since_revision INTEGER NOT NULL,
    marked_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, evidence_id, stale_since_revision),
    FOREIGN KEY (tenant_id, evidence_id)
        REFERENCES memory_evidence_snapshots(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_memory_evidence_staleness_session
ON memory_evidence_staleness(tenant_id, record_kind, session_id);
