# Wave 2 前置考古：会话正文的读写路径与透明压缩落点

> 范围：只读代码考古，未修改仓库任何文件。
> 仓库：`/Users/util6/code-space/assetiweave`，主战场 `backend/asset_core`。
> 结论均带 `文件:行号`；无法从代码确证的一律标「未验证」。

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
| 字节占比 | **72.5%** | 14.3% | 5.2% | 8.0% |
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

### 1.4 两种列形状相反，不能用同一套策略

| | `process` | `parts.content` |
|---|---|---|
| <1 KB 行占字节 | **2.6%** | 18.4% |
| >4 KB 行占字节 | 85.1%（其中 16–64KB 桶独占 39.2%） | — |
| <256 B 行 | 28.6% 的行 / 0.43% 的字节（压缩比仅 1.09） | 48.4% 的行 / 5.4% 的字节 |
| 结论 | **整列直接打包，不需要阈值分流** | **必须按 256 B 阈值分流**（小行入公共块，不单独压） |

另外 `prompt` 的正解不是压缩而是去重：p50 仅 152 B、总量 12.26 MB，**去重率 36.7%（全库最高）**。

「日均 3 亿 Token」的换算：**未验证**（现网实测正文总量约 234 MB，与"日均 3 亿 Token"不在同一量纲，需先确认统计口径）。

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
| 主库 | 1.6 GiB（VACUUM 免费可降到约 208 MiB） | 压缩目标；但只占全局 4.3% |

推论：**「正文压缩」只能解决主库里那部分；磁盘占用的大头目前在废弃索引代数（22.68 GiB）与裸副本（11.21 GiB）上。**
如果 Wave 2 的目标是「3 亿 Token/天可持续」，必须先回答这两块归谁管（清理策略 vs 压缩）——
否则正文压到 209 MB，机器占用仍从 ~37.1 GiB（= 48.3 − 11.2，扣除可确认的无用副本）起步。
**建议验收口径写成分账：索引泄漏 22.68 GiB / 裸副本 11.21 GiB / 主库 1.60 GiB → 208 MiB，三笔分开报，不加总。**

---

## 6. 「压缩层落点」候选判断

### 候选 1：写入漏斗编码 + repo 层解码（推荐）
**落点**：`snapshot.rs:499-580`（两个 insert）+ `row_mappers.rs:359-367` / `snapshot.rs:264-273` / `search_records.rs:146-155` 等读点。
- 优点：与「正文写入已收敛」这一事实完全吻合（§2.1）；可以逐列决策（先压 `content` + `process.items[].content`，`attrs_json` 暂不压）。
- 风险：约 20 个读点漏一个就静默损坏；需要「明文/密文双读」前缀协议（迁移脚本写的是明文）。

### 候选 2：`process` / `attrs_json` 内部字段级压缩，JSON 外壳保持合法
**落点**：`snapshot.rs:163-167`（process 序列化前）+ `fold_part_attrs`（`snapshot.rs:619-646`）；读取侧在 `serde` 层。
- 优点：`json_extract('$.cwd'|'$.turn_kind'|'$.exec_id')`、`GLOB '*"cwd":"/*'`、`LIKE '%"turn_kind"%'` **全部继续工作**；
  且 `process` 是「天然可压」的列（85.1% 字节在 >4 KB 行里，<1 KB 只占 2.6%），单列 4 MB 块可达 **6.64x**（§1.4）。
- 但**覆盖面只有 14.3%**（33.4 MB / 234 MB）——单独做它，收益上限就是主库的 1/7。
- 风险：压缩粒度是「字符串值」，需要给每个被压字段打标记（例如 `{"content":"~z:<b64>"}`）；`json_group_array` 的迁移路径要重算。
  且 `COALESCE(json_extract(...), p.content)`（B5）仍在 `p.content` 上翻车，需单独处理。

### 候选 3：sqlx `Decode/Encode` newtype（`BodyText`）
**落点**：`row_mappers.rs` + 所有 `query_as` 元组。
- 优点：编译器强制无遗漏；未来新增读取点自动继承。
- 风险：① 迁移脚本 raw SQL 写入明文 → 必须双读；② 列表类查询被迫解码不需要的正文；③ `try_get::<String>` 的位置（`snapshot.rs:268`、`path_convergence.rs:114`）要一并改，否则运行期 panic。

### 候选 4：新增影子列 `content_z BLOB` + `content_encoding` 小整数，保留原 TEXT 列
- 优点：零兼容风险（旧路径读原列），可灰度回填、可回滚。
- 风险：迁移期 **存储翻倍**（对 3 亿 Token/天是致命的过渡成本）；读路径要写 `COALESCE(decode(content_z), content)`，等价于每处都改。

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

1. **两列都得压，顺序是 `parts.content` 先**：`parts.content` 占 72.5%（170 MB），`process` 占 14.3%（33 MB）。
   只压 `process` 覆盖 12.6%，只压 `parts.content` 漏 62 MB——两者缺一不可，但不能因为"命令输出看起来最大"就只做 process。
2. **B1（`instr` 回退检索）、B3（`GLOB` 路径收敛）、B5（`COALESCE(json_extract, p.content)`）必须逐条改造**，
   它们不会抛错，只会让功能静默失效——上线后很难被发现。
3. **迁移脚本是 raw SQL**，因此任何透明层都必须容忍「明文与密文共存」，或提供一次性回填 + 版本列。

补充四条（来自 §1.3/§1.4/§5.1 的实测）：

4. **不要承诺「压缩后磁盘立刻下降」**：当前 ~37 GiB 占用里，主库只有 1.6 GiB，索引泄漏 22.68 GiB、
   裸副本 11.21 GiB。**三笔账必须分开报，不能加总**；即使正文压到 209 MB，机器占用仍从 ~37.1 GiB 起步，
   用户会看到"做了压缩、磁盘几乎没变"的负面对比。
5. **压缩单位是 MB 级块（≥1 MB，4 MB 最佳），不是行级也不是轮级**：轮级 2.83x，4 MB 块 4.93~5.03x。
6. **两种列形状相反，不能一套策略**：`process` 整列直接打包（<1 KB 只占 2.6% 字节）；
   `parts.content` 必须按 256 B 分流（48.4% 的行只占 5.4% 字节，小行单压压缩比仅 1.09）。
7. **`prompt` 的正解是去重不是压缩**：p50 仅 152 B、去重率 36.7%（全库最高）、4 MB 块压缩比 8.09x（全库最高）。

---

## 附：待补充验证

- ~~现网各正文列的实际字节占比~~ —— 已由 res-storage 全量实测补齐（§1.3）。
- `process` 中 `items[].content` 占 `process` 总字节的比例（决定候选 2 的真实收益上限）。
- `translated_text` 两列的字节占比与去重率（译文副本可能与原文高度相似，影响去重策略）。
- 桌面端（`desktop/src/adapters/tauri/commands/conversations/*`）是否存在额外的正文读取；本次只确认它通过应用服务间接调用 store，未逐行核对其返回体裁剪。
- §1.3/§1.4/§5.1 的体量与压缩比均为 res-storage 的实测引用（`/tmp/wayfinder-209/02-storage.md`），我未复现。
- §1.3 的体量分布与 §5.1 的磁盘占用均为 res-storage 的实测引用（`/tmp/wayfinder-209/02-storage.md`），我未复现。
