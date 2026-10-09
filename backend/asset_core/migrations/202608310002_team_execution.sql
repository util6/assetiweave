-- Team execution facts.  Conversation tables intentionally do not appear in
-- this migration: provider history and Team orchestration are separate domains.

CREATE TABLE agent_execution_bindings (
    tenant_id TEXT NOT NULL,
    execution_context_key TEXT NOT NULL,
    provider_session_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    installation_id TEXT,
    model TEXT,
    workspace_path TEXT NOT NULL,
    binding_version INTEGER NOT NULL DEFAULT 1,
    provider_metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, execution_context_key)
);

CREATE TABLE team_runs (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    team_id TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('drafting', 'awaiting_review', 'executing', 'terminal')),
    revision INTEGER NOT NULL DEFAULT 1,
    leader_member_id TEXT NOT NULL,
    roster_snapshot_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    finished_at TEXT,
    error_code TEXT,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, team_id) REFERENCES teams(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_team_runs_active
ON team_runs(tenant_id, team_id, state, updated_at DESC);

CREATE TABLE team_tasks (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    team_id TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    sort_order INTEGER NOT NULL,
    recommended_member_id TEXT NOT NULL,
    owner_member_id TEXT,
    state TEXT NOT NULL CHECK (state IN ('draft', 'queued', 'running', 'succeeded', 'failed', 'canceled')),
    revision INTEGER NOT NULL DEFAULT 1,
    result TEXT,
    error_code TEXT,
    dispatch_key TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, run_id) REFERENCES team_runs(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_team_tasks_run_order
ON team_tasks(tenant_id, run_id, sort_order ASC, created_at ASC);

CREATE UNIQUE INDEX idx_team_tasks_dispatch
ON team_tasks(tenant_id, dispatch_key)
WHERE dispatch_key IS NOT NULL;

CREATE TABLE team_mailbox_messages (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    team_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    task_id TEXT,
    sender_member_id TEXT NOT NULL,
    recipient_member_id TEXT NOT NULL,
    message_type TEXT NOT NULL,
    body TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    read_at TEXT,
    acked_at TEXT,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, idempotency_key),
    FOREIGN KEY (tenant_id, run_id) REFERENCES team_runs(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_team_mailbox_unread
ON team_mailbox_messages(tenant_id, run_id, recipient_member_id, acked_at, created_at ASC);

CREATE TABLE team_task_claims (
    tenant_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    dispatch_key TEXT NOT NULL,
    claimed_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, task_id),
    UNIQUE (tenant_id, dispatch_key)
);

CREATE TABLE team_tool_credentials (
    tenant_id TEXT NOT NULL,
    credential_hash TEXT NOT NULL,
    team_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    member_id TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, credential_hash)
);

CREATE INDEX idx_team_tool_credentials_expiry
ON team_tool_credentials(tenant_id, team_id, run_id, member_id, expires_at);
