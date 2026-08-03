ALTER TABLE conversation_parts ADD COLUMN source_execution_id TEXT;
ALTER TABLE web_record_parts ADD COLUMN source_execution_id TEXT;

CREATE INDEX idx_conversation_parts_execution
ON conversation_parts (tenant_id, turn_id, source_execution_id, part_index);

CREATE INDEX idx_web_record_parts_execution
ON web_record_parts (tenant_id, turn_id, source_execution_id, part_index);
