-- Recent Memory error retryable flag (Issue #44)
ALTER TABLE recent_memory_state ADD COLUMN latest_attempt_error_retryable INTEGER;
