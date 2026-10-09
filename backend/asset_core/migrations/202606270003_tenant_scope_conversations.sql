PRAGMA foreign_keys = OFF;

CREATE TABLE conversation_adapters_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    version TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    manifest_path TEXT,
    executable_path TEXT,
    content_hash TEXT,
    trusted_hash TEXT,
    trust_state TEXT NOT NULL,
    protocol_version INTEGER,
    capabilities TEXT NOT NULL,
    input_kinds TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id)
);

INSERT INTO conversation_adapters_new (
    tenant_id, id, name, kind, version, enabled, manifest_path, executable_path,
    content_hash, trusted_hash, trust_state, protocol_version, capabilities,
    input_kinds, created_at, updated_at
)
SELECT
    'default', id, name, kind, version, enabled, manifest_path, executable_path,
    content_hash, trusted_hash, trust_state, protocol_version, capabilities,
    input_kinds, created_at, updated_at
FROM conversation_adapters;

DROP TABLE conversation_adapters;
ALTER TABLE conversation_adapters_new RENAME TO conversation_adapters;

CREATE INDEX idx_conversation_adapters_tenant_order
ON conversation_adapters(tenant_id, kind, name);

CREATE TABLE conversation_sources_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    location TEXT NOT NULL,
    config_json TEXT,
    enabled INTEGER NOT NULL,
    last_synced_at TEXT,
    last_sync_status TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id)
);

INSERT INTO conversation_sources_new (
    tenant_id, id, adapter_id, name, kind, location, config_json, enabled,
    last_synced_at, last_sync_status, created_at, updated_at
)
SELECT
    'default', id, adapter_id, name, kind, location, config_json, enabled,
    last_synced_at, last_sync_status, created_at, updated_at
FROM conversation_sources;

DROP TABLE conversation_sources;
ALTER TABLE conversation_sources_new RENAME TO conversation_sources;

CREATE INDEX idx_conversation_sources_tenant_adapter
ON conversation_sources(tenant_id, adapter_id, name);

CREATE TABLE conversation_sessions_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    external_id TEXT NOT NULL,
    title TEXT NOT NULL,
    project_path TEXT,
    started_at TEXT,
    updated_at TEXT,
    source_locator TEXT,
    source_fingerprint TEXT,
    missing INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE(tenant_id, source_id, external_id)
);

INSERT INTO conversation_sessions_new (
    tenant_id, id, source_id, adapter_id, external_id, title, project_path,
    started_at, updated_at, source_locator, source_fingerprint, missing,
    created_at, imported_at
)
SELECT
    'default', id, source_id, adapter_id, external_id, title, project_path,
    started_at, updated_at, source_locator, source_fingerprint, missing,
    created_at, imported_at
FROM conversation_sessions;

DROP TABLE conversation_sessions;
ALTER TABLE conversation_sessions_new RENAME TO conversation_sessions;

CREATE INDEX idx_conversation_sessions_tenant_source
ON conversation_sessions(tenant_id, source_id, missing);
CREATE INDEX idx_conversation_sessions_tenant_adapter
ON conversation_sessions(tenant_id, adapter_id);

CREATE TABLE conversation_turns_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    external_id TEXT NOT NULL,
    turn_index INTEGER NOT NULL,
    user_text TEXT NOT NULL,
    title TEXT,
    started_at TEXT,
    ended_at TEXT,
    fingerprint TEXT NOT NULL,
    missing INTEGER NOT NULL,
    imported_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE(tenant_id, session_id, external_id)
);

INSERT INTO conversation_turns_new (
    tenant_id, id, session_id, external_id, turn_index, user_text, title,
    started_at, ended_at, fingerprint, missing, imported_at
)
SELECT
    'default', id, session_id, external_id, turn_index, user_text, title,
    started_at, ended_at, fingerprint, missing, imported_at
FROM conversation_turns;

DROP TABLE conversation_turns;
ALTER TABLE conversation_turns_new RENAME TO conversation_turns;

CREATE INDEX idx_conversation_turns_tenant_session
ON conversation_turns(tenant_id, session_id, turn_index);

CREATE TABLE conversation_parts_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    part_index INTEGER NOT NULL,
    role TEXT NOT NULL,
    kind TEXT NOT NULL,
    text TEXT,
    language TEXT,
    command TEXT,
    cwd TEXT,
    status TEXT,
    exit_code INTEGER,
    metadata_json TEXT,
    PRIMARY KEY (tenant_id, id),
    UNIQUE(tenant_id, turn_id, part_index)
);

INSERT INTO conversation_parts_new (
    tenant_id, id, turn_id, part_index, role, kind, text, language, command,
    cwd, status, exit_code, metadata_json
)
SELECT
    'default', id, turn_id, part_index, role, kind, text, language, command,
    cwd, status, exit_code, metadata_json
FROM conversation_parts;

DROP TABLE conversation_parts;
ALTER TABLE conversation_parts_new RENAME TO conversation_parts;

CREATE INDEX idx_conversation_parts_tenant_turn
ON conversation_parts(tenant_id, turn_id, part_index);

CREATE TABLE conversation_questions_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    question_index INTEGER NOT NULL,
    title TEXT,
    question_text TEXT NOT NULL,
    answer_text TEXT NOT NULL,
    code_text TEXT NOT NULL,
    command_text TEXT NOT NULL,
    grouping_origin TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE(tenant_id, session_id, question_index)
);

INSERT INTO conversation_questions_new (
    tenant_id, id, session_id, question_index, title, question_text, answer_text,
    code_text, command_text, grouping_origin, created_at, updated_at
)
SELECT
    'default', id, session_id, question_index, title, question_text, answer_text,
    code_text, command_text, grouping_origin, created_at, updated_at
FROM conversation_questions;

DROP TABLE conversation_questions;
ALTER TABLE conversation_questions_new RENAME TO conversation_questions;

CREATE INDEX idx_conversation_questions_tenant_session
ON conversation_questions(tenant_id, session_id, question_index);

CREATE TABLE conversation_question_turns_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    question_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    turn_order INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, question_id, turn_id),
    UNIQUE(tenant_id, turn_id)
);

INSERT INTO conversation_question_turns_new (
    tenant_id, question_id, turn_id, turn_order
)
SELECT 'default', question_id, turn_id, turn_order
FROM conversation_question_turns;

DROP TABLE conversation_question_turns;
ALTER TABLE conversation_question_turns_new RENAME TO conversation_question_turns;

CREATE TABLE web_record_sessions_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    external_id TEXT NOT NULL,
    title TEXT NOT NULL,
    project_path TEXT,
    started_at TEXT,
    updated_at TEXT,
    source_locator TEXT,
    source_fingerprint TEXT,
    missing INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE(tenant_id, source_id, external_id)
);

INSERT INTO web_record_sessions_new (
    tenant_id, id, source_id, adapter_id, external_id, title, project_path,
    started_at, updated_at, source_locator, source_fingerprint, missing,
    created_at, imported_at
)
SELECT
    'default', id, source_id, adapter_id, external_id, title, project_path,
    started_at, updated_at, source_locator, source_fingerprint, missing,
    created_at, imported_at
FROM web_record_sessions;

DROP TABLE web_record_sessions;
ALTER TABLE web_record_sessions_new RENAME TO web_record_sessions;

CREATE INDEX idx_web_record_sessions_tenant_source
ON web_record_sessions(tenant_id, source_id, missing);
CREATE INDEX idx_web_record_sessions_tenant_adapter
ON web_record_sessions(tenant_id, adapter_id);

CREATE TABLE web_record_turns_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    external_id TEXT NOT NULL,
    turn_index INTEGER NOT NULL,
    user_text TEXT NOT NULL,
    title TEXT,
    started_at TEXT,
    ended_at TEXT,
    fingerprint TEXT NOT NULL,
    missing INTEGER NOT NULL,
    imported_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE(tenant_id, session_id, external_id)
);

INSERT INTO web_record_turns_new (
    tenant_id, id, session_id, external_id, turn_index, user_text, title,
    started_at, ended_at, fingerprint, missing, imported_at
)
SELECT
    'default', id, session_id, external_id, turn_index, user_text, title,
    started_at, ended_at, fingerprint, missing, imported_at
FROM web_record_turns;

DROP TABLE web_record_turns;
ALTER TABLE web_record_turns_new RENAME TO web_record_turns;

CREATE INDEX idx_web_record_turns_tenant_session
ON web_record_turns(tenant_id, session_id, turn_index);

CREATE TABLE web_record_parts_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    part_index INTEGER NOT NULL,
    role TEXT NOT NULL,
    kind TEXT NOT NULL,
    text TEXT,
    language TEXT,
    command TEXT,
    cwd TEXT,
    status TEXT,
    exit_code INTEGER,
    metadata_json TEXT,
    PRIMARY KEY (tenant_id, id),
    UNIQUE(tenant_id, turn_id, part_index)
);

INSERT INTO web_record_parts_new (
    tenant_id, id, turn_id, part_index, role, kind, text, language, command,
    cwd, status, exit_code, metadata_json
)
SELECT
    'default', id, turn_id, part_index, role, kind, text, language, command,
    cwd, status, exit_code, metadata_json
FROM web_record_parts;

DROP TABLE web_record_parts;
ALTER TABLE web_record_parts_new RENAME TO web_record_parts;

CREATE INDEX idx_web_record_parts_tenant_turn
ON web_record_parts(tenant_id, turn_id, part_index);

CREATE TABLE web_record_questions_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    question_index INTEGER NOT NULL,
    title TEXT,
    question_text TEXT NOT NULL,
    answer_text TEXT NOT NULL,
    code_text TEXT NOT NULL,
    command_text TEXT NOT NULL,
    grouping_origin TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE(tenant_id, session_id, question_index)
);

INSERT INTO web_record_questions_new (
    tenant_id, id, session_id, question_index, title, question_text, answer_text,
    code_text, command_text, grouping_origin, created_at, updated_at
)
SELECT
    'default', id, session_id, question_index, title, question_text, answer_text,
    code_text, command_text, grouping_origin, created_at, updated_at
FROM web_record_questions;

DROP TABLE web_record_questions;
ALTER TABLE web_record_questions_new RENAME TO web_record_questions;

CREATE INDEX idx_web_record_questions_tenant_session
ON web_record_questions(tenant_id, session_id, question_index);

CREATE TABLE web_record_question_turns_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    question_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    turn_order INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, question_id, turn_id),
    UNIQUE(tenant_id, turn_id)
);

INSERT INTO web_record_question_turns_new (
    tenant_id, question_id, turn_id, turn_order
)
SELECT 'default', question_id, turn_id, turn_order
FROM web_record_question_turns;

DROP TABLE web_record_question_turns;
ALTER TABLE web_record_question_turns_new RENAME TO web_record_question_turns;

CREATE TABLE conversation_sync_runs_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    source_id TEXT,
    adapter_id TEXT,
    status TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    session_count INTEGER NOT NULL,
    turn_count INTEGER NOT NULL,
    warning_count INTEGER NOT NULL,
    error_message TEXT,
    PRIMARY KEY (tenant_id, id)
);

INSERT INTO conversation_sync_runs_new (
    tenant_id, id, source_id, adapter_id, status, started_at, finished_at,
    session_count, turn_count, warning_count, error_message
)
SELECT
    'default', id, source_id, adapter_id, status, started_at, finished_at,
    session_count, turn_count, warning_count, error_message
FROM conversation_sync_runs;

DROP TABLE conversation_sync_runs;
ALTER TABLE conversation_sync_runs_new RENAME TO conversation_sync_runs;

CREATE INDEX idx_conversation_sync_runs_tenant_started
ON conversation_sync_runs(tenant_id, started_at);

CREATE VIRTUAL TABLE conversation_question_fts_new USING fts5(
    tenant_id UNINDEXED,
    question_id UNINDEXED,
    session_id UNINDEXED,
    question_text,
    answer_text,
    code_text,
    command_text
);

INSERT INTO conversation_question_fts_new (
    tenant_id, question_id, session_id, question_text, answer_text, code_text, command_text
)
SELECT
    'default', question_id, session_id, question_text, answer_text, code_text, command_text
FROM conversation_question_fts;

DROP TABLE conversation_question_fts;
ALTER TABLE conversation_question_fts_new RENAME TO conversation_question_fts;

PRAGMA foreign_keys = ON;
