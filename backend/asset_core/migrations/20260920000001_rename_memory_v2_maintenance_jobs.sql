-- Rename memory_v2_maintenance_jobs to memory_maintenance_jobs to converge on clean domain naming.
ALTER TABLE memory_v2_maintenance_jobs RENAME TO memory_maintenance_jobs;

DROP INDEX IF EXISTS idx_memory_v2_maintenance_target;
DROP INDEX IF EXISTS idx_memory_v2_maintenance_scheduler;

CREATE UNIQUE INDEX idx_memory_maintenance_target
ON memory_maintenance_jobs(tenant_id, purpose, COALESCE(project_key, ''));

CREATE INDEX idx_memory_maintenance_scheduler
ON memory_maintenance_jobs(tenant_id, status, retry_at, updated_at);
