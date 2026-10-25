-- Migration: 202610230006_conversation_part_index.sql
--
-- 全文索引下沉到零件级：以 (tenant_id, part_id, session_id, turn_id, text) 的单列正文索引
-- 取代按问题聚合的四桶索引（question / answer / code / command）。索引正文是该零件投影出的
-- 卡片正文原样，写入端不再判断语义角色，因此工具输出、执行结果、文件变更与子代理正文与
-- 答复、代码、命令同等入索引。
--
-- 两条结构性变化：
-- 1. 索引粒度从「一个问题一行」变为「一个零件一行」，随零件写入增量维护，不再需要按问题
--    重建聚合，也就不再需要回答「这条正文算答复、代码还是命令」。
-- 2. conversation_* 与 web_record_* 两个表族共用同一张零件级索引。零件标识前缀
--    （conversation-part-* / web-record-part-*）天然不冲突，无需按记录类型分表。
--
-- 回填直接取零件正文列（text / command / translated_text），即投影正文的来源列，保证升级后
-- 立即可搜、既有命中不退化；投影版本变化或历史数据需要整理时，由后台流水线任务重建
-- （conversation_part_index_state.status 由 'ready' 转为 'stale'/'building'）。

CREATE VIRTUAL TABLE conversation_part_fts USING fts5(
    tenant_id UNINDEXED,
    part_id UNINDEXED,
    session_id UNINDEXED,
    turn_id UNINDEXED,
    text
);

INSERT INTO conversation_part_fts (tenant_id, part_id, session_id, turn_id, text)
SELECT
    p.tenant_id,
    p.id,
    t.session_id,
    p.turn_id,
    trim(
        COALESCE(p.text, '') || char(10) ||
        COALESCE(p.command, '') || char(10) ||
        COALESCE(p.translated_text, '')
    )
FROM conversation_parts p
JOIN conversation_turns t
  ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
WHERE trim(
        COALESCE(p.text, '') || COALESCE(p.command, '') || COALESCE(p.translated_text, '')
      ) <> '';

INSERT INTO conversation_part_fts (tenant_id, part_id, session_id, turn_id, text)
SELECT
    p.tenant_id,
    p.id,
    t.session_id,
    p.turn_id,
    trim(
        COALESCE(p.text, '') || char(10) ||
        COALESCE(p.command, '') || char(10) ||
        COALESCE(p.translated_text, '')
    )
FROM web_record_parts p
JOIN web_record_turns t
  ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
WHERE trim(
        COALESCE(p.text, '') || COALESCE(p.command, '') || COALESCE(p.translated_text, '')
      ) <> '';

-- 索引就绪状态：只在「是否可做正文匹配」这一层做判断，缺行即视为就绪，
-- 因此新租户不需要任何初始化写入。
CREATE TABLE conversation_part_index_state (
    tenant_id TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'ready'
        CHECK (status IN ('ready', 'building', 'stale')),
    part_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    rebuilt_at TEXT,
    updated_at TEXT NOT NULL
);

INSERT INTO conversation_part_index_state (
    tenant_id, status, part_count, rebuilt_at, updated_at
)
SELECT
    tenants.id,
    'ready',
    (
        SELECT COUNT(*) FROM conversation_part_fts f WHERE f.tenant_id = tenants.id
    ),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM tenants;

-- 旧的问题级四桶索引退役：正文改由零件级索引承载，问题表本身由后续票删除。
DROP TABLE IF EXISTS conversation_question_fts;
