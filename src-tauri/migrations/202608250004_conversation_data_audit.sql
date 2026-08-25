CREATE TABLE conversation_data_audit_issues (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    category TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('warning', 'error')),
    auto_repairable INTEGER NOT NULL DEFAULT 0 CHECK (auto_repairable IN (0, 1)),
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'resolved', 'ignored')),
    affected_count INTEGER NOT NULL DEFAULT 0 CHECK (affected_count >= 0),
    sample_ids_json TEXT NOT NULL DEFAULT '[]',
    details_json TEXT NOT NULL DEFAULT '{}',
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    resolved_at TEXT,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, fingerprint, status),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_conversation_data_audit_issues_status
    ON conversation_data_audit_issues (tenant_id, status, category);
