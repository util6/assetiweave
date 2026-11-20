# Wave 2 前置考古：会话正文的读写路径与透明压缩落点

> 范围：只读代码考古，未修改仓库任何文件。
> 仓库：`/Users/util6/code-space/assetiweave`，主战场 `backend/asset_core`。
> 结论均带 `文件:行号`；无法从代码确证的一律标「未验证」。
> **单位约定**：`B` = 字节；`MB` = 10⁶ B（十进制，沿用 SQLite/CLI 习惯）；`MiB` = 2²⁰ B。
> 例：205.0 MiB = 214,959,485 B ≈ 215.0 MB。同段混用时以显式换算为准。

---

## 0. 一句话结论

正文 **不是** 集中在一列，而是分散在 **7 个列**里。按实测字节占比（§1.3），主体是
`conversation_parts.content`（72.5%），其次是 `conversation_turns.process` 的 JSON `items[].content`
（14.3%，命令输出——历史迁移把它从 parts 行搬进了 process）。
写入侧有**唯一收口**（`replace_session_snapshot_sqlx_tx`），但读取侧有四类「把正文当 SQL 参数用」的路径
（`instr()` 全表扫、`GLOB`、`json_extract`、`json_remove`），这些是压缩最容易静默出问题的地方。
Tantivy 只索引 prompt / 最终回答 / 变更文件路径 / 命令标签，**不索引命令输出正文**，
所以「大块去重 + 压缩」对检索正确性的影响比直觉小，对读放大的影响比直觉大。

压缩规格上有一条实测结论压过架构直觉：**收益几乎全部来自块大小（MB 级），不是来自轮/会话聚簇**
（轮级 2.83x vs 4 MB 块 4.93x，见 §1.4）。

---

## 1. 正文列盘点

### 1.1 终态物理 Schema（以最后一次重建表的迁移为准）

| 表 | 列 | 类型 | 语义 | 证据 |
|---|---|---|---|---|
| `conversation_turns` | `prompt` | TEXT NOT NULL DEFAULT '' | 用户提问原文（Markdown/纯文本混合） | `migrations/20261029000001_converge_turn_sparse_columns_to_process.sql:18` |
| `conversation_turns` | `process` | TEXT NULL | **JSON**：cwd / title / turn_kind / agent_role / parent_turn_id / response_seq / `items[]`（`items[].content` 是**命令输出正文**） | 同上 `:19`；结构见 `domain/conversations/triad/types.rs:12-35,55-74` |
| `conversation_turns` | `response` | TEXT NULL | 该轮最终助手回答正文（Markdown） | 同上 `:20` |
| `web_record_turns` | `prompt` / `process` / `response` | 同上 | 同上（Web 抓取记录） | 同上 `:78-80` |
| `conversation_parts` | `content` | TEXT NULL | 非命令的中间零件正文：reasoning、code_block、file_change 摘要、被保留的 text | `migrations/20261025000001_turn_triad_and_conversation_parts_slimming.sql:271-280`（8 列终态） |
| `conversation_parts` | `attrs_json` | TEXT NULL | **JSON 属性**，含 `file_path` / `language` / `summary` / `exit_code` / `status` / `cmd_seq` 等；`summary` 是命令的自然语言摘要 | 同上 `:271-280`；键清单见 `:305-345` |
| `conversation_parts` | `translated_text` | TEXT NULL | 正文译文副本（**与 content 等量级**） | 同上 |
| `web_record_parts` | `content` | TEXT NOT NULL DEFAULT '' | 同上（answer / process 两类） | `migrations/20261024000004_web_record_parts_slimming.sql:10-19` |
| `web_record_parts` | `translated_text` | TEXT NULL | 译文副本 | 同上 |
| `conversation_sessions` | — | — | **无正文字段**；只有 `title`/`project_path`/`last_cwd` 等元数据 | `migrations/20260627000003_tenant_scope_conversations.sql:71-88`；`20261024000002_conversation_session_last_cwd.sql` |

关键推论：**`conversation_sessions` 与压缩无关**；正文只有 4 张表、7 个列（含 2 个 JSON 列、2 个译文列）。

### 1.2 `process` 为什么是正文列（不是元数据）

历史迁移把「命令」从 parts 行**折叠进 process JSON**：

- 迁移步骤 2/7 把 `kind='command'` 的 parts 聚合为 `items[]`，字段含 `content`：
  `migrations/20261025000001_...:24-50`（抽取）、`:176-200`（`json_group_array(json_object('content', c.content, ...))`）。
- 步骤 9 把命令行、空壳 result 行从 `conversation_parts` **物理删除**：
  同文件 `:348-357`（`WHERE kind != 'command' ... AND NOT (...)`）。
- 新导入路径同样如此：命令项进 `process.items`，其余进 `retained_parts`：
  `domain/conversations/triad/split.rs:103-112` vs `:203-210`、`:320-323`。

所以 **命令输出在 schema 上被搬进了 `conversation_turns.process`，而不是留在 `conversation_parts.content`**。
但实测（§1.3）推翻了「因此 process 是最大一块」的推断：`process` 只有 33.4 MB，`parts.content` 是它的 5.1 倍。
正确表述是：**`parts.content` 是主体（72.5%），`process` 是次体（14.3%）但必须一起压**——两者形状完全相反，见 §1.4。

### 1.3 实测体量（推翻代码里的上限直觉，也推翻上文的"最大一块"推断）

**实测分布**（来源：res-storage 对现网库的全量只读采样，`02-storage.md` §A.6；我未复现，仅引用）：

| 指标 | `parts.content` | `turns.process` | `turns.prompt` | `turns.response` |
|---|---:|---:|---:|---:|
| 非空行数 | 154,748 | **7,064** | 7,336 | 6,262 |
| 总字节 | **169,955,233** | 33,442,254 | 12,259,869 | 18,632,738 |
| 字节占比（四列分母 234.3 MB） | **72.5%** | 14.3% | 5.2% | 8.0% |
| p50 | 275 | **1,324** | **152** | 1,841 |
| p90 | 2,420 | 12,176 | 5,352 | 6,994 |
| p99 | 12,249 | **49,132** | 15,932 | 13,314 |
| max | 218,555 | 206,621 | 192,144 | 102,715 |

与代码里的上限对照：

| 证据 | 含义 |
|---|---|
| `infrastructure/conversations/types.rs:8-10`：控制行 8MB / 单 item 行 64MB / 总量 256MB | 这是 adapter **协议层天花板**，不是实际分布；实战最长 213 KB，差 300 倍 |
| `search_index_documents.rs:237`：`MAX_SESSION_DOCUMENT_BYTES = 512 * 1024` | 会话级聚合上限 512KB，与「中位数 275 B」并存 → 分布极度长尾 |
| `migrations/20261111000003_conversation_part_turn_indexes.sql:8-9`：「17 万零件」 | 与 154,748 行同量级，互相印证 |
| ADR-0008 | 大体积纯读命令输出已在 adapter 侧前置裁剪；存储层不做二次截断（代码中未见 content 截断） |

设计含义：**块大小不需要为「单行超大堂」留余量**——1 MB 分块已能覆盖实测最大值（213 KB），
且中位数 275 B 意味着「按行压缩 + 每行头部开销」会显著劣化小行（275 B 上挂 20~30 B 头 = 10% 白白浪费）。

⟶ **规格修正（res-storage 全量实测，`02-storage.md` §C.6）：收益来自「MB 级块」，不是「轮/会话聚簇」。**

| 打包方式 | 平均块大小 | 压缩比 |
|---|---:|---:|
| 逐轮打包（prompt+process+response 一轮一条） | 8.8 KB | **2.826** |
| 逐轮打包 + 4 MB 分块拼接 | 4 MB | **4.929** |
| 三列按列分开 + 各自 4 MB 分块（加权） | 4 MB | **5.031** |
| `prompt` 单列 + 4 MB 块 | 4 MB | **8.091** |
| `process` 单列 + 4 MB 块 | 4 MB | 6.642 |
| `response` 单列 + 4 MB 块 | 4 MB | 2.988 |

两条反直觉结论：

1. **2.83x → 4.93x 的跃升全部来自块大小**（8.8 KB → 4 MB）；**轮级只有 2.83x**。
   所以不能写成「压缩单位取轮次级」——那会让人以为收益上限是 2.8x。
2. 轮级聚簇（4.929）vs 列式分离（5.031）**只差 2%**，聚簇方式几乎不重要；但 `prompt` 单列能到 8.09x
   → **内容相似性 ≫ 时间局部性**。

### 1.4 全库 8 列压缩实测（ZSTD，无抽样无外推）

**分母澄清**：下表合计 = **265,644,465 B（253.3 MiB）**，这是「全库全部 8 个正文列」。
§1.3 上表的字节占比是**四列分母**（234,290,094 B）；以全库为分母时 `parts.content` = **63.98%**、
`process` = **12.59%**。两套百分比都对，但必须标分母，否则会被当成矛盾。

| 列 | 原始 B | 压缩后 B（全局 4 MB 块） | 压缩比 |
|---|---:|---:|---:|
| `parts.content` | 169,955,233 | 31,576,779 | 5.382 |
| `parts.attrs_json` | 9,838,886 | 539,986 | **18.221** |
| `web_record_parts.content` | 9,833,638 | 1,380,629 | 7.123 |
| `turns.prompt` | 12,259,869 | 1,515,257 | **8.091** |
| `turns.process` | 33,442,254 | 5,034,595 | 6.642 |
| `turns.response` | 18,632,738 | 6,236,882 | 2.988 |
| `wr_turns.response` | 10,651,083 | 4,057,835 | 2.625 |
| `wr_turns.prompt` | 977,609 | 342,408 | 2.855 |
| **合计（理论上限）** | **265,644,465** | **50,684,980** | **5.241（省 80.92% = 205.0 MiB）** |

> ⚠️ **全局 4 MB 块是理论上限、不可直接采纳**：它跨会话拼块，一次 37 KB 小会话的整包替换要重写 4 MB，
> **WAL 放大 30~100 倍**。真正可采纳的是「按会话边界封块 + 1 MB 上限」（见下表）。

**块大小 → 压缩比曲线**（`parts.content` 全量，只变块大小）：

| 块大小 | 256 KB | 1 MB | 4 MB | 16 MB |
|---|---:|---:|---:|---:|
| 压缩比 | 4.676 | 5.144 | 5.382 | 5.451 |

**会话封块实测**（上限 1 MB）：**4.661x**，1,693 个会话只产生 1,713 块（仅 20 个大会话被切分）。
会话体量分布：p50 **36.6 KB**、p90 247 KB、p99 1.07 MB、max 2.50 MB。

| 方案 | 压缩比 | 省下 | 写放大 | 可否采纳 |
|---|---:|---:|---:|---|
| 全局 4 MB 块 | 5.382 | 205.0 MiB | 30~100x | ❌ 理论上限 |
| 全局 1 MB 块 | 5.144 | ~196 MiB | 仍跨会话 | ❌ |
| **会话封块 + 1 MB 上限** | **4.661** | **200.3 MiB** | **≈1.0x** | ✅ **推荐** |

取舍：压缩产物比理论上限大 9.6%，但**净收益只少 4.7 MiB（205.0 → 200.3）**，换来写放大回到约 1.0×。
主库推演（res-storage 口径）：VACUUM 后 388 MB → **≈191 MB**；逐行模型 209 MB（8.2x）。
（注：按 388 MB − 200.3 MiB 反算约 178 MB，与 191 MB 有 ~13 MB 差，属 02 内部推算口径差异，**以 02 的 191 MB 为准**，此处不复算。）

> ⚠️ **适用边界**：以上压缩比全部测自**ASCII 为主**的语料（全库 B/char=1.128，`parts.content` 1.08、`process` 1.04）。
> 高压缩比来自「结构化 ASCII 冗余」，不是中文文本冗余。中文为主的语料需重测，详见 §1.7。

### 1.5 两列形状差异只在「逐行模型」下产生分支

| | `process` | `parts.content` |
|---|---|---|
| <1 KB 行占字节 | **2.6%** | 18.4% |
| >4 KB 行占字节 | 85.1%（16–64KB 桶独占 39.2%） | — |
| <256 B 行 | 28.6% 的行 / 0.43% 的字节 | 48.4% 的行 / 5.4% 的字节 |
| **逐行模型** | 可直接压 | **必须按 256 B 阈值分流**（单压仅 1.09x） |
| **块级模型（本项目）** | 同一套策略 | 同一套策略；小行混进 4 MB 块，**不拉低压缩比反而略升** |

res-storage 的验算：块级下强行剔除那 9.18 MB 小行，产物只从 31.58 MB 降到 ~30.02 MB —— **为省 1.5 MB 引入分流层不值**（`02-storage.md` §C.7）。
→ 结论：**块级模型下只有一套策略，两列的差异只体现在压缩比（6.64x vs 5.38x）。**

另外 `prompt` 的正解不是压缩而是去重：p50 仅 152 B、**去重率 36.7%（全库最高）**、4 MB 块压缩比 8.09x（全库最高）。

### 1.6 「日均 3 亿 Token」的口径分岔（立项前必须问清，直接影响技术选型）

**实测锚点**（res-storage 只读连接，按列给出 `length(TEXT)` 字符数 vs 字节数）：

| 列 | chars | bytes | **B/char** | 判读 |
|---|---:|---:|---:|---|
| `parts.content` | 157,274,017 | 169,955,233 | **1.08** | 纯 ASCII 级 |
| `parts.attrs_json` | 9,761,456 | 9,838,886 | **1.01** | 纯 ASCII |
| `turns.process` | 32,033,414 | 33,442,254 | **1.04** | 纯 ASCII（命令输出） |
| `wr_parts.content` | 8,720,449 | 9,833,638 | 1.13 | 基本 ASCII |
| `turns.prompt` | 9,613,472 | 12,259,869 | 1.28 | 少量 CJK |
| `turns.response` | 12,019,155 | 18,632,738 | **1.55** | 有 CJK |
| `wr_turns.response` | 5,494,063 | 10,651,083 | **1.94** | CJK 占比最高 |
| `wr_turns.prompt` | 635,634 | 977,609 | 1.54 | 有 CJK |
| **全库** | **235,604,815** | **265,644,465** | **1.128** | ASCII 为主 |

`B/char = 1.128` 距纯 ASCII（1.0）只差 13%，距「中英各半」（约 2.0）很远 →
**语料是英文/代码为主，中文只集中在两个 `response` 列**。（这推翻了我上一版"混合中英"的假设。）

| 换算假设 | 系数 | 全库 token |
|---|---|---:|
| 英文/代码为主 | 4.0 chars/token | 5,890 万 |
| 折中（response 列 CJK 拉低） | 3.5 chars/token | 6,730 万 |
| 保守上界 | 3.0 chars/token | 7,850 万 |
| 极上界 | 3 bytes/token | 8,855 万 |

→ **规划取值 ≈6,000 万；区间 5,900 万 ~ 8,900 万**（真实值应贴下沿，因为 B/char=1.128）。
对分词器敏感，**不给点值**：真实 5,900 万而按 8,900 万规划 → 低估增速 34%；反之高估 51%，两边都不安全。

**关键**：以 6,000 万为分母，「日均 3 亿 Token」= **每天重写全库 5 遍**。而现网库只装了约 6 小时的历史量——
若"3 亿"真是落盘口径，产品早就撑不住了。因此它极可能是**流量口径**，存在两种读法，技术选型完全相反：

| 读法 | 含义 | 真正要回答的问题 | 对本报告结论的影响 |
|---|---|---|---|
| **A. 落盘吞吐** | 每天真实写入库 3 亿 token | **写放大 + WAL 增长 + fsync**，而非压缩比 | 压缩是 P1/P2；但压缩的整块重写会**加剧**写放大 |
| **B. 上下文吞吐** | LLM 上下文流量（含同一会话重复投喂） | 存储不是瓶颈；真问题是索引泄漏 22.68 GiB | 压缩收益仅 205 MiB，**可以不排期** |

**假设 A 下的容量推算（必须按区间读，单点值无意义）**：

| | 低界假设（8,900 万 token，3.4x/天） | 高界假设（5,900 万 token，5.1x/天） |
|---|---:|---:|
| 原始增长 | 0.90 GB/天 ≈ 328 GB/年 | **1.35 GB/天 ≈ 494 GB/年** |
| 逐行 Zstd | 0.34 GB/天 ≈ 125 GB/年 | 0.52 GB/天 ≈ 189 GB/年 |
| 块级 Zstd | 0.17 GB/天 ≈ 63 GB/年 | 0.26 GB/天 ≈ 94 GB/年 |

→ **本条为推断，必须由 team-lead 向需求方确认**（一句话即可："3 亿 Token"指每天新落盘的正文量，还是 LLM 上下文吞吐？）。
→ 引用 res-storage `02-storage.md` §E.4 的推算时**必须带"假设 A"前缀，且引用区间而非单点**。

### 1.7 压缩比的适用边界（容易被忽略，但影响 P2 的核心数字）

§1.4 的 5.38x / 6.64x / 全库 5.241x **全部是在「ASCII 为主的语料」上测的**。
`parts.content`（B/char=1.08）与 `turns.process`（1.04）的高压缩比，本质是**结构化 ASCII 冗余多**，
不是"中文文本冗余"。若 Wave 2 用户群转为中文为主（B/char 升到 2.5+），这两列的压缩比会明显下降，
**205.0 MiB 的估算会变成高估**。

→ 结论仍成立（压缩该做、块级优于逐行），但**收益数字需要按目标用户的语言构成重测**；
若无法确定，建议在方案里把收益写成区间而不是"省 80.92%"。

---

## 2. 写入路径

### 2.1 唯一写入漏斗（正文事实）

`replace_session_snapshot_sqlx_tx`（`store/conversations/snapshot.rs:431-470`）→
`insert_turn_sqlx_tx`（`:499-528`，写 `prompt`/`process`/`response`）→
`insert_parts_sqlx_tx`（`:530-580`，写 `content`/`attrs_json`/`translated_text`）。
整包语义：先 `DELETE` parts 再 `DELETE` turns 再逐条 insert（`:440-467`）。

| # | 场景 | 位置 | 说明 |
|---|---|---|---|
| W1 | 适配器批量同步 | `conversation_repo/import.rs:130-157`（prepare → upsert session → replace） | 会话级整包替换；`CONVERSATION_IMPORT_BATCH_SIZE = 8`（`conversation_repo/common.rs:3`） |
| W2 | 单会话导入 | `conversation_repo/import_single.rs:37-60` | 同上，单条路径 |
| W3 | Web 记录导入 | `web_record_repo/import.rs:68` | 同一 replace 函数，`kind = Web` |
| W4 | 重新投影 / 重解析 | `conversation_repo/reproject.rs:297`、`:353-366` | 先 `load_existing_session_snapshot_sqlx_tx` 读旧正文，再整包重写 |
| W5 | 子代理折算修复 | `conversation_subagent_fold.rs:319`、`:328` | 冷路径：读全量 turns/parts → 重新组装 → 整包替换 |
| W6 | 迁移脚本（raw SQL） | `migrations/20261025000001_...`、`20261024000003/0004_*_slimming.sql` | **绕过 Rust 层**直接 `SELECT content ... INSERT content`，新装环境会写入明文 |

### 2.2 非快照的正文改写（read-modify-write，最脆弱）

| # | 位置 | 说明 |
|---|---|---|
| W7 | `conversation_repo/triad_update.rs:74` | `UPDATE {table} SET process = ?1`——为写一条译文，把整段 process JSON 在 Rust 里反序列化后整体覆写 |
| W8 | `conversation_repo/projection.rs:12-24` | `UPDATE conversation_parts SET translated_text = ?1 WHERE id_key = ?3` |
| W9 | `web_record_repo/list.rs:389-405` | `UPDATE web_record_parts SET translated_text = ?1` |
| W10 | `store/database.rs:257-264` | `UPDATE conversation_parts SET attrs_json = json_remove(attrs_json, '$.has_command') WHERE ... json_extract(attrs_json,'$.has_command') = 0 AND content IS NOT NULL`——**SQL 侧 JSON 函数 + `content IS NOT NULL` 判空** |
| W11 | `store/path_convergence.rs:101-104,156` | 启动期路径收敛：`SELECT rowid, process FROM ... WHERE process GLOB '*"cwd":"/*'`，Rust 反序列化 → `UPDATE ... SET process = ?1` |

---

## 3. 读取路径（分类）

### 3.1 分类总表

| 类别 | 入口 | 读到的列 | 证据 |
|---|---|---|---|
| **A. 详情页 / 卡片下钻（按 id 单条）** | `load_conversation_block_detail_sqlx` | parts.content/attrs_json/translated_text | `conversation_repo/block_details.rs:129-147`（`WHERE p.id_key = ?2`）；question 分支 `:94-118` |
| **A. 详情页 / 卡片下钻** | `load_conversation_question_detail_sqlx` | **整个会话**的 turns + parts | `conversation_repo/content_details.rs:39-115`；实际加载 `projection.rs:119-215` |
| **A. 详情页** | `load_conversation_session_detail_sqlx` | 整会话 turns + parts | `conversation_repo/sessions.rs:314-340` |
| **A. 详情页** | `load_web_record_session_detail_sqlx` | 整会话 turns + parts | `web_record_repo/list.rs:266-330` |
| **B. 列表 / 大纲** | `load_conversation_session_list` 系列 | 只取 `t.prompt`（最新/窗口首尾），不取 parts | `conversation_repo/sessions.rs:56-70,88-120`（`latest_user_prompt`、`window_*_user_prompt`） |
| **B. 列表 / 大纲** | CLI `outline` | **间接读整会话全部正文**，只为算 `content.chars().count()` | `application/conversations/conversation_records/outline.rs:145-151` → 同文件 `:206`、`:228`；`content_chars` 字段 `:167` |
| **C. 全文搜索（索引重建）** | `load_conversation_search_index_documents_inner` | 整库/整会话 `prompt`+`process`+`response`+`parts.content`+`attrs_json` | `store/conversations/search_index_documents.rs:280-307`（turns）、`:318-359`（parts）；语料抽取 `:141-148` |
| **C. 全文搜索（日常检索·Tantivy）** | `hydrate_conversation_search_matches_sqlx` | 命中会话的 turns + parts（拼摘要/高亮） | `conversation_repo/search_cards.rs:215-330`；摘要生成 `search_snippets.rs:36,106-135` |
| **C. 全文搜索（索引不可用回退）** | `search_conversation_cards_sqlx` | **本租户全部** sessions/turns/parts，逐行 `to_lowercase().contains()` | `conversation_repo/search_cards.rs:4-101`；`search_snippets.rs:36` |
| **C. 全文搜索（会话级回退）** | `search_session_ids_by_corpus_sqlx` | SQL 侧 `instr(lower(prompt/response/content/attrs_json))` | `search_index_documents.rs:366-464`，具体 `:436,446-447,454` |
| **D. 备份导出** | 数据库快照 | 整库文件 `VACUUM INTO` | `infrastructure/backup/mod.rs:157-175` |
| **D. 内容导出** | `export` / `export_raw` | 经 session detail 取投影后的正文 | `application/.../export.rs:26`、`:243-290`；`export_raw.rs:113-115` |
| **E. 用量统计** | `load_valid_turn_indices_sqlx` | **不读正文**，只 `turn_index` | `store/conversations/usage_repo.rs:541-557` |
| **F. Memory 投影** | `load_recall_parts_for_questions_sqlx` | `p.content` + `json_extract(attrs_json, ...)`，并在 SQL 里 `COALESCE(json_extract(...), p.content)` | `store/memory/memory_recall_query_repo.rs:229-252` |
| **F. Memory 召回时间线** | `RECALL_UNITS_SQL` | 不读正文，只 `started_at`/`imported_at` | `store/memory/memory_recall_query_repo.rs:13-50` |
| **F. Memory 追问文本** | `load_memory_recall_turn(s)_sqlx` | `COALESCE(t.prompt,'')` | `store/memory/memory_recall_repo/turns.rs:40-47,66-73` |
| **G. CLI** | `aiwc conversation outline/card get/search/list` | 全部落到上面 A/B/C 的 store 函数 | `backend/cli/src/service/conversation.rs:167-191`（outline）；CLI 层无直接 SQL（`grep conversation_parts` 在 `backend/cli/src` 无命中） |
| **H. 维护/自愈** | `conversation_maintenance_ops.rs` | `json_extract(p.attrs_json,'$.exec_id')` 全表 GROUP BY | `application/conversations/conversation_maintenance_ops.rs:48` |
| **H. 启动期路径收敛** | `converge_json_path_column` | `process` / `attrs_json` GLOB + Rust 反序列化 | `store/path_convergence.rs:95-142` |
| **H. 子代理折算候选筛选** | `load_candidate_sessions` | `t.process LIKE '%"turn_kind"%'` | `store/conversation_subagent_fold.rs:265-275` |

### 3.2 重点排查结论：**有**绕过 Tantivy 的正文全表匹配

不是 SQL 层的 `LIKE '%xx%'`，而是更隐蔽的两种：

1. **SQL `instr()` 全表扫**（真正的正文子串匹配）
   `store/conversations/search_index_documents.rs:441-458`：
   ```
   instr(lower(t.prompt), ?2) > 0 OR instr(lower(COALESCE(t.response,'')), ?2) > 0
   ...
   instr(lower(COALESCE(p.content,'')), ?2) > 0 OR instr(lower(COALESCE(attrs_json,'')), ?2) > 0
   ```
   触发条件：Tantivy 不可用 / 未就绪 → `search_conversation_session_ids` 返回 `None` →
   `application/conversations/conversation_records/sessions.rs:395-403` 走该回退。
   **这是压缩后第一个会静默返回空结果的地方**（BLOB 上 `lower()`/`instr()` 不报错，只是匹配不到）。

2. **Rust 层全表 `contains`**（`legacy_scan`）
   `conversation_repo/search_cards.rs:68-101` 加载本租户全部 sessions/turns/parts，
   在 `search_snippets.rs:36` 做 `text.to_lowercase().contains(needle)`。
   压缩后如果 repo 层不透明解压，这里同样静默失配。

3. **SQL `GLOB` 打在 JSON 正文上**（不是搜索，但同样是正文参与模式匹配）
   `store/path_convergence.rs:101-104`：`WHERE process GLOB '*"cwd":"/*'`。
   压缩后该行不再匹配 → 路径收敛静默跳过（不会报错，但也不会生效）。

4. **SQL `LIKE` 打在 process 正文上**
   `store/conversation_subagent_fold.rs:270`：`WHERE t.process LIKE '%"turn_kind"%'`。

除此之外，**没有**发现 `LIKE '%...%'` 直接打在 `content`/`response` 上的生产代码
（`snapshot_tests.rs:303,315` 是测试）。

---

## 4. 透明压缩收口点评估

### 4.1 当前正文的 Rust 类型

| 层 | 类型 | 证据 |
|---|---|---|
| 行映射（parts） | `content: Option<String>`、`attrs_json: Option<String>`、`translated_text: Option<String>` | `conversation_repo/row_mappers.rs:359-367` |
| 行映射（turns） | `prompt: String`、`process: Option<String>`、`response: Option<String>` | `conversation_repo/row_mappers.rs:307-319` |
| 领域（part） | `text: Option<String>`（+ `metadata_json`/`translated_text`） | `domain/conversations/mod.rs:421-441` |
| 领域（triad） | `TriadPart.content: Option<String>`；`TurnProcessItem.content: Option<String>` | `domain/conversations/triad/types.rs:79-85`、`:55-74` |

全部是 **`String` / `Option<String>`**，没有任何自定义 newtype、没有 `serde_json::Value` 直接落库
（`process`/`attrs_json` 虽为 JSON，但在 Rust 侧始终以 `String` 传层，只在个别位置 `from_str`）。
→ 加一层 `String` 兼容的新类型（`Type<Sqlite>` + `Decode`/`Encode`）在类型系统上是可行的，
但**大量查询用匿名元组 `query_as::<_, (String, i64, String, Option<String>, ...)>` 直接取列**，
例如 `conversation_repo/search_records.rs:146-155`、`snapshot.rs:264-273`、`search_index_documents.rs:309-316`、
`conversation_subagent_fold.rs:459-470`。改 newtype 必须逐个改这些元组类型，否则编译不过——
这反而是**优点**（编译器会逼你改全），但改动面比想象大。

### 4.2 三种插入层对比

| 方案 | 覆盖面 | 优点 | 致命缺点 |
|---|---|---|---|
| **(a) sqlx `Decode/Encode` newtype**（如 `BodyText`） | 所有 `query_as` / `bind` 自动 | 编译器强制、无遗漏；读写对称 | ① 迁移脚本是 raw SQL，**新装/升级路径写入的仍是明文**，必须做「双读」兼容；② 所有元组类型要改；③ 列表查询里不需要正文的列也会被解码（CPU 浪费）；④ 一旦有人写 `SELECT content` 到 `String` 就会运行期类型错配 |
| **(b) repository 层手写编解码** | 写入 `insert_parts_sqlx_tx`/`insert_turn_sqlx_tx` + 读取各 mapper | 精准可控；能对单列决策（例如只压 `content`、`process` 保持明文 JSON）；能做「读时判前缀」兼容明文 | 遗漏 = 静默损坏。当前正文读取点 ≈ 20 处（§3），其中 `query_as` 元组 6 处、`try_get` 逐列 4 处 |
| **(b') 混合：JSON 内部**只压 `items[].content` / `$.content` 字段，JSON 外壳保持合法 | `process`、`attrs_json` | `json_extract(...,'$.cwd')` / `'$.turn_kind'` / `GLOB '*"cwd":"/*'` / `LIKE '%"turn_kind"%'` **全部继续可用** | 只能压「值」不能压结构；`COALESCE(json_extract(...), p.content)` 这类跨列表达式仍需特判 |
| **(c) domain 投影层** | `ConversationPart.text` 等 | 与存储解耦 | **覆盖不了写入**（import 直接写 String）；且 Tantivy 文档/导出/Memory 都从 store 行取，会绕过投影层重复解码 |

**判断**：domain 层（c）不可行；sqlx newtype（a）最安全但被迁移脚本的明文数据打穿；
repository 层（b/b'）与现有代码结构最匹配——因为写入已收敛到 `replace_session_snapshot_sqlx_tx` 一个漏斗。

### 4.3 必须解压的地方（正文流向 Tantivy / 前端）

| 目的地 | 位置 | 说明 |
|---|---|---|
| Tantivy 索引 | `search_index_documents.rs:141-148` → `infrastructure/search/conversation/engine.rs:300-301` | `content_zh`/`content_en` 直接吃 `item.content`；字段是 **仅索引不 STORED**（`schema.rs:72-73`，非 `STORED`） |
| 前端摘要/高亮 | `search_snippets.rs:36,106-135` | 从 SQL 重新读回正文再切窗口 |
| CLI/前端卡片正文 | `ConversationCardDetail.content`（`domain/conversations/outline.rs:246`）、`ConversationBlockDetail.content`（`block_details.rs:207-234`） | 逐字返回给终端/UI |
| 导出 | `export.rs:243-290`、`export_raw.rs:113-115` | 直接 `fs::write` 正文 |
| Memory 投影 | `memory_recall_query_repo.rs:229-252` | `text` 列内容会进 LLM prompt |

---

## 5. 压缩会打破什么（正文被当作 SQL 参数）

按「静默失败」到「硬报错」排序：

| # | 位置 | 用法 | 压缩后行为 |
|---|---|---|---|
| B1 | `search_index_documents.rs:446-454` | `instr(lower(prompt/response/content/attrs_json), needle)` | **静默返回空**（BLOB 上 `lower()`/`instr()` 不匹配）→ 检索退化为「什么都没搜到」 |
| B2 | `search_cards.rs:68-101` + `search_snippets.rs:36` | Rust `text.to_lowercase().contains()` | 若 repo 未解压，同样静默空结果 |
| B3 | `path_convergence.rs:101-104` | `process GLOB '*"cwd":"/*'` | 静默不收敛；且 `row.try_get::<String>`（`:114`）遇 BLOB 会**硬报类型错**，启动失败 |
| B4 | `conversation_subagent_fold.rs:270` | `t.process LIKE '%"turn_kind"%'` | 静默漏掉候选会话 → 折算修复不生效（幂等，但不修复） |
| B5 | `memory_recall_query_repo.rs:232-248` | `COALESCE(json_extract(attrs_json,'$.summary'), p.content)` | `json_extract` 返回 NULL → 落到 BLOB 的 `p.content`，**把二进制塞进 LLM prompt** |
| B6 | `conversation_maintenance_ops.rs:48` | `json_extract(p.attrs_json,'$.exec_id') IS NOT NULL` + GROUP BY | 返回 NULL → 重复 exec_id 检测恒为 0（静默失效） |
| B7 | `database.rs:257-264` | `json_remove(attrs_json,'$.has_command')` + `content IS NOT NULL` | `json_remove` 对非法 JSON 报错；`content IS NOT NULL` 对 BLOB 恒真 → 自愈逻辑误判 |
| B8 | `triad_update.rs:74` | 整段 `process` 反序列化 → 覆写 | 若 process 被压缩：解析失败 → 译文写入丢失（无错误码） |
| B9 | `snapshot.rs:389,406,417` | Rust 侧 `old_item.content == item.content`、`op.content == part.content` | 压缩若发生在比较之前（如按密文比较），译文迁移会全部失配（**不崩溃、只是译文丢失**）；这是内存比较，只要两边同源解码即可 |
| B10 | `projection.rs:328`、`block_details.rs:201`、`outline.rs:206,228` | `content.chars().count()` | 必须基于解码后字符串；否则「字符数」变字节数，UI 的折叠阈值全错 |
| B11 | 迁移脚本 | `trim(content)`、`content IS NULL OR trim(content)=''`、`json_extract(...)` | 新装环境重放迁移时，如果历史数据已是密文，这些 `WHERE` 会误判（**当前不存在该场景**，但要保证「压缩只作用于运行时新写入」） |

另有两点非 SQL 破坏：

- **优化器/索引**：正文上没有索引、没有 UNIQUE、没有 `ORDER BY content`、也没有 `LENGTH(content)`
  （全仓 grep `LENGTH(` 仅命中 `sessions.rs:256-280` 的 `updated_at`）。→ 压缩本身不破坏任何查询计划。
- **备份语义**：App 内置备份是 `VACUUM INTO` 优先 + `checkpoint_and_copy` 兜底
  （`infrastructure/backup/mod.rs:154-176`（快照编排）、`:179-189`（VACUUM INTO）、`:191-200`（裸拷贝兜底），
  命名规则 `assetiweave-app-<ts>-<uuid8>.db`，`:204-208`）。
  `VACUUM INTO` 会**重建库**，因此压缩写入的行在备份里仍是压缩态、语义一致；
  但兜底的 `checkpoint_and_copy` 是裸文件拷贝，两者产物无法区分。
  旧版本 App 读新备份会看到 BLOB（是否接受：产品决策，未验证）。

### 5.1 明文落盘位置清单（压缩/加密方案必须一起处理）

这一节超出「SQL 列」范围，但直接决定压缩能带来多少实证收益。数据来源为 res-storage 的磁盘实测（`02-storage.md`），我未复现。

| 位置 | 实测 | 与压缩的关系 |
|---|---|---|
| 备份目录 | 7 份库文件，**每份精确 = 1,720,578,048 B**（与主库一字不差），合计 11.21 GiB；命名 `app.db.bak_*` / `bak_data_clean_*` / `bak_pathfix_*` / `bak_cwdfix_*` / `bak_compact_*` | **不符合 App 备份命名规则**（`:204-208`），说明是人工/修复脚本 `cp` 出来的裸副本。**只压主库不会让这 11 GiB 变小**，压缩的真实收益要先扣掉这部分 |
| Tantivy 索引目录 | `conversation-search-index` 24.69 GiB，其中 22.68 GiB 是 81 个 `generation-*.tmp` 废弃代数，从未清理 | 索引里是分词 term + 位置（`schema.rs:62-74` 只索引不 STORED）。`.store` 段仅 2.8 MiB（证明正文原文确实没进索引），**但 22.68 GiB 的 `.term`/`.pos`/`.idx` 保留了历史正文的倒排与位置信息——不是原文，足以还原大量词项**。压缩/加密都不会自动回收它 |
| 主库 | 1.6 GiB（VACUUM 后 388 MB；会话封块压缩后 ≈ **191 MB**） | 压缩目标；但只占全局 4.3% |

推论：**「正文压缩」只能解决主库里那部分；磁盘占用的大头目前在废弃索引代数（22.68 GiB）与裸副本（11.21 GiB）上。**
如果 Wave 2 的目标是「3 亿 Token/天可持续」，必须先回答这两块归谁管（清理策略 vs 压缩）——
否则正文压到 191 MB，机器占用仍从 ~37.1 GiB（= 48.3 − 11.2，扣除可确认的无用副本）起步。
**建议验收口径写成分账：索引泄漏 22.68 GiB / 裸副本 11.21 GiB / 主库 1.60 GiB → 191 MB，三笔分开报，不加总。**

### 5.2 块级压缩的唯一硬约束：原地 UPDATE 会放大成整块重写

这是块级方案相对逐行方案的**唯一结构性代价**。
把正文写入按「是否原地改写」分类后，结论是**本项目对块级友好，写放大不是硬阻断**：

| 写入模式 | 位置 | 对块级的影响 |
|---|---|---|
| **会话整包 DELETE + INSERT**（主流） | `snapshot.rs:440-467`；调用方 6 处（§2.1 W1-W5） | **天然友好**：整会话重写，块可整段重算，不需要 slack 也不需要 COW |
| 原地 `UPDATE translated_text` | `conversation_repo/projection.rs:12-24`、`web_record_repo/list.rs:389-405` | 不友好，但**实测这两列是空的**（见下），不纳入块级即可 |
| 原地 `UPDATE attrs_json = json_remove(...)` | `store/database.rs:257-264` | 一次性自愈（checksum 匹配才跑），低频 |
| 原地 `UPDATE process = ?1`（整段 JSON 覆写） | `conversation_repo/triad_update.rs:74` | 同上，改 process 列、体量更大 |
| 启动期路径收敛原地改 JSON | `store/path_convergence.rs:107-142,150-167` | **全表 GLOB 遍历式**，冷路径但会触发大量块重写 |

**折中是零成本的（实测支撑）**：`translated_text` 两列合计只有 **4 行非空 / 7,031 B（6.9 KiB）**
（res-storage `02-storage.md` §C.5.1）。在 160,465 行里只有 3 行有译文 → **把这一列留在逐行或明文，等于不付任何代价就消掉 2 处高频原地 UPDATE**。
顺带一个产品信号：**翻译功能实际上从未被大规模使用**；若 Wave 2 要正式上翻译，这一列会从 6.9 KiB 涨到几十 MB，届时再决定是否纳入块级。

**`path_convergence` 可以直接复用仓内已有的「一次性标记」范式**：
`conversation_subagent_fold.rs:19,53,146-178,204-222` 已把 `settings_id = conversation.subagent_session_fold_v1`
写进 `app_settings`，启动时只查一行就短路。`converge_portable_paths` 目前没有这道闸门，
照抄这个范式加一个 `conversation.path_converged_v1`，就能把唯一一处「冷路径全表块重写」彻底消掉。

**但照抄时必须改一个地方（比事务边界重要得多）**：`subagent_fold` 的标记语义是「**数据代际**」
（`FoldMarker.data_generation = (session_count, latest_imported_at, source_revisions)`，`:163-178` 要求 `recorded == current`），
而路径收敛的触发条件是「**这一版代码跑过没有**」，不是「数据变没变」。
所以正确做法是用 **`app_settings.schema_version` 做版本闸门**，而不是照抄 `data_generation`。
否则将来往扫描列表里加一个新列（例如把 `last_cwd` 也纳入归一化），老安装的 v1 标记会**永久跳过新列**——
这是这套范式唯一真正的坑，且与事务无关。

事务边界方面 res-storage 已核（`02-storage.md` §C.5.1）：标记在 `tx.commit()` 之后单独写（`:146-148`），
靠「**标记最后写 + 函数幂等**」保证安全（崩在写标记前只会多跑一次，不会漏跑）。我原先标的「未验证」据此撤销。

### 5.3 A 假设下的写路径账（压缩是净收益，不是风险）

我上一版判断「1 GB/天 + 整块重写 ⇒ CPU 与 WAL 要重算」。res-storage 实测后结论**方向相反**，但前提必须写清：

| 项 | 量级 | 判定 |
|---|---|---|
| 压缩 CPU | 1.35 GB/天 ÷ 345 MiB/s ≈ **4.0 秒/天** | 可忽略（<0.01% 单核） |
| **WAL 流量** | 写进 WAL 的是**压缩后**字节：1.35 GB 原始 → **≈55 MB/天** | **降 ~24 倍，不是升高** |
| 解压 CPU | 0.0144 ms/条 × 当日读取行数 | 与查询次数同阶 |

**关键洞察：压缩减少 WAL，而不是增加 WAL**，因为落盘的是压缩后字节。
**唯一硬约束：块必须按会话边界封、单块上限 1 MB**——块就是该会话自己的数据，替换时写的字节数 ≈ 原始字节数，写放大 ≈1.0×。
若误用全局 4 MB 块，37 KB 小会话的整包替换要重写 4 MB，**WAL 放大 30~100 倍**（这是不可采纳的唯一原因）。

→ 结论：**不建议为写放大退回逐行模型**——会话封块下写放大≈1.0×，只是压缩比从 5.382 降到 4.661（少赚 4.7 MiB）。

---

## 6. 「压缩层落点」候选判断

### 候选 0：分三期切，第一刀只做 `parts.content`（风险最低，拿 64.4% 收益）

各列对总收益 **214,959,485 B（205.0 MiB）** 的贡献（res-storage 全量实测，比例为复算后的值）。

> **测量口径**：本表是**单列独立测量**（每列单独按 **全局 4 MB 块**压缩），列间可相加等于总收益，
> 因为各列独立压缩、无跨列窗口共享——这也是全库加权 5.241x 与各列压缩比不同却仍能对上的原因。
> **但全局 4 MB 是不可采纳的理论上限**（跨会话拼块 → WAL 放大 30~100 倍）。
> 可采纳的「会话封块 + 1 MB 上限」总收益为 **200.3 MiB**（`parts.content` 4.661x），
> 各列占比近似不变但**未按会话封块重测**，此处标「未验证」。


| 列 | 压缩比 | 省下 | 占总收益 | 期次 |
|---|---:|---:|---:|---|
| `parts.content` | 5.382 | 138,378,454 | **64.37%** | 一 |
| `turns.process` | 6.642 | 28,407,659 | 13.22% | 二 |
| `turns.response` | 2.988 | 12,395,856 | 5.77% | 三 |
| `turns.prompt` | 8.091 | 10,744,612 | 5.00% | 三 |
| `parts.attrs_json` | 18.221 | 9,298,900 | 4.33% | 二 |
| `wr_parts.content` | 7.123 | 8,453,009 | 3.93% | 一 |
| `wr_turns.response` | 2.625 | 6,593,248 | 3.07% | 三 |
| `wr_turns.prompt` + `wr_turns.process` | — | 687,747 | 0.32% | 三 |
| **合计** | — | **214,959,485** | 100% | — |

分期建议（已与 res-storage `02-storage.md` §E.1 对齐，三期精确 100.00%）：

| 期 | 范围 | 收益 | 累计 | 风险 |
|---|---|---:|---:|---|
| **一** | `parts.content` + `wr_parts.content` | **146,831,463 B / 68.31%** | 68.31% | 最低：不碰 `turns.process` 整段 JSON 覆写（`triad_update.rs:74`），不碰 JSON 侧破坏（B4/B6/B7） |
| **二** | `turns.process` + `parts.attrs_json` | 37,706,559 B / 17.54% | 85.85% | 中：需处理 `triad_update.rs:74`、`path_convergence.rs`、B6 |
| **三** | 其余 4 项（`prompt`/`response`，含 web 侧） | 30,421,463 B / 14.15% | **100.00%** | 低：无原地更新；但 `prompt` 应先做去重（36.7%） |

**为什么把 `wr_parts.content` 放一期**（不是按体量，是按代码结构）：
`insert_parts_sqlx_tx`（`store/conversations/snapshot.rs:529-580`）收的是 `RecordTables`，
**同一个函数同时服务 `conversation_parts` 和 `web_record_parts`**，两条 SQL 分支只差 `tables.parts_have_attrs` 一个布尔。
也就是说 `content` 的 codec 挂点**物理上就在同一个函数的两个 `bind` 上**，不是"路径相似"。
放一期 = 写一遍 codec；拆到三期 = 回头再开一次工。三期合计均为 214,959,485 B，切法不改变总收益。

只做第一期主体 `parts.content` 一列 = **138.4 MB / 64.4%**（全局 4 MB 口径；会话封块下约 4.661x，比例近似不变），
且完全绕开 `process` 风险。
→ 建议把第一期作为 Wave 2 的**第一刀**：收益最大、关联风险最少、回滚最容易。

### 候选 1：写入漏斗编码 + repo 层解码（推荐）
**落点**：`snapshot.rs:499-580`（两个 insert）+ `row_mappers.rs:359-367` / `snapshot.rs:264-273` / `search_records.rs:146-155` 等读点。
- 优点：与「正文写入已收敛」这一事实完全吻合（§2.1）；可以逐列决策。
  按 §1.4 的实测收益排序，性价比最高的做法是 **`attrs_json`（18.2x，仅 9.4 MB → 0.54 MB）+ `content`（5.38x）+ `process`（6.64x）一起做**；
  `response` 只有 2.6~3.0x，可以最后做或不做。
- 风险：约 20 个读点漏一个就静默损坏；需要「明文/密文双读」前缀协议（迁移脚本写的是明文）。

### 候选 2：`process` / `attrs_json` 内部字段级压缩，JSON 外壳保持合法
**落点**：`snapshot.rs:163-167`（process 序列化前）+ `fold_part_attrs`（`snapshot.rs:619-646`）；读取侧在 `serde` 层。
- 优点：`json_extract('$.cwd'|'$.turn_kind'|'$.exec_id')`、`GLOB '*"cwd":"/*'`、`LIKE '%"turn_kind"%'` **全部继续工作**；
  且 `process` 是「天然可压」的列（85.1% 字节在 >4 KB 行里，<1 KB 只占 2.6%），单列 4 MB 块可达 **6.64x**（§1.4）。
- 但**覆盖面只有 12.6%**（33.4 MB / 265.6 MB）——单独做它，收益上限就是全库正文的 1/8。
- 反向优势：`process` 的 `attrs_json` 同类（JSON 属性列）压缩比高达 **18.2x**，是最便宜的一块（9.4 MB → 0.54 MB）。
- 风险：压缩粒度是「字符串值」，需要给每个被压字段打标记（例如 `{"content":"~z:<b64>"}`）；`json_group_array` 的迁移路径要重算。
  且 `COALESCE(json_extract(...), p.content)`（B5）仍在 `p.content` 上翻车，需单独处理。

### 候选 3：sqlx `Decode/Encode` newtype（`BodyText`）
**落点**：`row_mappers.rs` + 所有 `query_as` 元组。
- 优点：编译器强制无遗漏；未来新增读取点自动继承。
- 风险：① 迁移脚本 raw SQL 写入明文 → 必须双读；② 列表类查询被迫解码不需要的正文；③ `try_get::<String>` 的位置（`snapshot.rs:268`、`path_convergence.rs:114`）要一并改，否则运行期 panic。

### 候选 4：新增影子列 `content_z BLOB` + `content_encoding` 小整数，保留原 TEXT 列
- 优点：零兼容风险（旧路径读原列），可灰度回填、可回滚。
- 风险：迁移期 **存储翻倍**（若 §1.6 的口径是 A 落盘吞吐，约 1 GB/天的写入下这个过渡期是致命的；若是 B 则无所谓）；读路径要写 `COALESCE(decode(content_z), content)`，等价于每处都改。

### 候选 5：内容寻址去重（`content_hash` 表 + parts 存引用）
- 优点：重复的命令输出/工具回显去重率高（ADR-0008 已把重复源码裁掉，但 diff/日志仍有重复）。
- 风险：**`content` 列不再是文本**，§5 的 B1-B11 全部同时触发；且 `preserve_qa_and_translations`（`snapshot.rs:400-422`）
  依赖 `content` 相等做译文迁移，改成哈希相等是可行的，但「部分命中/近似」逻辑会失去依据。**不建议与透明压缩同批上线**。

### 候选 6：不做列级压缩，改做「冷热分层 + 不按时间分片的前提重估」
- 观察：`conversation_parts` 有 `(tenant_id, turn_id)` 与 `(tenant_id, id_key)` 索引
  （`migrations/20261111000003:17-21`、`20261025000001:345`），读取几乎全部「先定会话再按 turn_id 取」。
- 意味着：正文访问局部性极强（一个会话一个包），**大块压缩**比逐行压缩收益更高、CPU 更省。
- 实测支撑：**收益几乎全部来自块大小**——逐轮打包 8.8 KB 只有 2.83x，同样内容 4 MB 分块就到 4.93x；
  轮/会话聚簇 vs 按列分开只差 2%（§1.3 表）。→ 正确的规格是 **「MB 级块（≥1 MB，4 MB 最佳）」，
  而不是「会话级/轮次级」**；聚簇方式是可选项，不是收益来源。
- 「单行超大 BLOB」的担忧被实测排除（最大 213 KB，远低于 SQLite 1 GB blob 上限）。
- 仍需确认：块压缩后单行的物理行大小分布（决定 page 利用率）；`translated_text` 未采样。

---

## 7. 给决策的硬约束

1. **两列都得压，顺序是 `parts.content` 先**：全库分母下 `parts.content` 占 **64.0%**（170 MB），
   `process` 占 **12.6%**（33 MB）。只压 `process` 只覆盖 1/8，只压 `parts.content` 漏 62 MB——
   两者缺一不可，但不能因为"命令输出看起来最大"就只做 process。
   （四列分母下这两个数是 72.5% / 14.3%，两套百分比都对，务必标分母。）
2. **B1（`instr` 回退检索）、B3（`GLOB` 路径收敛）、B5（`COALESCE(json_extract, p.content)`）必须逐条改造**，
   它们不会抛错，只会让功能静默失效——上线后很难被发现。
3. **迁移脚本是 raw SQL**，因此任何透明层都必须容忍「明文与密文共存」，或提供一次性回填 + 版本列。

补充八条（来自 §1.3~§1.7、§5.1~§5.3 的实测）：

4. **不要承诺「压缩后磁盘立刻下降」**：当前 ~37 GiB 占用里，主库只有 1.6 GiB，索引泄漏 22.68 GiB、
   裸副本 11.21 GiB。**三笔账必须分开报，不能加总**；即使正文压到 191 MB，机器占用仍从 ~37.1 GiB 起步，
   用户会看到"做了压缩、磁盘几乎没变"的负面对比。
5. **压缩单位是「按会话边界封块 + 单块上限 1 MB」，不是行级、轮级、也不是全局 4 MB 块**。
   会话封块实测 4.661x、写放大≈1.0×、1,693 会话只产生 1,713 块；
   全局 4 MB 虽是理论上限（5.382x / 205.0 MiB），但跨会话拼块会让 37 KB 小会话的整包替换重写 4 MB，
   **WAL 放大 30~100 倍，不可采纳**。会话封块净收益 200.3 MiB，只比理论上限少 4.7 MiB。
6. **块级模型下只有一套策略**：两列不需分流（分叉只在逐行模型成立，见 §1.5）；差异仅体现在压缩比
   （`attrs_json` 18.2x / `process` 6.64x / `content` 5.38x / `response` 2.6~3.0x）。
7. **`prompt` 的正解是去重不是压缩**：p50 仅 152 B、去重率 36.7%（全库最高）、压缩比 8.09x（全库最高）。
8. **块级压缩的写放大可解，而且在 A 假设下是净收益**：整包快照替换天然友好（§5.2）；
   压缩 CPU 仅 4 秒/天、**WAL 流量降 ~24 倍**（写的是压缩后字节，§5.3）；
   `translated_text` 实测仅 6.9 KiB，留逐行零成本；`path_convergence` 用 `app_settings` + `schema_version` 闸门消掉。
   **不要为写放大退回逐行模型**（只少赚 4.7 MiB）。
9. **分期切分点明确**：第一期只做 `content` 两列（`parts` + `wr_parts`，同一个 `insert_parts_sqlx_tx`）
   = **146,831,463 B / 68.31%**（全局 4 MB 口径），零 `process` 风险；第二期 `process`+`attrs_json`（17.54%）；
   第三期 `prompt`/`response`（14.15%，`prompt` 先去重）。三期相加精确 100.00%。
10. **所有压缩比只在「ASCII 为主」的语料上成立**（全库 B/char=1.128，`parts.content` 1.08、`process` 1.04）。
    中文为主（B/char≥2.5）时高压缩比会明显下降，**200.3 MiB 的估算会变成高估**（§1.7）。
    无法确定目标用户语言构成时，收益应报区间而不是"省 80.92%"。
11. **「3 亿 Token」的口径未定，先别当既定事实**（§1.6）：全库仅 5,900 万~8,900 万 token，
    3 亿/天 = 每天重写全库 5 遍，而现网库只装 6 小时历史 → 极可能是流量口径而非落盘口径。
    若是流量口径，压缩收益仅 200 MiB，**可以不做**；真问题是索引泄漏 22.68 GiB。

---

## 附：待补充验证

- ~~现网各正文列的实际字节占比~~ —— 已由 res-storage 全量实测补齐（§1.3/§1.4）。
- ~~`translated_text` 的体量~~ —— 实测 4 行 / 6.9 KiB（§5.2），已排除出块级范围。
- `process` 中 `items[].content` 占 `process` 总字节的比例（决定候选 2 的真实收益上限）。
- 桌面端（`desktop/src/adapters/tauri/commands/conversations/*`）是否存在额外的正文读取；本次只确认它通过应用服务间接调用 store，未逐行核对其返回体裁剪。
- §1.3~§1.5、§5.1、§5.2、§6 候选 0 的体量、压缩比与压测方法均为 res-storage 的实测引用（`/tmp/wayfinder-209/02-storage.md`），我未复现。
