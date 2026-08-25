ALTER TABLE conversation_question_turns
ADD COLUMN assignment_origin TEXT NOT NULL DEFAULT 'imported';

ALTER TABLE conversation_question_turns
ADD COLUMN assigned_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z';

ALTER TABLE conversation_question_turns
ADD COLUMN updated_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z';

UPDATE conversation_question_turns
SET assignment_origin = COALESCE(
        (SELECT grouping_origin
         FROM conversation_questions q
         WHERE q.tenant_id = conversation_question_turns.tenant_id
           AND q.id = conversation_question_turns.question_id),
        'imported'
    ),
    assigned_at = COALESCE(
        (SELECT created_at
         FROM conversation_questions q
         WHERE q.tenant_id = conversation_question_turns.tenant_id
           AND q.id = conversation_question_turns.question_id),
        '1970-01-01T00:00:00Z'
    ),
    updated_at = COALESCE(
        (SELECT updated_at
         FROM conversation_questions q
         WHERE q.tenant_id = conversation_question_turns.tenant_id
           AND q.id = conversation_question_turns.question_id),
        '1970-01-01T00:00:00Z'
    );

ALTER TABLE web_record_question_turns
ADD COLUMN assignment_origin TEXT NOT NULL DEFAULT 'imported';

ALTER TABLE web_record_question_turns
ADD COLUMN assigned_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z';

ALTER TABLE web_record_question_turns
ADD COLUMN updated_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z';

UPDATE web_record_question_turns
SET assignment_origin = COALESCE(
        (SELECT grouping_origin
         FROM web_record_questions q
         WHERE q.tenant_id = web_record_question_turns.tenant_id
           AND q.id = web_record_question_turns.question_id),
        'imported'
    ),
    assigned_at = COALESCE(
        (SELECT created_at
         FROM web_record_questions q
         WHERE q.tenant_id = web_record_question_turns.tenant_id
           AND q.id = web_record_question_turns.question_id),
        '1970-01-01T00:00:00Z'
    ),
    updated_at = COALESCE(
        (SELECT updated_at
         FROM web_record_questions q
         WHERE q.tenant_id = web_record_question_turns.tenant_id
           AND q.id = web_record_question_turns.question_id),
        '1970-01-01T00:00:00Z'
    );

CREATE INDEX idx_conversation_question_turns_membership_order
ON conversation_question_turns (tenant_id, question_id, turn_order, turn_id);

CREATE INDEX idx_web_record_question_turns_membership_order
ON web_record_question_turns (tenant_id, question_id, turn_order, turn_id);

CREATE TABLE conversation_question_turn_audits (
    tenant_id TEXT NOT NULL,
    record_kind TEXT NOT NULL,
    question_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    detected_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, record_kind, question_id, turn_id, reason)
);

CREATE INDEX idx_conversation_question_turn_audits_scope
ON conversation_question_turn_audits (tenant_id, record_kind, detected_at);

INSERT OR IGNORE INTO conversation_question_turn_audits (
    tenant_id, record_kind, question_id, turn_id, reason, detected_at
)
SELECT
    qt.tenant_id,
    'session',
    qt.question_id,
    qt.turn_id,
    CASE
        WHEN q.id IS NULL THEN 'missing_question'
        WHEN t.id IS NULL THEN 'missing_turn'
        ELSE 'cross_session'
    END,
    COALESCE(q.updated_at, t.imported_at, '1970-01-01T00:00:00Z')
FROM conversation_question_turns qt
LEFT JOIN conversation_questions q
  ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
LEFT JOIN conversation_turns t
  ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
WHERE q.id IS NULL OR t.id IS NULL OR q.session_id <> t.session_id;

INSERT OR IGNORE INTO conversation_question_turn_audits (
    tenant_id, record_kind, question_id, turn_id, reason, detected_at
)
SELECT
    qt.tenant_id,
    'web',
    qt.question_id,
    qt.turn_id,
    CASE
        WHEN q.id IS NULL THEN 'missing_question'
        WHEN t.id IS NULL THEN 'missing_turn'
        ELSE 'cross_session'
    END,
    COALESCE(q.updated_at, t.imported_at, '1970-01-01T00:00:00Z')
FROM web_record_question_turns qt
LEFT JOIN web_record_questions q
  ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id
LEFT JOIN web_record_turns t
  ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
WHERE q.id IS NULL OR t.id IS NULL OR q.session_id <> t.session_id;
