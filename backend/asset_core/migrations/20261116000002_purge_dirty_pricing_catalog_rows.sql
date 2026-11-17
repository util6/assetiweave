-- Migration: 20261116000002_purge_dirty_pricing_catalog_rows.sql
-- Remove historical non-override, unlocked reference rows that are raw display names
-- (whitespace / parentheses), wildcard variants, or dated variants of a canonical base row.
-- Non-empty usage_aliases on removed rows are merged into the matching base row first,
-- and user_override / price_locked = 1 rows are never touched.

DROP TABLE IF EXISTS temp.pricing_dirty_rows;
DROP TABLE IF EXISTS temp.pricing_dirty_alias_targets;
DROP TABLE IF EXISTS temp.pricing_dirty_merged_aliases;

CREATE TEMP TABLE pricing_dirty_rows AS
SELECT c.id, c.tenant_id, c.model_id, c.provider, c.usage_aliases
FROM model_pricing_catalog c
WHERE c.source_channel != 'user_override'
  AND c.price_locked = 0
  AND (
    c.model_id != TRIM(c.model_id)
    OR INSTR(c.model_id, ' ') > 0
    OR INSTR(c.model_id, char(9)) > 0
    OR INSTR(c.model_id, '(') > 0
    OR INSTR(c.model_id, ')') > 0
    OR INSTR(c.model_id, '*') > 0
    OR EXISTS (
      SELECT 1
      FROM model_pricing_catalog b
      WHERE b.tenant_id = c.tenant_id
        AND LOWER(TRIM(b.provider)) = LOWER(TRIM(c.provider))
        AND b.source_channel != 'user_override'
        AND b.model_id = CASE
            WHEN c.model_id GLOB '*-[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
              THEN SUBSTR(c.model_id, 1, LENGTH(c.model_id) - 11)
            WHEN c.model_id GLOB '*-[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]'
              THEN SUBSTR(c.model_id, 1, LENGTH(c.model_id) - 9)
            ELSE NULL
          END
    )
  );

CREATE TEMP TABLE pricing_dirty_alias_targets AS
SELECT d.id AS dirty_id,
       (
         SELECT b.id
         FROM model_pricing_catalog b
         WHERE b.tenant_id = d.tenant_id
           AND b.id != d.id
           AND b.source_channel != 'user_override'
           AND b.model_id = TRIM(b.model_id)
           AND INSTR(b.model_id, ' ') = 0
           AND INSTR(b.model_id, '(') = 0
           AND INSTR(b.model_id, ')') = 0
           AND INSTR(b.model_id, '*') = 0
           AND (
             LOWER(TRIM(b.provider)) = LOWER(TRIM(d.provider))
             OR TRIM(d.provider) = ''
             OR TRIM(b.provider) = ''
           )
           AND (
             LOWER(TRIM(REPLACE(d.model_id, '*', ''))) = LOWER(TRIM(b.model_id))
             OR LOWER(TRIM(REPLACE(d.model_id, '*', ''))) LIKE LOWER(TRIM(b.model_id)) || '-%'
             OR LOWER(TRIM(REPLACE(d.model_id, '*', ''))) LIKE LOWER(TRIM(b.model_id)) || ' %'
             OR LOWER(TRIM(REPLACE(d.model_id, '*', ''))) LIKE LOWER(TRIM(b.model_id)) || '(%'
             OR LOWER(TRIM(REPLACE(d.model_id, '*', ''))) LIKE LOWER(TRIM(b.model_id)) || '.%'
             OR LOWER(TRIM(b.model_id)) LIKE LOWER(TRIM(REPLACE(d.model_id, '*', ''))) || '-%'
             OR LOWER(TRIM(b.model_id)) LIKE LOWER(TRIM(REPLACE(d.model_id, '*', ''))) || ' %'
           )
         ORDER BY LENGTH(b.model_id) DESC
         LIMIT 1
       ) AS target_id
FROM pricing_dirty_rows d
WHERE TRIM(d.usage_aliases) NOT IN ('', '[]');

CREATE TEMP TABLE pricing_dirty_merged_aliases AS
SELECT target_id, json_group_array(DISTINCT alias) AS aliases
FROM (
  SELECT t.target_id AS target_id, j.value AS alias
  FROM pricing_dirty_alias_targets t
  JOIN pricing_dirty_rows d ON d.id = t.dirty_id
  JOIN json_each(d.usage_aliases) j
  WHERE t.target_id IS NOT NULL
  UNION
  SELECT b.id AS target_id, j.value AS alias
  FROM model_pricing_catalog b
  JOIN json_each(b.usage_aliases) j
  WHERE b.id IN (SELECT target_id FROM pricing_dirty_alias_targets WHERE target_id IS NOT NULL)
)
GROUP BY target_id;

UPDATE model_pricing_catalog
SET usage_aliases = (
  SELECT m.aliases
  FROM pricing_dirty_merged_aliases m
  WHERE m.target_id = model_pricing_catalog.id
)
WHERE id IN (SELECT target_id FROM pricing_dirty_merged_aliases);

DELETE FROM model_pricing_catalog
WHERE id IN (
  SELECT d.id
  FROM pricing_dirty_rows d
  LEFT JOIN pricing_dirty_alias_targets t ON t.dirty_id = d.id
  WHERE TRIM(d.usage_aliases) IN ('', '[]') OR t.target_id IS NOT NULL
);

DROP TABLE IF EXISTS temp.pricing_dirty_rows;
DROP TABLE IF EXISTS temp.pricing_dirty_alias_targets;
DROP TABLE IF EXISTS temp.pricing_dirty_merged_aliases;
