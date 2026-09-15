-- Memory Q1-Q36 (Issue #35 / ADR-0015): Project memory state for tracking
-- last successful Project Consolidation, input fingerprint, and revision hash.

CREATE TABLE IF NOT EXISTS project_memory_state (
    tenant_id TEXT NOT NULL,
    project_key TEXT NOT NULL,
    project_path TEXT,
    last_successful_consolidation_at TEXT,
    last_input_fingerprint TEXT,
    revision_hash TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, project_key),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_project_memory_state_project_path
ON project_memory_state(tenant_id, project_path);
