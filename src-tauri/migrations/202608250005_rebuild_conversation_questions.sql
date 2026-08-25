PRAGMA foreign_keys = OFF;

INSERT INTO conversation_data_audit_issues (
    tenant_id, id, category, fingerprint, severity, auto_repairable, status,
    affected_count, sample_ids_json, details_json, first_seen_at, last_seen_at
)
SELECT
    t.id,
    'question-snapshot-' || t.id,
    'question_snapshot_dependencies',
    'question_snapshot_dependencies',
    'warning',
    0,
    'open',
    COALESCE((SELECT COUNT(*) FROM conversation_questions q WHERE q.tenant_id = t.id AND (
        length(trim(q.question_text)) > 0 OR length(trim(q.answer_text)) > 0 OR
        length(trim(q.code_text)) > 0 OR length(trim(q.command_text)) > 0
    )), 0) + COALESCE((SELECT COUNT(*) FROM web_record_questions q WHERE q.tenant_id = t.id AND (
        length(trim(q.question_text)) > 0 OR length(trim(q.answer_text)) > 0 OR
        length(trim(q.code_text)) > 0 OR length(trim(q.command_text)) > 0
    )), 0),
    '[]',
    '{"message":"Question snapshots were preserved as an audit finding before the contract rebuild"}',
    '1970-01-01T00:00:00Z',
    '1970-01-01T00:00:00Z'
FROM tenants t
WHERE EXISTS (SELECT 1 FROM conversation_questions q WHERE q.tenant_id = t.id AND (
    length(trim(q.question_text)) > 0 OR length(trim(q.answer_text)) > 0 OR
    length(trim(q.code_text)) > 0 OR length(trim(q.command_text)) > 0
)) OR EXISTS (SELECT 1 FROM web_record_questions q WHERE q.tenant_id = t.id AND (
    length(trim(q.question_text)) > 0 OR length(trim(q.answer_text)) > 0 OR
    length(trim(q.code_text)) > 0 OR length(trim(q.command_text)) > 0
))
ON CONFLICT (tenant_id, fingerprint, status) DO UPDATE SET
    affected_count = excluded.affected_count,
    last_seen_at = excluded.last_seen_at,
    details_json = excluded.details_json;

CREATE TABLE conversation_questions_rebuild (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    title TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, session_id)
        REFERENCES conversation_sessions(tenant_id, id)
        ON DELETE CASCADE
);

INSERT INTO conversation_questions_rebuild (
    tenant_id, id, session_id, title, created_at, updated_at
)
SELECT tenant_id, id, session_id,
       COALESCE(NULLIF(trim(title), ''), NULLIF(substr(question_text, 1, 200), '')),
       created_at, updated_at
FROM conversation_questions;

DROP TABLE conversation_questions;
ALTER TABLE conversation_questions_rebuild RENAME TO conversation_questions;

CREATE INDEX idx_conversation_questions_tenant_session_order
    ON conversation_questions (tenant_id, session_id, created_at, id);
CREATE INDEX idx_memory_recall_questions_created
    ON conversation_questions (tenant_id, created_at DESC, session_id);

CREATE TABLE web_record_questions_rebuild (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    title TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, session_id)
        REFERENCES web_record_sessions(tenant_id, id)
        ON DELETE CASCADE
);

INSERT INTO web_record_questions_rebuild (
    tenant_id, id, session_id, title, created_at, updated_at
)
SELECT tenant_id, id, session_id,
       COALESCE(NULLIF(trim(title), ''), NULLIF(substr(question_text, 1, 200), '')),
       created_at, updated_at
FROM web_record_questions;

DROP TABLE web_record_questions;
ALTER TABLE web_record_questions_rebuild RENAME TO web_record_questions;

CREATE INDEX idx_web_record_questions_tenant_session_order
    ON web_record_questions (tenant_id, session_id, created_at, id);
CREATE INDEX idx_memory_recall_web_questions_created
    ON web_record_questions (tenant_id, created_at DESC, session_id);

PRAGMA foreign_keys = ON;
