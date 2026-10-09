-- Team & TeamMember domain schema
-- Team is a tenant-scoped aggregate managing fixed member roster and roles.

CREATE TABLE teams (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_teams_tenant_created
ON teams(tenant_id, created_at DESC);

CREATE TABLE team_members (
    tenant_id TEXT NOT NULL,
    team_id TEXT NOT NULL,
    id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('leader', 'teammate')),
    sort_order INTEGER NOT NULL,
    agent_id TEXT NOT NULL,
    model TEXT,
    execution_context_key TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, team_id, id),
    FOREIGN KEY (tenant_id, team_id) REFERENCES teams(tenant_id, id) ON DELETE CASCADE,
    UNIQUE (tenant_id, execution_context_key)
);

CREATE INDEX idx_team_members_team_order
ON team_members(tenant_id, team_id, sort_order ASC);
