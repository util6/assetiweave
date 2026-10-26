-- Migration: 202610240001_cleanup_legacy_util6_tenant.sql
--
-- 清理非活跃遗留租户（util6）下重复导入的会话域与关联数据：
-- 当 util6 不是当前活跃租户（即不等于 tenant_state.active_tenant_id）时执行清理。

-- 1. 会话表族与全文索引
DELETE FROM conversation_part_fts WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_parts WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_turns WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_sessions WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_session_observations WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_sync_deltas WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_sync_runs WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_sources WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_adapters WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_adapter_packages WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_adapter_catalog_releases WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_usage_events WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_usage_source_states WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_part_index_state WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_payload_policy_state WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM conversation_search_index_state WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);

-- 2. Web 会话表族
DELETE FROM web_record_parts WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM web_record_turns WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
DELETE FROM web_record_sessions WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);

-- 3. 会话记忆源引用
DELETE FROM session_memory_source_references WHERE tenant_id = 'util6' AND tenant_id NOT IN (SELECT active_tenant_id FROM tenant_state);
