CREATE TABLE conversation_session_observations (
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('session', 'web')),
    external_id TEXT NOT NULL,
    observed_version TEXT NOT NULL,
    hydrated_version TEXT,
    last_seen_at TEXT NOT NULL,
    source_presence TEXT NOT NULL CHECK (source_presence IN ('present', 'absent', 'unknown')),
    dirty INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, source_id, record_kind, external_id)
);

CREATE INDEX idx_conversation_session_observations_dirty
ON conversation_session_observations(tenant_id, source_id, record_kind, dirty);

INSERT INTO conversation_session_observations (
    tenant_id, source_id, record_kind, external_id, observed_version,
    hydrated_version, last_seen_at, source_presence, dirty
)
SELECT
    tenant_id, source_id, 'session', external_id, source_fingerprint,
    source_fingerprint, imported_at, 'unknown', 0
FROM conversation_sessions
WHERE source_fingerprint IS NOT NULL;

INSERT INTO conversation_session_observations (
    tenant_id, source_id, record_kind, external_id, observed_version,
    hydrated_version, last_seen_at, source_presence, dirty
)
SELECT
    tenant_id, source_id, 'web', external_id, source_fingerprint,
    source_fingerprint, imported_at, 'unknown', 0
FROM web_record_sessions
WHERE source_fingerprint IS NOT NULL;
