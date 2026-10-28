-- 会话增量同步水位表极简收敛、源级契约版本上移与历史废弃表清退 (Issue #149)

-- 1. 将适配器内容哈希与策略版本上移至 conversation_sources 表
ALTER TABLE conversation_sources ADD COLUMN synced_adapter_hash TEXT;
ALTER TABLE conversation_sources ADD COLUMN synced_card_contract_version INTEGER;
ALTER TABLE conversation_sources ADD COLUMN synced_payload_policy_version INTEGER;
ALTER TABLE conversation_sources ADD COLUMN synced_projection_version INTEGER;

UPDATE conversation_sources
SET synced_adapter_hash = (
        SELECT hydrated_adapter_hash
        FROM conversation_session_observations
        WHERE conversation_session_observations.tenant_id = conversation_sources.tenant_id
          AND conversation_session_observations.source_id = conversation_sources.id
          AND hydrated_adapter_hash IS NOT NULL
        LIMIT 1
    ),
    synced_card_contract_version = (
        SELECT hydrated_card_contract_version
        FROM conversation_session_observations
        WHERE conversation_session_observations.tenant_id = conversation_sources.tenant_id
          AND conversation_session_observations.source_id = conversation_sources.id
          AND hydrated_card_contract_version IS NOT NULL
        LIMIT 1
    ),
    synced_payload_policy_version = (
        SELECT hydrated_payload_policy_version
        FROM conversation_session_observations
        WHERE conversation_session_observations.tenant_id = conversation_sources.tenant_id
          AND conversation_session_observations.source_id = conversation_sources.id
          AND hydrated_payload_policy_version IS NOT NULL
        LIMIT 1
    ),
    synced_projection_version = (
        SELECT hydrated_projection_version
        FROM conversation_session_observations
        WHERE conversation_session_observations.tenant_id = conversation_sources.tenant_id
          AND conversation_session_observations.source_id = conversation_sources.id
          AND hydrated_projection_version IS NOT NULL
        LIMIT 1
    );

-- 2. 重建 conversation_session_observations 为 5 列极简表 (WITHOUT ROWID)
CREATE TABLE conversation_session_observations_new (
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('session', 'web')),
    external_id TEXT NOT NULL,
    synced_version TEXT NOT NULL,
    PRIMARY KEY (tenant_id, source_id, record_kind, external_id)
) WITHOUT ROWID;

INSERT OR IGNORE INTO conversation_session_observations_new (
    tenant_id, source_id, record_kind, external_id, synced_version
)
SELECT
    tenant_id,
    source_id,
    record_kind,
    external_id,
    COALESCE(hydrated_version, observed_version) AS synced_version
FROM conversation_session_observations
WHERE dirty = 0
  AND COALESCE(hydrated_version, observed_version) IS NOT NULL
  AND COALESCE(hydrated_version, observed_version) != '';

DROP TABLE conversation_session_observations;

ALTER TABLE conversation_session_observations_new RENAME TO conversation_session_observations;

-- 3. 清理废弃的同步流水表与旧版 Memory 状态机表 (按外键依赖倒序删除，先子表后主表)
DROP TABLE IF EXISTS legacy_memory_item_evidence;
DROP TABLE IF EXISTS legacy_memory_item_revisions;
DROP TABLE IF EXISTS legacy_memory_items;

DROP TABLE IF EXISTS recent_memory_state;
DROP TABLE IF EXISTS recent_memory_events;
DROP TABLE IF EXISTS recent_memory_jobs;
DROP TABLE IF EXISTS recent_memory_snapshot_items;
DROP TABLE IF EXISTS recent_memory_snapshot_projects;
DROP TABLE IF EXISTS recent_memory_snapshots;

DROP TABLE IF EXISTS memory_item_supersessions;
DROP TABLE IF EXISTS memory_item_source_references;
DROP TABLE IF EXISTS memory_evidence_remap_audits;
DROP TABLE IF EXISTS memory_evidence_staleness;
DROP TABLE IF EXISTS memory_item_evidence;
DROP TABLE IF EXISTS memory_extraction_evidence;
DROP TABLE IF EXISTS memory_dream_note_evidence;
DROP TABLE IF EXISTS memory_run_evidence;
DROP TABLE IF EXISTS memory_evidence_snapshots;
DROP TABLE IF EXISTS memory_item_revisions;
DROP TABLE IF EXISTS memory_reference_audit_findings;
DROP TABLE IF EXISTS memory_promotion_observations;
DROP TABLE IF EXISTS memory_v2_maintenance_jobs;
DROP TABLE IF EXISTS memory_maintenance_jobs;
DROP TABLE IF EXISTS memory_items;
DROP TABLE IF EXISTS memory_dream_states;
DROP TABLE IF EXISTS memory_dream_notes;
DROP TABLE IF EXISTS memory_extractions;
DROP TABLE IF EXISTS memory_runs;

DROP TABLE IF EXISTS project_memory_sources;
DROP TABLE IF EXISTS project_memory_jobs;
DROP TABLE IF EXISTS project_memory_versions;
DROP TABLE IF EXISTS project_memory_state;
DROP TABLE IF EXISTS project_memories;

DROP TABLE IF EXISTS global_memory_sources;
DROP TABLE IF EXISTS global_memory_jobs;
DROP TABLE IF EXISTS global_memory_versions;
DROP TABLE IF EXISTS global_memory_state;
DROP TABLE IF EXISTS global_memories;

DROP TABLE IF EXISTS session_memory_source_references;
DROP TABLE IF EXISTS session_memory_jobs_v2;
DROP TABLE IF EXISTS session_memory_jobs;
DROP TABLE IF EXISTS session_memories;

DROP TABLE IF EXISTS conversation_sync_deltas_v2;
DROP TABLE IF EXISTS conversation_sync_deltas;
DROP TABLE IF EXISTS conversation_sync_runs;
