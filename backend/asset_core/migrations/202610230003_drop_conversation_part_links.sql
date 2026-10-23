-- Migration: 202610230001_drop_conversation_part_links.sql
-- 零件关系边表退役（主表族与 web 记录表族）。
--
-- 「零件属于哪次执行」由零件表既有的 source_execution_id 表达；
-- 「命令 ↔ 结果」配对由 source_execution_id 分组 + part_index 排序 + 卡片语义角色
-- 在读取时派生。边表只写不读，且唯一非派生信息（父零件 → 子会话）的目标标识在
-- 会话表中零命中，渲染路径读的是卡片正文里的子会话标识。

DROP INDEX IF EXISTS idx_conversation_part_links_target;
DROP TABLE IF EXISTS conversation_part_links;

DROP INDEX IF EXISTS idx_web_record_part_links_target;
DROP TABLE IF EXISTS web_record_part_links;
