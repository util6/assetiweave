CREATE TABLE memory_evidence_remap_audits (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('session', 'web')),
    session_id TEXT NOT NULL,
    previous_question_id TEXT,
    turn_id TEXT,
    part_id TEXT,
    block_id TEXT NOT NULL,
    reason TEXT NOT NULL CHECK (
        reason IN (
            'ambiguous_question',
            'question_missing',
            'locator_missing',
            'locator_mismatch',
            'source_unavailable'
        )
    ),
    candidate_question_ids_json TEXT NOT NULL CHECK (json_valid(candidate_question_ids_json)),
    status TEXT NOT NULL CHECK (status IN ('open', 'resolved', 'ignored')),
    detected_at TEXT NOT NULL,
    resolved_at TEXT,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, evidence_id)
        REFERENCES memory_evidence_snapshots (tenant_id, id)
        ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_memory_evidence_remap_audits_open
    ON memory_evidence_remap_audits (tenant_id, evidence_id)
    WHERE status = 'open';

CREATE INDEX idx_memory_evidence_remap_audits_session
    ON memory_evidence_remap_audits (tenant_id, record_kind, session_id, status);
