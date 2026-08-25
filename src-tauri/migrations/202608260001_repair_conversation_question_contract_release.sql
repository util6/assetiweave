CREATE INDEX IF NOT EXISTS idx_memory_recall_questions_created
    ON conversation_questions (tenant_id, created_at DESC, session_id);

CREATE INDEX IF NOT EXISTS idx_memory_recall_web_questions_created
    ON web_record_questions (tenant_id, created_at DESC, session_id);

UPDATE conversation_questions
SET title = (
    SELECT NULLIF(substr(trim(t.user_text), 1, 200), '')
    FROM conversation_question_turns qt
    JOIN conversation_turns t
      ON t.tenant_id = qt.tenant_id
     AND t.id = qt.turn_id
    WHERE qt.tenant_id = conversation_questions.tenant_id
      AND qt.question_id = conversation_questions.id
    ORDER BY qt.turn_order, t.turn_index, t.id
    LIMIT 1
)
WHERE (title IS NULL OR length(trim(title)) = 0)
  AND EXISTS (
      SELECT 1
      FROM conversation_question_turns qt
      JOIN conversation_turns t
        ON t.tenant_id = qt.tenant_id
       AND t.id = qt.turn_id
      WHERE qt.tenant_id = conversation_questions.tenant_id
        AND qt.question_id = conversation_questions.id
        AND length(trim(t.user_text)) > 0
  );

UPDATE web_record_questions
SET title = (
    SELECT NULLIF(substr(trim(t.user_text), 1, 200), '')
    FROM web_record_question_turns qt
    JOIN web_record_turns t
      ON t.tenant_id = qt.tenant_id
     AND t.id = qt.turn_id
    WHERE qt.tenant_id = web_record_questions.tenant_id
      AND qt.question_id = web_record_questions.id
    ORDER BY qt.turn_order, t.turn_index, t.id
    LIMIT 1
)
WHERE (title IS NULL OR length(trim(title)) = 0)
  AND EXISTS (
      SELECT 1
      FROM web_record_question_turns qt
      JOIN web_record_turns t
        ON t.tenant_id = qt.tenant_id
       AND t.id = qt.turn_id
      WHERE qt.tenant_id = web_record_questions.tenant_id
        AND qt.question_id = web_record_questions.id
        AND length(trim(t.user_text)) > 0
  );

INSERT INTO conversation_data_audit_issues (
    tenant_id, id, category, fingerprint, severity, auto_repairable, status,
    affected_count, sample_ids_json, details_json, first_seen_at, last_seen_at
)
SELECT
    t.id,
    'question-snapshot-' || t.id,
    'question_snapshot_dependencies',
    'question_snapshot_dependencies',
    'warning',
    0,
    'open',
    COALESCE((
        SELECT COUNT(*)
        FROM conversation_questions q
        WHERE q.tenant_id = t.id
    ), 0) + COALESCE((
        SELECT COUNT(*)
        FROM web_record_questions q
        WHERE q.tenant_id = t.id
    ), 0),
    '[]',
    '{"message":"The released Question contract migration removed legacy snapshots before audit capture; review authoritative Turn and Part facts","count_kind":"conservative_question_count"}',
    '1970-01-01T00:00:00Z',
    '1970-01-01T00:00:00Z'
FROM tenants t
WHERE EXISTS (
    SELECT 1 FROM conversation_questions q WHERE q.tenant_id = t.id
) OR EXISTS (
    SELECT 1 FROM web_record_questions q WHERE q.tenant_id = t.id
)
ON CONFLICT (tenant_id, fingerprint, status) DO NOTHING;
