CREATE TABLE domain_event_outbox_v2 (
    seq            INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id       TEXT NOT NULL UNIQUE,
    tenant_id      TEXT NOT NULL,
    event_type     TEXT NOT NULL,
    source_id      TEXT,
    revision_start INTEGER,
    revision_end   INTEGER,
    payload        TEXT NOT NULL,
    created_at     TEXT NOT NULL
);

INSERT INTO domain_event_outbox_v2 (
    seq, event_id, tenant_id, event_type, source_id,
    revision_start, revision_end, payload, created_at
)
SELECT seq, event_id, tenant_id, event_type, source_id,
       revision_start, revision_end, payload, created_at
FROM domain_event_outbox;

DROP TABLE domain_event_outbox;
ALTER TABLE domain_event_outbox_v2 RENAME TO domain_event_outbox;

CREATE INDEX idx_outbox_tenant_seq
ON domain_event_outbox(tenant_id, seq);

CREATE TABLE domain_event_consumer_offsets_v2 (
    consumer_id TEXT NOT NULL,
    tenant_id TEXT NOT NULL,
    last_seq INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (consumer_id, tenant_id)
);

INSERT INTO domain_event_consumer_offsets_v2 (
    consumer_id, tenant_id, last_seq, updated_at
)
SELECT consumer_id, tenant_id, last_seq, updated_at
FROM domain_event_consumer_offsets;

DROP TABLE domain_event_consumer_offsets;
ALTER TABLE domain_event_consumer_offsets_v2 RENAME TO domain_event_consumer_offsets;
