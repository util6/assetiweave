-- Migration: 202609300001_conversation_turn_model.sql
-- Add model column to conversation_turns and web_record_turns

ALTER TABLE conversation_turns ADD COLUMN model TEXT;
ALTER TABLE web_record_turns ADD COLUMN model TEXT;
