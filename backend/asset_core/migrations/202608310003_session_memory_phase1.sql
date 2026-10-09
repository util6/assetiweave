-- Session Memory is a rebuildable projection of canonical Conversation facts.
-- The source tables and their import transaction remain the only Conversation
-- authority; these tables contain only derived Phase 1 work and projections.

CREATE TABLE session_memory_jobs (
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
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'skipped')),
    not_before TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error TEXT,
    started_at TEXT,
    finished_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (
        tenant_id, session_id, source_revision, source_fingerprint,
        contract_version, prompt_version
    ),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_session_memory_jobs_ready
ON session_memory_jobs(tenant_id, status, not_before, created_at);

CREATE INDEX idx_session_memory_jobs_session
ON session_memory_jobs(tenant_id, session_id, source_revision DESC);

CREATE TABLE session_memories (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_revision INTEGER NOT NULL CHECK (source_revision >= 0),
    source_fingerprint TEXT NOT NULL CHECK (length(trim(source_fingerprint)) > 0),
    contract_version TEXT NOT NULL CHECK (length(trim(contract_version)) > 0),
    prompt_version TEXT NOT NULL CHECK (length(trim(prompt_version)) > 0),
    status TEXT NOT NULL CHECK (status IN ('active', 'invalid', 'failed')),
    project_path TEXT,
    summary TEXT NOT NULL CHECK (length(trim(summary)) > 0 AND length(summary) <= 12000),
    goal TEXT NOT NULL CHECK (length(goal) <= 12000),
    result TEXT NOT NULL CHECK (length(result) <= 12000),
    decisions_json TEXT NOT NULL CHECK (json_valid(decisions_json) AND json_type(decisions_json) = 'array'),
    verification_json TEXT NOT NULL CHECK (json_valid(verification_json) AND json_type(verification_json) = 'array'),
    blockers_json TEXT NOT NULL CHECK (json_valid(blockers_json) AND json_type(blockers_json) = 'array'),
    follow_up_json TEXT NOT NULL CHECK (json_valid(follow_up_json) AND json_type(follow_up_json) = 'array'),
    topics_json TEXT NOT NULL CHECK (json_valid(topics_json) AND json_type(topics_json) = 'array'),
    raw_output_json TEXT NOT NULL CHECK (json_valid(raw_output_json) AND length(raw_output_json) <= 100000),
    generated_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (
        tenant_id, session_id, source_revision, source_fingerprint,
        contract_version, prompt_version
    ),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_session_memories_recent
ON session_memories(tenant_id, session_id, status, generated_at DESC);

CREATE INDEX idx_session_memories_project
ON session_memories(tenant_id, project_path, status, generated_at DESC);

CREATE TABLE session_memory_source_references (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    memory_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind = 'session'),
    question_id TEXT,
    turn_id TEXT,
    part_id TEXT,
    node_id TEXT,
    node_order INTEGER CHECK (node_order IS NULL OR node_order >= 0),
    reference_key TEXT NOT NULL CHECK (length(trim(reference_key)) > 0),
    source_revision INTEGER NOT NULL CHECK (source_revision >= 0),
    created_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, memory_id, reference_key),
    FOREIGN KEY (tenant_id, memory_id)
        REFERENCES session_memories(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_session_memory_source_refs_session
ON session_memory_source_references(tenant_id, session_id, source_revision);

CREATE TABLE recent_memory_events (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    memory_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    category TEXT NOT NULL CHECK (
        category IN ('progress', 'decision', 'research', 'verification', 'blocker', 'follow_up')
    ),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0 AND length(title) <= 500),
    summary TEXT NOT NULL CHECK (length(trim(summary)) > 0 AND length(summary) <= 4000),
    occurred_at TEXT NOT NULL,
    source_reference_id TEXT,
    fingerprint TEXT NOT NULL CHECK (length(trim(fingerprint)) > 0),
    created_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, memory_id, fingerprint),
    FOREIGN KEY (tenant_id, memory_id)
        REFERENCES session_memories(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_recent_memory_events_recent
ON recent_memory_events(tenant_id, session_id, occurred_at DESC, id ASC);
