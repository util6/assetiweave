-- Memory v2 durable maintenance jobs for Project and Global consolidation.
-- Recent Snapshot has its own queue because it carries a watermark; these rows
-- represent the latest requested maintenance target for one tenant/scope.
CREATE TABLE memory_v2_maintenance_jobs (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    purpose TEXT NOT NULL CHECK (purpose IN ('project_consolidation', 'global_consolidation')),
    project_key TEXT,
    project_path TEXT,
    status TEXT NOT NULL CHECK (
        status IN ('queued', 'running', 'succeeded', 'failed', 'canceled', 'stale')
    ),
    ownership_token TEXT,
    lease_expires_at TEXT,
    heartbeat_at TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    retry_at TEXT,
    input_fingerprint TEXT NOT NULL CHECK (length(trim(input_fingerprint)) > 0),
    work_order_json TEXT NOT NULL CHECK (json_valid(work_order_json)),
    last_error_code TEXT,
    last_error_message TEXT,
    started_at TEXT,
    finished_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_memory_v2_maintenance_target
ON memory_v2_maintenance_jobs(tenant_id, purpose, COALESCE(project_key, ''));

CREATE INDEX idx_memory_v2_maintenance_scheduler
ON memory_v2_maintenance_jobs(tenant_id, status, retry_at, updated_at);
