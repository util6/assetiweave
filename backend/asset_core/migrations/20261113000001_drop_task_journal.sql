-- 废弃并物理移除 task_journal 持久化流水表及相关索引 (#186)
DROP INDEX IF EXISTS idx_task_journal_created_at;
DROP INDEX IF EXISTS idx_task_journal_state;
DROP INDEX IF EXISTS idx_task_journal_task_id;
DROP TABLE IF EXISTS task_journal;
