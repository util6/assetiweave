-- Migration: 20261111000005_drop_conversation_fts_objects.sql
--
-- ADR-0023 终态：Tantivy 是唯一的会话词法派生索引，FTS5 不再承担任何读写角色。
--
-- 历史 FTS 正文不迁移：Tantivy 会从已提交的 Session/Turn/Part 事实按当前高信号
-- 语料策略重建一代新索引，旧的噪音正文（工具输出、diff、推理日志）因此自然消失。
--
-- 一并删除只服务于 FTS5 的辅助对象：
-- - conversation_search_doc_row：仅为让 FTS5 按会话删除不至于全表扫描而引入的 rowid 映射
-- - conversation_part_index_state：FTS5 专属的就绪态/重建态，状态收敛到
--   conversation_search_index_state 的 Tantivy 代际契约
--
-- 这里刻意不执行 VACUUM：删表只释放到空闲页，启动期全库重写会把一次模式升级
-- 变成长时间独占操作。空间回收属于独立的显式维护动作。

DROP TABLE IF EXISTS conversation_part_fts;

DROP TABLE IF EXISTS conversation_search_doc_row;

DROP TABLE IF EXISTS conversation_part_index_state;
