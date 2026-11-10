-- Migration: 20261111000001_search_corpus_policy_and_inline_subagent_fingerprints.sql
--
-- ADR-0023 的两项存量收口：
--
-- 1. 检索语料由版本化策略统一生成（提问 / 回答 / 变更文件路径 / 命令语义标签）。
--    既有索引是按"零件正文全量下沉"生成的，口径已经不同，必须标记为需要一次受控重建，
--    而不是就地混用两套规则。重建由后台流水线按会话分批推进，本迁移只落标记。
--
-- 2. 内联子代理会话过去以 `source_fingerprint IS NULL` 落库，导致每轮增量都被判成
--    "无指纹 → 必须重导"。这里只回填**可无歧义推导**的行：父会话已有成功业务指纹，
--    且子会话在 `execution_purpose` 里带着稳定父键。父指纹缺失或父子关系不明时保持 NULL，
--    留给真正的重读去闭合水线——绝不用 `updated_at` / 文件 mtime 伪造一个"成功版本"。

ALTER TABLE conversation_part_index_state
    ADD COLUMN corpus_policy_version INTEGER NOT NULL DEFAULT 0;

ALTER TABLE conversation_search_index_state
    ADD COLUMN corpus_policy_version INTEGER NOT NULL DEFAULT 0;

-- 旧索引按旧口径生成，版本号归零即表示"需要重建"。状态同时降级，
-- 使列表正文匹配在重建完成前退化为标题级，而不是继续给出旧规则的命中。
--
-- 空索引（新建租户、从未写入正文）不需要重建：直接标记为当前口径（`1` = high-signal@1），
-- 避免全新安装在首次同步前就退化成只能按标题搜索。
UPDATE conversation_part_index_state
SET status = 'stale',
    corpus_policy_version = 0,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE status = 'ready'
  AND EXISTS (
      SELECT 1 FROM conversation_part_fts f
      WHERE f.tenant_id = conversation_part_index_state.tenant_id
  );

UPDATE conversation_part_index_state
SET corpus_policy_version = 1,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE status = 'ready';

UPDATE conversation_search_index_state
SET health = 'stale',
    corpus_policy_version = 0,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE health = 'ready'
  AND COALESCE(document_count, 0) > 0;

UPDATE conversation_search_index_state
SET corpus_policy_version = 1,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE health = 'ready';

UPDATE conversation_sessions
SET source_fingerprint = (
    SELECT parent.source_fingerprint || ':subagent:' || conversation_sessions.external_id
    FROM conversation_sessions AS parent
    WHERE parent.tenant_id = conversation_sessions.tenant_id
      AND parent.id = json_extract(conversation_sessions.execution_purpose, '$.parent_session_id')
      AND parent.source_fingerprint IS NOT NULL
      AND trim(parent.source_fingerprint) <> ''
      AND parent.missing = 0
)
WHERE source_fingerprint IS NULL
  AND execution_origin = 'subagent'
  AND json_valid(execution_purpose)
  AND json_extract(execution_purpose, '$.parent_session_id') IS NOT NULL
  AND EXISTS (
      SELECT 1
      FROM conversation_sessions AS parent
      WHERE parent.tenant_id = conversation_sessions.tenant_id
        AND parent.id = json_extract(conversation_sessions.execution_purpose, '$.parent_session_id')
        AND parent.source_fingerprint IS NOT NULL
        AND trim(parent.source_fingerprint) <> ''
        AND parent.missing = 0
  );
