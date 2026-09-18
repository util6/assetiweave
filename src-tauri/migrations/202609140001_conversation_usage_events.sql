-- Migration: 202609140001_conversation_usage_events.sql
-- Create conversation usage events and source scan states for multi-application token usage statistics

CREATE TABLE conversation_usage_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    external_event_id TEXT NOT NULL,
    session_id TEXT,
    turn_id TEXT,
    logical_request_id TEXT,
    attempt_index INTEGER NOT NULL DEFAULT 0,
    timestamp TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'completed',
    input_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    total_input_tokens INTEGER NOT NULL DEFAULT 0,
    total_output_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    host_reported_cost REAL,
    catalog_estimated_cost REAL,
    cost_basis TEXT NOT NULL CHECK (cost_basis IN ('host_reported', 'catalog_estimated', 'none')),
    currency TEXT NOT NULL DEFAULT 'USD',
    price_catalog_version TEXT,
    observation_run_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, source_id, external_event_id)
);

CREATE INDEX idx_usage_events_tenant_time ON conversation_usage_events(tenant_id, timestamp);
CREATE INDEX idx_usage_events_source_time ON conversation_usage_events(tenant_id, source_id, timestamp);
CREATE INDEX idx_usage_events_model_time ON conversation_usage_events(tenant_id, model, timestamp);
CREATE INDEX idx_usage_events_adapter_time ON conversation_usage_events(tenant_id, adapter_id, timestamp);
CREATE INDEX idx_usage_events_observation ON conversation_usage_events(tenant_id, source_id, observation_run_id);

CREATE TABLE conversation_usage_source_states (
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    decoder_profile TEXT,
    schema_fingerprint TEXT,
    opaque_cursor TEXT,
    last_success_scan_at TEXT,
    last_full_scan_at TEXT,
    status TEXT NOT NULL DEFAULT 'idle',
    diagnostics_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, source_id)
);
