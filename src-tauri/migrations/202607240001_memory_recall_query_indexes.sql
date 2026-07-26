-- Recall enumerates large Conversation scopes by recency before hydrating a bounded page.
CREATE INDEX idx_memory_recall_questions_created
ON conversation_questions(tenant_id, created_at DESC, session_id, question_index);

CREATE INDEX idx_memory_recall_web_questions_created
ON web_record_questions(tenant_id, created_at DESC, session_id, question_index);
