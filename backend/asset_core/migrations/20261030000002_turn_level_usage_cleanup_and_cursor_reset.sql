-- Migration: 20261030000002_turn_level_usage_cleanup_and_cursor_reset.sql
--
-- Issue #158: 多客户端对话用量统计 Turn 级精细化平滑迁移
-- 1. 清理已有无 turn_id 或采用旧版非确定性格式 external_event_id 的历史用量记录
-- 2. 重置所有数据源的用量扫描游标（opaque_cursor = NULL），触发下一次扫描进行全量重扫与修剪

-- 1. 清理无 turn_id 或旧格式 external_event_id 的用量事件
DELETE FROM conversation_usage_events
WHERE turn_id IS NULL
   OR trim(turn_id) = ''
   OR (external_event_id NOT LIKE '%-turn-%' AND external_event_id NOT LIKE '%-msg-%');

-- 2. 重置数据源用量扫描游标为 NULL
UPDATE conversation_usage_source_states
SET opaque_cursor = NULL,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now');
