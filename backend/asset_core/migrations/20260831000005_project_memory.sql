-- Project Memory is a rebuildable, tenant-scoped projection of successful
-- Session Memory rows.  The project directory is the aggregate boundary;
-- source repositories are never written by this projection.

CREATE TABLE project_memories (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    project_path TEXT NOT NULL CHECK (length(trim(project_path)) > 0),
    last_successful_version_id TEXT,
    last_successful_at TEXT,
    last_successful_watermark INTEGER NOT NULL DEFAULT 0 CHECK (last_successful_watermark >= 0),
    last_successful_input_fingerprint TEXT,
    document_path TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, project_path),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE TABLE project_memory_versions (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    version_number INTEGER NOT NULL CHECK (version_number > 0),
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed', 'invalid')),
    input_fingerprint TEXT NOT NULL CHECK (length(trim(input_fingerprint)) > 0),
    source_watermark INTEGER NOT NULL CHECK (source_watermark >= 0),
    content_markdown TEXT,
    raw_output_json TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, project_id, version_number),
    FOREIGN KEY (tenant_id, project_id)
        REFERENCES project_memories(tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE project_memory_sources (
    tenant_id TEXT NOT NULL,
    version_id TEXT NOT NULL,
    session_memory_id TEXT NOT NULL,
    source_revision INTEGER NOT NULL CHECK (source_revision >= 0),
    sort_order INTEGER NOT NULL CHECK (sort_order >= 0),
    PRIMARY KEY (tenant_id, version_id, session_memory_id),
    FOREIGN KEY (tenant_id, version_id)
        REFERENCES project_memory_versions(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, session_memory_id)
        REFERENCES session_memories(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_project_memory_versions_latest
ON project_memory_versions(tenant_id, project_id, status, version_number DESC);

CREATE TABLE project_memory_jobs (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    project_path TEXT NOT NULL CHECK (length(trim(project_path)) > 0),
    target_watermark INTEGER NOT NULL CHECK (target_watermark >= 0),
    input_fingerprint TEXT NOT NULL CHECK (length(trim(input_fingerprint)) > 0),
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'canceled')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    retry_at TEXT,
    last_error TEXT,
    ownership_token TEXT,
    lease_expires_at TEXT,
    heartbeat_at TEXT,
    started_at TEXT,
    finished_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, project_path),
    FOREIGN KEY (tenant_id, project_id)
        REFERENCES project_memories(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_project_memory_jobs_ready
ON project_memory_jobs(tenant_id, status, retry_at, updated_at);

CREATE INDEX idx_project_memory_jobs_lease
ON project_memory_jobs(status, lease_expires_at, updated_at);
