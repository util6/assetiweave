-- Migration: 202610230007_drop_conversation_question_tables.sql
--
-- 删除问题表、成员关系表与审计表：
-- 1. conversation_question_turns
-- 2. conversation_questions
-- 3. conversation_question_turn_audits
-- 4. web_record_question_turns
-- 5. web_record_questions
--
-- 确保轮次表上的分组索引完整：
-- - conversation_turns(tenant_id, session_id, question_id)
-- - conversation_turns(tenant_id, question_id)
-- - web_record_turns(tenant_id, session_id, question_id)
--
-- 可执行回滚路径说明（Rollback / Reversible Recovery）：
-- 如需回滚本迁移，可按以下结构重建 5 张表并基于 conversation_turns / web_record_turns 的 question_id、question_origin 列回填数据：
-- 1. 重建表结构：
--    CREATE TABLE conversation_questions (
--        tenant_id TEXT NOT NULL DEFAULT 'default',
--        id TEXT NOT NULL,
--        session_id TEXT NOT NULL,
--        title TEXT,
--        created_at TEXT NOT NULL,
--        updated_at TEXT NOT NULL,
--        PRIMARY KEY (tenant_id, id),
--        FOREIGN KEY (tenant_id, session_id) REFERENCES conversation_sessions(tenant_id, id) ON DELETE CASCADE
--    );
--    CREATE TABLE conversation_question_turns (
--        tenant_id TEXT NOT NULL DEFAULT 'default',
--        question_id TEXT NOT NULL,
--        turn_id TEXT NOT NULL,
--        turn_order INTEGER NOT NULL,
--        assignment_origin TEXT NOT NULL DEFAULT 'imported',
--        assigned_at TEXT NOT NULL,
--        updated_at TEXT NOT NULL,
--        PRIMARY KEY (tenant_id, question_id, turn_id),
--        UNIQUE (tenant_id, turn_id),
--        FOREIGN KEY (tenant_id, question_id) REFERENCES conversation_questions(tenant_id, id) ON DELETE CASCADE,
--        FOREIGN KEY (tenant_id, turn_id) REFERENCES conversation_turns(tenant_id, id) ON DELETE CASCADE
--    );
--    CREATE TABLE conversation_question_turn_audits (
--        tenant_id TEXT NOT NULL DEFAULT 'default',
--        record_kind TEXT NOT NULL,
--        question_id TEXT NOT NULL,
--        turn_id TEXT NOT NULL,
--        reason TEXT NOT NULL,
--        detected_at TEXT NOT NULL,
--        PRIMARY KEY (tenant_id, record_kind, question_id, turn_id, reason)
--    );
--    CREATE TABLE web_record_questions (
--        tenant_id TEXT NOT NULL DEFAULT 'default',
--        id TEXT NOT NULL,
--        session_id TEXT NOT NULL,
--        title TEXT,
--        created_at TEXT NOT NULL,
--        updated_at TEXT NOT NULL,
--        PRIMARY KEY (tenant_id, id),
--        FOREIGN KEY (tenant_id, session_id) REFERENCES web_record_sessions(tenant_id, id) ON DELETE CASCADE
--    );
--    CREATE TABLE web_record_question_turns (
--        tenant_id TEXT NOT NULL DEFAULT 'default',
--        question_id TEXT NOT NULL,
--        turn_id TEXT NOT NULL,
--        turn_order INTEGER NOT NULL,
--        assignment_origin TEXT NOT NULL DEFAULT 'imported',
--        assigned_at TEXT NOT NULL,
--        updated_at TEXT NOT NULL,
--        PRIMARY KEY (tenant_id, question_id, turn_id),
--        UNIQUE (tenant_id, turn_id),
--        FOREIGN KEY (tenant_id, question_id) REFERENCES web_record_questions(tenant_id, id) ON DELETE CASCADE,
--        FOREIGN KEY (tenant_id, turn_id) REFERENCES web_record_turns(tenant_id, id) ON DELETE CASCADE
--    );
-- 2. 基于轮次列回填问题与成员关系：
--    INSERT INTO conversation_questions (tenant_id, id, session_id, title, created_at, updated_at)
--    SELECT tenant_id, question_id, session_id,
--           (SELECT t_title.user_text FROM conversation_turns t_title WHERE t_title.tenant_id = t.tenant_id AND t_title.session_id = t.session_id AND t_title.question_id = t.question_id ORDER BY t_title.turn_index ASC, t_title.id ASC LIMIT 1),
--           MIN(imported_at), MAX(imported_at)
--    FROM conversation_turns t
--    WHERE question_id <> ''
--    GROUP BY tenant_id, session_id, question_id;
--    INSERT INTO conversation_question_turns (tenant_id, question_id, turn_id, turn_order, assignment_origin, assigned_at, updated_at)
--    SELECT tenant_id, question_id, id, turn_index, question_origin, imported_at, imported_at
--    FROM conversation_turns
--    WHERE question_id <> '';
--    （web_record_turns 对应回填 web_record_questions 与 web_record_question_turns）

DROP TABLE IF EXISTS conversation_question_turns;
DROP TABLE IF EXISTS conversation_questions;
DROP TABLE IF EXISTS conversation_question_turn_audits;
DROP TABLE IF EXISTS web_record_question_turns;
DROP TABLE IF EXISTS web_record_questions;

CREATE INDEX IF NOT EXISTS idx_conversation_turns_tenant_session_question
    ON conversation_turns (tenant_id, session_id, question_id);

CREATE INDEX IF NOT EXISTS idx_conversation_turns_tenant_question
    ON conversation_turns (tenant_id, question_id);

CREATE INDEX IF NOT EXISTS idx_web_record_turns_tenant_session_question
    ON web_record_turns (tenant_id, session_id, question_id);
