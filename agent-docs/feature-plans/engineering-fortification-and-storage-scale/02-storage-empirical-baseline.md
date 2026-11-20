# AssetIWeave 存储扩容实证报告（Wave 2 决策依据）

> 全部数字均为本机实测，只读采集。数据库以 `file:$HOME/.assetiweave/app.db?mode=ro` 打开，全程未执行任何写操作。
> 报告中不含任何会话正文、标题、路径明细或提示词文本，仅含聚合统计量。
> 采集时间：2026-11-20　采集者：res-storage

---

## 0. 一句话结论（先说结果）

**三个"免费"的清理动作能在不改一行代码的情况下把 `~/.assetiweave` 从 48.3 GiB 降到约 13.2 GiB（-73%）：**

| 动作 | 回收 | 前置条件 |
|---|---:|---|
| ① `VACUUM` 主库（77.5% 是空闲页） | **1.24 GiB** | 无，纯 SQLite |
| ② 删除 81 个废弃的 Tantivy `.tmp` 索引代数 | **22.68 GiB** | 无，索引本身只有 0.32 GiB 有效 |
| ③ 清理 7 个 `app.db.bak_*` 全量备份 | **11.21 GiB** | 需确认无回滚需求 |

**而"Zstd 压缩 + BLAKE3 去重"这套方案，在主库上只能再省约 170 MiB（370 MiB → ~200 MiB）。**
真正的大头不是正文，是索引目录的泄漏（占 48.3 GiB 的 51%）和主库的碎片（主库自身 77.5% 是空的）。

换句话说：**「先治索引膨胀/清备份，再谈压缩去重」是对的，而且前者工作量近乎为零、收益是后者的 12 倍。**

---

## A. 主库真实体积结构

### A.1 页级物理事实

| 指标 | 值 |
|---|---|
| `journal_mode` | `wal` |
| `auto_vacuum` | `0`（**关闭**，故 `PRAGMA incremental_vacuum` 是空操作） |
| `page_size` | 4,096 B |
| `page_count` | 420,063 |
| 文件大小 | 1,720,578,048 B = **1.602 GiB** |
| `freelist_count` | **325,384 页** = 1,332,772,864 B = **1.241 GiB** |
| **空闲页占比** | **77.47%** |
| 实际在用页 | 94,678 页 = 387,801,088 B = **369.9 MiB（22.53%）** |
| 有效 cell payload | 336,857,120 B = 321.3 MiB |
| 页内未用空间（`unused`） | 46,657,603 B = 44.5 MiB |
| 页内空间利用率 | 336.9 / 387.8 = **86.9%** |
| `-wal` | 36,824,592 B = 35.1 MiB |

**结论：主库是一具 1.6 GiB 的空壳。** 真实数据只有 370 MiB，其余 1.24 GiB 是历史 DELETE 留下的 freelist 页。

> **附带发现（强证据，指向一个比任何新特性都优先的 bug）**
>
> `~/.assetiweave/` 下有 **7 个**主库备份，**字节数全部精确等于 1,720,578,048 B —— 与主库一模一样，一个字节都不差**：
> `app.db.bak_1793541197` / `bak_data_clean_1793841520` / `bak_data_clean_1793841964` / `bak_pathfix_20261108_181713` / `bak_cwdfix_20261108_192137` / `bak_compact_20261115_103243` / `bak_compact_20261115_111731`（共 11.22 GiB）。
>
> 其中两个名为 `bak_compact_*`（2026-11-15 11:10 与 11:17，间隔 7 分钟）——**说明已经跑过两次"压缩"，但产物和源库同尺寸、主库 freelist 依旧 1.24 GiB。**
>
> 正常的 `VACUUM` / `VACUUM INTO` 会重建整库、产物必须显著变小。字节数精确相同只有两种解释：
> 1. **compact/backup 路径实际是整文件复制（`fs::copy`），不是 `VACUUM INTO`**；
> 2. 或者是 `VACUUM INTO` 的对象搞错了（拷的是源文件）。
>
> 无论哪一种，都意味着 **「压缩」功能从未生效**，而每跑一次就多一份 1.6 GiB 明文副本（这也解释了 11.22 GiB 的备份体积，并与路径侧发现的"迁移脚本绕过 Rust 层直写明文"风险同向）。
> 这条需要代码侧确认，但方向明确：**先修 compact，再谈扩容**。

### A.2 dbstat 按对象 Top（可用，共 94,678 页）

| 对象 | 类型 | 页数 | 字节 | 占在用空间 |
|---|---|---:|---:|---:|
| `conversation_parts` | 表 | 56,544 | 231,604,224 | **59.72%** |
| `conversation_turns` | 表 | 17,488 | 71,630,848 | 18.47% |
| `sqlite_autoindex_conversation_parts_1` | 索引 | 4,696 | 19,234,816 | 4.96% |
| `web_record_turns` | 表 | 3,557 | 14,569,472 | 3.76% |
| `web_record_parts` | 表 | 2,699 | 11,055,104 | 2.85% |
| `conversation_usage_events` | 表 | 2,578 | 10,559,488 | 2.72% |
| `idx_conversation_parts_id_key` | 索引 | 1,265 | 5,181,440 | 1.34% |
| `sqlite_autoindex_conversation_usage_events_1` | 索引 | 564 | 2,310,144 | 0.60% |
| `conversation_sessions` | 表 | 514 | 2,105,344 | 0.54% |
| `idx_usage_events_observation` | 索引 | 467 | 1,912,832 | 0.49% |
| `idx_usage_events_model_time` | 索引 | 414 | 1,695,744 | 0.44% |
| `idx_usage_events_source_time` | 索引 | 402 | 1,646,592 | 0.42% |
| `idx_conversation_usage_events_tenant_session_turn` | 索引 | 376 | 1,540,096 | 0.40% |
| `idx_usage_events_adapter_time` | 索引 | 360 | 1,474,560 | 0.38% |
| `model_pricing_catalog` | 表 | 327 | 1,339,392 | 0.35% |

`conversation_parts` + `conversation_turns` 两张表占在用空间的 **78.2%**。

### A.3 表 vs 索引占比

| 类别 | 字节 | 占比 |
|---|---:|---:|
| 表（含溢出页） | 344,981,504 | **88.96%** |
| 索引 | 42,754,048 | **11.03%** |
| 其他（sqlite_master 等） | 65,536 | 0.02% |

索引不是问题。索引里最大的是 `sqlite_autoindex_conversation_parts_1`（`UNIQUE(tenant_id, turn_id, part_index)`，19.2 MB = 全部索引的 45.0%）——原因是它在 16 万行里重复存了两次 TEXT 型 UUID 前缀。若改成整数自增 turn 引用，这一项能省约 15 MB。属于小优化，不是主战场。

### A.4 会话域核心表：行数与正文分布

| 表 | 行数 | 正文列 | 正文字节 | 平均 |
|---|---:|---|---:|---:|
| `conversation_sessions` | 2,109 | `title` 等 | 1,816,703（全行 payload） | — |
| `conversation_turns` | 7,336 | `prompt` | 12,259,869 | 1,671 B |
| | | `process` | 33,442,254 | 4,734 B |
| | | `response` | 18,632,738 | 2,935 B |
| `conversation_parts` | 160,465 | `content`（非空 154,748） | **169,955,233** | 1,098 B |
| | | `attrs_json` | 9,838,886 | 75 B |
| | | `translated_text` | 6,812（仅 3 行） | — |
| `web_record_sessions` | 637 | — | — | — |
| `web_record_turns` | 2,979 | `prompt` / `process` / `response` | 977,609 / 53,155 / 10,651,083 | 328 B / — / — |
| `web_record_parts` | 2,817 | `content` | 9,833,638 | 3,491 B |
| `conversation_usage_events` | 24,493 | — | — | — |

**全库正文合计 = 265,644,465 B = 253.3 MiB**，占有效 payload（321.3 MiB）的 **78.9%**。
也就是说：即使把正文全部凭空消灭，主库也还有 ~66 MiB 的 id / 索引 / 计费数据。**正文压缩的天花板就是 253 MiB。**

### A.5 `conversation_parts.content` 长度分布

| 指标 | 值 |
|---|---|
| 非空行数 | 154,748 |
| 总字节 | 169,955,233（160.0 MiB） |
| 平均 | 1,098.3 B |
| p50 | **275 B** |
| p90 | 2,420 B |
| p99 | 12,249 B |
| max | 218,555 B |

典型长尾分布：中位数只有 275 B，但前 1% 占了极大比例。

**长度直方图**

| 分桶 | 行数 | 行占比 | 总字节 | 字节占比 |
|---|---:|---:|---:|---:|
| < 256 B | 74,943 | 48.43% | 9,175,559 | 5.40% |
| 256 B – 1 KB | 40,340 | 26.07% | 22,147,678 | 13.03% |
| 1 KB – 4 KB | 31,006 | 20.04% | 57,034,211 | 33.56% |
| 4 KB – 16 KB | 7,544 | 4.88% | 55,891,642 | 32.89% |
| > 16 KB | 915 | 0.59% | 25,706,143 | 15.13% |

**决策含义**：约一半的行（48.4%）只贡献 5.4% 的字节。任何"逐行压缩"方案都必须做阈值分流，否则会为一半的行付出 CPU 和存储开销、却只换回 1% 的收益（见 C.2，<256B 的压缩比只有 1.09，压了等于没压）。

---

## B. Tantivy 索引体积（真正的元凶）

`$HOME/.assetiweave/conversation-search-index` = **26,510,647,675 B = 24.69 GiB，31,614 个文件**。

### B.1 目录构成 Top

两个索引实例命名空间：`bca14ae6*`（14 GiB）、`d86f8bf5*`（11 GiB）。

| 类别 | 字节 | 占比 | 文件数 |
|---|---:|---:|---:|
| **废弃 `.tmp` 代数** | **24,349,000,000（22.677 GiB）** | **91.8%** | 29,642 |
| 有效代数 | 2,161,000,000（2.013 GiB） | 8.2% | 1,972 |

**按扩展名（全目录）**

| 扩展名 | 体积 | 占比 | 文件数 |
|---|---:|---:|---:|
| `.idx`（倒排 posting） | 10.73 GiB | 43.5% | 5,213 |
| `.pos`（位置表） | 7.27 GiB | 29.4% | 5,212 |
| `.term`（词典） | 5.78 GiB | 23.4% | 5,213 |
| `.store`（正文字段存储） | 0.71 GiB | 2.9% | 5,214 |
| `.fieldnorm` | 0.20 GiB | 0.8% | 5,213 |
| `.fast` | ~0 | 0.0% | 5,213 |

**5,213 个 `.idx` 文件**——正常合并后的索引应该是个位数 segment。

### B.2 有效 vs 废弃（这是核心发现）

| 代数 | 字节 | segment 数 | 状态 |
|---|---:|---:|---|
| `d86f8bf5/generation-47f251c3…` | 0.989 GiB | 72 | 孤儿实例（`conversation_search_index_state` 无对应行） |
| `bca14ae6/generation-3af6321a…` | 0.364 GiB | 92 | 上一代，已被超越 |
| `d86f8bf5/generation-98929d2c…` | 0.343 GiB | 53 | 孤儿实例，opstamp=91,615（远落后） |
| **`bca14ae6/generation-8e175746…`** | **0.318 GiB** | **93** | **← 当前 ACTIVE** |
| 合计有效 | 2.013 GiB | 310 | |
| **81 个 `generation-*.tmp` 目录** | **22.677 GiB** | — | **全部 meta.json 为 `opstamp=0, segments=0`** |

`conversation_search_index_state` 的真实记录：

```
health = ready
document_count = 25,022
size_bytes = 301,611,440        (287.6 MiB — 应用自认为的索引体积)
active_generation = generation-8e175746…
```

**应用认为索引是 287 MiB，磁盘上是 24.69 GiB，相差 85 倍。**

那 81 个 `.tmp` 目录的 `meta.json` 全部写着 `opstamp=0, segments=0`，但目录里躺着几百 MB 已写出的 segment 文件——这是典型的 **Tantivy `IndexWriter` 在 commit 前被杀死/超时中断**：segment 文件已落盘，却从未写进 `meta.json`，于是永不可达、永不回收。每次失败的重建都会新开一个 `generation-*.tmp`，于是 81 次失败 = 22.68 GiB 垃圾。单个最大的垃圾代 2.09 GiB / 5,944 文件。

### B.3 活跃代内部构成（回答"主要占用来自哪些 segment 文件"）

| 文件类型 | 体积 | 占比 |
|---|---:|---:|
| `.term` | 151.6 MiB | **46.6%** |
| `.pos` | 91.7 MiB | 28.2% |
| `.idx` | 78.5 MiB | 24.1% |
| `.store` | 2.8 MiB | 0.8% |
| `.fieldnorm` | 0.7 MiB | 0.2% |

**关键：`.store` 只有 2.8 MiB。正文根本没有进索引**，索引体积 98% 花在倒排词典 + 位置表 + posting 上。

25,022 文档撑起 325 MiB 索引 = **13.3 KB/文档**。这个比值极不正常（正常英文全文索引约 1~3 KB/文档），说明 term 已经爆炸——最可能的原因是中文按字/ngram 切词，或者 `position` 对超长正文（p99 = 12 KB、max = 218 KB）逐 token 记录偏移。

### B.4 是否存在大量未合并 segment

**是。活跃代 93 个 segment / 25,022 文档 = 258 文档/segment。**

- Tantivy 默认的 `LogMergePolicy` 会持续把 segment 合并到个位数；93 个说明**合并线程从未有机会跑完**（与 `.tmp` 泄漏同源：进程活不到 merge 那一步）。
- 后果直接体现在 `.term` 占 46.6%：93 个 segment 各自持有一份高度重叠的词典，`FST` 无法跨 segment 共享。
- 保守估计：**把 93 个 segment 合并成 1~4 个，`.term` 至少能砍 30~50%**，即活跃索引从 325 MiB 降到 200 MiB 左右。此项未经实际 merge 验证，标注为推断。

### B.5 索引的账

- 磁盘实占：**24.69 GiB**
- 应有体积：**0.32 GiB**（当前 active 代）或 0.99 GiB（含另一实例的最新代）
- **可立即回收：22.68 GiB，回收率 91.8%，零代码、零风险（`.tmp` 目录在 Tantivy 语义上就是不可达垃圾）**

---

## C. Zstd 压缩收益实证

### C.1 环境

| 项 | 结果 |
|---|---|
| `zstd` CLI | `/opt/homebrew/bin/zstd` |
| python `zstandard` | **未安装** |
| 采用方案 | **`libzstd 1.5.7` via ctypes 直调 `ZSTD_compressCCtx` / `ZSTD_decompressDCtx`** —— 是真正的 Zstd 实测，非 gzip 替代 |
| 压缩级别 | `-3`（Tantivy/Zstd 默认档） |
| 落盘 | 无。全部在内存中完成，样本用完即弃 |

### C.2 分层抽样（每桶 40 条，共 200 条 / 1,626,509 B）

**逐行压缩（模拟"每条正文单独压缩存储"）**

| 分桶 | 压缩比 | 节省 |
|---|---:|---:|
| < 256 B | **1.092** | 8.4% ← 基本无收益，还倒赔头部开销 |
| 256 B – 1 KB | 1.613 | 38.0% |
| 1 KB – 4 KB | 2.906 | 65.6% |
| 4 KB – 16 KB | 3.804 | 73.7% |
| > 16 KB | 4.013 | 75.1% |
| **整体（等权）** | **3.792** | **73.63%** |

**延迟（逐行，含 ctypes + 分配开销）**

| 指标 | 值 |
|---|---|
| 压缩耗时中位数 | **0.0235 ms/条** |
| 压缩 p95 | 0.1385 ms/条 |
| 解压耗时中位数 | **0.0144 ms/条** |
| 按 154,748 行全量重压 | **≈ 3.6 秒 CPU** |

**压缩 CPU 成本可以忽略**，不构成任何设计约束。

**多条拼块（200 条拼成 1,626,509 B 一次压）**

| 指标 | 值 |
|---|---|
| 压缩比 | **6.491** |
| 节省 | 84.59% |
| 压缩中位数 | 4.49 ms（345 MiB/s） |
| 解压中位数 | 1.30 ms |

> ⚠️ 等权抽样会高估：上面每桶取 40 条，而真实库里 <256 B 的行占 48%。所以下面用**比例抽样**与**全量实测**校正。

### C.3 校正后的真实压缩比

| 测法 | 样本 | 逐行压缩比 | 批量压缩比 |
|---|---|---:|---:|
| 分层等权抽样（上表） | 200 行 / 1.63 MB | 3.792（高估） | 6.491（高估） |
| **顺序抽样（接近真实分布）** | 6,000 行 / 6.97 MB | **2.614** | **6.820** |
| **全量 `conversation_parts.content`，4 MB 分块批量** | **160.0 MiB 全量** | — | **5.382（节省 81.42%）** |

全量实测数字（无外推）：
`169,955,233 B → 31,576,779 B`，节省 **138,378,454 B = 132.0 MiB（81.42%）**。

**这是本报告最硬的一个数字**：`conversation_parts.content` 整列，4 MB 一块批量 Zstd-3，从 160 MiB 压到 30.1 MiB。

### C.4 阈值化收益（>512 B 与 >1 KB）

| 阈值 | 覆盖行数 | 覆盖字节 | 占正文 |
|---|---:|---:|---:|
| > 512 B | 60,531 | 153,899,109 | 90.55% |
| > 1 KB | 39,433 | 138,599,228 | 81.55% |

按 C.2 的分桶压缩比计算 `conversation_parts.content` 的节省（不外推）：

| 策略 | 压缩后 | 节省字节 | 节省率 |
|---|---:|---:|---:|
| 全量逐行压缩（比 2.614） | 65,017,303 | 104,937,930（100.1 MiB） | 61.7% |
| **> 512 B 阈值逐行**（<512 B 不压，比 1.09 视为不压） | 66,894,897 | **102,964,312（98.2 MiB）** | **66.9%** |
| **> 1 KB 阈值逐行** | 71,723,375 | **97,875,274（93.3 MiB）** | **70.6%** |

**结论：1 KB 阈值是甜点。** 它只覆盖 81.6% 的字节，却拿到 70.6% 的节省率；而全量逐行要多压 115,315 行小文本（CPU 翻倍、格式开销翻倍），只多换 7 MiB。

**按同比例外推到全库正文（265,644,465 B）：**
> > 1 KB 阈值逐行 Zstd-3 可节省 **≈ 163 MB（155.7 MiB）**；压缩后正文约 98 MB。

（外推口径：假设 turns/web_record 各列的压缩比与 `content` 同长度分桶一致。属估算，误差主要来自 `attrs_json` 这类小 JSON。）

### C.5 存储模型决定了能拿到几倍

| 存储模型 | 实测/推算比 | 全库正文（253.3 MiB）压缩后 |
|---|---:|---:|
| 逐行压缩，无阈值 | 2.61 | 97 MiB |
| 逐行压缩，>1 KB 阈值 | ~2.7（加权） | ~98 MiB |
| 批量/列式，4 MB 块 | **5.38（全量实测）** | **47 MiB** |

同一份数据，**"怎么存"比"压不压"影响更大（2.6x vs 5.4x）**。如果 Wave 2 真的面向 3 亿 token/天，值得考虑把正文列为独立 segment 存储（如 4 MB ~ 16 MB 打包 + 块索引），而不是逐行 BLOB。

---

## D. 去重率实证

> 方法说明：按 **4 KB 分块**（近似 BLAKE3 的块语义），**用 `sha256` 代替 BLAKE3** 计算块哈希。
> 对"内容重复率"这个结论而言，哈希算法与碰撞概率无关——我们要的是"两段字节是否相同"的判定，sha256 的 2^-128 碰撞概率远低于任何业务噪声。**去重率结论与用 BLAKE3 完全等价。**

### D.1 全量语料，非抽样

覆盖 5 张表的正文列，**全量流式扫描，共 254,774,815 B**：

| 指标 | 值 |
|---|---:|
| 总块数 | 210,918 |
| 唯一块数 | 174,068 |
| 重复块数 | 36,850 |
| **重复块占比** | **17.47%** |
| 总字节 | 254,774,815 |
| 唯一字节 | 225,911,648 |
| **可节省字节** | **28,863,167（27.5 MiB）** |
| **去重率** | **11.33%** |

### D.2 按来源拆分（谁在重复）

| 来源 | 总块 | 总字节 | 重复块 | 重复字节 | 该列去重率 |
|---|---:|---:|---:|---:|---:|
| `conversation_parts.content` | 171,086 | 169,955,233 | 31,232 | 20,741,769 | **12.20%** |
| `conversation_turns.prompt` | 9,393 | 12,259,869 | 2,458 | 4,502,826 | **36.73%** ← 最高 |
| `web_record_parts.content` | 4,634 | 9,833,638 | 534 | 1,702,338 | 17.31% |
| `conversation_turns.response` | 8,513 | 18,632,738 | 758 | 1,487,833 | 7.99% |
| `conversation_turns.process` | 13,123 | 33,442,254 | 1,784 | 386,179 | 1.15% |
| `web_record_turns.response` | 4,169 | 10,651,083 | 84 | 42,222 | 0.40% |

`conversation_parts.content` 贡献了全部可节省字节的 **71.9%**。
`prompt` 列的去重率高达 36.7%——**同一段提示词在多个 turn 里被反复记录**，这是最"白捡"的一类重复。

### D.3 重复的形态：几乎全是"整行完全相同"

| 指标 | 值 |
|---|---:|
| `conversation_parts` 中内容完全相同的重复行 | **28,916 行** |
| 这些行占用的重复字节 | **20,119,396 B（19.2 MiB）** |
| 占该列正文比例 | **11.84%** |

对比 D.2 里同列的 4 KB 分块去重率 **12.20%**——两者几乎相等。
**说明去重收益几乎全部来自"整行逐字节相同"，而不是"行内/行间的部分重叠"。**

**决策含义**：如果目标只是这 11%，**用整行内容哈希做主键（content-addressed）就够了，不需要 BLAKE3 大块 CDC 分块**。4 KB 分块换来的额外收益 < 0.4 个百分点，却要引入分块表、块引用计数、GC 等一整套复杂度。**"大块去重"在这个数据集上属于过度设计。**

### D.4 重复块的典型来源

按 `kind` 字段统计重复正文的出现次数（Top）：

| kind | 重复组数 |
|---|---:|
| `antigravity.file-change` | 2,847 |
| `antigravity.tool` | 1,696 |
| `antigravity.reasoning` | 1,529 |
| `codebuddy.reasoning` | 1,505 |
| `codebuddy.tool` | 1,315 |
| `antigravity.result` | 1,042 |
| `antigravity.ambient` | 809 |
| `codex.reasoning` | 243 |
| `codex.result` | 182 |
| `codebuddy.result` | 144 |
| `codex.ambient` | 133 |
| `opencode.result` | 115 |

分布高度集中在 **tool 调用 / reasoning / file-change / ambient** 这四类结构化小部件上，而不是人类自然语言。

**跨会话 vs 同会话**（11,883 个重复正文值）：

| 范围 | 组数 | 占比 |
|---|---:|---:|
| 跨会话重复 | 9,247 | **77.8%** |
| 同会话重复 | 2,636 | 22.2% |

**结论：去重作用域必须是全局的，不能按会话。** 如果只在单个会话内去重，只能拿到 22% 的收益。

**交叉验证**：`conversation_sessions` 2,109 条中，`user_visible=0` 且 `execution_origin='subagent'` 的有 **903 条（42.8%）**。子代理会话占了近一半——这与"跨会话重复占 78%"高度吻合：父会话与子代理各记一份同样的工具调用与推理过程，就是重复的主要机制。

### D.5 关键反直觉发现：去重与块压缩高度重叠

在同一份 200 行抽样上做三组对比：

| 处理 | 字节 |
|---|---:|
| 原始 | 1,626,509 |
| 只做 Zstd-3（拼块） | 250,597 |
| 先去重（4 KB）再 Zstd-3（拼块） | 250,552 |
| 差额 | **45 B（0.02%）** |

**先去重再压缩，几乎一字节都没多省。** 原因很直接：Zstd 的 4 MB 窗口本身就在做"重复匹配"，块编码器已经把重复块替换成了 3 字节的 offset/length 引用，比 BLAKE3 的分块表还要省。

**所以：**
- 如果走**批量/列式压缩**路线 → BLAKE3 去重基本没有增量价值，别做。
- 如果走**逐行压缩**路线 → 去重才有意义（每行独立压缩，窗口太小，只有几十~几千字节，zstd 找不到跨行的重复），此时 11.33% 去重 + 逐行 Zstd 是叠加的。
- 如果只是想在**不压缩**的前提下省空间 → 整行哈希去重，收 11.84%。

---

## E. 结论与优先级

### E.1 主库最终能压到多少

| 阶段 | 主库大小 | 说明 |
|---|---:|---|
| 现状 | **1.72 GB**（+35 MB WAL） | 其中 1.24 GiB 是空闲页 |
| ① `VACUUM` | **388 MB**（370 MiB） | 回收 1.24 GiB，纯 SQLite，无代码 |
| ② + 逐行 Zstd(>1 KB 阈值) | **≈ 225 MB**（215 MiB） | 正文 265.6 MB → 98 MB，回收 ~163 MB |
| ③ + 整行哈希去重(11.33%) | **≈ 209 MB**（200 MiB） | 去重再省 ~30 MB，抵掉引用表 ~3 MB，净省 ~16 MB |
| ④ 若改列式/分块存储(5.38x) | **≈ 180 MB**（172 MiB） | 正文 265.6 MB → 49 MB，需改存储布局 |

**1.72 GB → 209 MB（-87.8%，8.2 倍）。**
但其中 **1.24 GiB（占总收益的 74%）来自 `VACUUM` 一行命令**；整个"Zstd + 去重"工程只贡献 ~170 MiB。

### E.2 25 GB 索引是否才是第一优化目标

**是，而且不是"优化"级别，是"止血"级别。**

- 索引目录 24.69 GiB = 整个 `~/.assetiweave`（48.3 GiB）的 **51%**
- 其中 **22.68 GiB（91.8%）是 81 个 `.tmp` 废弃代数——纯垃圾，可直接 `rm -rf`**
- 有效索引只有 **0.32 GiB**
- 索引 : 主库 体积比 = **15.4 : 1**，而索引只含 25,022 个文档、连正文都不存（`.store` 仅 2.8 MiB）

**索引不是"太大"，是"漏了"。** 一个 25,022 文档的元数据索引占了 24.69 GiB，这件事本身说明构建流程有缺陷（writer 被杀 → 每次重试留下一个 200 MB~2 GB 的 `.tmp`）。

### E.3 优先级建议

| 优先级 | 动作 | 收益 | 工作量 | 风险 |
|---|---|---:|---|---|
| **P0** | 清理 81 个 `generation-*.tmp` 废弃目录 | **22.68 GiB** | 极低（一个 find/rm） | 极低（Tantivy 语义上不可达） |
| **P0** | `VACUUM` 主库 | **1.24 GiB** | 极低 | 低（需 2×磁盘瞬时余量；当前 586 GiB 可用，充足） |
| **P0** | 修 `.tmp` 泄漏根因（writer 中断/超时 → 清理逻辑） | 防止每天再涨 GB 级 | 中 | — |
| **P1** | 确认后清理 7 个 `app.db.bak_*` | **11.21 GiB** | 极低 | 中（需确认无回滚需求） |
| **P1** | 修 compact 无效的 bug（产物与源库同尺寸，疑为 fs 复制而非 `VACUUM INTO`） | 防止 1.24 GiB 碎片复发 + 每跑一次多 1.6 GiB 副本 | 低 | — |
| **P1** | 修 Tantivy merge 饥饿（93 segment → 1~4） | 索引 ~-35%（~110 MiB） | 中 | 中 |
| **P2** | 正文逐行 Zstd（>1 KB 阈值）+ 整行哈希去重 | **~170 MiB** | 中高（改 schema + 迁移） | 中 |
| **P3** | `conversation_parts` 的 TEXT UUID 改整数引用 | ~15 MiB | 低 | 低 |
| **✗ 不建议** | BLAKE3 大块 CDC 去重 | **~0.4 个百分点增量** | 高 | 高 |
| **✗ 不建议** | 按时间分片 | 无证据支持；当前瓶颈是泄漏与碎片，不是单表规模 | — | — |

**P0 + P1 合计回收 35.1 GiB，约占 `~/.assetiweave` 的 73%，且全部不需要改任何数据模型。**

### E.4 面向"日均 3 亿 Token"的容量推算

现有全库正文 253.3 MiB，约 2.45 亿字符；按混合中英 ~3 字符/token 估算 ≈ **8,200 万 token**。

- 3 亿 token/天 ≈ **每天把现有全库写 3.7 遍**
- 原始正文增长 ≈ **0.96 GB/天 ≈ 350 GB/年**
- 逐行 Zstd（2.6x）→ **0.37 GB/天 ≈ 135 GB/年**
- 列式/分块 Zstd（5.38x）→ **0.18 GB/天 ≈ 65 GB/年**

（token→字节 换算是粗估，假设已在正文中说明；结论对 ±30% 的偏差不敏感。）

**但索引更危险**：按当前 13.3 KB/文档 的比例，正文每涨 1 GB 会带来数 GB 的倒排。**如果 `.tmp` 泄漏和 merge 饥饿不修，索引会以每天 GB 级的速度失控增长，"压缩正文"完全追不上。** 这就是为什么顺序必须是「先治索引、再压正文」。

---

## 附录 A：执行的命令（脱敏后）

```bash
# --- 环境探测 ---
ls -la $HOME/.assetiweave/
which zstd; python3 -c "import zstandard"   # CLI 有、模块无 → 改用 libzstd via ctypes
sqlite3 --version                            # 3.54.0
/opt/homebrew/lib/libzstd.1.5.7.dylib        # zstd 1.5.7

# --- A. 主库结构（全部只读 URI）---
sqlite3 "file:$HOME/.assetiweave/app.db?mode=ro" <<'SQL'
PRAGMA page_size; PRAGMA page_count; PRAGMA freelist_count;
PRAGMA journal_mode; PRAGMA auto_vacuum; PRAGMA encoding;
SQL

# dbstat 按对象
sqlite3 "file:$HOME/.assetiweave/app.db?mode=ro" \
 "SELECT name, SUM(pgsize) b, COUNT(*) p FROM dbstat GROUP BY name ORDER BY b DESC LIMIT 20;"

# 表 / 索引精确拆分
sqlite3 "file:$HOME/.assetiweave/app.db?mode=ro" <<'SQL'
WITH d AS (SELECT name,sum(pgsize) b,count(*) p FROM dbstat GROUP BY name)
SELECT 'TABLE_OBJ', sum(b), sum(p) FROM d WHERE name IN (SELECT name FROM sqlite_master WHERE type='table');
WITH d AS (SELECT name,sum(pgsize) b,count(*) p FROM dbstat GROUP BY name)
SELECT 'INDEX_OBJ', sum(b), sum(p) FROM d WHERE name IN (SELECT name FROM sqlite_master WHERE type='index');
SQL

# 全库 payload / unused
sqlite3 "file:$HOME/.assetiweave/app.db?mode=ro" \
 "SELECT sum(pgsize), sum(payload), sum(unused), count(*) FROM dbstat;"

# content 长度分位（BLOB 口径，避开字符数/字节数混淆）
sqlite3 "file:$HOME/.assetiweave/app.db?mode=ro" <<'SQL'
WITH L AS (SELECT length(CAST(content AS BLOB)) n FROM conversation_parts WHERE content<>''),
R AS (SELECT n, row_number() OVER (ORDER BY n) rn, count(*) OVER () c FROM L)
SELECT c, sum(n), round(avg(n),1),
  max(CASE WHEN rn=(c*50+99)/100 THEN n END),
  max(CASE WHEN rn=(c*90+99)/100 THEN n END),
  max(CASE WHEN rn=(c*99+99)/100 THEN n END),
  max(n) FROM R;
SQL

# 长度直方图
sqlite3 "file:$HOME/.assetiweave/app.db?mode=ro" <<'SQL'
WITH L AS (SELECT length(CAST(content AS BLOB)) n FROM conversation_parts WHERE content<>'')
SELECT CASE WHEN n<256 THEN '1_<256B' WHEN n<1024 THEN '2_256B-1KB'
            WHEN n<4096 THEN '3_1KB-4KB' WHEN n<16384 THEN '4_4KB-16KB'
            ELSE '5_>16KB' END b, count(*), sum(n)
FROM L GROUP BY b ORDER BY b;
SQL

# 精确重复行
sqlite3 "file:$HOME/.assetiweave/app.db?mode=ro" <<'SQL'
SELECT sum(c-1), sum(sz*(c-1)) FROM
  (SELECT content, count(*) c, max(length(CAST(content AS BLOB))) sz
   FROM conversation_parts WHERE content<>'' GROUP BY content HAVING c>1);
SQL

# --- B. Tantivy 索引 ---
du -sh $HOME/.assetiweave/conversation-search-index
du -sh $HOME/.assetiweave/conversation-search-index/*/

# 按扩展名 / 按 generation / 按 .tmp 归属统计（见附录 B 脚本）
python3 -c "..."   # os.walk + collections.Counter，见下
python3 -c "import json,glob; [print(json.load(open(m))['opstamp'], len(json.load(open(m))['segments'])) for m in glob.glob('.../**/meta.json', recursive=True)]"

# --- C/D. 压缩与去重（内存内，不落盘）---
python3 /tmp/wayfinder-209/_probe_cd.py "$HOME/.assetiweave/app.db"   # 见附录 B

# 全量列式批量压缩（4 MB 分块，全量、无外推）
python3 -c "... ZSTD_compressCCtx over 4MB buffers ..."
```

## 附录 B：实验脚本

完整脚本见 `/tmp/wayfinder-209/_probe_cd.py`，原始输出见 `/tmp/wayfinder-209/_probe_out.json`。
核心做法：

1. **Zstd 绑定**：`ctypes.CDLL("/opt/homebrew/lib/libzstd.dylib")`，复用单个 `CCtx` / `DCtx` 避免每次分配（否则延迟测量会被分配器噪声淹没）。
2. **分层抽样**：对 5 个长度桶各 `ORDER BY rowid LIMIT 40`，总样本上限 50 MB。
3. **比例抽样/全量**：`ORDER BY rowid` 顺序读，按 4 MB 缓冲拼块后一次压缩。
4. **去重**：流式 `fetchmany(2000)`，对每行按 4,096 B 切片，`sha256` 摘要入 `set`；只保留 32 B 摘要，不保留正文。峰值内存 ≈ 摘要表（约 5.6 MB）+ 当前批。
5. **隐私**：脚本只输出长度、计数、哈希；正文变量出作用域即释放，全程 `text_factory = bytes`，无任何写盘。

## 附录 C：方法学与局限

| 项 | 说明 |
|---|---|
| 只读保证 | 全程 `file:...?mode=ro`；未建临时表、未 `VACUUM`、未 `ANALYZE`、未写任何文件到 `~/.assetiweave` |
| 哈希替代 | **sha256 代替 BLAKE3 计块，去重率结论等价**（碰撞概率差异对"内容是否相同"的判定无影响） |
| 压缩实测 | **libzstd 1.5.7 真实调用，非 gzip 替代** |
| 抽样偏差 | 分层等权抽样把压缩比从真实 2.61 抬到 3.79，报告已用比例抽样与全量实测双向校正，正文采用校正值 |
| 外推口径 | "全库节省 ~163 MB"由 `conversation_parts.content` 实测比分桶外推，误差主要来自 `attrs_json`（小 JSON，压缩比被低估/高估均可能） |
| 未验证 | ① Tantivy merge 后 `.term` 能降多少（推断 30~50%，未实跑 merge）；② `.tmp` 泄漏的确切代码路径（仅从 `opstamp=0` + 文件残留推断 writer 被杀）；③ 现有 compact 为何无效（**强证据**：7 个备份字节数与主库精确相同，但确切代码路径未核） |
| 敏感信息 | 报告只含行数、字节数、压缩比、耗时、`kind` 枚举值与表/索引名；无正文、无标题、无路径明细、无提示词 |
