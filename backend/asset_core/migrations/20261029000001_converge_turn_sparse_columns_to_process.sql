-- Migration: 20261029000001_converge_turn_sparse_columns_to_process.sql
--
-- 终态物理 Schema 收缩：
-- 1. 将 conversation_turns 从 18 列缩减为 14 列终态结构，物理删除 title、turn_kind、agent_role、parent_turn_id，将非空稀疏元数据合并到 process JSON 中。
-- 2. 将 web_record_turns 从 18 列缩减为 14 列终态结构，物理删除 title、turn_kind、agent_role、parent_turn_id，将非空稀疏元数据合并到 process JSON 中。

PRAGMA foreign_keys = OFF;

--------------------------------------------------------------------------------
-- 1. 收缩 conversation_turns（18 列 -> 14 列终态）
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
    prompt, process, response, started_at, ended_at,
    model, fingerprint, missing, imported_at
)
SELECT
    tenant_id, id, session_id, external_id, turn_index,
    prompt,
    CASE
        WHEN (title IS NOT NULL AND TRIM(title) != '')
          OR (turn_kind IS NOT NULL AND TRIM(turn_kind) != '')
          OR (agent_role IS NOT NULL AND TRIM(agent_role) != '')
          OR (parent_turn_id IS NOT NULL AND TRIM(parent_turn_id) != '')
        THEN json_patch(
            COALESCE(process, '{}'),
            json_patch(
                json_patch(
                    CASE WHEN title IS NOT NULL AND TRIM(title) != '' THEN json_object('title', title) ELSE '{}' END,
                    CASE WHEN turn_kind IS NOT NULL AND TRIM(turn_kind) != '' THEN json_object('turn_kind', turn_kind) ELSE '{}' END
                ),
                json_patch(
                    CASE WHEN agent_role IS NOT NULL AND TRIM(agent_role) != '' THEN json_object('agent_role', agent_role) ELSE '{}' END,
                    CASE WHEN parent_turn_id IS NOT NULL AND TRIM(parent_turn_id) != '' THEN json_object('parent_turn_id', parent_turn_id) ELSE '{}' END
                )
            )
        )
        ELSE process
    END,
    response, started_at, ended_at,
    model, fingerprint, missing, imported_at
FROM conversation_turns;

DROP TABLE conversation_turns;
ALTER TABLE conversation_turns_new RENAME TO conversation_turns;

CREATE INDEX idx_conversation_turns_tenant_session
ON conversation_turns (tenant_id, session_id);

--------------------------------------------------------------------------------
-- 2. 收缩 web_record_turns（18 列 -> 14 列终态）
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
    prompt, process, response, started_at, ended_at,
    model, fingerprint, missing, imported_at
)
SELECT
    tenant_id, id, session_id, external_id, turn_index,
    prompt,
    CASE
        WHEN (title IS NOT NULL AND TRIM(title) != '')
          OR (turn_kind IS NOT NULL AND TRIM(turn_kind) != '')
          OR (agent_role IS NOT NULL AND TRIM(agent_role) != '')
          OR (parent_turn_id IS NOT NULL AND TRIM(parent_turn_id) != '')
        THEN json_patch(
            COALESCE(process, '{}'),
            json_patch(
                json_patch(
                    CASE WHEN title IS NOT NULL AND TRIM(title) != '' THEN json_object('title', title) ELSE '{}' END,
                    CASE WHEN turn_kind IS NOT NULL AND TRIM(turn_kind) != '' THEN json_object('turn_kind', turn_kind) ELSE '{}' END
                ),
                json_patch(
                    CASE WHEN agent_role IS NOT NULL AND TRIM(agent_role) != '' THEN json_object('agent_role', agent_role) ELSE '{}' END,
                    CASE WHEN parent_turn_id IS NOT NULL AND TRIM(parent_turn_id) != '' THEN json_object('parent_turn_id', parent_turn_id) ELSE '{}' END
                )
            )
        )
        ELSE process
    END,
    response, started_at, ended_at,
    model, fingerprint, missing, imported_at
FROM web_record_turns;

DROP TABLE web_record_turns;
ALTER TABLE web_record_turns_new RENAME TO web_record_turns;

CREATE INDEX idx_web_record_turns_tenant_session
ON web_record_turns (tenant_id, session_id);

PRAGMA foreign_keys = ON;
