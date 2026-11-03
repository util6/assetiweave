-- Migration: 202610230004_conversation_turn_question_columns.sql
--
-- Question 降级为轮次上的两列：分组标识（question_id）与分组来源（question_origin）。
-- 本迁移只做「加列 + 回填 + 索引」，既有成员表原样保留，导入路径在双写期间同时维护
-- 成员表与轮次列，两条读法必须逐轮一致。
--
-- 回填直接沿用成员表上既有的分组标识，保证历史 conversation-question-* /
-- web-record-question-* 标识继续有效；没有成员关系的游离轮次自成一族，取轮次身份
-- 自身的哈希段，保证非空且重复执行不漂移。

ALTER TABLE conversation_turns ADD COLUMN question_id TEXT NOT NULL DEFAULT '';
ALTER TABLE conversation_turns ADD COLUMN question_origin TEXT NOT NULL DEFAULT 'imported';

ALTER TABLE web_record_turns ADD COLUMN question_id TEXT NOT NULL DEFAULT '';
ALTER TABLE web_record_turns ADD COLUMN question_origin TEXT NOT NULL DEFAULT 'imported';

UPDATE conversation_turns
SET question_id = (
        SELECT qt.question_id
        FROM conversation_question_turns qt
        WHERE qt.tenant_id = conversation_turns.tenant_id
          AND qt.turn_id = conversation_turns.id
    ),
    question_origin = (
        SELECT CASE
                   WHEN qt.assignment_origin IN ('imported', 'auto_merged', 'manual')
                       THEN qt.assignment_origin
                   ELSE 'imported'
               END
        FROM conversation_question_turns qt
        WHERE qt.tenant_id = conversation_turns.tenant_id
          AND qt.turn_id = conversation_turns.id
    )
WHERE question_id = ''
  AND EXISTS (
      SELECT 1
      FROM conversation_question_turns qt
      WHERE qt.tenant_id = conversation_turns.tenant_id
        AND qt.turn_id = conversation_turns.id
  );

UPDATE conversation_turns
SET question_id = 'conversation-question-' || substr(id, length('conversation-turn-') + 1),
    question_origin = 'imported'
WHERE question_id = '';

UPDATE web_record_turns
SET question_id = (
        SELECT qt.question_id
        FROM web_record_question_turns qt
        WHERE qt.tenant_id = web_record_turns.tenant_id
          AND qt.turn_id = web_record_turns.id
    ),
    question_origin = (
        SELECT CASE
                   WHEN qt.assignment_origin IN ('imported', 'auto_merged', 'manual')
                       THEN qt.assignment_origin
                   ELSE 'imported'
               END
        FROM web_record_question_turns qt
        WHERE qt.tenant_id = web_record_turns.tenant_id
          AND qt.turn_id = web_record_turns.id
    )
WHERE question_id = ''
  AND EXISTS (
      SELECT 1
      FROM web_record_question_turns qt
      WHERE qt.tenant_id = web_record_turns.tenant_id
        AND qt.turn_id = web_record_turns.id
  );

UPDATE web_record_turns
SET question_id = 'web-record-question-' || substr(id, length('web-record-turn-') + 1),
    question_origin = 'imported'
WHERE question_id = '';

CREATE INDEX IF NOT EXISTS idx_conversation_turns_tenant_session_question
    ON conversation_turns (tenant_id, session_id, question_id);

CREATE INDEX IF NOT EXISTS idx_conversation_turns_tenant_question
    ON conversation_turns (tenant_id, question_id);

CREATE INDEX IF NOT EXISTS idx_web_record_turns_tenant_session_question
    ON web_record_turns (tenant_id, session_id, question_id);
