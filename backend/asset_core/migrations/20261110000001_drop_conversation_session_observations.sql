-- 会话增量水位收敛：下线 per-session 水位行，版本指纹回归会话记录自身 (Issue #182)
--
-- 背景：水位表在瘦身改造中保留了 (external_id, synced_version) 逐会话行，
-- 与 conversation_sessions.source_fingerprint 完全冗余（现存可关联行 100% 等值），
-- 且源级契约一旦失配会先整源删除水位再开始全量重解析；只要该轮因超时或取消
-- 未跑完，契约永不回签，下一轮又从零删起，形成无法自愈的死循环。
--
-- 本迁移先把仍可用的旧水位回填到会话记录，再删除该表。回填是幂等的：
-- 已有指纹的会话不会被覆盖。

UPDATE conversation_sessions
SET source_fingerprint = (
    SELECT o.synced_version
    FROM conversation_session_observations o
    WHERE o.tenant_id = conversation_sessions.tenant_id
      AND o.source_id = conversation_sessions.source_id
      AND o.record_kind = 'session'
      AND o.external_id = conversation_sessions.external_id
)
WHERE (source_fingerprint IS NULL OR trim(source_fingerprint) = '')
  AND EXISTS (
    SELECT 1
    FROM conversation_session_observations o
    WHERE o.tenant_id = conversation_sessions.tenant_id
      AND o.source_id = conversation_sessions.source_id
      AND o.record_kind = 'session'
      AND o.external_id = conversation_sessions.external_id
      AND o.synced_version IS NOT NULL
      AND trim(o.synced_version) <> ''
  );

UPDATE web_record_sessions
SET source_fingerprint = (
    SELECT o.synced_version
    FROM conversation_session_observations o
    WHERE o.tenant_id = web_record_sessions.tenant_id
      AND o.source_id = web_record_sessions.source_id
      AND o.record_kind = 'web'
      AND o.external_id = web_record_sessions.external_id
)
WHERE (source_fingerprint IS NULL OR trim(source_fingerprint) = '')
  AND EXISTS (
    SELECT 1
    FROM conversation_session_observations o
    WHERE o.tenant_id = web_record_sessions.tenant_id
      AND o.source_id = web_record_sessions.source_id
      AND o.record_kind = 'web'
      AND o.external_id = web_record_sessions.external_id
      AND o.synced_version IS NOT NULL
      AND trim(o.synced_version) <> ''
  );

DROP INDEX IF EXISTS idx_conversation_session_observations_dirty;
DROP TABLE IF EXISTS conversation_session_observations;
