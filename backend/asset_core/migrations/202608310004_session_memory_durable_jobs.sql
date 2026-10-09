-- Extend the Phase 1 queue with recoverable ownership and retry state.
-- This is an append-only migration: existing jobs remain valid and queued.

DROP INDEX IF EXISTS idx_session_memory_jobs_ready;
DROP INDEX IF EXISTS idx_session_memory_jobs_session;

CREATE TABLE session_memory_jobs_v2 (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_revision INTEGER NOT NULL CHECK (source_revision >= 0),
    source_fingerprint TEXT NOT NULL CHECK (length(trim(source_fingerprint)) > 0),
    contract_version TEXT NOT NULL CHECK (length(trim(contract_version)) > 0),
    prompt_version TEXT NOT NULL CHECK (length(trim(prompt_version)) > 0),
    source_event_id TEXT NOT NULL,
    source_sync_run_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'skipped', 'canceled')),
    not_before TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error TEXT,
    started_at TEXT,
    finished_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    ownership_token TEXT,
    lease_expires_at TEXT,
    heartbeat_at TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    retry_at TEXT,
    watermark INTEGER,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (
        tenant_id, session_id, source_revision, source_fingerprint,
        contract_version, prompt_version
    ),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

INSERT INTO session_memory_jobs_v2 (
    tenant_id, id, session_id, source_id, source_revision,
    source_fingerprint, contract_version, prompt_version,
    source_event_id, source_sync_run_id, status, not_before,
    attempt_count, last_error, started_at, finished_at, created_at,
    updated_at, ownership_token, lease_expires_at, heartbeat_at,
    retry_count, retry_at, watermark
)
SELECT tenant_id, id, session_id, source_id, source_revision,
       source_fingerprint, contract_version, prompt_version,
       source_event_id, source_sync_run_id, status, not_before,
       attempt_count, last_error, started_at, finished_at, created_at,
       updated_at, NULL, NULL, NULL, 0, NULL, NULL
FROM session_memory_jobs;

DROP TABLE session_memory_jobs;
ALTER TABLE session_memory_jobs_v2 RENAME TO session_memory_jobs;

CREATE INDEX idx_session_memory_jobs_ready
ON session_memory_jobs(tenant_id, status, not_before, retry_at, created_at);

CREATE INDEX idx_session_memory_jobs_session
ON session_memory_jobs(tenant_id, session_id, source_revision DESC);

CREATE INDEX idx_session_memory_jobs_lease
ON session_memory_jobs(status, lease_expires_at, updated_at);
