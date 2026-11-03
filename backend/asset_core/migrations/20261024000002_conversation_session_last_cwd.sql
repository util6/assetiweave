-- Migration: 202610240002_conversation_session_last_cwd.sql
--
-- 1. 为 conversation_sessions 新增 last_cwd TEXT 列
ALTER TABLE conversation_sessions ADD COLUMN last_cwd TEXT;

-- 2. 从既有零件数据物化回填各会话最新非空工作目录
WITH latest_cwds AS (
    SELECT session_id, tenant_id, cwd
    FROM (
        SELECT t.session_id,
               t.tenant_id,
               p.cwd,
               ROW_NUMBER() OVER (
                   PARTITION BY t.tenant_id, t.session_id
                   ORDER BY COALESCE(t.ended_at, t.started_at) DESC,
                            t.turn_index DESC,
                            p.part_index DESC,
                            p.id DESC
               ) as rn
        FROM conversation_turns t
        JOIN conversation_parts p
          ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
        WHERE p.cwd IS NOT NULL AND trim(p.cwd) <> ''
    )
    WHERE rn = 1
)
UPDATE conversation_sessions
SET last_cwd = latest_cwds.cwd
FROM latest_cwds
WHERE conversation_sessions.tenant_id = latest_cwds.tenant_id
  AND conversation_sessions.id = latest_cwds.session_id;

-- 3. 移除与 UNIQUE(tenant_id, turn_id, part_index) 自动索引完全重复的冗余索引
DROP INDEX IF EXISTS idx_conversation_parts_tenant_turn;
