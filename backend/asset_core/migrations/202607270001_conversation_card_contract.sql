ALTER TABLE conversation_adapters ADD COLUMN card_contract_version INTEGER;
ALTER TABLE conversation_adapters ADD COLUMN card_kinds_json TEXT NOT NULL DEFAULT '[]';

ALTER TABLE conversation_parts ADD COLUMN content_card_json TEXT;
ALTER TABLE web_record_parts ADD COLUMN content_card_json TEXT;

ALTER TABLE conversation_session_observations ADD COLUMN hydrated_adapter_hash TEXT;
ALTER TABLE conversation_session_observations ADD COLUMN hydrated_card_contract_version INTEGER;
