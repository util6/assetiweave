-- Global Memory is a tenant-scoped, rebuildable projection of successful
-- Project Memory versions. It stores only the cross-project summary and index;
-- project detail remains in Project Memory.

CREATE TABLE global_memories (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    last_successful_version_id TEXT,
    last_successful_at TEXT,
    last_successful_watermark INTEGER NOT NULL DEFAULT 0 CHECK (last_successful_watermark >= 0),
    last_successful_input_fingerprint TEXT,
    summary_document_path TEXT,
    memory_document_path TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE TABLE global_memory_versions (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    version_number INTEGER NOT NULL CHECK (version_number > 0),
    status TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed', 'invalid')),
    input_fingerprint TEXT NOT NULL CHECK (length(trim(input_fingerprint)) > 0),
    source_watermark INTEGER NOT NULL CHECK (source_watermark >= 0),
    summary_markdown TEXT,
    memory_markdown TEXT,
    raw_output_json TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, version_number),
    FOREIGN KEY (tenant_id) REFERENCES global_memories(tenant_id) ON DELETE CASCADE
);

CREATE TABLE global_memory_sources (
    tenant_id TEXT NOT NULL,
    version_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    project_path TEXT NOT NULL CHECK (length(trim(project_path)) > 0),
    project_version_id TEXT NOT NULL,
    project_watermark INTEGER NOT NULL CHECK (project_watermark >= 0),
    sort_order INTEGER NOT NULL CHECK (sort_order >= 0),
    PRIMARY KEY (tenant_id, version_id, project_id),
    FOREIGN KEY (tenant_id, version_id)
        REFERENCES global_memory_versions(tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE global_memory_jobs (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
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
    UNIQUE (tenant_id),
    FOREIGN KEY (tenant_id, id)
        REFERENCES global_memories(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_global_memory_versions_latest
ON global_memory_versions(tenant_id, status, version_number DESC);

CREATE INDEX idx_global_memory_jobs_ready
ON global_memory_jobs(tenant_id, status, retry_at, updated_at);

CREATE INDEX idx_global_memory_jobs_lease
ON global_memory_jobs(status, lease_expires_at, updated_at);
