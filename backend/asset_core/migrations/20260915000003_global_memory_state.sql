-- Memory Q1-Q36 (Issue #35 / ADR-0015): Global memory state for tracking
-- last successful Global Consolidation, input fingerprint, and revision hash.

CREATE TABLE IF NOT EXISTS global_memory_state (
    tenant_id TEXT NOT NULL,
    last_successful_consolidation_at TEXT,
    last_input_fingerprint TEXT,
    revision_hash TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);
