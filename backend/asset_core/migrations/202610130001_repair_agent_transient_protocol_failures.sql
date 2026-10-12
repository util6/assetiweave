-- Repair agent transient protocol failures caused by probe cleanup false-positives
-- and initialize race conditions.
-- Directly heal erroneously failed agents back to 'ready' so that they are immediately
-- available in the execution registry across both resident and non-resident (CLI) runtimes.
UPDATE app_agent_installations
SET protocol_status = 'ready',
    protocol_error_code = NULL,
    protocol_error_message = NULL,
    protocol_checked_at = CURRENT_TIMESTAMP,
    model_status = 'ready',
    model_error_code = NULL,
    model_checked_at = CURRENT_TIMESTAMP,
    updated_at = CURRENT_TIMESTAMP
WHERE enabled = 1
  AND installation_status = 'ready'
  AND runtime_status = 'ready'
  AND protocol_status = 'failed';
