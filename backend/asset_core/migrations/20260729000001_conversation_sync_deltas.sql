CREATE TABLE conversation_sync_deltas (
    tenant_id TEXT NOT NULL,
    sync_run_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('session', 'web')),
    session_id TEXT NOT NULL,
    change_kind TEXT NOT NULL CHECK (change_kind IN ('new', 'updated')),
    observed_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, sync_run_id, record_kind, session_id)
);

CREATE INDEX idx_conversation_sync_deltas_recent
ON conversation_sync_deltas(tenant_id, record_kind, observed_at DESC);

CREATE INDEX idx_conversation_sync_deltas_session
ON conversation_sync_deltas(tenant_id, record_kind, session_id);
