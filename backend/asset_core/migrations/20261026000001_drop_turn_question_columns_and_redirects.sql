-- Migration: 202610260001_drop_turn_question_columns_and_redirects.sql
--
-- 终态物理 Schema 收缩：
-- 1. 将 conversation_turns 从 17 列缩减为 15 列终态结构，物理删除 question_id 与 question_origin。
-- 2. 将 web_record_turns 从 17 列缩减为 15 列终态结构，物理删除 question_id 与 question_origin。
-- 3. 彻底删除遗留的 conversation_question_redirects 跳转墓碑表。
-- 4. 删除旧版 question 相关索引，仅保留 tenant_session 索引。

PRAGMA foreign_keys = OFF;

--------------------------------------------------------------------------------
-- 1. 收缩 conversation_turns（17 列 -> 15 列终态）
--------------------------------------------------------------------------------
CREATE TABLE conversation_turns_new (
    tenant_id       TEXT NOT NULL DEFAULT 'default',
    id              TEXT NOT NULL,
    session_id      TEXT NOT NULL,
    external_id     TEXT NOT NULL,
    turn_index      INTEGER NOT NULL,
    prompt          TEXT NOT NULL DEFAULT '',
    process         TEXT,
    response        TEXT,
    title           TEXT,
    started_at      TEXT,
    ended_at        TEXT,
    model           TEXT,
    fingerprint     TEXT NOT NULL,
    missing         INTEGER NOT NULL DEFAULT 0,
    imported_at     TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, session_id, external_id)
);

INSERT INTO conversation_turns_new (
    tenant_id, id, session_id, external_id, turn_index,
    prompt, process, response, title, started_at, ended_at,
    model, fingerprint, missing, imported_at
)
SELECT
    tenant_id, id, session_id, external_id, turn_index,
    prompt, process, response, title, started_at, ended_at,
    model, fingerprint, missing, imported_at
FROM conversation_turns;

DROP TABLE conversation_turns;
ALTER TABLE conversation_turns_new RENAME TO conversation_turns;

DROP INDEX IF EXISTS idx_conversation_turns_tenant_session_question;
DROP INDEX IF EXISTS idx_conversation_turns_tenant_question;

CREATE INDEX idx_conversation_turns_tenant_session
ON conversation_turns (tenant_id, session_id);

--------------------------------------------------------------------------------
-- 2. 收缩 web_record_turns（17 列 -> 15 列终态）
--------------------------------------------------------------------------------
CREATE TABLE web_record_turns_new (
    tenant_id       TEXT NOT NULL DEFAULT 'default',
    id              TEXT NOT NULL,
    session_id      TEXT NOT NULL,
    external_id     TEXT NOT NULL,
    turn_index      INTEGER NOT NULL,
    prompt          TEXT NOT NULL DEFAULT '',
    process         TEXT,
    response        TEXT,
    title           TEXT,
    started_at      TEXT,
    ended_at        TEXT,
    model           TEXT,
    fingerprint     TEXT NOT NULL,
    missing         INTEGER NOT NULL DEFAULT 0,
    imported_at     TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, session_id, external_id)
);

INSERT INTO web_record_turns_new (
    tenant_id, id, session_id, external_id, turn_index,
    prompt, process, response, title, started_at, ended_at,
    model, fingerprint, missing, imported_at
)
SELECT
    tenant_id, id, session_id, external_id, turn_index,
    prompt, process, response, title, started_at, ended_at,
    model, fingerprint, missing, imported_at
FROM web_record_turns;

DROP TABLE web_record_turns;
ALTER TABLE web_record_turns_new RENAME TO web_record_turns;

DROP INDEX IF EXISTS idx_web_record_turns_tenant_session_question;
DROP INDEX IF EXISTS idx_web_record_turns_tenant_question;

CREATE INDEX idx_web_record_turns_tenant_session
ON web_record_turns (tenant_id, session_id);

--------------------------------------------------------------------------------
-- 3. 彻底删除遗留的 conversation_question_redirects 表
--------------------------------------------------------------------------------
DROP TABLE IF EXISTS conversation_question_redirects;

PRAGMA foreign_keys = ON;
