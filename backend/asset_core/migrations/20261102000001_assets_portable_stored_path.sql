-- assets 路径契约收敛：不再持久化主机绝对路径。
--
-- 旧列 `absolute_path` 里存的是扫描当刻的物理绝对路径（如 /Users/alice/... 或 C:\Users\alice\...），
-- 一旦换操作系统、换用户名或整体搬迁数据目录就会指向不存在的路径，跨平台不可用。
--
-- 新列 `stored_path` 只承载便携锚点形式（`~/`、`@config/`、`@local-data/`、`@data/`、`@cache/`），
-- 读取 I/O 前统一经 path_utils::expand_path 展开为当前主机的真实路径。
-- 历史数据的锚点回填在 store 启动阶段由 converge_asset_stored_paths 幂等完成
-- （纯 SQL 无法得知当前主机的 home / 系统数据目录）。
ALTER TABLE assets RENAME COLUMN absolute_path TO stored_path;
