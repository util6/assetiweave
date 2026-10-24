-- Migration: 202610240005_cleanup_util6_orphans_and_repair_card_kinds.sql
--
-- 1. 清理非活跃遗留租户（util6）在孤儿表中的残留：
--    当 util6 不是当前活跃租户（即不等于 tenant_state.active_tenant_id）时执行清理。
DELETE FROM tenants WHERE id = 'util6' AND id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_adapter_package_versions WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_adapter_packages WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_question_redirects WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM session_memories WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM session_memory_jobs WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM project_memories WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM project_memory_jobs WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM project_memory_state WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM global_memories WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM global_memory_jobs WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM global_memory_state WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM recent_memory_state WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM recent_memory_jobs WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM recent_memory_snapshots WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM agent_installations WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM task_journal WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);

-- 2. 修复真实库中因旧版 metadata_json.content_card.type 遗留导致 card_kind IS NULL 的数据：
UPDATE conversation_parts
SET card_kind = COALESCE(
    json_extract(metadata_json, '$.content_card.kind'),
    json_extract(metadata_json, '$.content_card.type')
)
WHERE card_kind IS NULL
  AND metadata_json IS NOT NULL
  AND (
      json_extract(metadata_json, '$.content_card.kind') IS NOT NULL
      OR json_extract(metadata_json, '$.content_card.type') IS NOT NULL
  );
