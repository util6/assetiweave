ALTER TABLE conversation_session_observations ADD COLUMN error_code TEXT;
ALTER TABLE conversation_session_observations ADD COLUMN error_message TEXT;
ALTER TABLE conversation_session_observations ADD COLUMN error_stage TEXT;
ALTER TABLE conversation_session_observations ADD COLUMN retryable INTEGER;
ALTER TABLE conversation_session_observations ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE conversation_session_observations ADD COLUMN last_attempt_at TEXT;
ALTER TABLE conversation_session_observations ADD COLUMN last_failure_at TEXT;
