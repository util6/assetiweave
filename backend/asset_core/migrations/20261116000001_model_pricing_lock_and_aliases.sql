-- Migration: 20261116000001_model_pricing_lock_and_aliases.sql
-- Add price lock and explicit usage aliases columns to model_pricing_catalog,
-- backfill lock state for existing user_override rows, and prune dirty non-override rows.

ALTER TABLE model_pricing_catalog ADD COLUMN price_locked INTEGER NOT NULL DEFAULT 0;
ALTER TABLE model_pricing_catalog ADD COLUMN usage_aliases TEXT NOT NULL DEFAULT '[]';

UPDATE model_pricing_catalog
SET price_locked = 1
WHERE source_channel = 'user_override';

DELETE FROM model_pricing_catalog
WHERE source_channel != 'user_override'
  AND (
    model_id LIKE '%"%'
    OR model_id LIKE '%''%'
    OR model_id LIKE '%{%'
    OR model_id LIKE '%}%'
    OR model_id LIKE '%' || char(10) || '%'
    OR model_id LIKE '%' || char(13) || '%'
  );
