CREATE TABLE IF NOT EXISTS memory_session_watermarks (
    tenant_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    project_path TEXT,
    source_updated_at TEXT NOT NULL,
    summary_rel_path TEXT NOT NULL,
    has_signal INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_memory_session_watermarks_tenant_project
ON memory_session_watermarks (tenant_id, project_path);
