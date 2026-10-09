-- Recall workflow state is durable metadata. Conversation tables remain the
-- authority for user/agent message content; these rows only bind turns to
-- that conversation and retain the validated structured result.

CREATE TABLE memory_recall_sessions (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('active', 'completed', 'failed', 'cancelled', 'resume_unavailable')),
    scope_json TEXT NOT NULL CHECK (json_valid(scope_json) AND json_type(scope_json) = 'object'),
    execution_context_key TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    model TEXT,
    turn_count INTEGER NOT NULL DEFAULT 0 CHECK (turn_count >= 0),
    active_turn_id TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, execution_context_key),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_memory_recall_sessions_status
ON memory_recall_sessions(tenant_id, status, updated_at DESC);

CREATE TABLE memory_recall_turns (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    conversation_session_id TEXT NOT NULL,
    conversation_turn_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'completed', 'failed', 'cancelled', 'resume_unavailable')),
    structured_output_json TEXT CHECK (structured_output_json IS NULL OR json_valid(structured_output_json)),
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, session_id, sequence),
    UNIQUE (tenant_id, session_id, conversation_turn_id),
    FOREIGN KEY (tenant_id, session_id)
        REFERENCES memory_recall_sessions(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_memory_recall_turns_session
ON memory_recall_turns(tenant_id, session_id, sequence ASC);

CREATE INDEX idx_memory_recall_turns_active
ON memory_recall_turns(tenant_id, status, updated_at ASC);
