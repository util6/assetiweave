CREATE TABLE memory_usage_events (
    tenant_id TEXT NOT NULL,
    memory_kind TEXT NOT NULL,
    memory_id TEXT NOT NULL,
    use_kind TEXT NOT NULL,
    use_id TEXT NOT NULL,
    used_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, memory_kind, memory_id, use_kind, use_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_memory_usage_events_recent
    ON memory_usage_events (tenant_id, memory_kind, memory_id, used_at DESC);
