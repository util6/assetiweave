CREATE TABLE IF NOT EXISTS principals (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    display_name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tenants (
    id TEXT PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tenant_memberships (
    tenant_id TEXT NOT NULL,
    principal_id TEXT NOT NULL,
    role TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, principal_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (principal_id) REFERENCES principals(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS tenant_state (
    principal_id TEXT PRIMARY KEY,
    active_tenant_id TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (principal_id) REFERENCES principals(id) ON DELETE CASCADE,
    FOREIGN KEY (active_tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT
);

INSERT OR IGNORE INTO principals (
    id, kind, display_name, created_at, updated_at
) VALUES (
    'local',
    'local',
    'Local User',
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
);

INSERT OR IGNORE INTO tenants (
    id, slug, name, kind, status, created_at, updated_at
) VALUES (
    'default',
    'default',
    'Default',
    'local_workspace',
    'active',
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
);

INSERT OR IGNORE INTO tenant_memberships (
    tenant_id, principal_id, role, created_at, updated_at
) VALUES (
    'default',
    'local',
    'owner',
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
);

INSERT OR IGNORE INTO tenant_state (
    principal_id, active_tenant_id, updated_at
) VALUES (
    'local',
    'default',
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
);
