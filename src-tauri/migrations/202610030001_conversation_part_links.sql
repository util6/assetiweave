-- Migration: 202610030001_conversation_part_links.sql
-- Universal edge table for part relationships and adapter projection versioning

CREATE TABLE IF NOT EXISTS conversation_part_links (
    tenant_id     TEXT NOT NULL DEFAULT 'default',
    part_id       TEXT NOT NULL,
    relation      TEXT NOT NULL,
    target_kind   TEXT NOT NULL,
    target_id     TEXT NOT NULL,
    metadata_json TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_id, part_id, relation, target_id)
);

CREATE INDEX IF NOT EXISTS idx_conversation_part_links_target
ON conversation_part_links (tenant_id, target_kind, target_id, relation);

CREATE TABLE IF NOT EXISTS web_record_part_links (
    tenant_id     TEXT NOT NULL DEFAULT 'default',
    part_id       TEXT NOT NULL,
    relation      TEXT NOT NULL,
    target_kind   TEXT NOT NULL,
    target_id     TEXT NOT NULL,
    metadata_json TEXT,
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_id, part_id, relation, target_id)
);

CREATE INDEX IF NOT EXISTS idx_web_record_part_links_target
ON web_record_part_links (tenant_id, target_kind, target_id, relation);

ALTER TABLE conversation_adapters ADD COLUMN projection_version INTEGER;
ALTER TABLE conversation_session_observations ADD COLUMN hydrated_projection_version INTEGER;
