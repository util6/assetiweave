-- Quarantine historical storm jobs in session_memory_jobs, project_memory_jobs, and global_memory_jobs.
-- 1. Any job that has exceeded retry/attempt limit (>= 5) is marked as canceled and retry_at set to NULL.
UPDATE session_memory_jobs
SET status = 'canceled',
    last_error = COALESCE(last_error || ' | ', '') || 'quarantined: retry limit exceeded during memory storm',
    retry_at = NULL,
    ownership_token = NULL,
    lease_expires_at = NULL,
    heartbeat_at = NULL
WHERE status IN ('queued', 'running', 'failed')
  AND (attempt_count >= 5 OR retry_count >= 5);

UPDATE project_memory_jobs
SET status = 'canceled',
    last_error = COALESCE(last_error || ' | ', '') || 'quarantined: retry limit exceeded during memory storm',
    retry_at = NULL,
    ownership_token = NULL,
    lease_expires_at = NULL,
    heartbeat_at = NULL
WHERE status IN ('queued', 'running', 'failed')
  AND (attempt_count >= 5 OR retry_count >= 5);

UPDATE global_memory_jobs
SET status = 'canceled',
    last_error = COALESCE(last_error || ' | ', '') || 'quarantined: retry limit exceeded during memory storm',
    retry_at = NULL,
    ownership_token = NULL,
    lease_expires_at = NULL,
    heartbeat_at = NULL
WHERE status IN ('queued', 'running', 'failed')
  AND (attempt_count >= 5 OR retry_count >= 5);

-- 2. Any old backfilled queued job that was created before 2026-09-14 and never completed
UPDATE session_memory_jobs
SET status = 'canceled',
    last_error = 'quarantined: expired historical backfill job',
    retry_at = NULL,
    ownership_token = NULL,
    lease_expires_at = NULL,
    heartbeat_at = NULL
WHERE status = 'queued'
  AND created_at < '2026-09-14T00:00:00Z';
