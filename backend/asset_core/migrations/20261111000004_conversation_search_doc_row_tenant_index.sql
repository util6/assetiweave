-- Migration: 20261111000004_conversation_search_doc_row_tenant_index.sql
--
-- 1) 撤销 20261111000003 引入的两条冗余索引。
--
--    `conversation_parts` / `web_record_parts` 已有 `UNIQUE (tenant_id, turn_id, part_index)`，
--    其隐式索引天然覆盖 `(tenant_id, turn_id)` 前缀，足以支撑"按会话取零件"。Issue #119
--    已经把 `idx_conversation_parts_tenant_turn` 作为冗余索引删除，000003 又重新建了回来，
--    这里按原决策恢复。
--
--    真正让零件加载慢的不是缺索引，而是查询写法：`FROM parts JOIN turns ORDER BY p.turn_id`
--    会让规划器为了省一次排序而选择"按 tenant_id 扫描全部零件"的计划，单会话 2.5~8.7 秒。
--    把读取路径改成 `FROM turns CROSS JOIN parts` 固定成轮次驱动后，走的就是上表已有的
--    唯一索引，200 个会话合计 61 毫秒（快约 130 倍），无需任何新增索引。
--
-- 2) 为派生索引的分批清空补一条索引。
--
--    `conversation_search_doc_row` 已有 `(tenant_id, session_id)`，按 tenant_id 取一批 rowid
--    时顺序不稳定；补 `(tenant_id, fts_rowid)` 让"取一批 + 按 rowid 点删"成为稳定的前缀扫描。

DROP INDEX IF EXISTS idx_conversation_parts_tenant_turn;
DROP INDEX IF EXISTS idx_web_record_parts_tenant_turn;

CREATE INDEX idx_conversation_search_doc_row_tenant_row
    ON conversation_search_doc_row(tenant_id, fts_rowid);
