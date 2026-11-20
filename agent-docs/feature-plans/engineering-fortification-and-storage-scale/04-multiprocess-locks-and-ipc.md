# Wave 2 · 多进程冲突面与本机 IPC 候选

> 采集方式：源码只读考古 + 实测 CLI 冷启动 + crates.io / Cargo.toml 一手核实。
> 相关工单：[#212 多进程消锁与 ADR-0004 演进](https://github.com/util6/assetiweave/issues/212)

## 1. 到底有几个进程打开同一个库

| 进程 | 入口 | 打开 | 证据 |
| --- | --- | --- | --- |
| Tauri 桌面宿主（GUI） | `AppService::bootstrap_runtime` | `app.db` + Tantivy 索引 | `application/system/bootstrap.rs:44` |
| `aiwc`（Rust 原生 CLI） | 同 `bootstrap_runtime`（In-Process） | 同上 | `backend/cli` → `AppService` |
| `assetiweave-engine --memory-recall-mcp-stdio` | `run_memory_recall_mcp_stdio()` | 同上（只读 MCP 桥） | `desktop/src/bin/assetiweave-engine.rs:2` |
| `assetiweave-engine`（stdio Engine 模式） | `adapters::engine::run_stdio()` | 同上 | `desktop/src/lib.rs:658` |

**第三个进程不是"可能"，而是必然。** `agent_engine/src/infrastructure/backends/acp.rs:393`
在跑 Recall Agent 时会拉起 `assetiweave-engine --memory-recall-mcp-stdio`，
而此时 GUI 很可能正在同步入库 —— 这是历史 `database is locked` 最直接的解释。

数据库入口是唯一的（`open_migrated_pool`），所以"谁在写"可控；但每个进程都各自持有
**独立的 5 连接池**，彼此之间只有 SQLite 文件锁与 `busy_timeout` 协调。

## 2. 索引侧的锁比数据库侧更硬

| 机制 | 位置 | 语义 | 跨进程失效点 |
| --- | --- | --- | --- |
| SQLite 写事务 | `BEGIN IMMEDIATE` | 单写者，WAL 下读不阻塞 | 有 `busy_timeout` 兜底 |
| 重建写者租约 | `conversation_search.rs:71-83` | DB 表内租约，10 分钟过期 | 只在"走同一套 AppService"时有效 |
| Tantivy `IndexWriter` | `engine.rs:259`、`:319`（`index.writer(50_000_000)`） | 独占 `.tantivy-writer.lock` | **第二者直接失败，错误原样透出，无重试、无排队** |

→ 结论：**即使把 SQLite 的 BUSY 全部消掉，索引重建仍需要一套跨进程写者仲裁。**
Tantivy 的单写者锁不受数据库租约保护，两套机制必须一起设计。

## 3. 现有兜底与盲区

| 兜底 | 位置 | 覆盖 | 盲区 |
| --- | --- | --- | --- |
| `busy_timeout = 10s` | `store/database.rs:68` | 瞬态 BUSY | 长事务（整包快照替换）超时后仍失败 |
| `begin_immediate_with_retry(pool, 8)`，退避 50+80n ms | `conversation_repo/common.rs:342-356` | 写锁升级冲突 | **只被 3 个文件使用**（packages / reproject / import_advanced）；最热的 `import.rs:108` 仍是 `pool.begin()`（DEFERRED） |
| 索引重建租约 | `conversation_search.rs:71` | 双进程重建 | 不管 Tantivy 文件锁；进程崩溃后需等 10 分钟过期 |
| 用户脚本重试 | 外部 shell 循环 | 人为绕过 | 不是产品行为 |

写事务全貌（详见 `03-sqlite-and-migration-baseline.md` §A.4）：**41 处 DEFERRED vs 16 处 IMMEDIATE**，
其中约 22 处属于"先读后写"的高风险形态（正是 `SQLITE_BUSY_SNAPSHOT` 517 的成因）。

## 4. 被忽略的事实：CLI 冷启动已经 1.2 秒

本机实测（GUI 同时运行，3 次）：

| 命令 | 耗时 |
| --- | --- |
| `aiwc --help` | 0.01 s |
| `aiwc conversation list --limit 1` | **1.06 / 1.33 / 1.45 s** |

参数解析只占 10 ms，其余全是 bootstrap：`open_migrated_pool`（8 段启动自愈）+ 默认种子 +
受管适配器目录发现 + 租户装载。其中 `converge_portable_paths` 每次启动都对 220 MiB 的
`conversation_parts` 做 `GLOB '/*'` 全表扫（`store/path_convergence.rs:66-67`），
且永远收敛不到 0（当前仍命中 110 行）。

这改变了 IPC 的价值排序：

- 「OneShot 直连是微秒级」（ADR-0004 语境）**不成立**，实际是秒级；
- 常驻进程转发不只消锁，还能摊掉每次 1.2 s 的启动开销 —— 脚本连续调用 N 次可省 N×1.2 s；
- 但**先修启动期全表扫描 + 迁移自愈**可能比上 IPC 更划算，且两者不冲突。

## 5. 依赖现状（一手核实）

| 候选 | 版本 / 来源 | 与现有依赖的关系 |
| --- | --- | --- |
| `tokio::net::UnixListener`（macOS/Linux） | tokio 1.x 内置，`cfg(unix)` | 已有 tokio，但 `backend/asset_core` 与 `desktop` 的 feature 列表**都没开 `net`**，需加一行 |
| `tokio::net::windows::named_pipe` | tokio 1.x 内置，`cfg(windows)` | 同上；跨平台需两套 `cfg` 分支 |
| `interprocess` crate | **2.4.9**（2026-11-18 发布，1,835 万次下载，活跃维护） | 新依赖；一套 API 抹平 UDS 与 Named Pipe，省掉 `cfg` 分支 |

> 来源：crates.io API `GET /api/v1/crates/interprocess`；本地各 `Cargo.toml` 的 tokio feature 列表。

## 6. 三方案对照

| | A. 只调参数（`busy_timeout` + 全写路径 IMMEDIATE） | B. 本机 IPC 单一写者（UDS / Named Pipe） | C. 先修启动与锁模式，不引入新机制 |
| --- | --- | --- | --- |
| 消除 SQLITE_BUSY | 覆盖大部分瞬态；长事务仍会撞 | 写操作串行化后理论归零 | 同 A |
| Tantivy 单写者 | 不解决 | 可一并纳管 | 不解决 |
| CLI 1.2 s 启动 | 不变 | 可摊销（转发到常驻进程） | 显著下降（去掉全表扫描） |
| 跨平台成本 | 零 | UDS + Named Pipe 两套端点与生命周期 | 零 |
| 新增故障面 | 无 | 端点不可达 → 必须无感降级 | 无 |
| 与 ADR-0004 关系 | 无冲突 | 需演进（OneShot 与常驻并存） | 无冲突 |

**判断**：A 与 C 是 B 的前置条件，不是替代品。先把 22 处高风险 DEFERRED 改成 IMMEDIATE、
把启动期全表扫降下来，再评估 B —— 否则会拿 IPC 掩盖住真正的原因。

「只转发写」还是「读写全转发」：
- 只转发写 → 消锁够用，但无法摊销启动开销，也无法复用 SQLite page cache 与 Tantivy 内存索引；
- 读写全转发 → 收益最大，但把"读也必须依赖常驻进程"变成新的单点，降级契约必须更严格。
- 折中：**写转发 + 读直连**，并在端点可用时把"索引重建/搜索"这类重活交给宿主。

## 7. 降级契约（若走 B，必须逐条验收）

1. 端点不存在 / 连接被拒 / 握手超时 **≤ 300 ms** 内切到 OneShot，退出码与输出不变。
2. 无 GUI 的纯 CLI 环境（终端、CI、脚本批量）**零退化**：不依赖任何常驻进程。
3. 转发失败不得静默降级为"部分成功"：写操作要么全部生效，要么明确失败并可重试。
4. 端点归属唯一（GUI 持有、进程退出即回收），禁止两个进程同时自认为写者。
5. 并发压测门禁：N 路 `aiwc` + GUI 同步同时写入，零 `database is locked`、零丢失。
6. 索引重建在端点可用时必须走宿主，禁止客户端直接抢 `.tantivy-writer.lock`。
