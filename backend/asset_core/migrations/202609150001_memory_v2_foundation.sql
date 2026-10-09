-- Memory Q1-Q36 (Issue #35 / ADR-0015): Foundation schema for L1/L2/L3 memory items,
-- recent memory snapshots, snapshot projects/items, promotion observations, durable jobs,
-- and recent memory state pointer.

-- Preserve legacy archived memory tables by renaming them out of the active namespace
-- so historical data remains intact without mutating previous published migrations.
ALTER TABLE memory_item_evidence RENAME TO legacy_memory_item_evidence;
ALTER TABLE memory_item_revisions RENAME TO legacy_memory_item_revisions;
ALTER TABLE memory_items RENAME TO legacy_memory_items;

-- 1. memory_items:最小语义单元，跨 Snapshot 延续并可晋升
CREATE TABLE memory_items (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    layer TEXT NOT NULL CHECK (layer IN ('l1', 'l2', 'l3')),
    project_key TEXT,
    current_revision_id TEXT,
    lifecycle TEXT NOT NULL CHECK (lifecycle IN ('current', 'superseded', 'retired')),
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_memory_items_project
ON memory_items(tenant_id, project_key, layer, lifecycle);

-- 2. memory_item_revisions:条目版本历史，正文与状态变化均生成新 revision
CREATE TABLE memory_item_revisions (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    revision_number INTEGER NOT NULL CHECK (revision_number >= 1),
    category TEXT NOT NULL CHECK (
        category IN ('progress', 'decision', 'research', 'verification', 'blocker', 'follow_up')
    ),
    status TEXT NOT NULL CHECK (
        status IN ('active', 'blocked', 'waiting', 'completed', 'verified', 'abandoned', 'superseded')
    ),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0 AND length(title) <= 500),
    summary TEXT NOT NULL CHECK (length(trim(summary)) > 0 AND length(summary) <= 4000),
    rationale TEXT NOT NULL CHECK (length(rationale) <= 4000),
    recommendation_rank INTEGER CHECK (recommendation_rank IS NULL OR recommendation_rank IN (1, 2, 3)),
    promotion_nomination TEXT NOT NULL CHECK (
        promotion_nomination IN (
            'none', 'project_decision', 'project_constraint',
            'recurring_blocker', 'recurring_todo', 'research_conclusion',
            'global_rule', 'cross_project_pattern'
        )
    ),
    occurred_at TEXT NOT NULL,
    evidence_fingerprint TEXT NOT NULL CHECK (length(trim(evidence_fingerprint)) > 0),
    generated_by_snapshot_id TEXT,
    supersedes_revision_id TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, item_id, revision_number),
    FOREIGN KEY (tenant_id, item_id)
        REFERENCES memory_items(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_memory_item_revisions_item_rev
ON memory_item_revisions(tenant_id, item_id, revision_number DESC);

-- 3. memory_item_source_references:引用的不可变 locator 副本
-- 注意：不对 Conversation 表声明 ON DELETE CASCADE（引用不可用由失效协调器更新）
CREATE TABLE memory_item_source_references (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    item_revision_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind = 'session'),
    source_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    question_id TEXT,
    turn_id TEXT,
    part_id TEXT,
    node_id TEXT,
    node_order INTEGER CHECK (node_order IS NULL OR node_order >= 0),
    reference_key TEXT NOT NULL CHECK (length(trim(reference_key)) > 0),
    source_revision INTEGER NOT NULL CHECK (source_revision >= 0),
    availability TEXT NOT NULL CHECK (availability IN ('available', 'unavailable')),
    unavailable_reason TEXT CHECK (
        unavailable_reason IS NULL OR
        unavailable_reason IN ('deleted', 'missing', 'excluded', 'source_disabled')
    ),
    unavailable_at TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, item_revision_id, reference_key),
    FOREIGN KEY (tenant_id, item_revision_id)
        REFERENCES memory_item_revisions(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_memory_item_source_refs_session
ON memory_item_source_references(tenant_id, session_id, source_revision);

-- 4. memory_item_supersessions:条目替代关系历史
CREATE TABLE memory_item_supersessions (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    superseded_item_id TEXT NOT NULL,
    superseding_item_id TEXT NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, superseded_item_id, superseding_item_id),
    FOREIGN KEY (tenant_id, superseded_item_id)
        REFERENCES memory_items(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, superseding_item_id)
        REFERENCES memory_items(tenant_id, id) ON DELETE CASCADE
);

-- 5. recent_memory_snapshots:目标水位的完整已发布 L1 快照（仅成功/复用进入）
CREATE TABLE recent_memory_snapshots (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence >= 1),
    target_watermark_utc TEXT NOT NULL,
    local_watermark_date TEXT NOT NULL,
    local_watermark_time TEXT NOT NULL,
    timezone_offset_minutes INTEGER NOT NULL,
    window_hours INTEGER NOT NULL CHECK (window_hours IN (24, 48, 72)),
    window_start_utc TEXT NOT NULL,
    window_end_utc TEXT NOT NULL,
    publication_kind TEXT NOT NULL CHECK (publication_kind IN ('generated', 'reused')),
    reused_from_snapshot_id TEXT,
    target_fingerprint TEXT NOT NULL CHECK (length(trim(target_fingerprint)) > 0),
    content_fingerprint TEXT NOT NULL CHECK (length(trim(content_fingerprint)) > 0),
    generation_skill_asset_id TEXT,
    generation_skill_revision INTEGER CHECK (generation_skill_revision IS NULL OR generation_skill_revision >= 0),
    generation_skill_content_hash TEXT,
    contract_version TEXT NOT NULL CHECK (length(trim(contract_version)) > 0),
    budget_policy_version TEXT NOT NULL CHECK (length(trim(budget_policy_version)) > 0),
    projection_policy_version TEXT NOT NULL CHECK (length(trim(projection_policy_version)) > 0),
    content_generated_at TEXT NOT NULL,
    published_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, target_watermark_utc, window_hours, contract_version),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_recent_memory_snapshots_watermark
ON recent_memory_snapshots(tenant_id, target_watermark_utc DESC);

-- 6. recent_memory_snapshot_projects:快照关联的项目分组摘要
CREATE TABLE recent_memory_snapshot_projects (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    project_key TEXT NOT NULL CHECK (length(trim(project_key)) > 0),
    project_title TEXT NOT NULL CHECK (length(trim(project_title)) > 0),
    project_path TEXT,
    summary TEXT NOT NULL CHECK (length(summary) <= 4000),
    no_material_change INTEGER NOT NULL DEFAULT 0 CHECK (no_material_change IN (0, 1)),
    latest_activity_at TEXT NOT NULL,
    source_session_count INTEGER NOT NULL CHECK (source_session_count >= 0),
    sort_order INTEGER NOT NULL CHECK (sort_order >= 0),
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, snapshot_id, project_key),
    FOREIGN KEY (tenant_id, snapshot_id)
        REFERENCES recent_memory_snapshots(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_recent_snapshot_projects_key
ON recent_memory_snapshot_projects(tenant_id, project_key);

-- 7. recent_memory_snapshot_items:快照条目成员关联
CREATE TABLE recent_memory_snapshot_items (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    project_key TEXT NOT NULL CHECK (length(trim(project_key)) > 0),
    item_id TEXT NOT NULL,
    item_revision_id TEXT NOT NULL,
    display_date TEXT NOT NULL,
    sort_order INTEGER NOT NULL CHECK (sort_order >= 0),
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, snapshot_id, item_id),
    FOREIGN KEY (tenant_id, snapshot_id)
        REFERENCES recent_memory_snapshots(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, item_id)
        REFERENCES memory_items(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, item_revision_id)
        REFERENCES memory_item_revisions(tenant_id, id) ON DELETE CASCADE
);

-- 8. memory_promotion_observations:晋升观察记录（仅 generated 且有新证据增加观察）
CREATE TABLE memory_promotion_observations (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    item_id TEXT NOT NULL,
    item_revision_id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    nomination TEXT NOT NULL CHECK (
        nomination IN (
            'project_decision', 'project_constraint', 'recurring_blocker',
            'recurring_todo', 'research_conclusion', 'global_rule', 'cross_project_pattern'
        )
    ),
    evidence_fingerprint TEXT NOT NULL CHECK (length(trim(evidence_fingerprint)) > 0),
    project_key TEXT NOT NULL CHECK (length(trim(project_key)) > 0),
    observed_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, item_id, snapshot_id),
    FOREIGN KEY (tenant_id, item_id)
        REFERENCES memory_items(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, snapshot_id)
        REFERENCES recent_memory_snapshots(tenant_id, id) ON DELETE CASCADE
);

-- 9. recent_memory_jobs:Durable Job 表，支持恢复、续借、重试与心跳
CREATE TABLE recent_memory_jobs (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN ('queued', 'running', 'succeeded', 'reused', 'failed', 'canceled', 'stale')
    ),
    ownership_token TEXT,
    lease_expires_at TEXT,
    heartbeat_at TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    retry_at TEXT,
    target_watermark_utc TEXT NOT NULL,
    window_hours INTEGER NOT NULL CHECK (window_hours IN (24, 48, 72)),
    target_fingerprint TEXT NOT NULL CHECK (length(trim(target_fingerprint)) > 0),
    content_fingerprint TEXT NOT NULL CHECK (length(trim(content_fingerprint)) > 0),
    work_order_json TEXT NOT NULL CHECK (json_valid(work_order_json)),
    last_error_code TEXT,
    last_error_message TEXT,
    started_at TEXT,
    finished_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, target_watermark_utc, window_hours, target_fingerprint),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_recent_memory_jobs_status
ON recent_memory_jobs(tenant_id, status, retry_at, updated_at);

-- 10. recent_memory_state:维护 tenant 的当前快照指针与任务状态
CREATE TABLE recent_memory_state (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    last_successful_snapshot_id TEXT,
    latest_attempt_task_id TEXT,
    latest_attempt_error_code TEXT,
    latest_attempt_error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, last_successful_snapshot_id)
        REFERENCES recent_memory_snapshots(tenant_id, id) ON DELETE SET NULL
);
