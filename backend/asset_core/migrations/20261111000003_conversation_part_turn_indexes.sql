-- Migration: 20261111000003_conversation_part_turn_indexes.sql
--
-- 会话零件按轮次取用的索引。
--
-- `conversation_parts` / `web_record_parts` 过去只按 (tenant_id, id_key) 建过索引，
-- 而几乎所有读取路径都是"先定会话、再按 turn_id 取零件"。缺少 (tenant_id, turn_id) 索引时
-- 查询规划器只能走"按 tenant_id 扫描全部零件"的路径：实测单会话零件加载 2.5~8.7 秒
-- （17 万零件），既拖慢会话同步，也拖慢搜索水合与语料重建。
--
-- 迁移只加索引，不改任何数据。

CREATE INDEX idx_conversation_parts_tenant_turn
    ON conversation_parts(tenant_id, turn_id);

CREATE INDEX idx_web_record_parts_tenant_turn
    ON web_record_parts(tenant_id, turn_id);
