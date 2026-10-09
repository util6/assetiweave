ALTER TABLE conversation_sessions ADD COLUMN execution_origin TEXT NOT NULL DEFAULT 'user';
ALTER TABLE conversation_sessions ADD COLUMN execution_purpose TEXT;
ALTER TABLE conversation_sessions ADD COLUMN user_visible INTEGER NOT NULL DEFAULT 1;

CREATE INDEX IF NOT EXISTS idx_conversation_sessions_visibility
ON conversation_sessions(tenant_id, user_visible, execution_origin);
