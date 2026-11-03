-- Migration: 202610050001_model_pricing_catalog.sql
-- Model pricing persistent catalog and metadata for tiered pricing fallback and CNY calculation

CREATE TABLE model_pricing_catalog (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tenant_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    provider TEXT NOT NULL DEFAULT '',
    currency TEXT NOT NULL DEFAULT 'CNY',
    input_per_million REAL NOT NULL DEFAULT 0.0,
    cache_read_per_million REAL NOT NULL DEFAULT 0.0,
    cache_write_per_million REAL NOT NULL DEFAULT 0.0,
    reasoning_per_million REAL NOT NULL DEFAULT 0.0,
    output_per_million REAL NOT NULL DEFAULT 0.0,
    raw_currency TEXT NOT NULL DEFAULT 'USD',
    raw_input_per_million REAL NOT NULL DEFAULT 0.0,
    raw_output_per_million REAL NOT NULL DEFAULT 0.0,
    source_channel TEXT NOT NULL DEFAULT 'bundled',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, model_id)
);

CREATE INDEX idx_pricing_catalog_lookup ON model_pricing_catalog(tenant_id, model_id);
CREATE INDEX idx_pricing_catalog_provider ON model_pricing_catalog(tenant_id, provider);

CREATE TABLE model_pricing_metadata (
    tenant_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, key)
);
