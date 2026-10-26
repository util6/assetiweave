-- Migration: 202610250001_turn_triad_and_conversation_parts_slimming.sql
--
-- 重构 Conversation 与 Web Record 轮次为三段式骨架（prompt / process / response），
-- 并将 conversation_parts 从 11 列精简为 8 列终态结构（与 web_record_parts 7 列共享同名核心列），
-- 一次性完成存量历史数据的自愈、压缩与净化。

--------------------------------------------------------------------------------
-- 1. 自愈 conversation_parts 存量缺失类型
--------------------------------------------------------------------------------
UPDATE conversation_parts
SET card_kind = COALESCE(
    card_kind,
    json_extract(metadata_json, '$.content_card.kind'),
    json_extract(metadata_json, '$.content_card.type'),
    CASE
        WHEN kind = 'command' THEN 'codex.command'
        WHEN kind = 'text' AND role = 'assistant' THEN 'codex.answer'
        WHEN kind = 'text' AND role = 'user' THEN 'codex.user'
        WHEN kind = 'code_block' THEN 'codex.code'
        WHEN kind = 'file_change' THEN 'codex.file-change'
        ELSE 'codex.' || kind
    END
)
WHERE card_kind IS NULL OR trim(card_kind) = '';

--------------------------------------------------------------------------------
-- 2. 提取并准备命令项（中间暂存表）
--------------------------------------------------------------------------------
CREATE TEMP TABLE temp_command_items AS
SELECT
    tenant_id,
    turn_id,
    part_index AS seq,
    card_kind AS kind,
    json_extract(attrs_json, '$.command_label') AS label,
    content,
    json_extract(attrs_json, '$.signal') AS signal,
    json_extract(attrs_json, '$.cwd') AS cwd,
    CASE
        WHEN CAST(json_extract(attrs_json, '$.exit_code') AS INTEGER) = 0 THEN NULL
        ELSE CAST(json_extract(attrs_json, '$.exit_code') AS INTEGER)
    END AS exit_code,
    json_extract(attrs_json, '$.summary') AS summary,
    CASE
        WHEN lower(trim(json_extract(attrs_json, '$.status'))) IN ('completed', 'success', 'ok', 'done') THEN NULL
        ELSE json_extract(attrs_json, '$.status')
    END AS status,
    json_extract(attrs_json, '$.exec_id') AS exec_id
FROM conversation_parts
WHERE kind = 'command' OR card_kind LIKE '%.command' OR card_kind = 'command';

--------------------------------------------------------------------------------
-- 3. 吸收空壳 .result 上的非零 exit_code 与失败 status 回填至对应命令
--------------------------------------------------------------------------------
UPDATE temp_command_items
SET
    exit_code = COALESCE(
        temp_command_items.exit_code,
        (
            SELECT CAST(json_extract(er.attrs_json, '$.exit_code') AS INTEGER)
            FROM conversation_parts er
            WHERE er.tenant_id = temp_command_items.tenant_id
              AND er.turn_id = temp_command_items.turn_id
              AND (er.card_kind LIKE '%.result' OR er.card_kind = 'result')
              AND (er.content IS NULL OR trim(er.content) = '')
              AND json_extract(er.attrs_json, '$.exec_id') IS NOT NULL
              AND json_extract(er.attrs_json, '$.exec_id') = temp_command_items.exec_id
              AND CAST(json_extract(er.attrs_json, '$.exit_code') AS INTEGER) != 0
            LIMIT 1
        )
    ),
    status = COALESCE(
        temp_command_items.status,
        (
            SELECT json_extract(er.attrs_json, '$.status')
            FROM conversation_parts er
            WHERE er.tenant_id = temp_command_items.tenant_id
              AND er.turn_id = temp_command_items.turn_id
              AND (er.card_kind LIKE '%.result' OR er.card_kind = 'result')
              AND (er.content IS NULL OR trim(er.content) = '')
              AND json_extract(er.attrs_json, '$.exec_id') IS NOT NULL
              AND json_extract(er.attrs_json, '$.exec_id') = temp_command_items.exec_id
              AND lower(trim(json_extract(er.attrs_json, '$.status'))) NOT IN ('completed', 'success', 'ok', 'done')
            LIMIT 1
        )
    );

--------------------------------------------------------------------------------
-- 4. 提取命令的轮次公共 cwd 至顶层，单条命令与顶层相同时省略
--------------------------------------------------------------------------------
CREATE TEMP TABLE temp_turn_cwds AS
SELECT
    tenant_id,
    turn_id,
    MIN(cwd) AS common_cwd
FROM temp_command_items
WHERE cwd IS NOT NULL AND trim(cwd) != ''
GROUP BY tenant_id, turn_id
HAVING COUNT(DISTINCT cwd) = 1;

UPDATE temp_command_items
SET cwd = NULL
WHERE (tenant_id, turn_id, cwd) IN (
    SELECT tenant_id, turn_id, common_cwd FROM temp_turn_cwds
);

--------------------------------------------------------------------------------
-- 5. 识别每个轮次的唯一最终响应（位于所有工具/命令之后末尾的 answer）
--------------------------------------------------------------------------------
CREATE TEMP TABLE temp_final_responses AS
WITH tool_max AS (
    SELECT
        tenant_id,
        turn_id,
        MAX(part_index) AS max_tool_idx
    FROM conversation_parts
    WHERE kind = 'command'
       OR card_kind LIKE '%.command'
       OR card_kind = 'command'
       OR card_kind LIKE '%.result'
       OR card_kind = 'result'
    GROUP BY tenant_id, turn_id
),
answer_candidates AS (
    SELECT
        cp.tenant_id,
        cp.turn_id,
        cp.part_index,
        cp.content,
        cp.translated_text
    FROM conversation_parts cp
    LEFT JOIN tool_max tm ON cp.tenant_id = tm.tenant_id AND cp.turn_id = tm.turn_id
    WHERE (cp.card_kind LIKE '%.answer' OR cp.card_kind = 'answer' OR (cp.role = 'assistant' AND cp.kind = 'text' AND cp.card_kind NOT LIKE '%.reasoning' AND cp.card_kind NOT LIKE '%.code'))
      AND cp.card_kind NOT LIKE '%.reasoning'
      AND (cp.part_index > tm.max_tool_idx OR tm.max_tool_idx IS NULL)
),
ranked_answers AS (
    SELECT
        tenant_id,
        turn_id,
        part_index,
        content,
        translated_text,
        ROW_NUMBER() OVER (PARTITION BY tenant_id, turn_id ORDER BY part_index DESC) AS rn
    FROM answer_candidates
)
SELECT
    tenant_id,
    turn_id,
    part_index AS response_seq,
    content AS response_content,
    translated_text AS response_translated_text
FROM ranked_answers
WHERE rn = 1;

--------------------------------------------------------------------------------
-- 6. 中途伪 .answer 归位为 .reasoning
--------------------------------------------------------------------------------
UPDATE conversation_parts
SET card_kind = CASE
    WHEN card_kind LIKE '%.answer' THEN REPLACE(card_kind, '.answer', '.reasoning')
    ELSE 'codex.reasoning'
END
WHERE (card_kind LIKE '%.answer' OR card_kind = 'answer' OR (role = 'assistant' AND kind = 'text' AND card_kind NOT LIKE '%.reasoning' AND card_kind NOT LIKE '%.code'))
  AND card_kind NOT LIKE '%.reasoning'
  AND (tenant_id, turn_id, part_index) NOT IN (
      SELECT tenant_id, turn_id, response_seq FROM temp_final_responses
  );

--------------------------------------------------------------------------------
-- 7. 聚合命令项为 JSON 数组并构建 turn.process
--------------------------------------------------------------------------------
CREATE TEMP TABLE temp_turn_process_json AS
SELECT
    c.tenant_id,
    c.turn_id,
    json_group_array(
        json_patch(
            '{}',
            json_object(
                'seq', c.seq,
                'kind', c.kind,
                'label', c.label,
                'content', c.content,
                'signal', c.signal,
                'cwd', c.cwd,
                'exit_code', c.exit_code,
                'summary', c.summary,
                'status', c.status
            )
        )
    ) AS items_json
FROM (
    SELECT * FROM temp_command_items ORDER BY tenant_id, turn_id, seq ASC
) c
GROUP BY c.tenant_id, c.turn_id;

CREATE TEMP TABLE temp_turn_process AS
SELECT
    t.tenant_id,
    t.id AS turn_id,
    NULLIF(
        json_patch(
            '{}',
            json_object(
                'cwd', cwd.common_cwd,
                'response_seq', resp.response_seq,
                'response_translated_text', resp.response_translated_text,
                'items', CASE WHEN pj.items_json IS NOT NULL THEN json(pj.items_json) ELSE NULL END
            )
        ),
        '{}'
    ) AS process_json,
    resp.response_content AS response_text
FROM conversation_turns t
LEFT JOIN temp_turn_cwds cwd ON t.tenant_id = cwd.tenant_id AND t.id = cwd.turn_id
LEFT JOIN temp_final_responses resp ON t.tenant_id = resp.tenant_id AND t.id = resp.turn_id
LEFT JOIN temp_turn_process_json pj ON t.tenant_id = pj.tenant_id AND t.id = pj.turn_id;

--------------------------------------------------------------------------------
-- 8. 升级 conversation_turns 为 17 列三段式结构
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
    question_id     TEXT NOT NULL DEFAULT '',
    question_origin TEXT NOT NULL DEFAULT 'imported',
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, session_id, external_id)
);

INSERT INTO conversation_turns_new (
    tenant_id, id, session_id, external_id, turn_index,
    prompt, process, response,
    title, started_at, ended_at, model, fingerprint, missing,
    imported_at, question_id, question_origin
)
SELECT
    t.tenant_id, t.id, t.session_id, t.external_id, t.turn_index,
    COALESCE(t.user_text, '') AS prompt,
    tp.process_json AS process,
    tp.response_text AS response,
    t.title, t.started_at, t.ended_at, t.model, t.fingerprint, t.missing,
    t.imported_at, t.question_id, t.question_origin
FROM conversation_turns t
LEFT JOIN temp_turn_process tp ON t.tenant_id = tp.tenant_id AND t.id = tp.turn_id;

DROP TABLE conversation_turns;
ALTER TABLE conversation_turns_new RENAME TO conversation_turns;

CREATE INDEX idx_conversation_turns_tenant_session
ON conversation_turns (tenant_id, session_id);

CREATE INDEX idx_conversation_turns_tenant_session_question
ON conversation_turns (tenant_id, session_id, question_id, turn_index);

CREATE INDEX idx_conversation_turns_tenant_question
ON conversation_turns (tenant_id, question_id);

--------------------------------------------------------------------------------
-- 9. 升级 conversation_parts 为 8 列终态结构，净化 attrs_json 并回填 cmd_seq
--------------------------------------------------------------------------------
CREATE TABLE conversation_parts_new (
    tenant_id       TEXT NOT NULL DEFAULT 'default',
    turn_id         TEXT NOT NULL,
    part_index      INTEGER NOT NULL,
    id_key          BLOB NOT NULL,
    kind            TEXT NOT NULL,
    content         TEXT,
    attrs_json      TEXT,
    translated_text TEXT,
    UNIQUE (tenant_id, turn_id, part_index)
);

INSERT INTO conversation_parts_new (
    tenant_id, turn_id, part_index, id_key,
    kind, content, attrs_json, translated_text
)
SELECT
    cp.tenant_id,
    cp.turn_id,
    cp.part_index,
    cp.id_key,
    cp.card_kind AS kind,
    cp.content,
    NULLIF(
        json_patch(
            '{}',
            json_object(
                'cmd_seq', CASE
                    WHEN (cp.card_kind LIKE '%.result' OR cp.card_kind = 'result') THEN (
                        SELECT cp_cmd.part_index
                        FROM conversation_parts cp_cmd
                        WHERE cp_cmd.tenant_id = cp.tenant_id
                          AND cp_cmd.turn_id = cp.turn_id
                          AND (cp_cmd.kind = 'command' OR cp_cmd.card_kind LIKE '%.command' OR cp_cmd.card_kind = 'command')
                          AND json_extract(cp_cmd.attrs_json, '$.exec_id') IS NOT NULL
                          AND json_extract(cp_cmd.attrs_json, '$.exec_id') = json_extract(cp.attrs_json, '$.exec_id')
                        LIMIT 1
                    )
                    ELSE NULL
                END,
                'language', NULLIF(trim(COALESCE(json_extract(cp.attrs_json, '$.language'), '')), ''),
                'signal', NULLIF(trim(COALESCE(json_extract(cp.attrs_json, '$.signal'), '')), ''),
                'file_path', NULLIF(trim(COALESCE(json_extract(cp.attrs_json, '$.file_path'), '')), ''),
                'tool_name', NULLIF(trim(COALESCE(json_extract(cp.attrs_json, '$.tool_name'), '')), ''),
                'summary', NULLIF(trim(COALESCE(json_extract(cp.attrs_json, '$.summary'), '')), ''),
                'exit_code', CASE
                    WHEN CAST(json_extract(cp.attrs_json, '$.exit_code') AS INTEGER) = 0 THEN NULL
                    ELSE CAST(json_extract(cp.attrs_json, '$.exit_code') AS INTEGER)
                END,
                'status', CASE
                    WHEN lower(trim(COALESCE(json_extract(cp.attrs_json, '$.status'), ''))) IN ('completed', 'success', 'ok', 'done', '') THEN NULL
                    ELSE json_extract(cp.attrs_json, '$.status')
                END
            )
        ),
        '{}'
    ) AS attrs_json,
    cp.translated_text
FROM conversation_parts cp
WHERE (cp.kind != 'command' AND cp.card_kind NOT LIKE '%.command' AND cp.card_kind != 'command')
  AND NOT ((cp.card_kind LIKE '%.result' OR cp.card_kind = 'result') AND (cp.content IS NULL OR trim(cp.content) = ''))
  AND (cp.role != 'user' AND cp.card_kind NOT LIKE '%.user' AND cp.card_kind != 'user')
  AND (cp.tenant_id, cp.turn_id, cp.part_index) NOT IN (
      SELECT tenant_id, turn_id, response_seq FROM temp_final_responses
  );

DROP TABLE conversation_parts;
ALTER TABLE conversation_parts_new RENAME TO conversation_parts;

CREATE INDEX idx_conversation_parts_id_key
ON conversation_parts (tenant_id, id_key);

DROP TABLE temp_command_items;
DROP TABLE temp_turn_cwds;
DROP TABLE temp_final_responses;
DROP TABLE temp_turn_process_json;
DROP TABLE temp_turn_process;

--------------------------------------------------------------------------------
-- 10. 升级 web_record_turns 为 17 列三段式结构，并同步自愈 web_record_parts
--------------------------------------------------------------------------------
-- 将 web_record_parts 中的中途检索 JSON 纠偏为 'process'
UPDATE web_record_parts
SET kind = 'process'
WHERE kind = 'answer'
  AND (trim(content) LIKE '{"queries":%' OR trim(content) LIKE '{"query":%');

-- 提取 Web 轮次最终响应
CREATE TEMP TABLE temp_web_final_responses AS
WITH ranked_web_answers AS (
    SELECT
        tenant_id,
        turn_id,
        part_index AS response_seq,
        content AS response_content,
        translated_text AS response_translated_text,
        ROW_NUMBER() OVER (PARTITION BY tenant_id, turn_id ORDER BY part_index DESC) AS rn
    FROM web_record_parts
    WHERE kind = 'answer'
)
SELECT
    tenant_id,
    turn_id,
    response_seq,
    response_content,
    response_translated_text
FROM ranked_web_answers
WHERE rn = 1;

-- 升级 web_record_turns 为 17 列
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
    question_id     TEXT NOT NULL DEFAULT '',
    question_origin TEXT NOT NULL DEFAULT 'imported',
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, session_id, external_id)
);

INSERT INTO web_record_turns_new (
    tenant_id, id, session_id, external_id, turn_index,
    prompt, process, response,
    title, started_at, ended_at, model, fingerprint, missing,
    imported_at, question_id, question_origin
)
SELECT
    t.tenant_id, t.id, t.session_id, t.external_id, t.turn_index,
    COALESCE(t.user_text, '') AS prompt,
    CASE
        WHEN r.response_seq IS NOT NULL THEN
            NULLIF(
                json_patch(
                    '{}',
                    json_object(
                        'response_seq', r.response_seq,
                        'response_translated_text', r.response_translated_text
                    )
                ),
                '{}'
            )
        ELSE NULL
    END AS process,
    r.response_content AS response,
    t.title, t.started_at, t.ended_at, t.model, t.fingerprint, t.missing,
    t.imported_at, t.question_id, t.question_origin
FROM web_record_turns t
LEFT JOIN temp_web_final_responses r ON t.tenant_id = r.tenant_id AND t.id = r.turn_id;

DROP TABLE web_record_turns;
ALTER TABLE web_record_turns_new RENAME TO web_record_turns;

CREATE INDEX idx_web_record_turns_tenant_session
ON web_record_turns (tenant_id, session_id);

CREATE INDEX idx_web_record_turns_tenant_session_question
ON web_record_turns (tenant_id, session_id, question_id, turn_index);

CREATE INDEX idx_web_record_turns_tenant_question
ON web_record_turns (tenant_id, question_id);

-- 从 web_record_parts 中移除已迁入 turn.response 的行
DELETE FROM web_record_parts
WHERE (tenant_id, turn_id, part_index) IN (
    SELECT tenant_id, turn_id, response_seq FROM temp_web_final_responses
);

-- 剩余的早先 answer 归位为 process
UPDATE web_record_parts
SET kind = 'process'
WHERE kind = 'answer';

DROP TABLE temp_web_final_responses;
