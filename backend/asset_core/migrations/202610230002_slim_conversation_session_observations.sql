-- 观测表瘦身：只保留真正参与增量同步决策的列。
-- 保留：外部版本三元组（observed_version / hydrated_version / last_seen_at）、
-- 唯一重试信号 dirty，以及四个重解析触发版本（适配器内容哈希、卡片契约版本、
-- 载荷策略版本、投影版本）。失败详情随同步结果即时返回，不再沉睡在库里。

CREATE TABLE conversation_session_observations_slim (
    tenant_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    record_kind TEXT NOT NULL CHECK (record_kind IN ('session', 'web')),
    external_id TEXT NOT NULL,
    observed_version TEXT NOT NULL,
    hydrated_version TEXT,
    last_seen_at TEXT NOT NULL,
    dirty INTEGER NOT NULL DEFAULT 0,
    hydrated_adapter_hash TEXT,
    hydrated_card_contract_version INTEGER,
    hydrated_payload_policy_version INTEGER,
    hydrated_projection_version INTEGER,
    PRIMARY KEY (tenant_id, source_id, record_kind, external_id)
);

INSERT INTO conversation_session_observations_slim (
    tenant_id, source_id, record_kind, external_id, observed_version,
    hydrated_version, last_seen_at, dirty, hydrated_adapter_hash,
    hydrated_card_contract_version, hydrated_payload_policy_version,
    hydrated_projection_version
)
SELECT
    tenant_id, source_id, record_kind, external_id, observed_version,
    hydrated_version, last_seen_at, dirty, hydrated_adapter_hash,
    hydrated_card_contract_version, hydrated_payload_policy_version,
    hydrated_projection_version
FROM conversation_session_observations;

DROP TABLE conversation_session_observations;

ALTER TABLE conversation_session_observations_slim RENAME TO conversation_session_observations;

CREATE INDEX idx_conversation_session_observations_dirty
ON conversation_session_observations(tenant_id, source_id, record_kind, dirty);
