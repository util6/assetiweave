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
| 全部正文类列合计 **253 MiB** | `parts.content` 162 MiB + `turns.*` 61 MiB + web 20 MiB | 逐行 Zstd 最多省 ~163 MiB，是索引垃圾的 1/140 |

**一句话**：Wave 2 的存储问题不是「正文太大」，而是「索引漏了、库空了、备份堆了」。
把 34.2 GiB 的垃圾与碎片清掉，工作量接近零、不需要改任何数据模型；
「Zstd + 去重」仍然值得做，但它是为**未来增长曲线**做的，不是为当下止血。

## 四条会改变下游决策的结论

### 1. 顺序必须是「先治索引、再压正文」

按实测（详见 `02-storage-empirical-baseline.md` §E）：

| 阶段 | 主库 | 累计收益 | 工作量 |
| --- | ---: | ---: | --- |
| 现状 | 1.72 GB | — | — |
| ① `VACUUM` | 388 MB | 1.24 GiB | 一条命令 |
| ② + 逐行 Zstd（>1 KB 阈值） | ≈225 MB | +163 MB | 改 schema + 迁移 |
| ③ + 整行哈希去重 | ≈209 MB | +16 MB | 中 |

`VACUUM` 一项占主库总收益的 **74%**。索引侧 22.68 GiB 更是完全没有技术含量的纯泄漏。

### 2. BLAKE3 大块分块去重，在本数据集上属于过度设计

- 4 KB 分块去重率 **11.33%**；而「整行逐字节相同」的重复占 **11.84%**。
- 分块相对整行只多 **0.4 个百分点**，却要引入块表、引用计数、GC 一整套复杂度。
- 更关键：同一批数据「先去重再压缩」与「只压缩」相差 **45 字节（0.02%）**——
  Zstd 的 4 MB 窗口本身就在做重复匹配，BLAKE3 的部分被它吃掉了。

→ 若走批量/列式压缩，去重没有增量价值；若走逐行压缩，**整行内容哈希**就够了，不需要 CDC 分块。
→ 去重必须是**全局作用域**（跨会话重复占 77.8%），按会话去重只能拿到 22%。

### 3. 压缩的硬约束不在写入侧，在 11 处「把正文当 SQL 参数用」的地方

写入侧只有一个漏斗（`snapshot.rs` 的 `insert_parts_sqlx_tx` / `insert_turn_sqlx_tx`），改造成本极低。
真正的风险是那些**不会报错、只会静默失效**的路径（详见 `01-content-read-write-paths.md` §5）：

- `instr(lower(content), ?)` 全表兜底检索（Tantivy 不可用时）→ 静默返回空
- `process GLOB '*"cwd":"/*'` 启动期路径收敛 → 静默不收敛，且 `try_get::<String>` 遇 BLOB 会**硬报类型错**
- `COALESCE(json_extract(attrs_json,'$.summary'), p.content)` → 把二进制塞进 LLM prompt
- `LIKE '%"turn_kind"%'`、`json_remove(...)`、`content.chars().count()` 等

→ 结论：压缩若要做，必须与「索引为唯一搜索真相源」的重构同批，不能单独上线。

### 4. 「CLI 是微秒级」不成立，实测冷启动 1.2 秒

| 命令 | 耗时 |
| --- | --- |
| `aiwc --help` | 0.01 s |
| `aiwc conversation list --limit 1` | 1.06 / 1.33 / 1.45 s |

参数解析只占 10 ms，其余全是 bootstrap：迁移自愈（8 段）+ 默认种子 + 适配器发现 + 租户装载，
其中 `converge_portable_paths` 每次启动都对 220 MiB 的 `conversation_parts` 做 `GLOB` 全表扫，
且永远收敛不到 0（当前仍命中 110 行）。

→ 这改变了 IPC 决策的价值排序：常驻进程转发不只消锁，还能摊掉 1.2 s 启动开销；
但**先修启动期全表扫描**可能比上 IPC 更划算，且两者不冲突。

## 对下游工单的影响（建议改写，未擅自改动他人已认领工单）

| 工单 | 影响 |
| --- | --- |
| [#211 拍板正文 Zstd 压缩、BLAKE3 大块去重与 ADR-0008 时间轴分层](https://github.com/util6/assetiweave/issues/211) | 「是否拆 `blobs.db`」已被实测排除（正文仅 253 MiB，收益 <1%）；BLAKE3 分块去重应降级为「整行哈希去重或不做」；阈值应从 1 KB 起 |
| [#213 拍板 Migration 基线快照与大库性能回归红线](https://github.com/util6/assetiweave/issues/213) | 性能红线必须包含「索引体积 / 段数 / 孤儿代数量」指标，否则泄漏型增长永远测不出来 |
| [#212 拍板多进程消锁与 ADR-0004 演进](https://github.com/util6/assetiweave/issues/212) | 冲突面是**三个进程**（GUI / `aiwc` / `--memory-recall-mcp-stdio` 的 MCP 桥），且 Tantivy 单写者锁不在 SQLite 管辖内 |
| 新增工单 | 索引与库文件的垃圾回收契约（回收触发点、TTL、自报体积口径、VACUUM 时机、备份留存）——见地图 Decisions so far |

## 文档导航

| 文件 | 内容 |
| --- | --- |
| `01-content-read-write-paths.md` | 7 个正文列的语义、唯一写入漏斗、5 类读取路径、**11 处压缩会静默打破的 SQL/Rust 用法**、6 种压缩落点候选 |
| `02-storage-empirical-baseline.md` | 页级体积结构、索引 22.68 GiB 垃圾的成因、Zstd 逐行/批量实测、4 KB 分块去重率、面向 3 亿 Token 的容量推算 |
| `03-sqlite-and-migration-baseline.md` | PRAGMA 现状与缺失项、41 处 DEFERRED 写事务、97 个迁移与 8 段启动自愈的代价、sqlx 0.9 baseline 三种机制、106 个索引中的 5 个冗余 |
| `04-multiprocess-locks-and-ipc.md` | 三进程冲突矩阵、Tantivy 单写者、现有兜底盲区、本机 IPC 方案对照与降级契约 |

## 方法与局限

- 所有数据库访问均以 `file:...?mode=ro` 只读 URI 打开，未执行任何写操作。
- 报告只含聚合统计量，不含任何会话正文、标题、提示词或用户路径明细。
- 去重率用 **sha256 代替 BLAKE3** 计块：对「两段字节是否相同」的判定而言算法无关，结论等价。
- 容量推算基于本机 9 个月真实数据（2,109 会话 / 25,022 索引文档 / 253 MiB 正文），
  「日均 3 亿 Token」为外推场景，误差主要来自 token→字节换算（假设 ~3 字符/token），结论对 ±30% 偏差不敏感。
