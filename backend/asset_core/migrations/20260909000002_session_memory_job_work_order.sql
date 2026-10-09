-- Migration: 20260909000002_session_memory_job_work_order.sql
-- Add Recipe snapshot, budget policy version, and Work Order persistence columns

ALTER TABLE session_memory_jobs ADD COLUMN recipe_id TEXT;
ALTER TABLE session_memory_jobs ADD COLUMN recipe_revision INTEGER DEFAULT 1;
ALTER TABLE session_memory_jobs ADD COLUMN recipe_content_hash TEXT;
ALTER TABLE session_memory_jobs ADD COLUMN budget_policy_version TEXT DEFAULT 'budget.v1';
ALTER TABLE session_memory_jobs ADD COLUMN work_order_json TEXT;

ALTER TABLE session_memories ADD COLUMN recipe_id TEXT;
ALTER TABLE session_memories ADD COLUMN recipe_content_hash TEXT;
ALTER TABLE session_memories ADD COLUMN work_order_json TEXT;
