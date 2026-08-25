PRAGMA foreign_keys = OFF;

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
SELECT tenant_id, id, session_id, title, created_at, updated_at
FROM conversation_questions;

DROP TABLE conversation_questions;
ALTER TABLE conversation_questions_rebuild RENAME TO conversation_questions;

CREATE INDEX idx_conversation_questions_tenant_session_order
    ON conversation_questions (tenant_id, session_id, created_at, id);

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
SELECT tenant_id, id, session_id, title, created_at, updated_at
FROM web_record_questions;

DROP TABLE web_record_questions;
ALTER TABLE web_record_questions_rebuild RENAME TO web_record_questions;

CREATE INDEX idx_web_record_questions_tenant_session_order
    ON web_record_questions (tenant_id, session_id, created_at, id);

PRAGMA foreign_keys = ON;
