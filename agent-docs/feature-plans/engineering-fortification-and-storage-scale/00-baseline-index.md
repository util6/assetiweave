# Wave 2 存储与并发基线：研究结论索引

> 研究工单：[#209 实证摸底 Wave 2 存储与并发基线](https://github.com/util6/assetiweave/issues/209)
> 所属地图：[#207 工程防腐、3 亿 Token 存储扩容与契约/渲染演进规划（Wave 1~3）](https://github.com/util6/assetiweave/issues/207)
> 采集时间：2026-11-20　｜　采集方式：源码只读考古 + 本机真实库只读统计 + Zstd/哈希实测
> 环境：macOS / SQLite 3.54.0 / tantivy 0.26.1 / sqlx 0.9.0 / zstd 1.5.7

## 一页结论：三个数字改写了 Wave 2 的前提

| 事实 | 实测 | 含义 |
| --- | --- | --- |
| 主库 `app.db` 1.60 GiB 里 **77.5% 是 freelist 空闲页** | 在用仅 369.9 MiB，空页 1.24 GiB | 一条 `VACUUM` 就能回收 1.24 GiB，零代码改动 |
| 索引目录 24.69 GiB 里 **91.8% 是孤儿 `generation-*.tmp`** | 81 个废弃代 = 22.68 GiB，活跃代仅 0.318 GiB | 纯垃圾，`rm -rf` 即可；根因是重建被中断后无人回收 |
| 全部正文类列合计 **265.6 MB（253.3 MiB）** | `parts.content` 162 MB（72.5%）+ `turns.*` 61 MB + web 20 MB | 块级 Zstd 全量实测省 **205.0 MiB（5.241x）**；逐行只有 163.6 MB |

> 口径：本节正文总量统一用 **265.6 MB（字节）**；`parts.content` 单列为 169,955,233 B = 162.1 MiB。
> 「1 KB 阈值覆盖 81.55% 字节」是 `conversation_parts.content` 单列的占比，**不能**套到全库总量上。

**一句话**：Wave 2 的存储问题不是「正文太大」，而是「索引漏了、库空了、备份堆了」。
其中 **23.9 GiB 是应用自身可回收的垃圾**（22.68 GiB 索引孤儿代 + 1.24 GiB 空闲页），
另有约 10.3 GiB 是用户的外部备份副本（7 个命名不符的 `app.db.bak_*`，需确认后才能动）。
「Zstd + 去重」仍然值得做，但它是为**未来增长曲线**做的，不是为当下止血。

## 四条会改变下游决策的结论

### 1. 顺序必须是「先治索引、再压正文」

按实测（详见 `02-storage-empirical-baseline.md` §E）：

| 阶段 | 主库 | 累计收益 | 工作量 |
| --- | ---: | ---: | --- |
| 现状 | 1.72 GB | — | — |
| ① `VACUUM` | ≈370 MB（区间 350~388 MB） | 1.24 GiB | 一条命令 |
| ② + 逐行 Zstd（>1 KB 阈值） | ≈225 MB | +163.6 MB | 改 schema + 迁移 |
| ③ + 整行哈希去重（与 ② 叠加） | ≈209 MB | +16 MB | 中 |
| ④ 块级 Zstd（**替代 ②③**，全列实测 5.241x） | **≈186 MB** | +205.0 MiB | 改存储布局 |
| ⑤ ⑤→① 的叠加（块级 + VACUUM + 去重） | ≈170 MB | — | 高 |

`VACUUM` 一项占主库总收益的 **74%**。索引侧 22.68 GiB 更是完全没有技术含量的纯泄漏。
②③ 与 ④ 是**互斥的两条路线**，不能相加：逐行路线能保留按行随机读，块级路线读取必须按块解压。

### 2. BLAKE3 大块分块去重，在本数据集上属于过度设计

- 4 KB 分块去重率 **11.33%**；而「整行逐字节相同」的重复占 **11.84%**。
- 分块相对整行只多 **0.4 个百分点**，却要引入块表、引用计数、GC 一整套复杂度。
- 更关键：同一批数据「先去重再压缩」与「只压缩」相差 **45 字节（0.02%）**——
  Zstd 的 4 MB 窗口本身就在做重复匹配，BLAKE3 的部分被它吃掉了。

→ 若走批量/列式压缩，去重没有增量价值；若走逐行压缩，**整行内容哈希**就够了，不需要 CDC 分块。
→ 去重必须是**全局作用域**（跨会话重复占 77.8%），按会话去重只能拿到 22%。
→ 但别再给 `prompt` 列设计压缩：`turns.prompt` 中位数仅 152 B、`zstd(-3)` 压不过原始大小，
  而它的去重率高达 **36.73%**（同一段提示词跨 turn 反复记录）——这一列只该去重，不该压。

### 3. 压缩的硬约束不在写入侧，在 11 处「把正文当 SQL 参数用」的地方

写入侧只有一个漏斗（`snapshot.rs` 的 `insert_parts_sqlx_tx` / `insert_turn_sqlx_tx`），改造成本极低。
真正的风险是那些**不会报错、只会静默失效**的路径（详见 `01-content-read-write-paths.md` §5）：

- `instr(lower(content), ?)` 全表兜底检索（Tantivy 不可用时）→ 静默返回空
- `process GLOB '*"cwd":"/*'` 启动期路径收敛 → 静默不收敛，且 `try_get::<String>` 遇 BLOB 会**硬报类型错**
- `COALESCE(json_extract(attrs_json,'$.summary'), p.content)` → 把二进制塞进 LLM prompt
- `LIKE '%"turn_kind"%'`、`json_remove(...)`、`content.chars().count()` 等

→ 结论：压缩若要做，必须与「索引为唯一搜索真相源」的重构同批，不能单独上线。

**并且压缩单位不该是「逐行」。** `conversation_parts.content` 的中位数只有 **275 B**，
48.4% 的行贡献 5.4% 的字节；逐行压缩时 20~30 B 的帧头就是 10% 的净损耗。
更具决定性的是**块大小**：同一份数据，1 MB 块压比 2.826、4 MB 块 4.929、拼成整块 9.492，
逐行（约 1 KB 块）只有 2.61。**压缩单位必须 ≥1 MB，4 MB 为甜点**——
「按会话分块」看起来有效，只是因为会话平均 80 KB 已远大于 1 KB，不是会话边界的功劳。

### 4. 「CLI 是微秒级」不成立，而且它有一半时候根本起不来

| 命令 | 耗时 |
| --- | --- |
| `aiwc --help` | 0.01 s |
| `aiwc conversation list --limit 1`（GUI 运行中，release） | 1.06 / 1.33 / 1.45 s |
| 隔离库 + `target/debug/aiwc`（子代理实测） | 0.72–0.84 s（热）/ 4.65–5.84 s（冷） |
| 另一进程持写锁时 | **10.57 s 后 `(code: 5) database is locked`** |

参数解析只占 10 ms，其余全是 bootstrap（8 段迁移自愈 + 默认种子 + 适配器发现 + 租户装载）。

**而第一根因不在并发写，在于「每个进程启动都无条件发一条 UPDATE」**：
`repair_legacy_migration_ledger_versions` 在任何库上都会执行
`UPDATE _sqlx_migrations SET version = ...`（`store/database.rs:126`），而 `open_migrated_pool`
是 GUI 与 CLI 的唯一入口（`database.rs:61` ← `bootstrap.rs:44`）。
这条语句在稳态下是空操作，但**依然要写锁**——所以 GUI 持锁期间，100% 的 `aiwc` 命令
（包括纯只读命令）都会死在这一行上。这决定了「只把写操作转发到 IPC」并不能修掉 CLI 的故障。

→ 三件低成本的事比上 IPC 更优先：①把这条 UPDATE 改成条件写（`WHERE` 命中才执行）；
②修 `.tmp` 代际泄漏；③给 bootstrap 链加分级重试。IPC 的真正收益是**提速**（0.7–1.4 s → 10–55 ms），不是"消锁"。

## 顺带挖出的三条事实（含两处子代理互相纠偏后的定稿）

| 事实 | 证据 | 为什么重要 |
| --- | --- | --- |
| **那 7 个 `app.db.bak_*` 不是本仓产出的**：`backup_file_name()` 生成的是 `assetiweave-app-<ts>-<uuid8>.db`，命名结构完全不同 | `infrastructure/backup/mod.rs:202-206` | 它们是外部脚本/手工副本（与两个 `bak_compact_*` 间隔 7 分钟吻合），清理前需用户确认，**不能**当成"应用的压缩功能坏了" |
| **但备份路径有一条静默降级分支**：`vacuum_into` 走裸 rusqlite 连接（继承其 **5 s 默认 busy_timeout**，本次已核对 `rusqlite-0.37.0/src/inner_connection.rs:119`），持续写竞争超时后 `or_else` 直接退回 `checkpoint_and_copy`（整文件 `fs::copy`），**不告警**，且上层只做 `quick_check`，1.6 GiB 胖副本照样通过 | `infrastructure/backup/mod.rs:154-205` | 备份可能悄悄从"紧凑快照"退化成"整库拷贝"，调用方无法区分；属可观测性缺陷，不是数据损坏 |
| **启动期 `converge_portable_paths` 每次全表扫 220 MiB 且永远收敛不到 0**（当前仍命中 110 行，无 marker） | `store/path_convergence.rs:66-67` | 这是 CLI 每次冷启动 1.2 s 的主要构成之一，也是"修一次就好"的典型 |

## 两个容易被算错的账

1. **不要承诺"磁盘立刻下降 X GB"。** 字节减了 163 MB，磁盘占用可能在 `VACUUM` 前一点不降（freelist 继续吃页）；
   压缩写入后旧明文页也仍留在库里，直到下一次 `VACUUM`。
2. **三笔账不能加总。** ①裸扫描 48.3 GiB → ②删 `.tmp` + 清备份 14.4 GiB → ③`VACUUM` 后主库 + 索引 ≈13 GiB，
   每一档的口径都不同（是否含备份、是否含无效索引）。对外沟通必须注明口径。

## 一个容易做反的顺序

主库 370 MiB 中有 **35.8% 是溢出页（overflow page）**——正是承载长正文的那些页（leaf 占 63.75%，其余为索引与 schema）。
这带来一个执行顺序约束：

1. **先压缩、再 `VACUUM`**：压缩把长正文塞回行内，溢出链消失，随后 `VACUUM` 才能把 leaf 页也收干净。
2. 若先 `VACUUM` 再压缩，压缩后的写入会立刻把页重新撑开，碎片原地复发。
3. `auto_vacuum=INCREMENTAL` 必须**先 `VACUUM` 一次**才生效（当前 `auto_vacuum=0`，`incremental_vacuum` 是空操作）。
4. 全部做完再过一遍 **`PRAGMA optimize`**（当前库里连 `sqlite_stat1` 都没有，查询计划全靠猜）。
5. **不要动 `page_size`**：它是候选参数里唯一"无收益 + 需重建全库"的一项，收益验证为零。

## 对下游工单的影响（建议改写，未擅自改动他人已认领工单）

| 工单 | 影响 |
| --- | --- |
| [#211 拍板正文 Zstd 压缩、BLAKE3 大块去重与 ADR-0008 时间轴分层](https://github.com/util6/assetiweave/issues/211) | 「是否拆 `blobs.db`」已被实测排除（正文仅 253 MiB，收益 <1%）；BLAKE3 分块去重应降级为「整行哈希去重或不做」；**一旦采用 ≥1 MB 块级压缩，「阈值分流」这个问题自动消失**（它只在逐行压缩下才成立）；`prompt` 列只去重不压缩 |
| [#213 拍板 Migration 基线快照与大库性能回归红线](https://github.com/util6/assetiweave/issues/213) | 性能红线必须包含「索引体积 / 段数 / 孤儿代数量」指标，否则泄漏型增长永远测不出来；`repair_legacy_migration_ledger_versions` 应随 baseline 一并条件化或删除 |
| [#212 拍板多进程消锁与 ADR-0004 演进](https://github.com/util6/assetiweave/issues/212) | 冲突面是**三个进程**（GUI / `aiwc` / `--memory-recall-mcp-stdio` 的 MCP 桥）；但第一根因是启动期无条件 `UPDATE`（`database.rs:126`）而非并发写；Tantivy 已被"单写者 + DB 租约"降解，租约缺陷是 10 分钟 TTL 不续租；ADR-0004 的 engine 协议是**零调用的活契约** |
| 新增工单 | 索引与库文件的垃圾回收契约（回收触发点、TTL、自报体积口径、VACUUM 时机、备份留存）——见地图 Decisions so far |

## 文档导航

| 文件 | 内容 |
| --- | --- |
| `01-content-read-write-paths.md` | 7 个正文列的语义、唯一写入漏斗、5 类读取路径、**11 处压缩会静默打破的 SQL/Rust 用法**、6 种压缩落点候选 |
| `02-storage-empirical-baseline.md` | 页级体积结构、索引 22.68 GiB 垃圾的成因、Zstd 逐行/批量实测、4 KB 分块去重率、面向 3 亿 Token 的容量推算 |
| `03-sqlite-and-migration-baseline.md` | PRAGMA 现状与缺失项、41 处 DEFERRED 写事务、97 个迁移与 8 段启动自愈的代价、sqlx 0.9 baseline 三种机制、106 个索引中的 5 个冗余 |
| `04-multiprocess-locks-and-ipc.md` | 冲突矩阵与复现实测、`database.rs:126` 第一根因、ADR-0004 遗产考古（252 命令契约 / engine 二进制零调用）、三方案对照与降级契约 |

## 方法与局限

- 所有数据库访问均以 `file:...?mode=ro` 只读 URI 打开，未执行任何写操作。
- 报告只含聚合统计量，不含任何会话正文、标题、提示词或用户路径明细。
- 去重率用 **sha256 代替 BLAKE3** 计块：对「两段字节是否相同」的判定而言算法无关，结论等价。
- 容量推算基于本机 9 个月真实数据（2,109 会话 / 25,022 索引文档 / 253 MiB 正文），
  「日均 3 亿 Token」为外推场景，误差主要来自 token→字节换算（假设 ~3 字符/token），结论对 ±30% 偏差不敏感。
