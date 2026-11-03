CREATE TABLE conversation_question_redirects (
    tenant_id TEXT NOT NULL,
    source_question_id TEXT NOT NULL,
    target_question_id TEXT NOT NULL,
    operation_kind TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, source_question_id),
    CHECK (source_question_id <> target_question_id)
);

CREATE INDEX idx_conversation_question_redirects_target
ON conversation_question_redirects (tenant_id, target_question_id);
