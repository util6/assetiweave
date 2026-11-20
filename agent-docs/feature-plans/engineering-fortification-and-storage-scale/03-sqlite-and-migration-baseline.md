# Wave 2 · SQLite 现状考古：参数、迁移、索引

> 调研方式：**只读**。代码层面通读 `backend/asset_core/src/store/database.rs` 全文 + 相关写入路径；
> 数据层面用 `file:$HOME/.assetiweave/app.db?mode=ro` 只读连接（SQLite 3.54.0 / `-readonly`），
> 全程无写入、无 git 操作、未改动仓库任何文件。
> 真实库快照时间 2026-11-20；当时存在 36 MB WAL（`app.db-wal`），说明有未 checkpoint 的页。

---

## 0. 结论速览

| # | 结论 | 证据 |
|---|---|---|
| 1 | 库文件 1.60 GiB 里有 **77.5% 的页在 freelist**，`VACUUM` 可回收 ≈ **1.24 GiB**（落地后 ≈ **350–388 MiB**，不是精确 370） | `freelist_count=325384 / page_count=420063`；`payload=321.3 MiB`、`unused=44.5 MiB` |
| 2 | 连接池只设了 4 个参数（FK / WAL / NORMAL / busy_timeout）+ `max_connections(5)`；`cache_size`、`mmap_size`、`temp_store`、`wal_autocheckpoint`、`journal_size_limit`、`auto_vacuum`、`optimize` 全部没设 | `database.rs:62-73` |
| 3 | **从未跑过 `ANALYZE`/`PRAGMA optimize`**（库里没有 `sqlite_stat1`），查询计划全靠猜 | `sqlite_master` 无 `sqlite_stat%` |
| 4 | 写事务里 **41 处默认 `DEFERRED`**，只有 **16 处**走 `BEGIN IMMEDIATE`；最热的批量导入 `import.rs:108` 正是"先读后写 + DEFERRED" | 见 A.4 |
| 5 | 97 个迁移的版本号已全部统一 14 位；但 **6 个迁移里的 `PRAGMA foreign_keys = OFF` 全是空操作**（sqlx 把每个迁移包在事务里，SQLite 文档明确该 pragma 在事务内 no-op） | `sqlx-sqlite/src/migrate.rs` + sqlite.org/pragma |
| 6 | 启动期有 8 段自愈逻辑，其中 `converge_portable_paths` 每次启动**全表扫 220 MiB 的 `conversation_parts`**，且当前仍命中 110 行、永远收敛不到 0 | `path_convergence.rs:42` |
| 7 | 索引 106 个（44 命名 + 62 自建），实测可安全删除 **5 个**，其中最干净的一个是 `UNIQUE(tenant_id, model_id)` 的**逐列完全副本** | 见 C.3 |
| 8 | **11.21 GiB 的 `app.db.bak_*` 是"孤儿资产"**：不是 App 产物（App 产物名 `assetiweave-app-*.db`，一个都没有），是人工/脚本 `cp` 的裸副本；App 备份轮转不覆盖它们，压缩上线后也不会缩小。App 的 `VACUUM INTO` 逻辑本身正确 | 见 B.5；`backup/mod.rs:204-208` |
| 9 | **优先级提醒**：SQLite 引擎侧上限 1.24 GiB，而 `conversation-search-index` 垃圾代数 22.68 GiB（res-storage 实测，为本项 18 倍）→ 引擎参数是 P1/P2，不是主线 | 见 D 开头 |
| 10 | **只读命令会被写锁连坐**：`database.rs:124-131` 的零命中 `UPDATE` 每次启动都抢 RESERVED 锁；别人持写事务时 `aiwc overview` 10 s 后 `code:5` 启动失败。且修它**不够**——`bootstrap.rs:46` 的 `seed_defaults` 里还有 4 处无条件 upsert（`tenant_repo.rs:14-35` 等）。实测：`UPDATE`/`INSERT OR IGNORE` 零命中也抢锁，`SELECT` 与已存在的 `CREATE TABLE IF NOT EXISTS` 不抢 | 见 A.6（含实测表）；res-ipc 原始复现 |

---

## A. 连接池与 PRAGMA 现状

### A.1 唯一的连接池入口

生产路径只有一个：`application/system/bootstrap.rs:44` → `store::open_migrated_pool(&db_path)`
（`grep SqlitePoolOptions` 全仓只有测试另建池）。所以改 `database.rs:62-73` 就是改全局。

### A.2 已设置 / 未设置参数对照

`database.rs:62-73` 原文：

```rust
let options = SqliteConnectOptions::new()
    .filename(db_path)
    .create_if_missing(true)
    .foreign_keys(true)                       // :65
    .journal_mode(SqliteJournalMode::Wal)     // :66
    .synchronous(SqliteSynchronous::Normal)   // :67
    .busy_timeout(Duration::from_secs(10));   // :68
let pool = SqlitePoolOptions::new()
    .max_connections(5)                       // :70
    .connect_with(options)
```

| 参数 | 现状 | 代码/默认依据 | Wave 2 建议 |
|---|---|---|---|
| `journal_mode` | **WAL**（已设，正确） | `database.rs:66`；库内实测 `wal` | 保持 |
| `synchronous` | **NORMAL**（已设） | `database.rs:67`；sqlx 默认是 `FULL`（docs.rs `synchronous`） | 保持（WAL+NORMAL 是标准组合） |
| `foreign_keys` | **ON**（已设） | `database.rs:65`；sqlx 默认本来就 ON | 保持 |
| `busy_timeout` | **10s**（已设，sqlx 默认 5s） | `database.rs:68` | 保持；**但**代码里另有自己的退避重试（`common.rs:342-356`），两者叠加 |
| `max_connections` | **5** | `database.rs:70` | 见 A.3 |
| `cache_size` | ❌ 未设 → 每连接 ~2 MB（`-2000`） | sqlite.org/pragma：默认 `-2000` ≈ 2,048,000 B | 建议 `-16000`（16 MB/连接 × 5 ≈ 80 MB） |
| `mmap_size` | ❌ 未设 → 生效值 0（无 mmap） | 本机实测 `PRAGMA mmap_size` = 0 | 建议 256 MiB（5 连接共享同一文件映射，物理页由 OS 共享） |
| `temp_store` | ❌ 未设 → 0/DEFAULT（落盘） | 实测 `temp_store=0` | 建议 `MEMORY`：用量报表有 `ORDER BY timestamp` + `model IN (...)`，会走临时 B 树 |
| `wal_autocheckpoint` | ❌ 未设 → 1000 页（≈4 MB） | 实测 1000；当前 WAL 已 36 MB | 建议显式写死并**排查 checkpoint 饥饿**（见 A.3 末尾） |
| `journal_size_limit` | ❌ 未设 → -1（无上限） | sqlite.org/pragma："default journal size limit is -1 (no limit)" | 建议 64 MiB，防止 WAL 无限膨胀 |
| `auto_vacuum` | ❌ 未设 → 0/NONE | sqlite.org/pragma："default setting is 0 or 'none'"；实测 0 | 见 B.4：先 `VACUUM` 才能切 `INCREMENTAL` |
| `PRAGMA optimize` | ❌ 从未执行（无 `sqlite_stat1`） | sqlite.org/pragma 建议：短连接在关闭前跑一次 | **sqlx 0.9 原生支持** `optimize_on_close(true, None)`（docs.rs：默认关闭） |
| `statement_cache_capacity` | ❌ 未设 → **100 条/连接** | docs.rs："The default cache capacity is 100 statements." | 生产 SQL 数量远超 100×5，命中率会抖；建议 256 |
| `txlock` | ⚠ **不存在该 API** | sqlx `SqliteConnectOptions` 无 `txlock`；只能像 `database.rs:42-52` 那样显式 `BEGIN IMMEDIATE` | 保持现状方案，靠改调用点（A.4） |
| `locking_mode` | 未设 → NORMAL | docs.rs："The default locking mode is NORMAL." | 保持（改 EXCLUSIVE 会挡住备份/外部工具） |
| `shared_cache` | 未设 → 关闭 | docs.rs："By default, this is disabled." | **保持关闭**（WAL + shared cache 已知有锁语义坑） |
| `page_size` | 未设 → 4096 | 库内实测 4096 | **保持 4096，不进候选**：`internal` 页全库仅 447 页 / 1.83 MB，靠调大 page_size 减少 B 树层数的收益上限就是 1.83 MB，且 WAL 下改 page_size 必须整库重建（见 B.4） |

> 注：`mmap_size` / `temp_store` / `wal_autocheckpoint` / `journal_size_limit` / `cache_size` 在 sqlx 里**没有专用方法**，统一用 `.pragma("名字", "值")`（docs.rs `pragma` 签名：`pragma<K: Into<Cow<'static,str>>, V: Into<Cow<'static,str>>>(self, key: K, value: V)`）。

### A.3 `max_connections(5)` 对"大库 + 长事务"是否合理

**结论：5 个连接本身不是瓶颈，真正的问题在事务粒度与锁模式。**

- WAL 下读不阻塞写，5 个连接对"多路读 + 单写"是够的（甚至偏多：`SQLITE_MAX_DEFAULT` 与内存相关，5 个连接 × 2 MB page cache ≈ 10 MB，可忽略）。
- 写入在 SQLite 里天然串行（一个 RESERVED 锁），**连接数再多也不能提升写吞吐**；把 `max_connections` 调到 8/10 只会增加空闲连接与内存。
- 关键长事务/大批量写点（按影响排序）：

| 写点 | 位置 | 事务边界 | 规模 |
|---|---|---|---|
| 批量导入（会话×8） | `conversation_repo/import.rs:107-108`，`common.rs:3` `CONVERSATION_IMPORT_BATCH_SIZE = 8` | **`pool.begin()`（DEFERRED）**，8 个会话整包替换 | 每会话 DELETE parts+turns 再全量重插；单事务可能上千 parts |
| 快照整包替换 | `store/conversations/snapshot.rs:431-475`（`replace_session_snapshot_sqlx_tx`） | 由调用方提供 tx | `DELETE parts WHERE turn_id IN (...)` + `DELETE turns` + 逐轮 INSERT |
| 重解析/重投影 | `conversation_repo/reproject.rs:296,315` | ✅ `begin_immediate_with_retry(pool, 8)` | 全量重写 |
| 高级导入（阶段化） | `import_advanced.rs:146,208,301,346` | ✅ IMMEDIATE + 8 次重试 | 多阶段，事务之间可观测 |
| 缺失对账 / 收尾 | `import_advanced.rs:296-315,346` | ✅ IMMEDIATE | 全源扫描 |
| 伪轮次折算 | `conversation_subagent_fold.rs:140,309` | ❌ `pool.begin()` | 逐会话整包替换 |
| 迁移前自愈 | `database.rs:255,389,415` | ❌ `pool.begin()` | 全表 UPDATE `conversation_parts`（见 B.2 #4） |

- **WAL 36 MB 而 `wal_autocheckpoint=1000`（4 MB）**：说明 checkpoint 长期被"活跃读快照"顶住，或者写量大到追不上。36 MB WAL 会让每次随机读多走一层 WAL 索引，是当前可观测的性能税。建议：显式设 `wal_autocheckpoint`，并在空闲期主动 `PRAGMA wal_checkpoint(TRUNCATE)`（仓里已有该调用：`infrastructure/backup/mod.rs:197`、`store/system/backup_repo.rs:5`）。

### A.4 `begin_immediate` 使用点盘点

全仓 `begin_immediate` 出现 28 次（含 1 处定义、1 处 helper、1 处 `Database` 方法、2 处测试）。

**IMMEDIATE 写路径 16 处**（`database.rs:42` 定义 / `common.rs:342` helper）：

| 文件 | 行 |
|---|---|
| `conversation_repo/import_advanced.rs` | 146, 208, 301, 346 |
| `conversation_repo/packages.rs` | 138, 199, 234, 408 |
| `conversation_repo/reproject.rs` | 296, 315 |
| `web_record_repo/import.rs` | 34, 44, 81 |
| `conversation_repo/observations.rs` | 281 |
| `memory/memory_recall_repo/turns.rs` | 241 |
| `store/path_convergence.rs` | 159 |

**DEFERRED 41 处**（`pool.begin()`，全部非测试），按风险分级：

| 风险 | 原因 | 位置 |
|---|---|---|
| 🔴 高（先读后写，正是 `SDK 517` 的成因，见 `database.rs:37-41` 注释） | 读建立快照后再升级写锁 | `conversation_repo/import.rs:108`（最热）、`import.rs:183`、`conversation_repo/projection.rs:11`、`conversation_repo/triad_update.rs:96`、`conversation_repo/sources.rs:67`、`conversation_repo/adapters.rs:168,210,274`、`conversation_repo/reproject.rs:77,350`、`usage_repo.rs:111`、`pricing_repo.rs:25,450`、`pricing_repo_ops.rs:115,342,442`、`search_index_repo.rs:220,277`、`conversation_turn_identity.rs:99,107`、`conversation_subagent_fold.rs:140,309`、`memory_recall_repo/turns.rs:112,194`、`conversation_maintenance_ops.rs:334` |
| 🟡 中（启动期批量修复） | 与前台写入并发 | `database.rs:255,389,415` |
| 🟢 低（纯写，无前置读） | DEFERRED 只在第一条写语句取锁，语义等价 | `mounting/mount_snapshot_repo.rs:26`、`mounting/profile_repo.rs:74`、`mounting/mount_repo.rs:82,96`、`catalog/asset_repo.rs:70`、`catalog/group_repo.rs:108,160`、`system/tenant_repo.rs:16,144`、`system/menu_repo.rs:82`、`system/extension_catalog_releases.rs:151`、`system/agent_installations.rs:101,364` |

> 建议：**只改 🔴 那批**（约 22 处）统一换 `begin_immediate_with_retry(pool, 8)`；🟢 保持 DEFERRED，因为 DEFERRED 在纯写场景下不增加任何失败概率，还少一次无谓的锁持有。

### A.5 缺失 PRAGMA 的收益排序与安全默认值

排序依据 = 对本库真实负载的边际收益 ÷ 风险。负载画像：读侧以 `conversation_parts`（220 MiB / 160k 行）随机取段 + `ORDER BY timestamp` 报表；写侧以防批导入整包替换为主，**单写者、写天然串行**；`sqlite_stat1` 不存在。

| 排名 | PRAGMA | 安全默认值 | 收益 | 风险 / 注意 |
|---|---|---|---|---|
| **1** | `optimize_on_close`（`PRAGMA optimize`） | `.optimize_on_close(true, None)` | **当前完全没有统计信息**（无 `sqlite_stat1`），优化器对 6 个 usage 索引、`id_key` 区间扫、`turn_id IN (...)` 全在猜。建统计后 `ORDER BY timestamp` / `model IN (...)` 这类会直接换计划 | 极低。官方原文："This pragma is usually a no-op or nearly so and is very fast."（[sqlite.org/pragma#optimize](https://www.sqlite.org/pragma.html#pragma_optimize)） |
| **2** | `mmap_size` | `268435456`（256 MiB） | 消掉 read() 拷贝；本库热点是 220 MiB 表 + 18 MiB 索引的随机访存，内存映射收益最直接 | 低。32 位构建需 `cfg` 降级；5 连接映射同一文件，物理页由 OS 共享，不叠加 |
| **3** | `cache_size` | `-16000`（16 MiB/连接） | 默认仅 `-2000` ≈ 2 MB/连接（官方："The default suggested cache size is -2000, which means the cache size is limited to 2048000 bytes of memory."），5 连接合计 10 MB，装不下 `conversation_parts` 的任何有效工作集 | 低。×5 ≈ 80 MB 常驻 |
| **4** | `journal_size_limit` | `67108864`（64 MiB） | 当前默认 `-1`（无上限，官方："The default journal size limit is -1 (no limit)."），WAL 已长到 **36.8 MB**；设上限后 checkpoint 完会自动截断，避免 WAL 无限膨胀拖慢每次随机读 | 零。只在 checkpoint 后生效 |
| **5** | `temp_store` | `MEMORY` | 用量报表 `ORDER BY u.timestamp ASC` + `model IN (?)` + `DISTINCT model ORDER BY` 会建临时 B 树；默认 0/DEFAULT 走磁盘临时文件 | 低。排序规模 ≤ 24,493 行，内存可控；大结果集排序时会吃 RAM |
| **6** | `statement_cache_capacity` | `256` | 默认 **100 条/连接**（docs.rs 原文："The default cache capacity is 100 statements."）；本仓 SQL 数量（含动态拼的 usage 报表、`ASSERT_SQL_SAFE` 的 `{parts}` 模板）远超 500 条，LRU 会频繁重编译 | 低。只吃内存 |
| **7** | `wal_autocheckpoint` | `2000`（≈8 MB）后再调 | 当前默认 1000 页（≈4 MB）却有 36.8 MB WAL → 说明 checkpoint 长期被读快照顶住。**这是症状不是病因**，先显式化再观测 | 中。调大只会让 WAL 更长；真正解法是排查长读事务/主动 `wal_checkpoint(TRUNCATE)` |
| 8 | `auto_vacuum` | 先 `VACUUM` 再 `INCREMENTAL` | 77.5% 空洞的根因是"只删不还页"。**当前 `auto_vacuum=0` 时 `PRAGMA incremental_vacuum` 是空操作**——碎片会持续复发，这是机制性的 | 高成本：必须先做一次全库 `VACUUM`（≈400 MiB 临时盘 + 全程写锁），之后 `INCREMENTAL` 才生效。官方："For existing databases, a change to this value does not take effect unless a VACUUM command is executed." |
| **9** | `synchronous` | **保持 `NORMAL`，勿改 FULL** | 改 FULL 会让每次 commit 多一次 fsync；导入是"8 会话/事务"的批式提交，代价被放大 | 不建议改 |

**`PRAGMA optimize` 的官方最佳实践**（三条，直接对应本仓形态）：

> 1. "Applications with short-lived database connections should run 'PRAGMA optimize;' once, just prior to closing each database connection."
> 2. "Applications that use long-lived database connections should run 'PRAGMA optimize=0x10002;' when the connection is first opened, and then also run 'PRAGMA optimize;' periodically, perhaps once per day or once per hour."
> 3. "All applications should run 'PRAGMA optimize;' after a schema change, especially after one or more CREATE INDEX statements."

本仓是**长连接池**（`SqlitePool`，5 连接常驻），按第 2 条应：连接建立后跑 `PRAGMA optimize=0x10002`（只做轻量分析，带 `analysis_limit` 保护），再加一个每日/每小时的 `PRAGMA optimize`。sqlx 0.9 已把这个能力内置成 `optimize_on_close(enabled, analysis_limit)`（docs.rs 明确 "Not enabled by default"），开启后连接归还/关闭时自动执行——**这是零代码侵入的接入点**，建议第 1 档先只开它，观察 `sqlite_stat1` 是否出现、慢查询是否变化。

> 注：`cache_size` / `mmap_size` / `temp_store` / `wal_autocheckpoint` / `journal_size_limit` 在 sqlx 里**没有专用方法**，统一用 `.pragma("名字", "值")`。

### A.6 启动期无条件写 ⇒ 只读命令被写锁「连坐」（实测，含对 res-ipc 结论的修正）

res-ipc 实测：另一进程持 `BEGIN IMMEDIATE` 时，`aiwc overview`（纯只读命令）在 10.57 s 后报
`(code: 5) database is locked` 而 `user` CPU 仅 0.01 s。我在临时库上把机制拆开验证（结果确定性、可复现）：

**实测（WAL 模式，另一进程持 `BEGIN IMMEDIATE`，busy_timeout=800 ms）**

| 语句 | 结果 | 结论 |
|---|---|---|
| `SELECT count(*) FROM t` | OK 0.00s | 读不受影响（WAL） |
| `UPDATE ... WHERE version < 1e13`（**命中 0 行**） | **FAIL** 0.88s `database is locked` | **零命中也照样开写事务** |
| `UPDATE ... SET version=version WHERE 0` | **FAIL** 0.88s | 同上，与是否命中无关 |
| `CREATE TABLE IF NOT EXISTS <已存在的表>` | OK 0.00s | **SQLite 会短路，不取写锁** |
| `INSERT OR IGNORE`（主键已存在） | **FAIL** 0.88s | 任何 DML 都取写锁 |

→ 机制：`UPDATE`/`INSERT`/`DELETE` 在执行时即开写事务（`OP_Transaction` 带写标志），**与 rows_affected 无关**；
`SELECT` 与"已存在的 `CREATE TABLE IF NOT EXISTS`"不会。这条正是 `database.rs:124-131` 每次启动都抢 RESERVED 锁的原因。

**但要修正 res-ipc 的"第 0 档"：只改 `database.rs:126` 不够，失败点会往后挪几行。**

修好它之后，`open_migrated_pool` 剩下的部分确实是纯读的（重要正面结论）：
- `MIGRATOR.run()` 里的 `ensure_migrations_table` 是 `CREATE TABLE IF NOT EXISTS` → 已存在时**不取锁**（上表实测）；
- `dirty_version` / `list_applied_migrations` 都是 SELECT；
- 8 段自愈里 `normalize_legacy_part_ids`（无 `id` 列即 return）、`fold_*`（marker 命中即 return）、`converge_portable_paths`（`write_pending` 在 `pending.is_empty()` 时 return，`path_convergence.rs:155-157`）稳态都是零写。

**但 `open_migrated_pool` 返回之后，`bootstrap.rs:46` 立刻进入 `seed_defaults_with_catalog`，那里还有 ≥4 处无条件写：**

| 位置 | 行为 | 是否读守卫 |
|---|---|---|
| `store/system/tenant_repo.rs:14-35` `ensure_local_identity_sqlx` | 4 条 UPSERT（principal / tenant / membership / tenant_state）在一个 DEFERRED 事务里 | ❌ **无守卫，每次启动必写** |
| `application/system/defaults.rs:111-115` `ensure_system_skill_source_sqlx` | 直接 `upsert_source_sqlx` | ❌ 无守卫 |
| `application/system/defaults.rs:117-122` `normalize_existing_sources_sqlx` | 读出**全部** sources 再逐条 upsert（未变更也写） | ❌ 无差异判断 |
| `application/system/defaults.rs:132-146` `normalize_all_tenant_paths_sqlx` | 对每个非默认租户重复上面两步 | ❌ 无守卫 |
| `defaults.rs:95-109` `ensure_library_source_sqlx` | 先 `load_source_sqlx(...).is_some()` 再写 | ✅ 有守卫 |
| `store/system/menu_repo.rs:25+` / `shortcut_repo.rs:42+` | 先读现有集合再补缺 | ✅ 有守卫 |

→ **所以只改 `database.rs:126`，只读命令会在 `tenant_repo.rs:16` 的 `pool.begin()` + 第一条 UPSERT 上再死一次**（同样是 10 s 后 `code: 5`）。

**正确排序（替换我 D 节原第 1 档之前的位置）**

- **第 0 档（根因）**：只读命令**不复用** `bootstrap_runtime`。`aiwc overview/list` 这类命令应当走"只读启动"：
  用 `SqliteConnectOptions::new().filename(..).read_only(true)` 建池 + 跳过 `seed_defaults_with_catalog` / `migrate_legacy_*` / 适配器播种。
  这是唯一能保证"别人持写锁时只读命令仍 100% 可用"的方案；否则无论怎么优化写点，只要有 1 处写就可能被锁住。
- **第 0.5 档（低成本，收益大）**：把上述 5 处无条件写改成"状态签名不变则不写"：
  `database.rs:126` 改成先 `SELECT MIN(version) FROM _sqlx_migrations` 判定；`ensure_local_identity_sqlx` 包一层 `EXISTS`；
  `normalize_existing_*` 改成逐行比较后再写。目标是**稳态启动零写事务**。
- **明确反对**：调大 `busy_timeout`（res-ipc 的判断正确）——10 s 变 30 s 只是把失败推迟，`user` CPU 0.01 s 说明它根本没事可做，等更久纯属惩罚用户。

---

## B. 迁移体系结构

### B.1 文件盘点

| 指标 | 值 |
|---|---|
| 文件数 | **97** |
| 总字节 | **239,264 B ≈ 233.6 KiB**（平均 2,467 B/个） |
| 单文件最大 | `20261025000001_turn_triad_and_conversation_parts_slimming.sql` = **17,927 B** |
| 次大 | `20260627000003_tenant_scope_conversations.sql` 13,534 B / `20260723000001_memory_domain.sql` 12,395 B / `20260915000001_memory_v2_foundation.sql` 11,432 B / `20261031000001_unified_app_extensions_and_agent_market.sql` 10,565 B |
| 版本号格式 | **97/97 全是 14 位 `YYYYMMDDNNNNNN`**，无例外 |

最早 5 个：

```
20260618000001_initial_schema.sql
20260627000001_tenant_identity.sql
20260627000002_tenant_scope_catalog.sql
20260627000003_tenant_scope_conversations.sql
20260629000001_conversation_part_translations.sql
```

最新 5 个：

```
20261111000004_conversation_search_doc_row_tenant_index.sql
20261111000005_drop_conversation_fts_objects.sql
20261113000001_drop_task_journal.sql
20261116000001_model_pricing_lock_and_aliases.sql
20261116000002_purge_dirty_pricing_catalog_rows.sql
```

**命名演变证据**：`database.rs:124-131` 的重写公式是
`version/10000*1000000 + version%10000`，即把 `202606180001`（12 位）映射成 `20260618000001`（14 位）
—— 说明历史格式是 12 位 `YYYYMMDDNNNN`。旁证：`20261026000001_drop_turn_question_columns_and_redirects.sql:1`
的文件头注释仍写着 12 位的 `-- Migration: 202610260001_...`，文件名已是 14 位。

### B.2 迁移前自愈逻辑逐个评估

调用链在 `database.rs:75-91`（**全部在 `MIGRATOR.run` 之前**，除后两段在之后）：

| # | 函数 | 位置 | 修的历史坑 | 每次启动代价 | baseline 后可否废弃 |
|---|---|---|---|---|---|
| 1 | `is_untracked_legacy_database` + `upgrade_legacy_schema` | `94-118`、`300-371` | 极早期库有 `sources` 表但**没有 `_sqlx_migrations`**，靠 `ALTER TABLE ADD COLUMN` 补 14 个列（`sources.scanner_kind` 等） | 2 次 `sqlite_master` 查询；命中时 14×(1 次表存在 + 1 次 `PRAGMA table_info`) | ✅ 可废弃（新库必有账本表，条件直接短路） |
| 2 | `repair_legacy_migration_ledger_versions` | `120-133` | 12 位 → 14 位账本版本号 | 1 条 `UPDATE _sqlx_migrations WHERE version < 1e13`（97 行表，可忽略） | ✅ 可废弃（且采用 B.3 M1/M3 后账本里根本没有旧版本） |
| 3 | `repair_known_modified_conversation_question_contract_migration` | `135-177` | `20260825000005` 文件被事后改过，账本 checksum 对不上会 panic；先验证 6 个删除列确实不存在，再按硬编码 SHA384 改写 checksum（`179`） | 1 次账本 SELECT + 每张表 6 次 `PRAGMA table_info`（仅 checksum 命中时才做） | ✅ 可废弃（文件不再随包分发） |
| 4 | `repair_known_modified_cleanup_util6_orphans_migration` | `238-281` | `20261024000005_cleanup_util6_orphans_and_repair_card_kinds.sql` 首版误删正常 command 零件的 `has_command` 标记，**数据真的被伤了**：反向 `UPDATE conversation_parts ... WHERE json_extract(attrs_json,'$.has_command')=0 AND content IS NOT NULL`（`258-267`） | 命中时**全表扫 `conversation_parts`（160,465 行 / 220 MiB）**并 `json_remove` 改写；改动量取决于数据 | ⚠ **代码可删，但必须先保留一个版本的数据修复窗口**——这是唯一一段修"数据"而非"元数据"的自愈 |
| 5 | `repair_superseded_session_snapshot_migration` | `194-236` | `20261106000001` 首版把轮次位置映射写成内连接，会静默丢弃孤儿轮次；修正版改左连接后账本 checksum 不匹配 → 启动直接 panic（`183-186` 注释） | 1 次账本 SELECT；命中时校验 11 列结构 + 从 `MIGRATOR.iter()` 取当前 checksum 回写（`223-234`，**这是 B.3 直接可复用的范式**） | ✅ 可废弃 |
| 6 | `normalize_legacy_part_ids` | `373-432` | 旧 `conversation_parts` 有 `id` 列，格式不是 `conversation-part-<hash>` | 当前库**无 `id` 列** → `377` 行直接 return，代价仅 2 次 `PRAGMA table_info` | ✅ 可废弃（且已是死代码） |
| 7 | `repair_positional_turn_identities` | `conversation_turn_identity.rs:50-63` | 迁移脚本算不出 SHA-256，位置主键必须 Rust 侧重算并级联 parts/FTS/父引用 | **无条件全表读 turns**：`SELECT tenant_id, id, session_id, turn_index FROM {turns}`（`:58-63`）；命中才写 | ✅ 可废弃（新库 id 一出生就对） |
| 8 | `fold_legacy_subagent_pseudo_turns` | `conversation_subagent_fold.rs:98-150` | ADR-0022：历史 subagent/continuation 伪轮次折算 | ✅ 便宜：marker + 代际短路（`fold_already_done` `:166-184`），代价是 `SELECT COUNT(*), MAX(imported_at) FROM conversation_sessions`（2,109 行）+ 一张小表 | ✅ 可废弃 |
| 9 | `converge_portable_paths` | `path_convergence.rs:42-52` | ADR-0020：把历史主机绝对路径折成便携锚点（`~` / 系统目录占位），纯 SQL 迁移拿不到 home | ❌ **最贵**：6 张表各一次 `GLOB '/*'` 全表扫（`:66-67`、`:102-103`），其中 `conversation_parts.attrs_json` 是 220 MiB 表；**无 marker，每次启动都跑**。实测该查询热缓存 0.27 s，且当前仍命中 110 行 → 永远收敛不到 0 | ❌ **不能废弃**：这是"本机相关"的运行期不变量，不是历史坑。应改为 ① 写路径归一 + ② 一次性 marker（照抄 #8 的代际短路） |

> 顺序上的隐患：`#2/#3/#4/#5` 都在 `MIGRATOR.run` **之前**直接改 `_sqlx_migrations`。一旦 baseline 化，这 4 段就变成"给不存在的版本号打补丁"，必须同步删掉，否则会往新账本里写垃圾行。

### B.3 Baseline Snapshot 可行性

#### 事实基线（已核实，非记忆）

- `Cargo.lock:4987-4990` → `sqlx 0.9.0`（feature 见 `backend/asset_core/Cargo.toml:37`：`runtime-tokio, sqlite, migrate, macros`）。
- `database.rs:9`：`static MIGRATOR: Migrator = sqlx::migrate!("./migrations");` —— 编译期嵌入 SQL 文本 + 每文件 SHA-384 checksum。
- **sqlx 0.9.0 `Migrator` 可用 API**（docs.rs `sqlx::migrate::Migrator`）：
  - `set_ignore_missing(&mut self, bool) -> &mut Migrator` —— "Specify whether applied migrations that are missing from the resolved migrations should be ignored."
  - `skip(&self, conn, target: Option<i64>)` —— "Skipping entails not executing the SQL of the migrations, but marking them as applied in the [_migrations] table."
  - `run_to(&self, target: i64, conn)`、`run_direct`、`with_migrations(Vec<Migration>)`、`iter()`、`version_exists(i64)`、`dangerous_set_table_name`。
- **`ignore_missing` 的真实语义**（`sqlx-core v0.9.0 src/migrate/migrator.rs`，`validate_applied_migrations`）：

  ```rust
  fn validate_applied_migrations(applied: &[AppliedMigration], migrator: &Migrator) -> Result<(), MigrateError> {
      if migrator.ignore_missing { return Ok(()); }     // ← 直接短路，checksum 一个都不校验
      let migrations: HashSet<_> = migrator.iter().map(|m| m.version).collect();
      for applied_migration in applied {
          if !migrations.contains(&applied_migration.version) {
              return Err(MigrateError::VersionMissing(applied_migration.version));
          }
      }
      Ok(())
  }
  ```

  以及 `run_direct` 的主循环：已应用的迁移**必须 checksum 相等**否则 `VersionMismatch`；未应用的走 `skip` 或 `apply`。

- **`skip` 写到账本里的 checksum 是"源文件当前值"**（`sqlx-sqlite v0.9.0 src/migrate.rs`）：

  ```rust
  // Migrate for SqliteConnection::skip
  INSERT INTO {table_name} (version, description, success, checksum, execution_time)
  VALUES (?1, ?2, TRUE, ?3, -1)
  ```

  账本表 DDL（同文件 `ensure_migrations_table`）：`version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMP DEFAULT CURRENT_TIMESTAMP, success BOOLEAN NOT NULL, checksum BLOB NOT NULL, execution_time BIGINT NOT NULL`。

- 真实库现状（只读 SQL）：`_sqlx_migrations` **97 行、0 失败**，`min=20260618000001`、`max=20261116000002`。

#### 证据链接（可点开核对，非记忆）

| 断言 | 一手来源 |
|---|---|
| sqlx 版本 = **0.9.0** | `Cargo.lock:4987-4990`；`backend/asset_core/Cargo.toml:37` 声明 `sqlx = "0.9.0"`，features `runtime-tokio, sqlite, migrate, macros` |
| `set_ignore_missing` / `skip` / `run_to` / `with_migrations` / `iter` / `version_exists` 存在 | https://docs.rs/sqlx/0.9.0/sqlx/migrate/struct.Migrator.html |
| `validate_applied_migrations` 在 `ignore_missing` 时 early-return | https://github.com/launchbadge/sqlx/blob/v0.9.0/sqlx-core/src/migrate/migrator.rs |
| `skip` 写入的 SQL 与 `_sqlx_migrations` DDL；`apply` 把迁移 SQL + 账本 INSERT 包在同一事务 | https://github.com/launchbadge/sqlx/blob/v0.9.0/sqlx-sqlite/src/migrate.rs |
| `optimize_on_close` / `statement_cache_capacity` 默认 100 / `auto_vacuum` 需 VACUUM 才生效 | https://docs.rs/sqlx/0.9.0/sqlx/sqlite/struct.SqliteConnectOptions.html |
| `PRAGMA foreign_keys` 在事务内 no-op；`journal_mode` 事务内不可改；`cache_size` 默认 -2000；`journal_size_limit` 默认 -1；`optimize` 使用建议 | https://www.sqlite.org/pragma.html |

#### 三种可行机制对比

| | M1·baseline(v1) + 老库 `skip` 打点 + `ignore_missing` ★推荐 | M2·双轨：旧 97 个冻结，只给新库 baseline | M3·账本伪造：老库 `DELETE 97` + `INSERT baseline` |
|---|---|---|---|
| 新库 | `run()` → 执行 `0001_baseline.sql` → 账本 1 行 | 同上 | 同上 |
| 老库升级 | 启动时检测到账本存在且无 `version=1` → `MIGRATOR.skip(&pool, Some(1))`（写入**源文件当前 checksum**）→ 之后正常 `run()`，checksum 校验通过 | 老库继续跑 legacy migrator（97 个文件原样），永久不走 baseline | `DELETE FROM _sqlx_migrations` 后 `INSERT (1,'baseline',1,<checksum from MIGRATOR.iter()>,-1)` |
| 必须的开关 | `set_ignore_missing(true)`：否则老账本里 96 个"源中不存在"的版本触发 `VersionMissing(20260618000001)` | 无需开关，校验保持严格 | 不需要（账本里已无旧版本），校验保持严格 |
| 代价 | ① `ignore_missing=true` 会**连带关掉全部已应用迁移的 checksum 校验**（见上面的 early return）——正好是现在 3 个 `repair_*` 想解决的问题；建议**只在检测到老账本的那一次**用带 flag 的 Migrator，常规路径仍走严格版 ② `static MIGRATOR` 不可变，需 `let mut m = Migrator::with_migrations(MIGRATOR.iter().cloned().collect()); m.set_ignore_missing(true);`（`Migration: Clone`） | 二进制里多 233.6 KiB 文本 + 两套 DDL 维护成本；未来每个迁移要决定"只进新库还是两边都要"；version 空间必须隔离 | 丢掉 `installed_on` / `execution_time` / description 等审计信息；`version=1` 之后新迁移必须 >1；一旦 `run()` 因别的原因失败，账本已被清空，不可回滚 |
| 风险 | 中（可控，且能保留严格校验） | 低（老库零改动） | 中高（一次性破坏账本） |
| 可复用范式 | `database.rs:223-234` 已是"从 `MIGRATOR.iter()` 取当前 checksum 回写账本" | `database.rs:9` | 同左 + `sqlx-sqlite` `skip` 的 INSERT 语句形状 |

**不推荐**：`run_to(target)` —— 它只向上执行到某版本，无法"跳过第一个"。

#### 落地要点（凡涉及会踩坑的地方都点明）

1. baseline 文件名必须是 `0001_baseline.sql`：sqlx 按 `{version}_{description}.sql` 解析，`0001` → `1`，且 **1 必须小于现有最小版本 `20260618000001`**，否则新库迁移排序错乱。
2. baseline 内容建议从真实库反向生成再人工裁剪：本库 `sqlite_master` 有 **51 张表、106 个索引**（终态），但必须先剔除历史 `DROP` 残留（如 `20261111000005_drop_conversation_fts_objects.sql` 已删的 FTS 对象、`20261113000001_drop_task_journal.sql`）。生成后必须逐表 `PRAGMA table_info` 与老库比对**列顺序与 DEFAULT**（`ALTER TABLE ADD COLUMN` 的列一定排在末尾，手写 baseline 很容易写错顺序，而 `INSERT ... VALUES` 无列名时会错位）。
3. baseline 是**单个大事务**（sqlx `apply` 默认 `self.begin()` + `execute_migration` + 账本 INSERT 同一事务，除非文件头写 `-- no-transaction`）。DDL 在 SQLite 事务内合法，但**绝不能**在 baseline 里写 `PRAGMA journal_mode` / `PRAGMA foreign_keys`（前者 sqlite.org/pragma："journal_mode cannot be changed while a transaction is active"；后者"no-op within a transaction"）。
4. 老库检测条件别只看"账本存在"：要加 `max(version) >= 20261116000002`（最后一版），否则"账本存在但只跑到一半"的库会被误标 baseline 并永久跳过剩余迁移。

### B.4 真实库实测（只读）

| 项 | 值 |
|---|---|
| 库路径 | `$HOME/.assetiweave/app.db`（`mode=ro` + `sqlite3 -readonly`） |
| 文件大小 | 1,720,578,048 B ≈ **1.60 GiB**；`app.db-wal` 36,824,592 B |
| `page_size` | 4096 |
| `page_count` | **420,063**（≈1,640.9 MiB） |
| `freelist_count` | **325,384** → **77.5%** |
| 实际使用页 | 94,679 → **369.8 MiB** |
| 页内有效载荷 `payload` | **321.3 MiB**（页内利用率 **86.9%**） |
| 页内空隙 `unused` | **44.5 MiB**（其中 `conversation_parts` 单表 28.1 MiB） |
| `journal_mode` | `wal` |
| `integrity_check` | `ok` |
| `_sqlx_migrations` | **97 行**，0 失败，`min 20260618000001`，`max 20261116000002`（与目录 97 个文件一一对应） |
| `sqlite_stat1` | **不存在**（从未 `ANALYZE`） |

**VACUUM 可回收估算**：`325,384 × 4096 = 1,332,772,864 B ≈ 1.24 GiB`；回收后即 **−77%**。

回收后**精确尺寸**要分两层算（与 res-storage 独立互验，两人数字一致）：

- 下界 = 有效载荷 **321.3 MiB**（`sum(payload)`）；
- 实际 = 载荷 + 重打包后的页内空隙。当前 94,679 页里有 **44.5 MiB 页内碎片**（利用率 86.9%，`conversation_parts` 独占 28.1 MiB），VACUUM 按页重排后这部分会大幅压缩但不会归零（每页 4 KiB 边界必然留空）。
- 所以落地预期是 **≈350–388 MiB**（我此前写的"≈370 MiB"是取在用页数上界，偏乐观 5%；res-storage 按 payload/利用率反推得 ~388 MB，两者区间一致）。**结论方向不变（−77%），但别对外承诺 370 这个具体数。**

体量分布（`dbstat`，`aggregate=1`）：

| 对象 | 大小 | 备注 |
|---|---|---|
| `conversation_parts`（表） | **220 MiB** | 160,465 行；**平均 1,059 B 正文/行（字节口径），p50 仅 253 B**，只有 24.6% 的行正文 > 1 KiB |
| `conversation_turns` | 68 MiB | 7,336 行（`process`/`response` 很长） |
| `sqlite_autoindex_conversation_parts_1` | **18.34 MiB** | UNIQUE(tenant_id, turn_id, part_index)，占**全部索引的 45%** |
| `web_record_turns` / `web_record_parts` | 13 / 10 MiB | |
| `conversation_usage_events` | 10 MiB | 24,493 行 |
| 全部索引合计 | **40.8 MiB** | |
| 全部表合计 | **369.8 MiB** | |

**页类型拆分（`dbstat.pagetype`，与 res-storage 独立复测，逐行完全一致）**

| 范围 | `leaf` | `overflow` | `internal` |
|---|---|---|---|
| 全库 | 60,337 页 / **247.1 MB**（63.75%）· 页内未用 45.2 MB | 33,894 页 / **138.8 MB**（35.80%）· 页内未用仅 0.88 MB | 447 页 / **1.83 MB**（0.47%） |
| `conversation_parts` | 39,096 页 / **160.1 MB**（69.1%）· 页内未用 28.8 MB | 17,339 页 / **71.0 MB**（30.7%） | 109 页 / 0.45 MB（0.2%） |

**这组数纠正了我此前一句错话**：我原先写"`content` 溢出页占大头"——**不对**。溢出链只占该表 30.7%（71.0 MB），69.1%（160.1 MB）仍在 leaf 页。正确表述是"**leaf 页吃掉 69.1%、溢出链吃掉 30.7%，两者都是靶子**"。

由此得到三个可执行结论（与 res-storage 的 §A.7 交叉验证）：

1. **溢出链几乎不浪费**（全库页内未用仅 0.88 MB），浪费几乎全在 leaf（45.2 MB，其中 `parts` 28.8 MB）。
2. ⇒ 空间治理的**内部顺序是：先压缩把正文压到 ≤ ~980 B（1/4 页）以内以消灭溢出链，再用 `VACUUM` 收 leaf 碎片**，不能反过来。当前 **81.55% 的字节来自正文 >1 KiB 的行**（该数双向独立复测完全一致），所以压缩收益集中在这批行。
3. ⇒ **`page_size` 不进候选**：`internal` 全库只有 447 页 / 1.83 MB，任何"调大 page_size 减少 B 树层数"的收益天花板就是 **1.83 MB**，且 WAL 下改 page_size 必须整库重建。收益/风险不成比例。

> 行数占比的口径差异（记录以免误引）：我按 `length(cast(content as blob)) > 1024` 测得 **24.57%** 的行、占 **81.55%** 字节；res-storage 报行数占比 39.4%。**唯一双方独立复测完全一致的是 81.55% 这个字节占比**，对外请只用它；行数占比待口径统一后再引。

其他表行数参考：`conversation_sessions` 2,109 · `assets` 185 · `sources` 13 · `memory_recall_turns` **0**。

> **口径指针**：备份/压缩后的磁盘占用见 `02-storage.md §E.3`（三笔账分开报，压缩只动 1.60 GiB 那一档），本文所有数字仅限 SQLite 这一档。

> **VACUUM 注意**：1.6 GiB 库执行 `VACUUM` 需要**约等于回收后大小 + 回滚日志**的临时磁盘（≈400 MiB 量级），并全程持有写锁（启动期做会阻塞整个 app）。`auto_vacuum=INCREMENTAL` 也**必须先 `VACUUM` 一次**才生效（docs.rs `auto_vacuum`："For existing databases, a change to this value does not take effect unless a VACUUM command is executed"）。压缩本身建议用一次性独立连接或离线路径，不要复用 `infrastructure/backup` 的 `snapshot_sqlite_database`——那条路失败时会静默退化（见 B.5，低严重度但不可观测）。

---

### B.5 备份降级链与「孤儿备份」（含一条归因更正）

**一、先更正归因：那 7 个 `app.db.bak_*` 不是 App 产物**

res-storage 早前观察到 7 个 `app.db.bak_*` 与主库**字节数完全相同**（全部 1,720,578,048 B，合计 11.21 GiB），其中两个叫 `bak_compact_*`（2026-11-15 11:10 / 11:17）。经三方核对后**该归因撤回**：

- App 产物命名是 `assetiweave-app-{YYYYMMDD-HHMMSS-mmm}-{uuid8}.db`（`infrastructure/backup/mod.rs:204-208`），7 个文件全不符合；
- `~/.assetiweave` 下**没有任何** `assetiweave-app-*.db`；
- 仓内 `grep bak_compact|bak_data_clean|bak_pathfix|bak_cwdfix` 零命中。

→ 它们是**人工/修复脚本 `cp` 出来的裸副本**，不是 App 备份路径的产物。**App 的 `VACUUM INTO` 逻辑本身是正确的**（`mod.rs:179-189` 优先，`:191-200` 才是兜底）。

**二、真正值得写的结论：11.21 GiB「孤儿资产」不受任何生命周期管辖**

这 7 个文件（4.68 + 4.68 + … 合计 11.21 GiB）：
- App 不知道它们存在，备份轮转/清理逻辑也不覆盖它们；
- 压缩方案上线后它们**一个字节都不会变小**；
- 它们还和主库同尺寸，肉眼看不出哪份是"压缩产物"。

→ 需要独立的孤儿清理策略（建议：白名单命名模式 + 尺寸等于某一代主库 ⇒ 判为孤儿）。**这属于 res-storage 的域，此处只做标记，不计入 SQLite 引擎收益。**

**三、代码路径上仍有一个低严重度的可观测性缺口（不是 bug，别夸大）**

`snapshot_sqlite_database`（`mod.rs:154-177`）的降级链：

```rust
let snapshot_result = vacuum_into(db_path, &temp_path).or_else(|vacuum_error| {
    fs::remove_file(&temp_path).ok();
    checkpoint_and_copy(db_path, &temp_path).map_err(|copy_error| { ... })
});
```

- 失败兜底是 `checkpoint_and_copy`（`mod.rs:191-200`）= `PRAGMA wal_checkpoint(TRUNCATE)` + **`fs::copy`** → 产物与源库逐字节同级（含 77.5% 空洞）。
- **降级是静默的**：`or_else` 把 `vacuum_error` 丢弃，只要 copy 成功就返回 `Ok`。
- 唯一校验 `verify_sqlite_snapshot`（`mod.rs:128-141`）只跑 `PRAGMA quick_check`，一个 1.6 GiB 的胖副本照样返回 `ok`（quick_check 只验结构，不看空间）。
- **更正一条我此前写错的事实**：`vacuum_into` 并没有显式设 `busy_timeout`，但 **rusqlite 0.37.0 会在每次 open 时默认设 5000 ms**（`rusqlite-0.37.0/src/inner_connection.rs:119`：`sqlite3_busy_timeout(db, 5000)`）。所以不是"遇锁立即失败"，而是"**锁等待超过 5 s 才失败**"。对一个 1.6 GiB 库的 `VACUUM INTO`（分钟级），5 s 的窗口偏窄，但触发概率不高——这也是为什么我把本条降为低严重度。

**四、结论与修法（随手做，不必单列优先级）**

正确表述是：**"App 备份在正常路径下语义正确；只在 VACUUM INTO 失败时会静默退化成一个不可区分的胖副本。"** 修法 3 行 + 1 个断言：

1. `vacuum_into` / `checkpoint_and_copy` 显式 `.busy_timeout(Duration::from_secs(30))`（覆盖 5 s 默认）；
2. `or_else` 里加 `tracing::warn!(%vacuum_error, "VACUUM INTO failed, degrading to raw copy")`，并把"是否降级"回填到 `DatabaseBackupTarget`；
3. 对产物断言 `freelist_count ≈ 0`（或 `page_count` 未下降即判为未压缩），否则报错或标 `compacted: false`。

---

## C. 索引审查

### C.1 总览

- **106 个索引**：44 个命名（`CREATE INDEX`） + 62 个 sqlite 自建（PK / UNIQUE）。
- 51 张表；索引总占用 **40.8 MiB**（占已用空间的 11%）。
- 组合形态：`(tenant_id, ...)` 多租户前缀 + 业务列，是本仓的统一约定（`tenant_id` 恒为第一列，利于租户隔离与分区跳跃）。

### C.2 关键表的索引与约束全景

| 表 | PK / UNIQUE | 二级索引 |
|---|---|---|
| `conversation_parts` | U(tenant_id, turn_id, part_index) | `idx_conversation_parts_id_key(tenant_id, id_key)` |
| `conversation_turns` | PK(tenant_id, id)；U(tenant_id, session_id, turn_index) | `idx_conversation_turns_tenant_session(tenant_id, session_id)` ⚠前缀重复 |
| `web_record_turns` | PK(tenant_id, id)；U(tenant_id, session_id, turn_index) | `idx_web_record_turns_tenant_session(tenant_id, session_id)` ⚠前缀重复 |
| `web_record_parts` | U(tenant_id, turn_id, part_index) | `idx_web_record_parts_id_key(tenant_id, id_key)` |
| `conversation_sessions` | PK(tenant_id, id)；U(tenant_id, source_id, external_id) | `_tenant_adapter`、`_tenant_source(tenant_id, source_id, missing)`、`_visibility(tenant_id, user_visible, execution_origin)` |
| `conversation_usage_events` | PK(id AUTOINCREMENT)；U(tenant_id, source_id, external_event_id) | `_tenant_session_turn`、`_adapter_time`、`_model_time`、`_observation`、`_source_time`、`_tenant_time` |
| `model_pricing_catalog` | PK(id)；U(tenant_id, model_id) | `idx_pricing_catalog_lookup(tenant_id, model_id)` ⚠完全重复、`idx_pricing_catalog_provider(tenant_id, provider)` |
| `memory_recall_turns` | PK(tenant_id, id)；U(tenant_id, session_id, sequence)；U(tenant_id, session_id, conversation_turn_id) | `_active(tenant_id, status, updated_at)`、`_session(tenant_id, session_id, sequence)` ⚠完全重复 |
| `sources`（13 行） | PK(tenant_id, id) | `_tenant_order(tenant_id, priority, name)`、`_tenant_scanner(tenant_id, scanner_kind)` |
| `assets`（185 行） | PK(tenant_id, id) | `_tenant_kind(tenant_id, kind, name)`、`_tenant_source(tenant_id, source_id)` |

**覆盖索引**：本库没有刻意设计的覆盖索引；但对 `conversation_parts` 而言
`idx_conversation_parts_id_key(tenant_id, id_key)` 对
`outline.rs:138,150,160`（`WHERE tenant_id=?1 AND id_key>=?2 LIMIT 11`）与
`block_details.rs:135`（`WHERE p.tenant_id=?1 AND p.id_key=?2`）只回表取少量列，接近覆盖。

### C.3 可安全删除的候选索引

| 索引 | 表 | 定义 | 页数 / 占用 | 判定依据 | 结论 |
|---|---|---|---|---|---|
| `idx_pricing_catalog_lookup` | `model_pricing_catalog` | `(tenant_id, model_id)` | **87 p / 348 KiB** | 与 `sqlite_autoindex_model_pricing_catalog_1` 的 `UNIQUE(tenant_id, model_id)` **列完全相同、顺序相同** → 唯一索引必然已能服务 `WHERE tenant_id=? AND model_id=?`（`pricing_repo.rs:220`）。建表语句 `UNIQUE (tenant_id, model_id)` 与 `20261005000001_model_pricing_catalog.sql:24` 的 CREATE INDEX 并存 | ✅ **删除（零风险）** |
| `idx_conversation_turns_tenant_session` | `conversation_turns` | `(tenant_id, session_id)` | **212 p / 848 KiB** | 严格前缀于 `U(tenant_id, session_id, turn_index)`；所有该形态查询都还要 `ORDER BY turn_index`（`snapshot.rs:230`、`projection.rs:131,159`），唯一索引能同时给出顺序，命名索引反而强制额外排序 | ⚠ **逻辑冗余，但被 `INDEXED BY` 钉住**（`projection.rs:131,159`）+ 测试断言计划（`conversation_repo_tests.rs:141,3391`）。删需同步去掉 hint |
| `idx_web_record_turns_tenant_session` | `web_record_turns` | `(tenant_id, session_id)` | **76 p / 304 KiB** | 同上前缀重复；未发现 `INDEXED BY` 引用 | ✅ **删除（低风险）** |
| `idx_usage_events_observation` | `conversation_usage_events` | `(tenant_id, source_id, observation_run_id)` | **467 p / 1.8 MiB** | 全仓唯一谓词是 `usage_repo.rs:259`：`WHERE tenant_id=? AND source_id=? AND (observation_run_id IS NULL OR observation_run_id != ?)` —— `!=` / `IS NULL` 组合让第 3 列**无法用于索引定位**，只剩 `(tenant_id, source_id)` 前缀，而该前缀已被 `idx_usage_events_source_time(tenant_id, source_id, timestamp)` 覆盖 | ✅ **删除（低风险，省最大一块）** |
| `idx_conversation_usage_events_tenant_session_turn` | `conversation_usage_events` | `(tenant_id, session_id, turn_index)` | **376 p / 1.5 MiB** | 全仓**没有任何**生产 SQL 以 `session_id`/`turn_index` 过滤 `conversation_usage_events`（只有写：`usage_repo.rs:189` 的 `ON CONFLICT DO UPDATE`）。`conversation_subagent_fold.rs:25` 明确写了"不迁移用量事件" | ⚠ **疑似无用**，需确认是否给未来轮次级成本归因预留 |
| `idx_memory_recall_turns_session` 与 `U(tenant_id, session_id, sequence)` | `memory_recall_turns` | 同列同序 | 0 行（表空） | 完全重复 | ✅ 删除（表空，纯语义清理） |
| `idx_sources_tenant_scanner` | `sources` | `(tenant_id, scanner_kind)` | 数页 | 表只有 **13 行**，任何索引都退化为全表扫；`scanner_kind` 只有 3 个取值 | 🟡 极低收益，随 baseline 一并清理 |

**合计可直接回收 ≈ 1,218 页 ≈ 4.9 MiB**。相比 1.24 GiB 的 freelist 不值一提，但价值在**写放大**：`conversation_turns`/`web_record_turns`/`conversation_usage_events` 是快照整包替换与用量 upsert 的必经表，每条写入少维护 1 个 B 树。

**明确不在候选里的**（列了防止误删）：

- `idx_conversation_parts_id_key` — 被 `outline.rs:138/150/160/204/216/226`、`block_details.rs:135`、`projection.rs:16` 高频使用，且支持 `id_key >= ?` 区间扫（BLOB 前缀定位），价值极高。
- `idx_usage_events_{tenant,adapter,source,model}_time` — 用量报表 `usage_query.rs:186-215` 是动态拼接：`WHERE tenant_id=? [+timestamp>=?] [+timestamp<?] [+adapter_id] [+source_id] [+model IN (...)] ORDER BY timestamp`，四张 `(tenant_id, 过滤列, timestamp)` 各对应一个过滤分支，**都不能删**。
- `idx_conversation_sessions_tenant_source(tenant_id, source_id, missing)` — 与 `U(tenant_id, source_id, external_id)` 共享前两列但第 3 列不同（`missing` 是高频对账谓词），不是前缀重复。
- `idx_team_tasks_dispatch` — 部分唯一索引 `WHERE dispatch_key IS NOT NULL`，与 `team_task_claims` 上的唯一索引**不是同一张表**，不重复。

**比"删索引"大 3 倍的一块：`conversation_parts` 的唯一索引列宽**

`sqlite_autoindex_conversation_parts_1`（`UNIQUE(tenant_id, turn_id, part_index)`）= **18.34 MiB，占全部索引的 45%**。
表只有 160,465 行，却在 4 KiB 页上把 `tenant_id TEXT` + `turn_id TEXT(UUID/哈希)` 各存了两遍（表内 + 索引内）。
实测键长：`turn_id` **恒为 82 字符**（`conversation-turn-<64 hex>`，min=max=82）、`tenant_id` 7 字符 ⇒ 每条索引项 ≈ 82+7+2+rowid ≈ 95–110 B，`160,465 × ~110 B ≈ 17.6 MB`，与实测 18.34 MiB 吻合。
**不能删**（它是唯一性约束，也是快照替换 `DELETE ... WHERE turn_id IN (...)` 的唯一可用索引），
但如果把 `turn_id` 换成整数代理键（键长降到 ~23 B），索引可落到 ~4 MB，单项省 **≈14–15 MiB** ——
是我上面 5 个可删索引合计（4.9 MiB）的 3 倍，且同时降低 `conversation_parts` 的每次写入成本。
代价是 DDL 重构 + 全表重建（正好可以和 baseline 一起做）。列为 P2，不进 Wave 2 短期范围。

---

## D. Wave 2 建议（按"性价比 / 风险"排序）

> **先定优先级，避免误读**：SQLite 引擎侧的天花板是 **1.24 GiB**（freelist）+ **388 MiB**（页内碎片可压）。
> 而 res-storage 实测 `~/.assetiweave` 共 48.3 GiB，其中 `conversation-search-index` **24.69 GiB**
> （51%，内含 22.68 GiB 的 81 个 `generation-*.tmp` 废弃代数）—— 那是本项收益的 **18 倍**。
> 所以：**搜索引擎垃圾代数是 P0；SQLite 参数/空间治理是 P1/P2，不是主线。**
> 下面 5 档只在 SQLite 域内排序。

### 第 0 档（最高性价比）：让只读命令不被写锁连坐

两条一起做，无迁移、可回滚（详见 A.6）：

1. **根因**：只读 CLI 不走 `bootstrap_runtime` 的写链（`bootstrap.rs:44-52`），改用 `read_only(true)` 池并跳过 `seed_defaults_with_catalog`。否则只要链上剩 1 处写，就仍会 10 s 后 `code: 5`。
2. **稳态零写**：`database.rs:126` 的零命中 `UPDATE` 先加 `SELECT MIN(version)` 判定；`tenant_repo.rs:14-35`、`defaults.rs:111,117,132` 共 4 处无条件 upsert 加存在性/差异判断。实测证明 `UPDATE`/`INSERT OR IGNORE` 零命中也抢写锁，而 `SELECT` 与已存在的 `CREATE TABLE IF NOT EXISTS` 不抢。

> 不要靠调大 `busy_timeout` 解决——那只是把 10 s 失败变成 30 s 失败。

### 第 1 档：纯参数，改动 8 行、无迁移、可回滚

```rust
// database.rs:62-73
.pragma("cache_size", "-16000")          // 16 MiB/连接
.pragma("mmap_size", "268435456")        // 256 MiB
.pragma("temp_store", "MEMORY")
.pragma("journal_size_limit", "67108864")// 64 MiB
.pragma("wal_autocheckpoint", "2000")    // 显式化，配合排查 checkpoint 饥饿
.optimize_on_close(true, None)           // 等价 PRAGMA optimize，sqlx 0.9 原生
.statement_cache_capacity(256)
```

风险：`temp_store=MEMORY` 会让大 `ORDER BY` 吃内存（当前最大排序 24k 行，可忽略）；`mmap_size` 在 32 位构建上要降级，需 `cfg` 保护。**建议先灰度：只开 `optimize_on_close` + `journal_size_limit` + `statement_cache_capacity`，观察一周。**

### 第 2 档：把 22 处 🔴 DEFERRED 换成 `begin_immediate_with_retry`

单点改动，收益是消掉 `SQLITE_BUSY_SNAPSHOT (517)` 这一类偶发失败（`database.rs:37-41` 已写明该机制）。优先 `import.rs:108`。

### 第 3 档：空间治理（SQLite 域内收益最大，但需停机窗口）

内部顺序要写死（依据 B.4 的页类型拆分）：

1. **先消灭溢出链**：把 `conversation_parts.content` 压到 ≤ ~980 B（1/4 页）以内。81.55% 的字节集中在正文 >1 KiB 的行，压下去后 33,894 个 overflow 页（138.8 MB）整条消失——这是最"干净"的一块，不依赖 VACUUM。
2. **再 `VACUUM`**（回收 ≈1.24 GiB freelist，落地后 ≈350–388 MiB）→ 收回 leaf 页的 45.2 MB 页内碎片。必须无写入时执行，用独立一次性连接或离线路径。
3. **然后**才切 `auto_vacuum=INCREMENTAL` + 定期 `PRAGMA incremental_vacuum(N)`。顺序不能反：`auto_vacuum=0` 时 `incremental_vacuum` 是空操作，而改 `auto_vacuum` 本身也要一次 `VACUUM` 才生效。
4. 顺带 `PRAGMA optimize` 建 `sqlite_stat1`（当前查询计划完全没有统计信息）。
5. **不要碰 `page_size`**：`internal` 只有 447 页 / 1.83 MB，天花板 1.83 MB，且 WAL 下要整库重建。

### 第 4 档：Migration Baseline（工程量最大）

按 **B.3 M1** 落地：`0001_baseline.sql` + 老库 `skip(Some(1))` + 单次 `ignore_missing`；同步删除 B.2 的 #1–#7 元数据类自愈（保留 #4 的数据修复一个版本，#8 保留 marker 版，#9 改造为 marker 版）。

### 第 5 档：索引清理

按 C.3 删 4–5 个（先删零风险的 `idx_pricing_catalog_lookup` + `idx_usage_events_observation` + `idx_web_record_turns_tenant_session`），`idx_conversation_turns_tenant_session` 需连带改 `projection.rs` 与 2 处测试断言。

### 附带项（低严重度，随手做）：备份降级链加可观测性

`infrastructure/backup/mod.rs` 三处：显式 `busy_timeout(30s)`、`or_else` 加 `tracing::warn!`、对产物断言 `freelist_count ≈ 0`。
**不是 bug**（正常路径正确），只是"VACUUM INTO 失败 → 静默产出不可区分的胖副本"这个观测缺口。详见 B.5。

---

## 附录：复现命令（全部只读）

```bash
# 账本与页面统计
sqlite3 -readonly "file:$HOME/.assetiweave/app.db?mode=ro" \
  "select count(*), min(version), max(version) from _sqlx_migrations;
   select page_size, page_count, freelist_count from pragma_page_count(), pragma_page_size(), pragma_freelist_count();"

# 索引清单（表 | 索引 | 唯一 | 列）
sqlite3 -readonly -json "file:$HOME/.assetiweave/app.db?mode=ro" \
  "select m.name ix, m.tbl_name tbl,
     (select il.\"unique\" from pragma_index_list(m.tbl_name) il where il.name=m.name) uniq,
     (select group_concat(ii.name,',') from pragma_index_info(m.name) ii) cols
   from sqlite_master m where m.type='index' order by m.tbl_name;"

# 索引/表占用（需 SQLITE_ENABLE_DBSTAT_VTAB，本机 sqlite3 3.54.0 已具备）
sqlite3 -readonly "file:$HOME/.assetiweave/app.db?mode=ro" \
  "select name, count(*) pages, sum(pgsize)/1024 kb from dbstat group by name order by pages desc;"

# 页内碎片与有效载荷（VACUUM 后尺寸的上下界）
sqlite3 -readonly "file:$HOME/.assetiweave/app.db?mode=ro" \
  "select round(sum(payload)/1048576.0,1) payload_mib,
          round(sum(unused)/1048576.0,1)  unused_mib,
          round(100.0*sum(payload)/sum(pgsize),1) util_pct
     from dbstat;"
```
