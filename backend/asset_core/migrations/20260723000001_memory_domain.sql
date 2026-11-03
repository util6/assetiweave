CREATE TABLE memory_runs (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    kind TEXT NOT NULL
        CHECK (kind IN ('auto_dream', 'deep_recall', 'full_organize')),
    trigger_kind TEXT NOT NULL
        CHECK (trigger_kind IN ('automatic', 'manual', 'user_question')),
    scope_json TEXT NOT NULL CHECK (json_valid(scope_json)),
    scope_fingerprint TEXT NOT NULL CHECK (length(scope_fingerprint) > 0),
    source_revision_start INTEGER NOT NULL DEFAULT 0 CHECK (source_revision_start >= 0),
    source_revision_end INTEGER CHECK (source_revision_end IS NULL OR source_revision_end >= source_revision_start),
    provider TEXT,
    model TEXT,
    prompt_version TEXT,
    phase TEXT NOT NULL DEFAULT 'queued'
        CHECK (phase IN ('queued', 'gates', 'context', 'phase1', 'phase2', 'finalizing', 'completed')),
    processed_count INTEGER NOT NULL DEFAULT 0 CHECK (processed_count >= 0),
    total_count INTEGER NOT NULL DEFAULT 0 CHECK (total_count >= 0),
    skipped_count INTEGER NOT NULL DEFAULT 0 CHECK (skipped_count >= 0),
    failed_count INTEGER NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'completed', 'failed', 'interrupted', 'cancelled')),
    result_json TEXT CHECK (result_json IS NULL OR json_valid(result_json)),
    error_kind TEXT,
    error_message TEXT,
    started_at TEXT,
    finished_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_memory_runs_tenant_status
ON memory_runs(tenant_id, status, updated_at DESC);
CREATE INDEX idx_memory_runs_tenant_scope
ON memory_runs(tenant_id, scope_fingerprint, created_at DESC);

CREATE TABLE memory_dream_states (
    tenant_id TEXT NOT NULL,
    scope_fingerprint TEXT NOT NULL CHECK (length(scope_fingerprint) > 0),
    scope_json TEXT NOT NULL CHECK (json_valid(scope_json)),
    last_successful_run_id TEXT,
    source_revision_cursor INTEGER NOT NULL DEFAULT 0 CHECK (source_revision_cursor >= 0),
    session_cursor TEXT,
    next_gate_at TEXT,
    last_error_kind TEXT,
    last_error_message TEXT,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, scope_fingerprint),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, last_successful_run_id)
        REFERENCES memory_runs(tenant_id, id)
);

CREATE INDEX idx_memory_dream_states_next_gate
ON memory_dream_states(tenant_id, next_gate_at);

CREATE TABLE memory_dream_notes (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    scope_json TEXT NOT NULL CHECK (json_valid(scope_json)),
    scope_fingerprint TEXT NOT NULL CHECK (length(scope_fingerprint) > 0),
    markdown TEXT NOT NULL CHECK (length(markdown) <= 6144),
    session_count INTEGER NOT NULL DEFAULT 0 CHECK (session_count >= 0),
    question_count INTEGER NOT NULL DEFAULT 0 CHECK (question_count >= 0),
    evidence_count INTEGER NOT NULL DEFAULT 0 CHECK (evidence_count >= 0),
    source_revision INTEGER NOT NULL DEFAULT 0 CHECK (source_revision >= 0),
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'promoted', 'archived', 'stale')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, run_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, run_id) REFERENCES memory_runs(tenant_id, id)
);

CREATE INDEX idx_memory_dream_notes_tenant_status
ON memory_dream_notes(tenant_id, status, created_at DESC);
CREATE INDEX idx_memory_dream_notes_tenant_scope
ON memory_dream_notes(tenant_id, scope_fingerprint, created_at DESC);

CREATE TABLE memory_extractions (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    batch_index INTEGER NOT NULL CHECK (batch_index >= 0),
    scope_json TEXT NOT NULL CHECK (json_valid(scope_json)),
    scope_fingerprint TEXT NOT NULL CHECK (length(scope_fingerprint) > 0),
    raw_memories_json TEXT NOT NULL CHECK (json_valid(raw_memories_json)),
    session_summary TEXT NOT NULL,
    question_count INTEGER NOT NULL DEFAULT 0 CHECK (question_count >= 0),
    input_char_count INTEGER NOT NULL DEFAULT 0 CHECK (input_char_count >= 0),
    evidence_count INTEGER NOT NULL DEFAULT 0 CHECK (evidence_count >= 0),
    validation_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (validation_status IN ('pending', 'valid', 'invalid')),
    attempt_count INTEGER NOT NULL DEFAULT 1 CHECK (attempt_count >= 1),
    error_message TEXT,
    expires_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, run_id, batch_index),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, run_id) REFERENCES memory_runs(tenant_id, id)
);

CREATE INDEX idx_memory_extractions_tenant_run
ON memory_extractions(tenant_id, run_id, batch_index);
CREATE INDEX idx_memory_extractions_expiry
ON memory_extractions(tenant_id, expires_at);

CREATE TABLE memory_items (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    kind TEXT NOT NULL
        CHECK (kind IN ('preference', 'decision', 'method', 'context', 'follow_up')),
    status TEXT NOT NULL
        CHECK (status IN ('candidate', 'active', 'completed', 'superseded', 'archived', 'rejected')),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    content_markdown TEXT NOT NULL CHECK (length(trim(content_markdown)) > 0),
    scope_json TEXT NOT NULL CHECK (json_valid(scope_json)),
    scope_fingerprint TEXT NOT NULL CHECK (length(scope_fingerprint) > 0),
    origin TEXT NOT NULL
        CHECK (origin IN ('manual', 'auto_dream', 'deep_recall', 'full_organize')),
    origin_run_id TEXT,
    origin_dream_note_id TEXT,
    origin_extraction_id TEXT,
    confidence REAL CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
    supersedes_item_id TEXT,
    source_revision INTEGER NOT NULL DEFAULT 0 CHECK (source_revision >= 0),
    verified_revision INTEGER NOT NULL DEFAULT 0 CHECK (verified_revision >= 0),
    stale_reason TEXT
        CHECK (stale_reason IS NULL OR stale_reason IN ('evidence_changed', 'evidence_missing', 'source_unavailable')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, origin_run_id) REFERENCES memory_runs(tenant_id, id),
    FOREIGN KEY (tenant_id, origin_dream_note_id) REFERENCES memory_dream_notes(tenant_id, id),
    FOREIGN KEY (tenant_id, origin_extraction_id) REFERENCES memory_extractions(tenant_id, id),
    FOREIGN KEY (tenant_id, supersedes_item_id) REFERENCES memory_items(tenant_id, id),
    CHECK (verified_revision <= source_revision)
);

CREATE INDEX idx_memory_items_tenant_status_kind
ON memory_items(tenant_id, status, kind, updated_at DESC);
CREATE INDEX idx_memory_items_tenant_scope
ON memory_items(tenant_id, scope_fingerprint, updated_at DESC);
CREATE INDEX idx_memory_items_tenant_origin
ON memory_items(tenant_id, origin, created_at DESC);

CREATE TABLE memory_item_revisions (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    change_kind TEXT NOT NULL
        CHECK (change_kind IN ('create', 'accept', 'update', 'status', 'supersedes')),
    kind TEXT NOT NULL
        CHECK (kind IN ('preference', 'decision', 'method', 'context', 'follow_up')),
    status TEXT NOT NULL
        CHECK (status IN ('candidate', 'active', 'completed', 'superseded', 'archived', 'rejected')),
    title TEXT NOT NULL,
    content_markdown TEXT NOT NULL,
    scope_json TEXT NOT NULL CHECK (json_valid(scope_json)),
    scope_fingerprint TEXT NOT NULL,
    origin TEXT NOT NULL
        CHECK (origin IN ('manual', 'auto_dream', 'deep_recall', 'full_organize')),
    confidence REAL CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
    supersedes_item_id TEXT,
    source_revision INTEGER NOT NULL CHECK (source_revision >= 0),
    verified_revision INTEGER NOT NULL CHECK (verified_revision >= 0),
    stale_reason TEXT
        CHECK (stale_reason IS NULL OR stale_reason IN ('evidence_changed', 'evidence_missing', 'source_unavailable')),
    changed_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, item_id, revision_number),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, item_id) REFERENCES memory_items(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, supersedes_item_id) REFERENCES memory_items(tenant_id, id),
    CHECK (verified_revision <= source_revision)
);

CREATE INDEX idx_memory_item_revisions_item
ON memory_item_revisions(tenant_id, item_id, revision_number DESC);

CREATE TABLE memory_evidence_snapshots (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('session', 'web')),
    source_id TEXT,
    session_id TEXT NOT NULL,
    question_id TEXT,
    turn_id TEXT,
    part_id TEXT,
    block_id TEXT NOT NULL CHECK (length(block_id) > 0),
    content_hash TEXT NOT NULL CHECK (length(content_hash) > 0),
    excerpt TEXT NOT NULL CHECK (length(excerpt) <= 8192),
    translated_excerpt TEXT,
    event_time TEXT,
    source_revision INTEGER NOT NULL DEFAULT 0 CHECK (source_revision >= 0),
    source_unavailable INTEGER NOT NULL DEFAULT 0 CHECK (source_unavailable IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, record_kind, block_id, content_hash),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_memory_evidence_source
ON memory_evidence_snapshots(tenant_id, record_kind, session_id, question_id, block_id);
CREATE INDEX idx_memory_evidence_freshness
ON memory_evidence_snapshots(tenant_id, source_revision, source_unavailable);

CREATE TABLE memory_run_evidence (
    tenant_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0 CHECK (sort_order >= 0),
    PRIMARY KEY (tenant_id, run_id, evidence_id),
    FOREIGN KEY (tenant_id, run_id) REFERENCES memory_runs(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, evidence_id) REFERENCES memory_evidence_snapshots(tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE memory_dream_note_evidence (
    tenant_id TEXT NOT NULL,
    dream_note_id TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0 CHECK (sort_order >= 0),
    PRIMARY KEY (tenant_id, dream_note_id, evidence_id),
    FOREIGN KEY (tenant_id, dream_note_id) REFERENCES memory_dream_notes(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, evidence_id) REFERENCES memory_evidence_snapshots(tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE memory_extraction_evidence (
    tenant_id TEXT NOT NULL,
    extraction_id TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0 CHECK (sort_order >= 0),
    PRIMARY KEY (tenant_id, extraction_id, evidence_id),
    FOREIGN KEY (tenant_id, extraction_id) REFERENCES memory_extractions(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, evidence_id) REFERENCES memory_evidence_snapshots(tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE memory_item_evidence (
    tenant_id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0 CHECK (sort_order >= 0),
    PRIMARY KEY (tenant_id, item_id, evidence_id),
    FOREIGN KEY (tenant_id, item_id) REFERENCES memory_items(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, evidence_id) REFERENCES memory_evidence_snapshots(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_memory_run_evidence_evidence
ON memory_run_evidence(tenant_id, evidence_id);
CREATE INDEX idx_memory_dream_note_evidence_evidence
ON memory_dream_note_evidence(tenant_id, evidence_id);
CREATE INDEX idx_memory_extraction_evidence_evidence
ON memory_extraction_evidence(tenant_id, evidence_id);
CREATE INDEX idx_memory_item_evidence_evidence
ON memory_item_evidence(tenant_id, evidence_id);
