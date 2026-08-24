# Tantivy Conversation 本地全文搜索改造计划

## Summary

- 本计划只覆盖 Conversation 的 `session` 与 `web` 记录，不包含 Catalog、Source、Group、Prompt、手册搜索或全局 Cmd/Ctrl+K 搜索。
- 当前 `conversation.search` 会批量加载 Session、Question、Turn、Part 后逐卡片做子串匹配；`conversation.session.list` 使用 `instr(lower(...)) + EXISTS`；现存 FTS5 表持续写入但不是这两个 API 的查询主路径。
- SQLite 继续是唯一事实源；Tantivy 是按数据库、租户隔离的可删除派生索引。缺失、过期、损坏或版本不匹配时回退现有 `legacy_scan`，不得返回已知不完整的索引结果。
- v1 完成 Session 搜索、卡片搜索、Facet、高亮、后台索引和自动修复；v1.1 增加“查找相似 Question”，跨 Session 发现相关历史问题。
- 使用 `tantivy 0.26.1` 与 `tantivy-jieba 0.20.0`。后者基于 `tantivy-tokenizer-api 0.7` 并以 Tantivy 0.26 作为开发依赖，版本可直接匹配；不预设第二套分词器降级实现，兼容性 smoke test 失败时暂停 Foundation 阶段重新评估。[Tantivy 文档](https://docs.rs/tantivy/latest/tantivy/)，[tantivy-jieba 依赖清单](https://docs.rs/crate/tantivy-jieba/0.20.0/source/Cargo.toml.orig)

## 架构设计与搜索行为 (Architecture and Search Behavior)

- 在 Rust backend 增加 Conversation 专用搜索模块，分离 schema/document mapping、query、index lifecycle、hydration 和 retrieval strategy；不创建跨域 `search.local` 公共 API。
- 每个租户使用一个 Conversation 索引，包含三类文档：
  - `session`：标题、项目路径、外部 ID 和聚合内容，用于 Session 列表搜索。
  - `card`：Question、Answer、Tool、Command、Code、Result 的精确卡片定位。
  - `question`：完整 Question 聚合文本，专供相似 Question 检索。
- 索引只存检索字段与稳定标识；结果正文、Session 统计和导航对象从 SQLite 批量 hydration，避免 N+1 查询，并保持 SQLite 为展示事实源。
- 中文正文使用 Jieba search mode，英文使用 lowercase/default tokenizer；代码、命令和路径按 `/._:-` 等边界拆词。Session 标题、Question 标题、项目路径和外部 ID 增加低权重 n-gram 字段以支持输入中途命中；不为完整 Answer/Tool 正文建立任意子串 n-gram，控制索引体积。[NgramTokenizer](https://docs.rs/tantivy/latest/tantivy/tokenizer/struct.NgramTokenizer.html)
- 默认权重：Session 标题 `4.0`、Question 标题 `3.5`、Question 正文 `3.0`、项目路径 `2.5`、Answer/Tool/Result `2.0`、Code/Command `1.5`、n-gram `0.6`。权重先作为内部 schema 配置，不暴露任意 `field_boosts` 公共参数。
- `fuzzy=auto` 只作用于长度至少 4 的 ASCII/数字词，默认 edit distance 1，并限制查询词数和扩展数；CJK、路径和短标识符不做 fuzzy。用户输入不直接交给 Tantivy 查询语法解析器，统一构造受限 BooleanQuery。
- 默认按 BM25 相关性排序，分数相同时按 Session 时间降序、稳定 ID 升序；`timeline=true` 保持当前时间升序及 Question/Card 原始顺序。`score` 继续返回整数，但定义为“仅供同一响应内比较的非稳定相关性分数”。
- Facet 在应用 adapter/source/project/time 等范围过滤后、应用卡片类型选择前统计，确保切换类型时仍能看到完整类型数量。
- 高亮使用 Tantivy `Snippet::fragment()` 与 `highlighted()`，后端转换成 `{text, matched}` 片段数组；保留现有 `snippet` 字符串，不返回 HTML，也不把 Rust 字节偏移直接交给 JavaScript。[Snippet API](https://docs.rs/tantivy/latest/tantivy/snippet/struct.Snippet.html)

## 索引一致性与生命周期 (Index Consistency and Lifecycle)

- 新增 `conversation_search_index_state`，按租户记录 `index_instance_id`、schema/tokenizer version、`source_revision`、`indexed_revision`、active generation、health、文档数、索引大小、最近构建时间、错误和跨进程 writer lease。
- Conversation 导入/同步、merge/split、translation 和后续显式删除在同一 SQLite 事务内将 `source_revision` 增加一次，并返回受影响的 Session ID；事务提交后按 Session 执行“删除旧文档并重建当前文档”的单次 Tantivy commit。
- 查询只有在 schema/tokenizer 匹配且 `source_revision == indexed_revision` 时使用 Tantivy；增量更新失败、hydration 缺失或并发 revision 变化时标记 stale，并完整回退 `legacy_scan`。
- 全量重建写入独立临时 generation；完成后再次检查 revision，只有未变化时才通过 SQLite 原子切换 active generation。失败继续保留旧 generation，但旧索引若已 stale 不参与查询。
- SQLite lease 防止桌面与 CLI 同时写同一租户索引；租约超时可恢复。进程内 reader registry 以 database/tenant/generation 为键，增量 commit 后显式 reload，查询不持有 App 全局锁。[Tantivy commit](https://docs.rs/tantivy/latest/tantivy/indexer/struct.IndexWriter.html#method.commit)，[ReloadPolicy](https://docs.rs/tantivy/latest/tantivy/enum.ReloadPolicy.html)
- 保留 active generation 和上一代 generation；启动时清理过期临时目录及多余 generation，删除失败时延后重试，兼容仍被 reader/mmap 占用的文件。
- 索引目录使用 app-owned 配置目录和租户隔离，Unix 权限限制为当前用户；不纳入数据库备份或导出，不记录查询正文、Snippet 或命中内容，`doctor` 只报告状态、大小、版本和耗时。

## 公开接口与交互体验 (Public Interfaces and UX)

| 接口 (Interface) | 变更说明 (Changes) |
|---|---|
| `conversation.search` | 保留所有现有必填字段和输出；`search_options` 可选增加 `retrieval_mode`、`fuzzy_mode`。结果可选增加 `backend`、`index_status`、`content_type_counts`，Hit 可选增加 `highlight_segments`。 |
| `conversation.session.list` | 空 query 保持 SQLite 时间排序；非空 query 在索引 ready 时使用 Session 文档并按 ID 批量 hydration，接口形状不变。 |
| `conversation.similar` | 新增 Question 级相似检索；输入包含 `record_kind`、`question_id`、可选范围过滤、`retrieval_mode`、`include_same_session`、limit/offset。默认排除源 Question 和当前 Session。 |
| `conversation.search.index.status` | 返回 health、revision、generation、文档数、大小、最近构建、错误、是否正在重建及 `supported_modes`。 |
| `conversation.search.index.rebuild` | Engine/CLI 中同步执行并返回最终报告；风险标记为普通 write，因为只改派生缓存。 |
| Tauri task commands | `start_conversation_search_index_rebuild` 立即返回 snapshot，`get_conversation_search_index_task` 用于轮询；事件与 polling fallback 复用现有后台任务模式。 |

- 公共 `RetrievalMode` 从一开始定义为 `lexical | semantic | hybrid`，但 v1 的 `supported_modes` 只有 `lexical`。桌面和 CLI 不展示另外两种模式；外部直接传入 `semantic/hybrid` 时返回参数/能力不可用错误，不静默降级。
- `conversation.similar` 的 lexical 实现使用 Question 聚合文本构造 `MoreLikeThisQuery`，设置 `min_doc_frequency=1`、`min_term_frequency=1`、`max_query_terms=32`，适配较小的本地语料，并通过 Boolean filters 排除源记录和限制 record kind。[More Like This API](https://docs.rs/tantivy/latest/src/tantivy/query/more_like_this/query.rs.html)
- Conversation 页面增加索引状态入口、重建按钮、构建/降级提示；新增 `SearchIndexProvider` 统一监听事件并轮询，AppRouter 显示全局构建进度，关闭检查包含该任务。
- Question/Card 菜单增加“查找相似 Question”，结果按 Session 展示并可跳转到目标 Question。CLI 增加 `conversation similar`、`conversation search index status/rebuild`，Similar 支持 JSON、compact JSON、Markdown 和 prompt 输出。
- 首次启动、租户切换或发现 missing/stale 时后台自动重建；重建期间只禁用重复重建，搜索、导航、查看详情和其他 CRUD 保持可用。

## 实施阶段计划 (Implementation Phases)

1. **基线与基础架构 (Baseline and Foundation)**
   - 建立 10k/100k 混合中英、代码、命令 fixture，记录当前 Session SQL 和卡片 legacy scan 的耗时与内存。
   - 完成依赖/tokenizer smoke test、ADR、schema version、路径解析、状态表、generation 和 writer lease。
   - 验收：临时索引可创建、提交、重开并命中中英样例；租户和数据库路径隔离成立。

2. **会话搜索垂直切片 (Conversation Search Vertical Slice)**
   - 构建 session/card 文档，接入 `conversation.search` 和非空 Session list query。
   - 实现范围过滤、Facet、批量 hydration、高亮片段、相关性排序和 legacy fallback。
   - 验收：现有调用无需改参数即可工作，旧响应必填字段和 CLI 输出保持兼容。

3. **新鲜度与后台工作流 (Freshness and Background Workflow)**
   - 为所有 Conversation 内容变更增加事务级 revision 和受影响 Session 回传，完成批量增量更新。
   - 接入桌面后台任务、事件/轮询、全局进度、退出警告、自动重建和 corruption recovery。
   - Engine rebuild 保持前台执行，因为当前 CLI 每次启动一个单请求 Engine 子进程；不得返回无法继续运行的内存 task ID。

4. **相似问题检索 (Similar Question)**
   - 增加 question 文档、`conversation.similar`、More Like This 查询、前端侧栏和 CLI 输出。
   - 验收：结果跨 Session、默认排除当前 Session，点击后定位正确 Question；小语料库也能产生稳定结果。

5. **稳定性加固与迁移 (Hardening and Migration)**
   - 增加 `legacy|compare|auto` 开发/回滚开关；compare 模式只记录匿名命中 ID 差异和耗时，不记录查询或正文。
   - 加入状态到 `doctor.run`，验证临时 generation 清理、租约恢复、磁盘错误、权限错误和索引损坏。
   - FTS5 表在 Tantivy 默认启用后的首个稳定版本继续保留写入；确认无读取消费者且 Tantivy/legacy fallback 覆盖所有场景后，下一独立迁移停止 FTS 双写并删除表。

## 测试与验收计划 (Test and Acceptance Plan)

- Rust unit tests：schema/tokenizer 版本、中文/英文/混合词、metadata n-gram、fuzzy 边界、权重、Snippet 分段、输入上限、unsupported mode。
- Rust integration tests：session/web、六类卡片、范围过滤、Facet、分页、timeline、增量更新、merge/split/translation、revision race、租约竞争、租约过期、损坏恢复、tenant 隔离和 generation 切换。
- Similar tests：跨 Session 命中、排除源 Question/当前 Session、范围过滤、小语料、无有效词项和 stale fallback。
- Frontend tests：IME/debounce、过期请求忽略、loading/fallback/index-building 状态、事件丢失后的轮询恢复、非冲突操作保持可用、安全高亮和相似 Question 跳转。
- CLI/contract tests：旧 `conversation search` 输出不破坏；新增 status/rebuild/similar；App task command 与前台 Engine rebuild 语义分离；重新运行 `pnpm cli:contract`。
- 性能基准使用 release build、Apple Silicon 参考机和 warm filesystem cache：
  - 10k card 查询 + hydration p95 ≤ 120ms。
  - 100k card 查询 + hydration p95 ≤ 250ms，且至少比 legacy scan 快 3 倍。
  - one-shot Engine 冷打开 100k 索引并查询 p95 ≤ 500ms。
  - 100k card 全量重建 ≤ 60 秒；索引大小 ≤ 可搜索 UTF-8 文本的 2.5 倍。
  - 输入和结果渲染无超过 50ms 的主线程长任务；查询、重建和增量更新不持有全局 App lock。
- 完整验证：`cargo fmt --all -- --check && cargo test --workspace`、`pnpm typecheck && pnpm test && pnpm build`、`pnpm cli:contract`、`go vet -C cli ./... && go test -C cli -race ./...`、`pnpm cli:test:e2e`。

## Assumptions and Boundaries

- v1 只承诺 lexical 检索；semantic/hybrid 仅形成稳定枚举和 strategy 扩展点，不引入模型、向量库或 embedding 后台任务。
- 当前 Session 内 Question 小列表搜索继续使用前端过滤，不进入 Tantivy。
- 混合匹配兼容标题、项目路径和标识符的部分输入；不承诺任意长正文的所有中间子串继续命中。
- 同义词、自定义词典、保存搜索、全局搜索、Catalog/Prompt 索引均不纳入本计划；需要时另立 Conversation 搜索增强或跨域搜索计划。
- Tantivy 索引不具备数据库事务地位，任何 freshness 不确定性都以正确性优先，回退 SQLite/legacy scan，而不是展示可能过期的命中。
