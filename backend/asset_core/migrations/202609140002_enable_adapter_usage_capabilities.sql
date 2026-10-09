-- Migration: 202609140002_enable_adapter_usage_capabilities.sql
-- Ensure built-in conversation adapters support read_usage capability

UPDATE conversation_adapters
SET capabilities = json_insert(capabilities, '$[#]', 'read_usage')
WHERE id IN ('codex', 'antigravity', 'claude-code', 'opencode', 'zcode')
  AND capabilities NOT LIKE '%"read_usage"%';
