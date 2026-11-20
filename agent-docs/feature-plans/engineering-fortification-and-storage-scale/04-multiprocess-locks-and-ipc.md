# Wave 2 · 本机并发冲突、IPC 遗产与「共享写队列」可行性

> 范围：只读代码考古 + 官方一手资料 + 一次隔离库上的可复现实测。
> 协作边界：SQLite 参数/迁移/索引见 `03-sqlite.md`，Tantivy 体积与 `.tmp` 泄漏见 `02-storage.md`，路径便携化见 `01-paths.md`。本文不重复推导，只做交叉引用。
> 实测环境：macOS 26 (`Darwin 27.2.0` / arm64)，真实库 `~/.assetiweave/app.db` = 1.6 GB，隔离副本上跑 `target/debug/aiwc`（debug 构建，未 strip）。

---

## 0. 结论速览

1. **`database is locked` 的第一根因不是「两个写者抢 WAL」，而是「每个进程启动都无条件发若干条 DML」。** 第一条是 `repair_legacy_migration_ledger_versions` 里的 `UPDATE _sqlx_migrations SET version = ...`（`backend/asset_core/src/store/database.rs:126`；`open_migrated_pool` 是 GUI 与 CLI 的唯一入口，`database.rs:61`，被 `application/system/bootstrap.rs:44` 调用）；紧接着 `seed_defaults_with_catalog`（`bootstrap.rs:46`）里还有 4 个必写点：`store/system/tenant_repo.rs` 的 `ensure_local_identity_sqlx`（事务 `16-33`，4 条 UPSERT）、`application/system/defaults.rs` 的 `ensure_system_skill_source_sqlx`(:111) / `normalize_existing_sources_sqlx`(:117) / `normalize_existing_profiles_sqlx`(:124)，外加 1 个只扇出不写自己的 `normalize_all_tenant_paths_sqlx`(:132)。**只修 126 只会把失败顺延到下一条。** 而且**并发写者至少有三类**：GUI 主进程、`aiwc`、以及 GUI 在跑 recall agent 时用 `current_exe()` 自己再拉起的 `--memory-recall-mcp-stdio` 子进程（`agent_engine/src/infrastructure/backends/acp.rs:382-407`）。实测：另一进程持有写锁时，`aiwc` 在 **10.57 s / 10.75 s** 后抛 `(code: 5) database is locked` 并退出，`user` CPU 仅 0.01 s——它几乎什么都没干就死在了第一条写语句上。
2. **这些 DML 在稳态下全是空操作，但只要执行就要写锁**（实测 `UPDATE ... WHERE` 命中 0 行、`INSERT OR IGNORE` 零命中照样 `database is locked`；而 `CREATE TABLE IF NOT EXISTS` 与 `SELECT` 0.00 s 通过）。 因此 **100% 的 `aiwc` 命令（包括纯只读命令）在 GUI 持锁期间都会启动失败**，而不是只有写命令会失败。这决定了「只把写操作转发出去」并不能修掉 CLI 的故障。
3. **Tantivy 的单写者问题在本仓库已经被降解为「单写者 + DB 逻辑租约」**，且 CLI 根本不写索引（事件消费者只在 `ResidentHost` 启动，`desktop/src/lib.rs:228`）。真正的抢锁面只剩「GUI + 手工跑 `assetiweave-engine` 二进制」。但租约有一个真实缺陷：**10 分钟 TTL 且不续租**（`application/conversations/conversation_search.rs:70`、`:218`），超过 10 分钟的整代重建会被第二方抢走。
4. **ADR-0004 是「零调用的活契约」**：`assetiweave-engine` 二进制仍在构建与打包（`scripts/bundle-cli.js:39`、`desktop/tauri.conf.json:31`），252 条命令的契约仍在生成（`cli/internal/schema/contract.json`，protocol=1 / contract=3），但 `aiwc` 已改 in-process（`backend/cli/src/service/mod.rs:23`），**没有任何代码路径拉起它**。它的 stdio 实现还是「一次性读完 stdin → 一条响应」（`desktop/src/adapters/engine/transport.rs:91-95`），不是长连服务。
5. **不建议现在建 UDS 写队列。** 先做三件成本不高的事：①把启动链改成「稳态零写」（5 处无条件写加守卫，或把只读命令切到独立 `read_only` bootstrap）；②修 `.tmp` 代际泄漏（25 GB 里约 24 GB 是垃圾，见 `02-storage.md` §B）；③给 bootstrap 链加分级重试。这三件能吃掉绝大多数冲突。UDS 的**真正**收益不是「避免锁冲突」，而是「CLI 冷启动从 0.72–0.84 s（热缓存）/ 4.65–5.84 s（冷缓存）降到 ~10–55 ms」——这是一个 15–100× 的**提速**，不是降级。

---

## A. 冲突面测绘

### A.1 谁在独立打开 SQLite

全仓库只有**一个**建池函数 `open_migrated_pool`（`backend/asset_core/src/store/database.rs:61`），所有进程都收敛到它。

| # | 进程 / 入口 | 角色 | 建库调用链（文件:行号） |
|---|---|---|---|
| 1 | `AssetIWeave.app` 主进程 | `RuntimeRole::ResidentHost` | `desktop/src/lib.rs:186` → `bootstrap.rs:44` |
| 2 | `AssetIWeave.app --startup-self-check` | `OneShot` | `desktop/src/lib.rs:99` |
| 3 | `assetiweave-engine`（stdio 二进制） | `OneShot` | `desktop/src/lib.rs:607`（入口）→ `:690`（建池） |
| 4 | `AssetIWeave.app --memory-recall-mcp-stdio`（**GUI 把自己当子进程再拉起**） | `OneShot` | 拉起方：`backend/agent_engine/src/infrastructure/backends/acp.rs:382-407`（`std::env::current_exe()` + `ASSETIWEAVE_DB_PATH` 注入）；建池：`desktop/src/lib.rs:628` |
| 5 | `aiwc`（Rust CLI，唯一在用的 CLI） | `OneShot` | `backend/cli/src/service/mod.rs:23` → `backend/asset_core/src/application/system/system.rs:38-49` |
| 6 | 测试 / 集成测试 | `OneShot` | `backend/asset_core/tests/*.rs`、`backend/cli/tests/cli_tests.rs:1227` 等 |

要点：

- #1 与 #5 是**生产常态下的并发对**。二者之间没有任何单实例保护——`tauri-plugin-single-instance` 未引入（`desktop/Cargo.toml` 无该依赖），所以**同时开两个 GUI 也会互撞**。
- #2 到 #4 都在 `RuntimeRole::OneShot` 下建自己的池（`desktop/src/lib.rs:690`、`:628`，入口分别是 `:607` 与 `:674`），都会与 GUI 争同一把写锁。
- **#4 是最容易被忽略、却最频繁的一个**：它不是"手动跑二进制"，而是 **GUI 在正常 AI 使用中被自动拉起的第三个进程**。`recall_mcp_servers` 用 `std::env::current_exe()` 把自己再 exec 一遍，并显式注入 `ASSETIWEAVE_DB_PATH`（`agent_engine/src/infrastructure/backends/acp.rs:388-392`、`:397-400`）。⇒ 每跑一次 recall agent，就多一个进程完整 bootstrap 同一个 1.6 GB 库（并按 §A.5 实测要拿一次写锁）。这条路径把"第三个写者"从理论可能性变成了**每次 AI 回忆都会发生的事实**，而且它与 GUI 自身的 `SessionMemoryConsumer`（`desktop/src/lib.rs:213`）在时间上高度重叠。
- #5 的 `CliService::bootstrap()` 对失败**不做任何重试**，只把错误包装成 `context("failed to bootstrap AppService in OneShot CLI mode")`（`backend/cli/src/service/mod.rs:38`）。这就是「CLI 只能靠 shell 循环重试」的代码出处。

### A.2 谁在打开 Tantivy（单写者语义）

Tantivy 版本：`0.26.1`（`Cargo.toml` 依赖声明 `tantivy = "0.26.1"`，`Cargo.lock` 解析同版本），中文分词 `tantivy-jieba 0.20.0`。

| 操作 | 位置 | 是否抢写锁 |
|---|---|---|
| `Index::open_in_dir` | `backend/asset_core/src/infrastructure/search/conversation/engine.rs:219` | 否（只读 meta） |
| `Index::create_in_dir` | `engine.rs:193` | 否（仅建目录） |
| `index.writer(50_000_000)` 整代替换 | `engine.rs:259`（`replace_documents_with_checkpoint`） | **是** |
| `index.writer(50_000_000)` 增量推进 | `engine.rs:319`（`apply_session_documents`） | **是** |
| `index.reader()` | `engine.rs:231` / `:349` / `:401` | 否 |
| 调用入口 | `infrastructure/search/conversation/storage.rs:161`（读）、`:174`（写）、`:184`（读） | — |

`IndexWriter` 的独占语义（读 tantivy 0.26.1 源码确认，非记忆）：

- `Index::writer_with_options` 调 `directory.acquire_lock(&INDEX_WRITER_LOCK)`，失败即 `TantivyError::LockFailure(...)`，提示语明确写着 *"there is already an `IndexWriter` working on this `Directory`, in this process or in a different process"*（`tantivy-0.26.1/src/index/index.rs:536-556`）。
- `INDEX_WRITER_LOCK` 定义为 `{ filepath: ".tantivy-writer.lock", is_blocking: false }`（`tantivy-0.26.1/src/directory/directory_lock.rs:45-49`），`is_blocking: false` ⇒ `retry_policy` 为 `no_retry`（`directory.rs:89-96`）。
- 实现方式是 **`O_CREAT|O_EXCL` 语义的独占建文件**：`try_acquire_lock` 用 `directory.open_write(filepath)`，遇到 `FileAlreadyExists` 直接返回 `LockBusy`（`directory.rs:74-87`、`:191-212`）。**第二个进程会立刻失败，不会等待。**
- 锁文件**在 Drop 时删除**：`DirectoryLockGuard::drop → directory.delete(&path)`（`directory.rs:56-62`）。
  ⇒ **进程被 SIGKILL / abort / panic 后锁文件残留，此后所有写者永久失败**，官方注释也承认这点并说「确认无进程后再手工删」（`directory_lock.rs:38-44`、`index.rs:523-526`）。

**当前代码没有任何抢占失败处理分支**：`engine.rs:259` 与 `:319` 都是 `index.writer(50_000_000).map_err(InfraError::external)?` —— 无重试、无 stale-lock 识别、无「降级为只读」分支。这是最干净的盲区。

已被一层「应用级串行化」挡住的部分：

- 所有索引写都必须先拿 DB 内的写入租约 `try_acquire_conversation_search_writer_lease_sqlx`（`application/conversations/conversation_search.rs:71-83`、`:219-231`；实现 `store/conversations/search_index_repo.rs:160-188`，条件 `lease_owner IS NULL OR lease_expires_at <= now`）。
- 拿不到时**直接返回 `AppError::Conflict`**，不排队、不重试（`conversation_search.rs:79-83`、`:227-231`）。
- 缺陷：租约 `expires_at = now + Duration::minutes(10)`（`conversation_search.rs:70`、`:218`），**中途不续租**。25 GB 级别的整代重建（当前真实规模见 §A.5）很容易超过 10 分钟 ⇒ 第二方会合法抢走租约，两边同时 `index.writer()` ⇒ 后来者拿到 `LockFailure`（被 `.map_err(InfraError::external)` 吞成通用外部错误），或先者 `commit` 后被 `source_revision` CAS 挡住（`conversation_search.rs:100-116`）。

### A.3 已有的锁冲突处理，与它们的覆盖盲区

| 手段 | 位置 | 覆盖范围 |
|---|---|---|
| `busy_timeout = 10 s` | `store/database.rs:68` | 所有经该池的语句 |
| `journal_mode = WAL` + `synchronous = NORMAL` | `database.rs:66-67` | 读不阻塞写、写不阻塞读（`https://www.sqlite.org/wal.html` §2.2） |
| `max_connections(5)` | `database.rs:70` | 进程内并发上限 |
| `BEGIN IMMEDIATE` 显式立即写事务 | `database.rs:42-52`（注释解释 `SQLITE_BUSY_SNAPSHOT (517)`） | 11 处业务写路径 |
| `begin_immediate_with_retry(pool, 8)` | `store/conversations/conversation_repo/common.rs:342-358` | 11 个调用点 |
| `is_retryable_db_lock_err` 字符串匹配 | `common.rs:360-370`（`database is locked` / `code: 517` / `code: 5` / `busy` / `timed out`） | 同上 |
| 索引写入租约 | `search_index_repo.rs:160` | 跨进程索引写串行化（不排队） |

盲区（按危险度排序）：

1. **startup 链完全不参与重试**。`bootstrap.rs:44` 起整条链（`open_migrated_pool` → `seed_defaults_with_catalog` → `seed_discovered_conversation_adapters` → `migrate_legacy_adapter_hashes`）全部用 `?` 直抛。这是唯一一个「失败即进程退出、用户看不到任何降级」的路径。
2. **`bootstrap` 内部有 5 个「必写点」+ 1 个扇出器**，它们让「只读命令」也必须写锁。**`database.rs:126` 只是第一个凶手**——只修它，失败会往后挪几行：
   
   **必写点（5 个，全部无条件）**
   1. `store/database.rs:126` `UPDATE _sqlx_migrations`（条件 `version < 1e13`，稳态命中 0 行）
   2. `store/system/tenant_repo.rs:14` `ensure_local_identity_sqlx`（事务 `16-33`）：**4 条 UPSERT 包在一个 `pool.begin()` 里，无任何读守卫**，且是 `bootstrap.rs:46 → defaults.rs:26` 的第一件事
   3. `application/system/defaults.rs:111` `ensure_system_skill_source_sqlx`：直接 `upsert_source_sqlx`，无守卫
   4. `application/system/defaults.rs:117` `normalize_existing_sources_sqlx`：读出全部 sources 再逐条 upsert，**无差异判断**
   5. `application/system/defaults.rs:124` `normalize_existing_profiles_sqlx`：同上，profiles 版
   
   **扇出器（1 个，自身不写）**：`application/system/defaults.rs:132` `normalize_all_tenant_paths_sqlx` —— 它自己不执行任何 DML，但会对**每个非默认租户**重跑 #4/#5（`sources` + `profiles` 两条），多租户下把写入量成倍放大。计数口径统一为「**5 个必写点 + 1 个扇出器 = 6 个函数**」，与 `03-sqlite.md` §A.6 一致。
   
   有守卫、无需改的对照：`defaults.rs:95` `ensure_library_source_sqlx` 先 `load_source_sqlx(...).is_some()` 才写。
   实测依据（res-sqlite 在临时库 WAL + 持写锁 + `busy_timeout=800ms` 下逐条验证，见 `03-sqlite.md` §A.6）：`UPDATE`/`INSERT`/`DELETE` **只要执行就开写事务，与 `rows_affected` 是否为 0 无关**；`INSERT OR IGNORE` 零命中同样 `database is locked`；而 `CREATE TABLE IF NOT EXISTS <已存在>` 与纯 `SELECT` 均 0.00 s 通过。
3. **重试只在 `conversation_repo` 子树里**。`memory_recall_repo/turns.rs:241`、`web_record_repo/import.rs:34/44/81`、`observations.rs:281`、`path_convergence.rs:159` 都是裸 `begin_immediate` / `pool.begin()`，没有重试（另有约 22 处 `DEFERRED` 事务，见 `03-sqlite.md` §A.4）。
4. **重试语义是「重开事务」，不是「重放事务」**——只对 `BEGIN` 阶段的冲突有效；事务中途遇到 `SQLITE_BUSY` 仍然直接失败。
5. **`busy_timeout` 是「安静地等 10 秒再死」**，对交互式 CLI 而言 10 秒等待 ≈ 卡死；对 GUI 后台任务而言 10 秒又太短。一个全局常数无法同时满足两种调用方。
6. **Tantivy 侧零处理**（见 §A.2）。

### A.4 长写事务 / 长锁持有清单

| 路径 | 位置 | 事务粒度 | 是否在 Tauri 同步命令路径 |
|---|---|---|---|
| 启动期无条件 `UPDATE _sqlx_migrations` | `store/database.rs:126` | 单语句自动提交 | 每次 bootstrap 必经（GUI 与 CLI 都是） |
| `ensure_local_identity_sqlx` 4×UPSERT | `store/system/tenant_repo.rs:14`（事务 `16-33`） | 单事务 4 条写 | bootstrap 同步路径（`defaults.rs:26` 第一件事） |
| `ensure_system_skill_source_sqlx` / `normalize_existing_sources_sqlx` / `normalize_existing_profiles_sqlx` / `normalize_all_tenant_paths_sqlx` | `application/system/defaults.rs:111/117/124/132` | 逐条 upsert（默认租户 + 每个非默认租户） | bootstrap 同步路径 |
| `repair_positional_turn_identities` | `store/conversation_turn_identity.rs:29-115` | 400 轮 / 批（`:31`、`:99`、`:107`） | bootstrap 同步路径 |
| `normalize_legacy_part_ids` | `store/database.rs:373-420+` | 全表扫 `conversation_parts`（实测 16 万行）后单事务逐行 UPDATE | bootstrap 同步路径 |
| `converge_portable_paths` | `store/path_convergence.rs:42-70`、`:150-175` | 8 个列 GLOB 扫描 + 单事务 `write_pending` | bootstrap 同步路径 |
| `fold_legacy_subagent_pseudo_turns` | `store/conversation_subagent_fold.rs:99-160` | 按会话独立事务；有 marker 短路 | bootstrap 同步路径（稳态 O(1)） |
| 会话快照整包替换 | `store/conversations/snapshot.rs:431-468` | **8 会话 / 事务**（`common.rs:3` 的 `CONVERSATION_IMPORT_BATCH_SIZE = 8`，`import.rs:107`） | 否（后台任务，`desktop/src/adapters/tauri/commands/conversations/sync.rs:12`） |
| 高级导入（batch-first + 逐条降级） | `conversation_repo/import_advanced.rs:139-141` | 25 会话 / 批 | 否（后台任务） |
| 用量事件写入 | `store/conversations/usage_repo.rs:75-120` | 单事务批量 upsert | 部分（翻译/统计命令同步触发） |
| **Tantivy 整代重建** | `application/conversations/conversation_search.rs:87-129` + `storage.rs:124-138` | `spawn_blocking`，持 DB 租约 10 分钟，全量 2.5 万文档 | 否（后台任务，`tauri/commands/conversations/search.rs:72-130`） |
| **`VACUUM INTO` 备份** | `infrastructure/backup/mod.rs:179-190` | 单条语句，**长读快照**（不拿写锁但阻塞 checkpoint，1.6 GB ⇒ 数十秒）；耗时随库体积线性增长，与其能回收多少空间是两件事 | App 关闭路径 |

结论：**Tauri 命令的同步路径上没有长写事务**（已按 ADR-0017 全面后台化），这部分的架构是健康的。真正的痛点在 **bootstrap 这条「同步、无重试、且必须写」的启动路径**。

### A.5 实测：冲突矩阵

隔离副本（`/tmp`，`ASSETIWEAVE_HOME` 指向副本，不触碰真实库），1.6 GB 库，debug 构建：

| 场景 | 命令 | 实测结果 |
|---|---|---|
| 无竞争，页缓存热 | `aiwc overview` | **0.72 / 0.73 / 0.74 / 0.84 s** |
| 无竞争，页缓存冷 | `aiwc overview` | **4.78 / 5.84 s** |
| 另一进程持 `BEGIN IMMEDIATE`（写锁） | `aiwc overview` | **10.57 s / 10.75 s 后 `Error: failed to bootstrap AppService in OneShot CLI mode: error returned from database: (code: 5) database is locked`**，`user 0.01 s` |
| 另一进程持 `BEGIN` + `SELECT`（读快照） | `aiwc overview` | **成功**（4.65 s 冷缓存 / 0.72 s 热） |
| 不 bootstrap 的基线 | `aiwc --version` | **0.01–0.05 s** |

配套的 SQL 级探针（同一把写锁被占用时逐条执行）：

| 语句 | 结果 |
|---|---|
| `PRAGMA journal_mode=WAL` | ✅ `wal` |
| `PRAGMA foreign_keys=ON` | ✅ |
| `CREATE TABLE IF NOT EXISTS _sqlx_migrations (...)` | ✅ |
| `SELECT count(*) FROM sqlite_master` | ✅ `157` |
| `INSERT ... ON CONFLICT DO NOTHING` | ❌ `database is locked` |

> 两个失败时延的标定：**0.88 s** 是 res-sqlite 在 `busy_timeout=800 ms` 的临时库上精确定位单条语句的观测点；**10.57 s** 是本报告在默认 `busy_timeout=10 s` 的生产配置下观测整条 bootstrap 的失败时延。两者是**同一机制的两个观测点**，差值恰为 busy_timeout 的差（0.88−0.8≈0.08 s 为进程内开销；10.57−10≈0.57 s 为 bootstrap 前置只读阶段 + 启动开销）。完整探针脚本见文末「附录 B」。

**冲突矩阵（谁挡谁）**：

| 持有方 → / 请求方 ↓ | 无竞争 | 别人持读快照 | 别人持写事务 | 别人持 Tantivy writer |
|---|---|---|---|---|
| CLI 只读命令（`overview`/`list`） | 0.7 s ✅ | ✅（WAL 保证） | ❌ 10 s 后死 | ✅ |
| CLI 只读命令（`skill list` 等同样 bootstrap） | 同上 | 同上 | **同上 ❌** | ✅ |
| CLI 写命令（`conversation sync`/`memory commit`） | ✅ | ✅ | ⚠️ 11 处有重试，其余直死 | ✅ |
| GUI 后台同步 | ✅ | ✅ | ❌（`begin_immediate_with_retry(8)` 约 8×0.6 s 后仍可能死） | ✅ |
| GUI 索引重建 | ✅ | ✅ | ❌ | ❌ `LockFailure`（无处理） |
| GUI 关闭时 `VACUUM INTO` | ✅（读） | ✅ | ✅ | ✅ |
| 手工 `assetiweave-engine` / recall MCP 子进程 | ✅ | ✅ | ❌ | ❌ |

> 注意最反直觉的一格：**「CLI 只读命令」在别人持写事务时同样会死**，因为它要执行 `database.rs:126` 的 UPDATE。这就是为什么「读走 WAL 直连、写走队列」的方案不能修掉用户感知到的故障。

---

## B. 现存的 IPC 遗产（ADR-0004 考古）

### B.1 ADR-0004 原文主张 vs 当前事实

`agent-docs/adr/0004-engine-stdio-protocol-and-appservice-authority.md` 四条决策，逐条核对：

| ADR-0004 条款 | 原文 | 当前事实 | 判定 |
|---|---|---|---|
| ① 唯一业务权威 `AppService` | `ADR:18` | ✅ 仍然成立。GUI / CLI / engine / MCP 全部经 `AppService`（`bootstrap.rs:44`、`cli/src/service/mod.rs:23`） | **活契约** |
| ② Go CLI 通过 Stdio JSON 与 `assetiweave-engine` 通信 | `ADR:19` | ❌ Go CLI 留在 `cli/`（115 个 `.go` 文件）但**只用于归档测试**（`package.json: "cli:test:go:archive"`）；真实发布的 `aiwc` 是 Rust（`scripts/build-cli.js:21` `cargo build -p cli --bin aiwc`） | **历史** |
| ③ 契约 CodeGen 到 `cli/internal/schema/contract.json` | `ADR:20` | ✅ 文件仍在生成（2026-11-13，`protocol_version=1`、`contract_version=3`、252 条命令），生产脚本 `pnpm cli:contract` 仍在 | **活契约（但是"无人消费的契约"）** |
| ④ 禁止 CLI 直连数据库 | `ADR:21` | ❌ **已被推翻**。`aiwc` 现在就是直连：`CliService::bootstrap()` → `AppService::open_for_engine()` → `bootstrap_runtime(..., RuntimeRole::OneShot)`（`cli/src/service/mod.rs:23`、`system.rs:38-49`），自己建池。CLI 版本静态声明里也直接写了 `"in_process_runtime"`（`cli/src/service/mod.rs:51`） | **历史（且与现状冲突）** |

**结论**：ADR-0004 需要一次明确的演进（见 §E），因为它现在同时是「唯一权威」的活基石和「禁止 CLI 直连」的失效教条。任何 Wave 2 的 IPC 方案都在事实上挑战第 ④ 条。

### B.2 stdio engine 协议：活着，但零调用

- **二进制在**：`desktop/src/bin/assetiweave-engine.rs` 编译为 `assetiweave` crate 的第二个 bin。
- **被打包**：`scripts/bundle-cli.js:39` 构建并把 `target/release/assetiweave-engine` 复制进 `desktop/bundled-cli/cli/`；`desktop/tauri.conf.json:31` 的 `resources: ["bundled-cli/cli"]` 整体打包；`desktop/src/adapters/cli_tools.rs:28,89` 还会给它装 PATH shim。
- **协议在**：`desktop/src/adapters/engine/protocol.rs:8-9`（`PROTOCOL_VERSION = 1`、`CONTRACT_VERSION = 3`），`transport.rs` 完整实现 JSON 请求/响应 + 策略鉴权 + 参数校验。
- **但没有任何调用方**：`grep` 全仓，除 `desktop/src/lib.rs:607`（`run_engine_stdio` 自身入口）与测试外，无 `Command::new("assetiweave-engine")`；`aiwc` 的 `schema` / `api` 子命令也已改为 in-process（`cli/src/commands/system.rs:91-113` → `service.system().schema_get(...)` / `api_call(...)`，而 `api_call` 只硬编码代理 7 个方法，`cli/src/service/system.rs:103-165`）。

**它是什么形态的协议**：`run_stdio` 用 `io::stdin().read_to_string(&mut input)` 一次性读完整个 stdin，解析**一个**请求对象，写**一个**响应后退出（`transport.rs:91-95`、`:139`、`:148-154`；文件头注释还写着 "逐行读取"，与实现不符）。也就是说它**不是**长连守护进程，而是「一次调用 = 一次进程启动 = 一次 bootstrap」。这与 §A.5 的实测吻合：每调用一次就付一次 0.7–5 s 的 bootstrap 成本 + 一次写锁风险。

### B.3 能否复用 `desktop/adapters/engine/` 作为 UDS 传输层

可以复用的部分（占比约 70%）：

| 层 | 位置 | 复用判断 |
|---|---|---|
| 请求/响应 DTO | `transport.rs:17-89`（`WireEngineRequest` / `EngineResponse` / `EngineError`） | ✅ 直接复用 |
| 方法注册表 + 参数校验 | `registry/mod.rs:19-42`、`registry/dispatch.rs:15-120` | ✅ 直接复用（252 条命令已登记） |
| 风险分级 + 确认 | `registry/types.rs:17-21`、`dispatch.rs:11-13` | ✅ 直接复用（134 条写命令已被标 `Write`/`HighRiskWrite`） |
| 调用钩子/计时 | `engine/runtime.rs` | ✅ 直接复用 |
| 版本握手 | `transport.rs:173-198` | ✅ 改成连接级握手 |

必须重写的部分（关键）：

1. **消息边界**：现在是「读完整 stdin = 一条消息」，`Stream` 上没有帧。UDS 上必须改成 **NDJSON 行分帧**（或 `u32` 长度前缀），否则无法长连复用。
2. **进程生命周期**：现在进程 = 一次请求（`lib.rs:607-672` 里 `run_engine_stdio` 处理完一个请求就退出）。要长连必须把 `bootstrap_runtime` 提到监听循环之前，并常驻。
3. **单实例语义**：`install_process_runtime` 是 `OnceLock::set`，重复安装返回 `Conflict`（`infrastructure/runtime/app_runtime.rs:472-476`）。长连服务里没问题，但要保证 `desktop/src/lib.rs:186` 的 ResidentHost 和 UDS 监听**不重复安装**——二者只能有一个进程装。
4. **错误→退出码的契约**：`run_engine_stdio` 里任何 init 失败直接 `std::process::exit(1)`（`lib.rs:607-672` 内有 5 处 `std::process::exit(1)`）。改成常驻后必须换成「启动失败→端点不可达」的可观测信号，否则客户端无法区分「服务没起」和「服务崩了」。

---

## C. 本机 IPC 方案调研（一手资料）

### C.1 `interprocess` crate

- **当前版本**：`2.4.9`，2026-11-18 发布（`https://crates.io/api/v1/crates/interprocess`，`max_version=2.4.9`，`updated_at=2026-11-18T22:48:09Z`），累计下载 18,355,300 次。**活跃维护**。
- **平台支持（官方原文分级）**（`https://docs.rs/interprocess/latest/interprocess/`）：
  - *Explicit support*：**Windows、Linux、macOS** —— "guaranteed to compile and succeed in running all tests"，GitHub Actions 三平台 CI。
  - *Explicit support with incomplete CI*：FreeBSD、Android；*without CI*：OpenBSD、NetBSD；*by association*：iOS/tvOS/watchOS 等；*assumed*：其余 `#[cfg(unix)]`。
- **local_socket 抽象**：`Listener` / `Stream` / `RecvHalf` / `SendHalf` 是 enum-dispatch 类型（`https://docs.rs/interprocess/2.4.8/interprocess/local_socket/index.html`），**"there is only one underlying local socket implementation per platform, with Windows only using named pipe based local sockets and Unix only using Unix-domain socket based local sockets"**。
- **关键承诺（对复用 stdio 协议极其有利）**：*"Interprocess never inserts its own message framing or any other type of metadata into the stream – the bytes you write are the exact bytes that come out the other end."* ⇒ 我们可以在上面直接铺 NDJSON，与 `transport.rs` 的 DTO 完全兼容。
- **命名**：`GenericFilePath` 在 Unix 上是 UDS 路径原样，在 Windows 上**只接受 `\\.\pipe\` 前缀**，其它路径（含 `\\?\`）直接报错（`https://docs.rs/interprocess/2.4.9/interprocess/local_socket/enum.GenericFilePath.html`）。⇒ 跨平台命名要么用 `GenericNamespaced`（Unix 抽象命名空间 / Windows 管道命名空间），要么按平台分叉。
- **异步**：`local_socket::tokio` 模块，feature `tokio`（默认关闭）。

**已知坑**：
1. Windows 下每个管道实例只能连一个客户端，必须"accept 后立刻建下一个实例"，否则客户端会收到 `ErrorKind::NotFound`（tokio 官方文档原文：*"we must ensure that at least one server instance is available at all times"*，`https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.NamedPipeServer.html`）。interprocess 在 Windows 上封装了这层，但并发连接数受 `max_instances` 限制。
2. macOS/Linux 路径型 UDS 受 `sun_path` 长度限制（Linux 108 / macOS 104 字节含结尾 NUL）。`~/.assetiweave/...` 通常安全，但**用户自定义 `ASSETIWEAVE_HOME` 且路径很长时会 bind 失败**——必须用 `GenericNamespaced` 或把 socket 放进短路径（如 `$TMPDIR`）。
3. 崩溃残留 socket 文件：Unix 上 `bind` 到已存在路径会 `EADDRINUSE`，需要「先尝试 connect，连不上再 unlink」的探活逻辑（这是所有 UDS 方案的共同坑，不是 interprocess 特有）。

### C.2 `tokio::net::UnixListener` + `tokio::net::windows::named_pipe`

- Unix 侧：`tokio::net::UnixListener` 在 `net` feature 下可用，成熟稳定，零额外依赖（tokio 已在依赖里：`desktop/Cargo.toml`、`backend/asset_core/Cargo.toml` 均已启用 `net` 之外的 feature 集，需补 `net`）。
- Windows 侧：`tokio::net::windows::named_pipe` 仅 **Windows + feature `net`**（`https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/index.html`），需要手写「accept 后立即 `ServerOptions::new().create(name)` 建下一个实例」的循环（见 C.1 坑 1）。
- **代价**：两份 `#[cfg]` 分叉代码（约 150–250 行），以及两套测试矩阵。相比 `interprocess` 统一接口，多出的工程量主要在 Windows 分支和 CI。

### C.3 Tauri 2 是否自带可用通道

`tauri::ipc::Channel` 的官方定位是 *"An IPC channel, used to stream values from Rust to the frontend"*（`https://docs.rs/tauri/2/tauri/ipc/struct.Channel.html`），走的是 webview 的 `invoke` 通道，生命周期绑定在一个 `#[tauri::command]` 调用上。**它解决的是「Rust 后端 → 前端 JS」的流式回传，与「进程 A ↔ 进程 B」无关**，也无法被 `aiwc` 使用（CLI 里没有 webview）。⇒ **不可用**。

### C.4 「只加 SQLite 参数」能不能解决 80%？

| 参数 | 现状 | 加大后的效果 | 判断 |
|---|---|---|---|
| `busy_timeout` 10 s → 30/60 s | `database.rs:68` | 只把「10 s 后死」变成「30 s 后死」。对 CLI 是**体验更差**（卡更久），对后台任务是边际改善 | 单独加长 ❌ |
| WAL 读并发 | 已开（`database.rs:66`） | 读-读、读-写已经不冲突（`sqlite.org/wal.html` §2.2） | 已用尽 ✅ |
| `synchronous` | 已 `NORMAL`（`:67`） | 再降 `OFF` 有丢数据风险，收益是写延迟 | 不建议 |
| `wal_autocheckpoint` / `journal_size_limit` | 未设 | 可减少长读快照下的 WAL 膨胀（当前 `-wal` 实测 36 MB） | 小收益 |
| **把 5 处无条件写改成条件写**（`database.rs:126` + `tenant_repo.rs` `ensure_local_identity_sqlx` + `defaults.rs` `ensure_system_skill_source_sqlx`/`normalize_existing_sources_sqlx`/`normalize_existing_profiles_sqlx`/`normalize_all_tenant_paths_sqlx`） | 全部无条件 | 修完后整条启动链可做到**稳态零写事务**，只读命令在别人持写锁时能正常启动（`MIGRATOR.run` 在库已迁移完时不抢锁：`ensure_migrations_table` 是 `CREATE TABLE IF NOT EXISTS`，SQLite 对已存在表短路，实测 0.00 s） | ✅ **最高性价比**，但**只改 126 不够**，会顺延到下一条（5 个必写点 + 1 个扇出器，口径见 §A.3） |
| **只读命令不复用 `bootstrap_runtime`**（`read_only(true)` 池 + 跳过 `seed_defaults_with_catalog`） | 结构性 | **唯一能兜住「还剩任何一处写」的方案**；也是 CLI 转发方案的天然第一步 | ✅ **结构性**，但需处理 WAL 恢复边界（见下） |
| `busy_timeout` 分级（CLI 2–3 s 快速失败 / GUI 30 s） | 单一常数 | 让 CLI 从"卡 10 s 再死"变成"卡 2 s 后走降级" | ✅ 配合降级契约必需 |

**结论**：单纯拉长 `busy_timeout` **解决不了**用户感知的问题（实测就是 10 s 后硬失败）；但「一条条件写 + 分级 timeout」能吃掉**绝大多数**真实场景，因为读路径本来就不冲突、写路径本来就有重试。真正剩下的 ~20%（长导入 vs 重建 vs CLI 写命令撞在同一个写事务窗口）才需要队列。

### C.5 方案对照表

| 方案 | 跨平台 | 改造量 | 端点生命周期归属 | GUI 未启动时 | 主要风险 |
|---|---|---|---|---|---|
| **A0. 启动链拆「只读 bootstrap / 读写 bootstrap」+ 5 处无条件写改条件写 + 分级 busy_timeout** | ✅ 天然 | **中偏小：5 处守卫 ~60–80 行 + 只读 bootstrap 分支 ~40 行** | 无需端点 | 不变（OneShot） | ①`read_only(true)` 在需要 WAL 恢复时会拿到 `SQLITE_READONLY_RECOVERY`，必须能回退一次读写打开；②`-shm` 不存在且目录不可写时读不开（`sqlite.org/wal.html` §1 第 4 条） |
| **A1. A0 + 22 处裸事务收敛到 `begin_immediate_with_retry`** | ✅ | ~150 行（机械替换） | 无需端点 | 不变 | 长事务仍可能超时 |
| **B. `interprocess` 2.4.9 + NDJSON，写队列（读仍直连）** | ✅（Win/Linux/macOS 官方担保） | 中：传输 ~300 行 + 131 条写命令路由 + 帧协议 + 端点发现 | GUI 进程（ResidentHost）监听；socket 路径 = `~/.assetiweave/run/<hash>.sock`（Win: `\\.\pipe\assetiweave-<hash>`） | **必须降级 OneShot**（否则 CLI 无端点可用） | ①GUI 未启动时契约必须零退化；②GUI 退出瞬间的断连竞态；③stale socket 清理；④**修不掉只读命令的启动写锁**（除非同时改 A0） |
| **C. `interprocess` + 读写全转发（CLI 变纯客户端）** | ✅ | 大：B + CLI 去掉 bootstrap + 全部 252 条读命令路由 + 流式结果 + 认证 | GUI 进程独占建池与建索引 | OneShot 全量回退（等价今天的代码路径） | ①GUI 变成 CLI 的单点故障；②版本不一致（CLI 0.8 vs GUI 0.7 契约）；③权限（谁都能连 → 需要 `PeerCreds` 校验）；④CI/无人值守场景必须保证纯 OneShot 可用 |
| **D. `tokio::net::UnixListener` + 手写 Windows named_pipe** | ✅ | 大：B + 两份 cfg 分叉 + Windows accept 循环 | 同 B | 同 B | 同 B + Windows 分支自研 bug 面 |
| **E. tauri `ipc::Channel`** | — | — | — | — | **不适用**（前端↔后端，非进程间） |

补充：`interprocess` 的 `PeerCreds` 可以用来做本机同用户校验（`local_socket/struct.PeerCreds.html`），这对「任何本地进程都能写你的数据库」是必要的一道门——纯文件权限（0700 目录）也可以，但显式校验更稳。

---

## D. 收益与代价判断

### D.1 只把「写操作」串行化转发，能消掉多少锁冲突？

必须先把冲突分成两类：

- **类 I：长事务互撞**（GUI 后台导入/重建 vs CLI 写命令）。串行化写队列**能消掉**这一类。当前实测量级：8 会话/事务（`common.rs:3`）、25 会话/批（`import_advanced.rs:139`），单事务持锁时间取决于会话大小，最容易撞的是「整代重建（持租约 10 分钟）+ 任意写」。
- **类 II：启动期写锁**（CLI bootstrap 的 `database.rs:126`）。串行化**消不掉**——因为 CLI 在拿到连接后第一时间就要写，而队列只转发「业务写」，不会转发「启动期维护写」。

因此：**只串行化写 ⇒ 类 I 基本消失，类 II 完全保留**。而用户实际抱怨的 `database is locked` 从 §A.5 的实测看，主要表现就是类 II（CLI 一启动就死，10 s 后报错）。**所以「只转发写」不足以让 Wave 2 的目标达成**，必须同时把启动链变成「稳态零写」——注意是 **5 处**（`database.rs:126`、`tenant_repo.rs` `ensure_local_identity_sqlx`、`defaults.rs` `ensure_system_skill_source_sqlx`/`normalize_existing_sources_sqlx`/`normalize_existing_profiles_sqlx`/`normalize_all_tenant_paths_sqlx`），不是 1 处；或者把只读命令切到独立的 `read_only` bootstrap。

Tantivy 单写者是否仍需单独解决？——**需要，但不是一个新问题**：

- 现状已用 DB 租约把「GUI 内部并发」串行化（`conversation_search.rs:71`、`:219`）。
- 剩余暴露面 = 手工 `assetiweave-engine` + 多个 GUI 实例。这两者都不是写队列能覆盖的（它们不走队列）。
- 且租约 10 分钟不续租是**真实缺陷**：一次冷缓存整代重建（2.5 万文档 → 实测活跃代 288 MB）在慢机上完全可能超过 10 分钟。修法是租约续租（重建循环里每 N 个会话 renew 一次）或改成 `lease_expires_at = NULL` + 心跳。
- 结论：**Tantivy 需要「续租 + stale-lock 自愈」两件小事，不需要进程级 IPC 队列**。

### D.2 读写全部转发：CLI 冷启动会变成多少？

先纠正一个前提：**「当前 in-process 直连是微秒级」只对「单次函数调用」成立，对「CLI 一次调用」不成立。** 因为每次 `aiwc` 都要跑完整 bootstrap。实测（§A.5）：

| 阶段 | 实测 |
|---|---|
| 纯进程启动 + clap（`aiwc --version`，不 bootstrap） | **0.01–0.05 s** |
| bootstrap 中的只读扫描（sqlite3 只读探针） | turns 全表 37 ms + `parts.attrs_json` GLOB 256 ms + `turns.process` GLOB 165 ms |
| **完整一次 `aiwc overview`（页缓存热）** | **0.72–0.84 s** |
| **完整一次 `aiwc overview`（页缓存冷）** | **4.65–5.84 s** |

转发方案下，CLI 冷启动 = `exec` + `connect` + 1 RTT + 渲染：

- `exec` + 动态链接：**10–50 ms**（实测基线）
- `connect`（UDS 本地）：**< 1 ms**
- 1 次 RTT（JSON 请求/响应，载荷 ~KB 级）：**< 1 ms**
- **合计 ≈ 10–55 ms，且与库大小、页缓存状态无关**

⇒ 相比今天：热缓存 **↓ 约 15–80×**，冷缓存 **↓ 约 100×**。**转发是提速，不是拖慢。** 原因很直接：转发把「每次都要跑的 0.7–5 s bootstrap」替换成「0 ms 的 bootstrap」，代价只是一次本地往返。

**代价（真实存在，必须量化）**：

1. **GUI 是单点**。GUI 正在做长写事务（导入 / 整代重建）时，转发的读请求要么排队（CLI 从 50 ms 变成数百 ms 甚至数秒），要么必须设 deadline 后降级 OneShot（而这又要付 0.7–5 s）。所以每个请求必须带**明确的 deadline**，且 deadline 到期要能"优雅地失败并降级"，而不是无限等。
2. **CLI 二进制可以瘦身**（不再需要 `asset_core` 的 sqlite/tantivy 依赖），但**不能**从包里去掉 OneShot 路径——否则无 GUI 场景失效。真正能省的是启动时的动态库加载与 page cache 压力。
3. **版本不匹配风险**：GUI 0.7 与 CLI 0.8 的 `contract_version` 不一致时，`transport.rs:187-196` 会返回 `engine_incompatible`，CLI 必须能识别并降级到 OneShot，而不是报错退出。

### D.3 无 GUI 场景的零退化：降级契约草案（可验收）

> 目标：**同一个 `aiwc` 二进制，在有/无 GUI、在端点可用/不可用/半死状态下，行为可预期、结果正确、延迟有上界。**

| # | 契约条款 | 可验收条件（可自动化） |
|---|---|---|
| **C1 端点探测不超时预算** | 端点探测（尝试 `connect`）必须在 **≤ 200 ms** 内得出「可用/不可用」结论，且**不重试超过 1 次** | 注入一个"只 listen 不 accept"的假端点，`aiwc` 总耗时 ≤ 启动基线 + 200 ms + 一次命令本身；不得出现 10 s 级别等待 |
| **C2 端点不可达 ⇒ 当次命令走 OneShot，退出码与直连完全一致** | 无 GUI 时 `aiwc <cmd>` 的 stdout / stderr / exit code 与「抹掉 IPC 代码的构建」逐字节相同（黄金样本对比，覆盖读/写/错误三类命令） | CI 里跑双份二进制 diff：`git stash` 前的 tag 与当前，同一 DB 副本，100 条命令输出哈希一致 |
| **C3 端点半死（连上但不响应）⇒ 有界失败 + 有界降级** | 单次 RPC 的 deadline 默认 **2 s**（可 `ASSETIWEAVE_IPC_TIMEOUT_MS` 覆盖）；超时后**必须**回退 OneShot，或明确以 `ipc_timeout` 错误码退出（二选一，由 `--no-fallback` 决定），**绝不静默重试超过 1 次** | 注入"accept 后 sleep 10 s"的假端点：`aiwc` 在 ≤ 2 s + 单次 OneShot 时间内完成；日志里恰好 1 条 `ipc_timeout` + 1 条 `fallback_oneshot` |
| **C4 写命令的语义不因路径而变** | 无论走队列还是 OneShot，写命令都在**同一个** `AppService` 方法里执行（同一校验、同一事件、同一幂等键）；队列只是传输层，不得引入第二条业务路径 | 写命令的 SQL 影响行数、outbox 事件条数、`conversation_search_index_state.source_revision` 增量，两条路径必须逐项相等 |
| **C5 纯 CLI / CI 环境零 IPC 依赖** | 当 `ASSETIWEAVE_IPC=0`、或 socket 目录不存在、或 `CI=1` 时，**完全不产生任何 IPC 系统调用**（无 `connect`、无 socket 文件创建） | `dtrace`/`strace` 断言：以 `ASSETIWEAVE_IPC=0` 跑 100 条命令，`connect(` 调用数为 0；且不创建 `<home>/run/` 目录 |
| **C6 端点生命周期自愈** | GUI 崩溃留下的 stale socket，下一次 CLI 启动必须**能自愈**（探测 → 判定 stale → 降级），不得报 `EADDRINUSE` 或卡死；GUI 下次启动必须能重新 bind | 手工 `kill -9` GUI 后立即跑 `aiwc`：成功（走 OneShot），且 stderr 无 panic；随后重启 GUI：`aiwc` 自动回到队列路径（日志有 `ipc_attached`） |

建议把这 6 条写成 `docs/` 下的一份独立 `IPC_DEGRADATION_CONTRACT.md`，并在 `backend/cli/tests/` 里落成 6 个集成测试。**C1/C3/C6 是硬门槛**，任何一条不满足就不该上 UDS。

**与第 0 档（只读 bootstrap）的交叉约束**：若采用「只读命令走 `read_only(true)` 池」，需要额外满足 **R1–R2**，否则会把「锁冲突」换成「读不开」：

| # | 约束 | 说明与依据 |
|---|---|---|
| **R1 WAL 恢复回退** | 只读打开遇到 `SQLITE_READONLY_RECOVERY`（上次进程崩溃、WAL 待恢复）时，必须回退为**一次**读写打开完成恢复，然后继续以只读运行 | SQLite 文档明确：只读连接无法执行恢复；`https://www.sqlite.org/wal.html` §1 第 4 条（只读 WAL 库要求 `-shm`/`-wal` 已存在或可创建） |
| **R2 `-shm` 不可创建时降级** | 若 `-shm` 不存在且目录不可写（只读介质 / 容器只读挂载），应回退到 `immutable=1` 或直接报可读错误，不得静默返回空结果 | 同上；`immutable=1` 会假定无写入者，**仅在确认无 GUI 时可用** |


---

## E. 对 ADR-0004 的演进建议

建议新增 `ADR-0024: 单机写权威与 CLI 端点降级契约`，把 ADR-0004 收敛为两块：

1. **保留（活契约）**：`AppService` 是唯一业务权威；engine 命令注册表 + `contract.json` 是唯一的能力清单与风险分级来源（252 条 / 121 读 / 131 写）。
2. **显式作废（历史）**：①「Go CLI + stdio engine 进程」这条实现路线；②「禁止 CLI 直连数据库」——改为「**CLI 允许直连，但只允许在 OneShot 角色下直连**；有可用本机权威端点时优先复用，不可用时无感降级」。
3. **新增不变式**：
   - **I-1 启动期不得无条件写库**：`bootstrap` 链中任何写语句必须由读判定守卫，读路径（`aiwc overview`/`list`/`search`）在纯 WAL 读快照下必须能完成。
   - **I-2 单一索引写者**：Tantivy `IndexWriter` 只能由持有 DB 写入租约且租约有效的一方打开；租约必须可续。
   - **I-3 端点不可达 ⇒ 零退化**：见 §D.3 的 C1–C6。

`assetiweave-engine` 二进制的去留需要一个决定，因为它是**唯一**一个「ADR 已废但仍在发布」的产物：要么把它降级为「UDS 服务端入口」（复用 §B.3 的 70% 代码，正好给 UDS 一个自然的归属），要么从 `bundle-cli.js` 与 `cli_tools.rs` 里摘掉。**现状（打包但不调用）是三种选项里最差的**：白占 25 MB 体积，还留着一个能撞写锁的隐藏入口。

---

## F. 需要主控复核的三个反直觉点

1. **「25 GB 索引」是假象**：DB 里 `conversation_search_index_state` 记录当前 `active_generation` 为 `generation-8e175746-...`，`size_bytes = 301,611,440`（288 MB）、`document_count = 25022`、`health = ready`、`source_revision = indexed_revision = 3658`。同目录下还有 **46 个 `generation-*.tmp`**，合计约 24 GB——`cleanup_old_generations` 显式跳过 `.tmp`（`infrastructure/search/conversation/storage.rs:85` 的 `!name.ends_with(".tmp")`），而 `materialize_index_generation` 只有在 `build_result` 返回 Err 时才清理（`storage.rs:149-153`），**崩溃/取消路径的 `.tmp` 永远不会被回收**。⇒ 一切基于「25 GB 重建很贵」的推理都要重算；真实重建规模是 288 MB / 2.5 万文档。（与 `02-storage.md` §B 互为印证。）
2. **CLI 失败点在 `user 0.01 s`**：10.57 s 里有 10.5 s 是 busy handler 在睡，几乎没有 CPU 工作 ⇒ 失败发生在**极早期**，不是「读到一半被卡」，也不是「大事务超时」。这直接否定「再加长 busy_timeout 就好」的方案。
3. **`CREATE TABLE IF NOT EXISTS` 与 `PRAGMA journal_mode=WAL` 可以在别人持写事务时成功**（实测），只有真正的 DML 才会 `SQLITE_BUSY`。所以「把启动期写降级为纯 schema 检查」在技术上是可行的，不存在"SQLite 连 schema 检查都要写锁"的阻碍。

---

## 附录 A：证据与复现（本报告实测）

**关键代码位置索引**

```
启动/建池
  backend/asset_core/src/application/system/bootstrap.rs:44      open_migrated_pool
  backend/asset_core/src/store/database.rs:61-92                  建池 + 修复链
  backend/asset_core/src/store/database.rs:126                    无条件 UPDATE（第一个根因）
  backend/asset_core/src/store/system/tenant_repo.rs:14            ensure_local_identity_sqlx（事务 16-33，4×UPSERT 无守卫）
  backend/asset_core/src/application/system/defaults.rs:111/117/124/132  4 处无守卫/无差异判断的 upsert
  backend/asset_core/src/store/database.rs:373-420                normalize_legacy_part_ids
  backend/asset_core/src/application/system/system.rs:38-49       open_for_engine（CLI 入口）
  desktop/src/lib.rs:186 / :99 / :690 / :628                      四个 GUI 侧建池点

锁
  backend/asset_core/src/store/database.rs:42-52                  begin_immediate
  backend/asset_core/src/store/conversations/conversation_repo/common.rs:342-370  retry + 分类
  backend/asset_core/src/store/conversations/search_index_repo.rs:160-188         索引写入租约

Tantivy
  backend/asset_core/src/infrastructure/search/conversation/engine.rs:219/259/319/231/349/401
  backend/asset_core/src/infrastructure/search/conversation/storage.rs:85/124-138/149-153
  ~/.cargo/.../tantivy-0.26.1/src/index/index.rs:536-556
  ~/.cargo/.../tantivy-0.26.1/src/directory/directory_lock.rs:45-49
  ~/.cargo/.../tantivy-0.26.1/src/directory/directory.rs:56-62/74-87/191-212

IPC 遗产
  agent-docs/adr/0004-engine-stdio-protocol-and-appservice-authority.md:18-21
  desktop/src/adapters/engine/transport.rs:91-95/148-154/173-198
  desktop/src/adapters/engine/protocol.rs:8-9
  desktop/src/adapters/engine/registry/mod.rs:19-42
  desktop/src/adapters/cli_tools.rs:28/89
  scripts/bundle-cli.js:39
  desktop/tauri.conf.json:31
```

**实测复现步骤（全部在隔离副本上，不触碰真实库）**

```bash
# 1) 复制真实库到隔离 home（含 WAL），删掉派生 -shm 让 SQLite 自行恢复
mkdir -p /tmp/aiwc-bench && cp ~/.assetiweave/app.db ~/.assetiweave/app.db-wal /tmp/aiwc-bench/
rm -f /tmp/aiwc-bench/app.db-shm

# 2) 基线：完整 bootstrap（热/冷缓存各跑 3 次）
ASSETIWEAVE_HOME=/tmp/aiwc-bench /usr/bin/time -p target/debug/aiwc overview

# 3) 不 bootstrap 的下界
/usr/bin/time -p target/debug/aiwc --version

# 4) 写锁竞争复现（另一进程持 BEGIN IMMEDIATE 16 s）
python3 - <<'PY' &
import sqlite3, time
c = sqlite3.connect('/tmp/aiwc-bench/app.db', isolation_level=None, timeout=1)
c.execute('BEGIN IMMEDIATE'); c.execute("UPDATE app_settings SET settings_json = settings_json")
time.sleep(16); c.execute('COMMIT')
PY
ASSETIWEAVE_HOME=/tmp/aiwc-bench /usr/bin/time -p target/debug/aiwc overview
# 期望：10.5~10.8 s 后 `(code: 5) database is locked`

# 5) 读快照竞争（应当成功，验证 WAL 读不互斥）
#    python3: BEGIN; SELECT count(*) FROM conversation_parts; sleep(25);
```

## 附录 B：语句级精确定位探针（res-sqlite 提供，`busy_timeout=800 ms`）

> 目的：把「整条 bootstrap 10.57 s 失败」拆成「哪一条语句失败、失败多快」。全部在临时库上跑，不触碰真实库。
>
> ⚠️ **脚本里的 `_sqlx_migrations` 建表语句是为跑通探针手写的精简版**（省掉了 `installed_on` 的 `DEFAULT CURRENT_TIMESTAMP`、`description` 的约束、`checksum` 的 BLOB 语义等），**只用于复现加锁行为，不可当真实账本 DDL 引用**。真实 DDL 由 sqlx 的 `ensure_migrations_table` 生成，字段与默认值语义见 `03-sqlite.md` §B.3；漏掉 `installed_on` 的默认值会误判账本的时间语义。

```python
# python3 /tmp/lock_probe.py
import sqlite3, time
path = "/tmp/pragma-probe/w.db"

c = sqlite3.connect(path, isolation_level=None)
c.execute("PRAGMA journal_mode=WAL")
c.execute("CREATE TABLE IF NOT EXISTS t(a INTEGER)"); c.execute("INSERT INTO t VALUES(1)")
c.execute('''CREATE TABLE IF NOT EXISTS _sqlx_migrations(
  version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMP,
  success BOOLEAN NOT NULL, checksum BLOB NOT NULL, execution_time BIGINT NOT NULL)''')
c.execute("INSERT INTO _sqlx_migrations VALUES(20260618000001,'x','t',1,X'00',-1)")
c.close()

holder = sqlite3.connect(path, isolation_level=None)
holder.execute("PRAGMA journal_mode=WAL")
holder.execute("BEGIN IMMEDIATE")          # 持 RESERVED 写锁，模拟"另一个进程在写"
holder.execute("INSERT INTO t VALUES(2)")

def probe(label, sql, ms=800):
    conn = sqlite3.connect(path, timeout=ms/1000, isolation_level=None)
    conn.execute(f"PRAGMA busy_timeout={ms}")
    t0 = time.time()
    try:
        conn.execute(sql); print(f"{label:46s} OK    {time.time()-t0:.2f}s")
    except Exception as e:
        print(f"{label:46s} FAIL  {time.time()-t0:.2f}s  {e}")
    finally:
        conn.close()

probe("SELECT", "SELECT count(*) FROM t")
probe("UPDATE 命中 0 行（database.rs:126 原语句）",
      "UPDATE _sqlx_migrations SET version=(version/10000)*1000000+(version%10000) "
      "WHERE version<10000000000000")
probe("UPDATE ... WHERE 0", "UPDATE _sqlx_migrations SET version=version WHERE 0")
probe("CREATE TABLE IF NOT EXISTS（已存在）",
      "CREATE TABLE IF NOT EXISTS _sqlx_migrations(version BIGINT PRIMARY KEY)")
probe("INSERT OR IGNORE（主键已存在）", "INSERT OR IGNORE INTO t VALUES(1)")

holder.execute("ROLLBACK"); holder.close()
```

实测输出（SQLite 3.54.0，rollback-journal 与 WAL 两种模式结论一致）：

```
SELECT                                             OK    0.00s
UPDATE 命中 0 行（database.rs:126 原语句）           FAIL  0.88s  database is locked
UPDATE ... WHERE 0                                 FAIL  0.85s  database is locked
CREATE TABLE IF NOT EXISTS（已存在）                OK    0.00s
INSERT OR IGNORE（主键已存在）                      FAIL  0.85s  database is locked
```

**结论**：`UPDATE` / `INSERT` / `DELETE` 只要被**执行**就会开写事务（`OP_Transaction` 带写标志），**与 `rows_affected` 是否为 0 完全无关**；`CREATE TABLE IF NOT EXISTS` 对已存在的表走短路，不取写锁；纯 `SELECT` 不取写锁。这就是「稳态空操作仍然会 `database is locked`」的机制级解释，也是「把 5 处无条件 DML 加读守卫」能生效的前提。

**官方资料（一手）**

- SQLite WAL 并发模型：<https://www.sqlite.org/wal.html> §2.2（"there can only be one writer at a time"；readers/writers 互不阻塞；§9 列出 WAL 下仍会 `SQLITE_BUSY` 的三种情形：独占锁模式、最后连接清理、崩溃后恢复）
- SQLite `PRAGMA busy_timeout`：<https://www.sqlite.org/pragma.html#pragma_busy_timeout>
- `interprocess` 2.4.9 文档与平台分级：<https://docs.rs/interprocess/latest/interprocess/>
- `interprocess::local_socket`（无自带分帧承诺、按平台映射到 UDS / named pipe）：<https://docs.rs/interprocess/2.4.8/interprocess/local_socket/index.html>
- `interprocess::local_socket::GenericFilePath`（Windows 只认 `\\.\pipe\`）：<https://docs.rs/interprocess/2.4.9/interprocess/local_socket/enum.GenericFilePath.html>
- tokio `named_pipe`（Windows + `net` feature）：<https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/index.html>
- tokio `NamedPipeServer`（一个实例一个客户端，必须持续补实例）：<https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.NamedPipeServer.html>
- `tauri::ipc::Channel`（仅 Rust→前端流式回传）：<https://docs.rs/tauri/2/tauri/ipc/struct.Channel.html>
- crates.io `interprocess` 版本元数据：<https://crates.io/api/v1/crates/interprocess>（`max_version=2.4.9`，`updated_at=2026-11-18`）
