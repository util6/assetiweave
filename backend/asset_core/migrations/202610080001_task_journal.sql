-- SQLite Task Journal (ADR-0017 / Issue #77)
-- 仅追加记录生命周期关键帧（任务创建、阶段进出、终态结算、停机中断），高频数值进度不写盘。
CREATE TABLE IF NOT EXISTS task_journal (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL,
    tenant_id TEXT,
    kind TEXT NOT NULL,
    state TEXT NOT NULL,
    stage_id TEXT,
    error_summary TEXT,
    result_ref TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_task_journal_task_id ON task_journal(task_id);
CREATE INDEX IF NOT EXISTS idx_task_journal_state ON task_journal(tenant_id, state);
CREATE INDEX IF NOT EXISTS idx_task_journal_created_at ON task_journal(created_at);
