CREATE TABLE conversation_payload_policy_state (
    tenant_id TEXT PRIMARY KEY,
    applied_version INTEGER NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);
