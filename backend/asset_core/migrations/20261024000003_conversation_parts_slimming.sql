-- Migration: 202610240003_conversation_parts_slimming.sql
--
-- Agent 零件表 17 列精简为 11 列：
-- 1. 移除 82 字节字符串 id 列及其主键索引，改为确定性派生；存储 8 字节二进制 id_key BLOB 并建立索引；
-- 2. 合并 command 与 text 为 content（command 优先，原 text 迁入 attrs_json.summary）；
-- 3. 将 language, cwd, status, exit_code, command_label, source_execution_id(exec_id) 及展示属性归拢入 attrs_json；
-- 4. 将 content_card_json 替换为开放类型名 card_kind。

CREATE TABLE conversation_parts_new (
    tenant_id       TEXT NOT NULL DEFAULT 'default',
    turn_id         TEXT NOT NULL,
    part_index      INTEGER NOT NULL,
    id_key          BLOB NOT NULL,
    role            TEXT NOT NULL,
    kind            TEXT NOT NULL,
    card_kind       TEXT,
    content         TEXT,
    attrs_json      TEXT,
    metadata_json   TEXT,
    translated_text TEXT,
    UNIQUE (tenant_id, turn_id, part_index)
);

INSERT INTO conversation_parts_new (
    tenant_id,
    turn_id,
    part_index,
    id_key,
    role,
    kind,
    card_kind,
    content,
    attrs_json,
    metadata_json,
    translated_text
)
SELECT
    tenant_id,
    turn_id,
    part_index,
    unhex(substr(id, -64, 16)) AS id_key,
    role,
    kind,
    json_extract(content_card_json, '$.kind') AS card_kind,
    CASE
        WHEN kind = 'command' AND command IS NOT NULL AND command != '' THEN command
        ELSE text
    END AS content,
    NULLIF(
        json_patch(
            '{}',
            json_object(
                'language', CASE WHEN language IS NOT NULL AND language != '' THEN language ELSE NULL END,
                'cwd', CASE WHEN cwd IS NOT NULL AND cwd != '' THEN cwd ELSE NULL END,
                'status', CASE WHEN status IS NOT NULL AND status != '' THEN status ELSE NULL END,
                'exit_code', exit_code,
                'command_label', CASE WHEN command_label IS NOT NULL AND command_label != '' THEN command_label ELSE NULL END,
                'exec_id', CASE WHEN source_execution_id IS NOT NULL AND source_execution_id != '' THEN source_execution_id ELSE NULL END,
                'summary', CASE WHEN kind = 'command' AND command IS NOT NULL AND command != '' AND text IS NOT NULL AND trim(text) != '' THEN text ELSE NULL END,
                'renderer', json_extract(content_card_json, '$.renderer'),
                'semantic_role', json_extract(content_card_json, '$.semantic_role'),
                'signal', json_extract(metadata_json, '$.signal'),
                'file_path', json_extract(metadata_json, '$.file_path'),
                'tool_name', json_extract(metadata_json, '$.tool_name')
            )
        ),
        '{}'
    ) AS attrs_json,
    metadata_json,
    translated_text
FROM conversation_parts;

DROP TABLE conversation_parts;

ALTER TABLE conversation_parts_new RENAME TO conversation_parts;

CREATE INDEX idx_conversation_parts_id_key
ON conversation_parts (tenant_id, id_key);
