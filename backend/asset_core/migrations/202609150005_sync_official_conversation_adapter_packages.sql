-- Align official conversation adapter package versions and reset gate errors for official packages.
-- Official adapters are bundled with AssetIWeave; when their files are refreshed on disk,
-- the app_conversation_adapter_packages records must reflect the current version and clear stale hash mismatches.

UPDATE app_conversation_adapter_packages
SET version = '1.3.8',
    runtime_ready = 1,
    runtime_gate_status = 'ready',
    error_message = NULL,
    updated_at = '2026-09-15T23:59:59Z'
WHERE package_id = 'io.github.util6.opencode-session'
  AND version = '1.3.7';

UPDATE app_conversation_adapter_packages
SET runtime_ready = 1,
    runtime_gate_status = 'ready',
    error_message = NULL,
    updated_at = '2026-09-15T23:59:59Z'
WHERE package_id IN (
    'io.github.util6.codex-session',
    'io.github.util6.claude-code-session',
    'io.github.util6.antigravity-session',
    'io.github.util6.zcode-session',
    'io.github.util6.opencode-session'
)
  AND (error_message LIKE '%content hash mismatch%' OR error_message LIKE '%does not match active version%');
