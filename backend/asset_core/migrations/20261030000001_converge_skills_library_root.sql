-- Migration: 20261030000001_converge_skills_library_root.sql
--
-- 收敛用户技能库根目录：从旧的 ~/.assetiweave/library/skills 或 ~/.assetiweave/tenants/default/library/skills
-- 统一收拢到 ~/.assetiweave/skills。

-- 1. 更新 sources 表中 default 租户的 assetiweave-library-skills 根路径与排除规则
UPDATE sources
SET root_path = '~/.assetiweave/skills',
    exclude_globs = '["**/.git/**","**/.system/**","**/node_modules/**","**/target/**","**/dist/**"]'
WHERE id = 'assetiweave-library-skills'
  AND (root_path LIKE '%library/skills%' OR root_path = '~/.assetiweave/skills');

-- 2. 更新 assets 表中属于 assetiweave-library-skills 的绝对路径
UPDATE assets
SET absolute_path = REPLACE(absolute_path, '/library/skills/', '/skills/')
WHERE source_id = 'assetiweave-library-skills'
  AND absolute_path LIKE '%/library/skills/%';
