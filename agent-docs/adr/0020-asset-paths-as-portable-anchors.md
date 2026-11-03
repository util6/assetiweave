# 0020: 资产路径以便携锚点存储、读取时展开

> **状态**：已接受
> **决策日期**：2026-11-02
> **关联**：`backend/asset_core/src/infrastructure/host_paths.rs`（锚点解析器）、`assets` 表路径列契约

---

## 背景

`assets` 表自初始 schema 起就带一列 `absolute_path`，写入方是扫描器的 `build_asset`，取值直接是 `asset_path.to_string_lossy()`——即扫描当刻本机的物理绝对路径。

这带来三个问题：

1. **跨平台不可用**：`/Users/alice/.claude/skills/x` 与 `C:\Users\alice\.claude\skills\x` 是两套互不兼容的路径语义。数据库随用户数据目录迁移到另一台机器或另一个操作系统后，整张资产表指向不存在的路径，所有依赖它的挂载、备份、删除、reveal 操作全部失效。
2. **与 `sources` 表契约分裂**：`sources.root_path` 早在 `normalize_source_paths` 中折叠为 `~/`、`@config/` 等锚点，写入前折叠、读取前展开；`assets` 是唯一仍然直存物理绝对路径的目录类表。
3. **隐式绝对路径假设**：多处代码直接 `Path::new(&asset.absolute_path)` 做存在性判断或路径相等比较，绕过了 `expand_path`。一旦路径形态改变，这些比较会静默返回 false 而不报错。

`absolute_path` 在语义上是 `source.root_path + relative_path` 的派生值，把派生值持久化即为冗余副本，是跨平台失效的根因。

---

## 决策

### 1. 列改名：`assets.absolute_path` → `assets.stored_path`

迁移 `20261102000001_assets_portable_stored_path.sql` 执行 `ALTER TABLE assets RENAME COLUMN absolute_path TO stored_path`。

改名而非仅改内容，是为了让列名与新契约一致：列里存的是**便携存储路径**，不是绝对路径。保留 `absolute_path` 这个名字会持续诱导后来者再次写入物理路径。

### 2. 写入侧：一律折叠为便携锚点

- 扫描器 `build_asset` 用 `normalize_std_path_for_storage` 折叠。
- 可折叠的目标包括用户目录 `~/`、系统数据目录 `@config/`、`@local-data/`、`@data/`、`@cache/`。
- 不在上述目录之下（如外接卷、系统目录）的路径无法折叠，按原样保留绝对形式——这是可接受的降级，不是遗漏。

### 3. 读取侧：I/O 与比较前必须展开

- 统一入口 `path_utils::resolve_asset_path(&asset)`（内部即 `expand_path(&asset.stored_path)`）。
- 严禁 `Path::new(&asset.stored_path)` 直接做 I/O、存在性判断或路径相等比较。
- `expand_path` 对锚点与遗留绝对路径都能解析，因此迁移过渡期天然兼容旧数据。

### 4. 历史数据：启动阶段幂等收敛

纯 SQL 迁移无法得知当前主机的 home 与系统数据目录，因此 `store::database::open_migrated_pool` 在迁移执行后调用 `converge_asset_stored_paths`：

- 全表读取 `stored_path`，经 `normalize_path_for_storage` 归一；
- 与原值不同的才写入，锚点形式归一后不变，因此**幂等且零写入**；
- 无法折叠的路径保持原样。

### 5. 展示层与前端

- `CatalogAsset.display_path` 继续提供折叠展示形态。
- 前端 `Asset` 类型同步改为 `stored_path`。`reveal_path` 命令本就走 `expand_path`，因此传锚点即可，无需前端自行拼接 home。

---

## 后果

**收益**

- 数据库文件可以跨机器、跨操作系统整体搬迁，资产路径在目标主机上自动重解析。
- `sources` 与 `assets` 两张表的路径契约统一，读写两侧规则一致。
- 消除了多处以物理绝对路径做相等比较的静默失败隐患。

**代价与约束**

- 任何新增的资产路径使用点都必须走 `resolve_asset_path` / `expand_path`，直接在 SQL 或 Rust 里拼路径会绕过契约。
- 无法折叠到用户目录或系统数据目录的资产（外接卷、系统目录）仍不可跨平台，需要在 UI 层按“不可移植”处理，而不是假装可移植。
- `stored_path` 的字典序排序跨平台不保证一致；它只被用作资产并列时的 tie-breaker，不承载业务语义。
