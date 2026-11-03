-- Migration: 202610240004_web_record_parts_slimming.sql
--
-- Web 零件表 17 列精简为 7 列二元终态结构（answer / process）：
-- 1. 移除 83 字节字符串 id 列及其主键索引，改为确定性派生；存储 8 字节二进制 id_key BLOB 并建立索引；
-- 2. 仅区分最终回答 ('answer') 与中间过程输出 ('process')；
-- 3. 凡助手回答或独立代码块统一归为 'answer'，代码块包裹为带语言标记的 Markdown 围栏代码块写入 content；
-- 4. 其余所有工具调用、检索、思考或中间过程输出无论上报何种类型，一律归为 'process'；
-- 5. 移除已废弃稀疏列 (language, command, cwd, status, exit_code, command_label, metadata_json, content_card_json, source_execution_id)。

CREATE TABLE web_record_parts_new (
    tenant_id       TEXT NOT NULL DEFAULT 'default',
    turn_id         TEXT NOT NULL,
    part_index      INTEGER NOT NULL,
    id_key          BLOB NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('answer', 'process')),
    content         TEXT NOT NULL DEFAULT '',
    translated_text TEXT,
    UNIQUE (tenant_id, turn_id, part_index)
);

INSERT INTO web_record_parts_new (
    tenant_id,
    turn_id,
    part_index,
    id_key,
    kind,
    content,
    translated_text
)
SELECT
    tenant_id,
    turn_id,
    part_index,
    unhex(substr(id, -64, 16)) AS id_key,
    CASE
        WHEN (role = 'assistant' AND (kind = 'text' OR kind = 'code_block'))
             OR kind = 'code_block'
             OR json_extract(content_card_json, '$.kind') LIKE '%.answer'
             OR json_extract(metadata_json, '$.content_card.type') = 'answer'
        THEN 'answer'
        ELSE 'process'
    END AS kind,
    CASE
        WHEN kind = 'code_block' THEN
            CASE
                WHEN language IS NOT NULL AND trim(language) <> ''
                THEN '```' || trim(language) || char(10) || COALESCE(text, '') || char(10) || '```'
                ELSE '```' || char(10) || COALESCE(text, '') || char(10) || '```'
            END
        ELSE COALESCE(text, command, '')
    END AS content,
    translated_text
FROM web_record_parts;

DROP TABLE web_record_parts;

ALTER TABLE web_record_parts_new RENAME TO web_record_parts;

CREATE INDEX idx_web_record_parts_id_key
ON web_record_parts (tenant_id, id_key);
