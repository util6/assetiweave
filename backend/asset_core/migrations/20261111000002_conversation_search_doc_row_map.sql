-- Migration: 20261111000002_conversation_search_doc_row_map.sql
--
-- 派生语料索引的删除通道。
--
-- `conversation_part_fts` 的 `tenant_id` / `session_id` / `turn_id` 都是 FTS5 的 UNINDEXED 列，
-- 按它们过滤删除只能扫描全部正文行。实测单次 `DELETE ... WHERE session_id = ?` 需要 8~19 秒，
-- 这正是增量同步被索引维护拖住、并反复争抢写锁的根因。
--
-- 这里落一张只存 rowid 的窄映射表：写入时记下 FTS 行号，删除时走点查。
-- 它不承载任何检索语义，只解决"按会话精确删除"，语料口径仍由版本化策略单点决定。
--
-- 回填不重建索引：现有行原样保留映射，后续整代重建或按会话替换时再逐步收敛到新口径。

CREATE TABLE conversation_search_doc_row (
    fts_rowid INTEGER PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    session_id TEXT NOT NULL
);

CREATE INDEX idx_conversation_search_doc_row_session
    ON conversation_search_doc_row(tenant_id, session_id);

INSERT INTO conversation_search_doc_row (fts_rowid, tenant_id, session_id)
SELECT rowid, tenant_id, session_id FROM conversation_part_fts;
