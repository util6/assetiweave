-- Add subagent conversation modeling columns (Issue #148)
ALTER TABLE conversation_turns ADD COLUMN turn_kind TEXT;
ALTER TABLE conversation_turns ADD COLUMN agent_role TEXT;
ALTER TABLE conversation_turns ADD COLUMN parent_turn_id TEXT;

ALTER TABLE web_record_turns ADD COLUMN turn_kind TEXT;
ALTER TABLE web_record_turns ADD COLUMN agent_role TEXT;
ALTER TABLE web_record_turns ADD COLUMN parent_turn_id TEXT;
