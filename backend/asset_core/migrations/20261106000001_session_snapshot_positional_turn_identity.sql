-- Migration: 20261106000001_session_snapshot_positional_turn_identity.sql
--
-- ADR-0021：会话快照整包替换与轮次位置身份。
--
-- 1. 把 conversation_turns / web_record_turns 从 14 列收缩为 11 列纯业务事实表：
--    物理删除 external_id、fingerprint、missing。
-- 2. 按会话把 turn_index 归一化为连续的 0..N，唯一约束改为 (tenant_id, session_id, turn_index)。
-- 3. 把 started_at 收紧为 NOT NULL，按“轮次原生时间 → 会话更新时间 → 会话开始时间 → 导入时间”兜底。
-- 4. 轮次 id 无法在 SQL 内重算（需要 SHA-256），由启动期 Rust 钩子原地重建并级联 Parts / FTS / 父引用。
-- 5. conversation_usage_events 增加 turn_index 列，并按旧 (session, external_id) 对齐回填。

PRAGMA foreign_keys = OFF;

--------------------------------------------------------------------------------
-- 1. 轮次位置映射：表重建与用量事件回填必须共用同一份排序结果
--------------------------------------------------------------------------------
DROP TABLE IF EXISTS _conversation_turn_position_map;
CREATE TABLE _conversation_turn_position_map AS
SELECT
    t.tenant_id                                   AS tenant_id,
    s.source_id                                   AS source_id,
    s.external_id                                 AS session_external_id,
    t.external_id                                 AS turn_external_id,
    t.id                                          AS old_turn_id,
    ROW_NUMBER() OVER (
        PARTITION BY t.tenant_id, t.session_id
        ORDER BY t.turn_index ASC, t.id ASC
    ) - 1                                         AS new_turn_index
FROM conversation_turns t
-- 会话行缺失的孤儿轮次也必须进入映射，否则重建时会静默丢行。
LEFT JOIN conversation_sessions s
  ON s.tenant_id = t.tenant_id AND s.id = t.session_id;

DROP TABLE IF EXISTS _web_record_turn_position_map;
CREATE TABLE _web_record_turn_position_map AS
SELECT
    t.tenant_id                                   AS tenant_id,
    s.source_id                                   AS source_id,
    s.external_id                                 AS session_external_id,
    t.external_id                                 AS turn_external_id,
    t.id                                          AS old_turn_id,
    ROW_NUMBER() OVER (
        PARTITION BY t.tenant_id, t.session_id
        ORDER BY t.turn_index ASC, t.id ASC
    ) - 1                                         AS new_turn_index
FROM web_record_turns t
LEFT JOIN web_record_sessions s
  ON s.tenant_id = t.tenant_id AND s.id = t.session_id;

--------------------------------------------------------------------------------
-- 2. 收缩 conversation_turns（14 列 -> 11 列终态）
--------------------------------------------------------------------------------
CREATE TABLE conversation_turns_new (
    tenant_id       TEXT NOT NULL DEFAULT 'default',
    id              TEXT NOT NULL,
    session_id      TEXT NOT NULL,
    turn_index      INTEGER NOT NULL,
    prompt          TEXT NOT NULL DEFAULT '',
    process         TEXT,
    response        TEXT,
    started_at      TEXT NOT NULL,
    ended_at        TEXT,
    model           TEXT,
    imported_at     TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, session_id, turn_index)
);

INSERT INTO conversation_turns_new (
    tenant_id, id, session_id, turn_index, prompt, process, response,
    started_at, ended_at, model, imported_at
)
SELECT
    t.tenant_id,
    t.id,
    t.session_id,
    m.new_turn_index,
    t.prompt,
    t.process,
    t.response,
    COALESCE(
        NULLIF(TRIM(t.started_at), ''),
        NULLIF(TRIM(s.updated_at), ''),
        NULLIF(TRIM(s.started_at), ''),
        NULLIF(TRIM(t.imported_at), ''),
        '1970-01-01T00:00:00+00:00'
    ),
    t.ended_at,
    t.model,
    t.imported_at
FROM conversation_turns t
JOIN _conversation_turn_position_map m
  ON m.tenant_id = t.tenant_id AND m.old_turn_id = t.id
LEFT JOIN conversation_sessions s
  ON s.tenant_id = t.tenant_id AND s.id = t.session_id;

DROP TABLE conversation_turns;
ALTER TABLE conversation_turns_new RENAME TO conversation_turns;

CREATE INDEX idx_conversation_turns_tenant_session
ON conversation_turns (tenant_id, session_id);

--------------------------------------------------------------------------------
-- 3. 收缩 web_record_turns（14 列 -> 11 列终态）
--------------------------------------------------------------------------------
CREATE TABLE web_record_turns_new (
    tenant_id       TEXT NOT NULL DEFAULT 'default',
    id              TEXT NOT NULL,
    session_id      TEXT NOT NULL,
    turn_index      INTEGER NOT NULL,
    prompt          TEXT NOT NULL DEFAULT '',
    process         TEXT,
    response        TEXT,
    started_at      TEXT NOT NULL,
    ended_at        TEXT,
    model           TEXT,
    imported_at     TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, session_id, turn_index)
);

INSERT INTO web_record_turns_new (
    tenant_id, id, session_id, turn_index, prompt, process, response,
    started_at, ended_at, model, imported_at
)
SELECT
    t.tenant_id,
    t.id,
    t.session_id,
    m.new_turn_index,
    t.prompt,
    t.process,
    t.response,
    COALESCE(
        NULLIF(TRIM(t.started_at), ''),
        NULLIF(TRIM(s.updated_at), ''),
        NULLIF(TRIM(s.started_at), ''),
        NULLIF(TRIM(t.imported_at), ''),
        '1970-01-01T00:00:00+00:00'
    ),
    t.ended_at,
    t.model,
    t.imported_at
FROM web_record_turns t
JOIN _web_record_turn_position_map m
  ON m.tenant_id = t.tenant_id AND m.old_turn_id = t.id
LEFT JOIN web_record_sessions s
  ON s.tenant_id = t.tenant_id AND s.id = t.session_id;

DROP TABLE web_record_turns;
ALTER TABLE web_record_turns_new RENAME TO web_record_turns;

CREATE INDEX idx_web_record_turns_tenant_session
ON web_record_turns (tenant_id, session_id);

--------------------------------------------------------------------------------
-- 4. 用量事件按 turn_index 归因
--------------------------------------------------------------------------------
ALTER TABLE conversation_usage_events ADD COLUMN turn_index INTEGER;

-- 4.1 精确对齐：旧 turn_id 恰好等于旧轮次的 external_id
UPDATE conversation_usage_events
SET turn_index = (
    SELECT m.new_turn_index
    FROM _conversation_turn_position_map m
    WHERE m.tenant_id = conversation_usage_events.tenant_id
      AND m.source_id = conversation_usage_events.source_id
      AND m.session_external_id = conversation_usage_events.session_id
      AND m.turn_external_id = conversation_usage_events.turn_id
    LIMIT 1
)
WHERE conversation_usage_events.session_id IS NOT NULL
  AND conversation_usage_events.turn_id IS NOT NULL
  AND TRIM(conversation_usage_events.turn_id) <> '';

UPDATE conversation_usage_events
SET turn_index = (
    SELECT m.new_turn_index
    FROM _web_record_turn_position_map m
    WHERE m.tenant_id = conversation_usage_events.tenant_id
      AND m.source_id = conversation_usage_events.source_id
      AND m.session_external_id = conversation_usage_events.session_id
      AND m.turn_external_id = conversation_usage_events.turn_id
    LIMIT 1
)
WHERE conversation_usage_events.turn_index IS NULL
  AND conversation_usage_events.session_id IS NOT NULL
  AND conversation_usage_events.turn_id IS NOT NULL
  AND TRIM(conversation_usage_events.turn_id) <> '';

-- 4.2 序号回退：宿主用 `turn-N` 约定时按序号对齐，且必须能命中真实轮次
UPDATE conversation_usage_events
SET turn_index = CAST(substr(turn_id, 6) AS INTEGER)
WHERE turn_index IS NULL
  AND session_id IS NOT NULL
  AND turn_id GLOB 'turn-[0-9]*'
  AND substr(turn_id, 6) NOT GLOB '*[^0-9]*'
  AND EXISTS (
        SELECT 1
        FROM conversation_turns t
        JOIN conversation_sessions s
          ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE t.tenant_id = conversation_usage_events.tenant_id
          AND s.source_id = conversation_usage_events.source_id
          AND s.external_id = conversation_usage_events.session_id
          AND t.turn_index = CAST(substr(conversation_usage_events.turn_id, 6) AS INTEGER)
        UNION ALL
        SELECT 1
        FROM web_record_turns t
        JOIN web_record_sessions s
          ON s.tenant_id = t.tenant_id AND s.id = t.session_id
        WHERE t.tenant_id = conversation_usage_events.tenant_id
          AND s.source_id = conversation_usage_events.source_id
          AND s.external_id = conversation_usage_events.session_id
          AND t.turn_index = CAST(substr(conversation_usage_events.turn_id, 6) AS INTEGER)
  );

CREATE INDEX idx_conversation_usage_events_tenant_session_turn
ON conversation_usage_events (tenant_id, session_id, turn_index);

DROP TABLE _conversation_turn_position_map;
DROP TABLE _web_record_turn_position_map;

PRAGMA foreign_keys = ON;
