-- Memory 引用一致性审计账本。
--
-- 三张持久化了「问题标识」的引用表结构不同（只有 memory_item_source_references 自带
-- availability 列），因此统一由本表承载「已修复」与「需人工判定」两种终态，避免为
-- session_memory_source_references 与 memory_evidence_snapshots 各自新增标记列。
CREATE TABLE memory_reference_audit_findings (
    tenant_id TEXT NOT NULL,
    id TEXT NOT NULL,
    reference_table TEXT NOT NULL CHECK (
        reference_table IN (
            'memory_evidence_snapshots',
            'session_memory_source_references',
            'memory_item_source_references'
        )
    ),
    reference_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('session', 'web')),
    session_id TEXT NOT NULL,
    previous_question_id TEXT NOT NULL,
    turn_id TEXT,
    resolution TEXT NOT NULL CHECK (resolution IN ('remapped', 'invalidated')),
    strategy TEXT NOT NULL CHECK (
        strategy IN (
            'redirect_tombstone',
            'turn_membership',
            'nearest_group',
            'single_session_group',
            'session_group_missing',
            'ambiguous_group'
        )
    ),
    resolved_question_id TEXT,
    detected_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, reference_table, reference_id),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    CHECK ((resolution = 'remapped') = (resolved_question_id IS NOT NULL))
);

CREATE INDEX idx_memory_reference_audit_findings_state
ON memory_reference_audit_findings(tenant_id, resolution);
