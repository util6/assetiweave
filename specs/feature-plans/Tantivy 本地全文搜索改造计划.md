# Tantivy 本地全文搜索改造计划

## Summary

- 用 Tantivy 做嵌入式本地全文索引，SQLite 仍是唯一事实源；Tantivy 索引只作为可删除、可重建的派生缓存。
- v1 优先替换最卡的 Conversation Session 搜索和“搜索内容并定位卡片”；v1.1 覆盖 Catalog 资产、来源、分组；Prompt 卡片需先从 `localStorage` 迁到后端后再纳入索引。
- 保留现有 `conversation.search` / `conversation.session.list` 合约，背后切到 Tantivy；再新增通用本地搜索和索引状态 API，供后续所有本地搜索框复用。
- 采用 `tantivy 0.26.1`，中文分词默认选 `tantivy-jieba 0.20.0`，若兼容性失败再切 `cang-jie 0.20.0`；Tantivy 文档确认它支持 BM25、增量索引、字段、Facet、可配置 tokenizer，中文需第三方 tokenizer 接入。参考：Tantivy docs.rs ，tokenizer API docs.rs 。

## Key Changes

- 新增 Rust 搜索域模块：`/Users/util6/code-space/assetiweave/src-tauri/src/backend/search/`。
  - `schema`：定义 Tantivy schema、schema version、字段权重默认值。
  - `documents`：把 Conversation session/card、web record、asset/source/group 转为 `SearchDocument`。
  - `engine`：定义 `SearchEngine` trait 和 `TantivySearchEngine` 实现。
  - `indexer`：全量重建、按 entity 删除再重建、提交并 reload reader。
  - `query`：分词、同义词展开、模糊查询、多字段 boost、Facet/type filters。
  - `status`：索引版本、stale 状态、重建进度、错误信息。

- 索引位置和生命周期：
  - 索引目录放在 app-owned 配置目录，例如 `~/.assetiweave/search-index/<db_hash>/<tenant_id>/schema-1/`。
  - SQLite 不存 Tantivy 内容，只存可选的索引元数据：`tenant_id`、`schema_version`、`last_rebuilt_at`、`source_revision`、`stale_reason`。
  - 全量重建写入临时目录，成功后切换 manifest；失败时继续使用旧索引或 SQL fallback。
  - Tantivy reader 用进程内 registry 缓存，锁只保护 handle map，查询本身不在全局锁内执行。

- API/contract：
  - 保留 `conversation.search` 输入输出，`score` 继续返回整数，使用 `round(tantivy_score * 1000)`，避免 CLI 输出破坏。
  - `ConversationSearchParams` 只做向后兼容扩展：新增可选 `search_options`，包含 `fuzzy_mode`、`synonyms_enabled`、`field_boosts`。
  - `ConversationSearchResult` 增加可选 `facets`、`index_status`、`matched_fields`、`highlights`；旧客户端可忽略。
  - 新增 Engine/Tauri 方法：`search.local`、`search.index.status`、`search.index.rebuild`、`search.index.task.get`；新增后运行 `pnpm cli:contract`。
  - `conversation.session.list` 在 `query` 非空时走 Tantivy session 文档，再按 id 从 SQLite hydration；空查询保持当前 SQL 排序。

- 搜索行为默认值：
  - 分词：同一文本写入中文 tokenizer 字段和英文/default tokenizer 字段；path/code/command 用独立字段。
  - 模糊：默认 `auto`，英文/数字词长度 3-5 使用 edit distance 1，长度 6+ 使用 edit distance 2；CJK 单字不做 fuzzy。
  - 同义词：先做 query-time expansion，避免修改同义词后必须全量重建。
  - 默认权重：title/name `4.0`，tag `3.0`，question `3.0`，project/path `2.5`，answer/tool/result `2.0`，code/command `1.5`。
  - 排序：默认相关性；`timeline=true` 继续按 session time 排序，再用 score 做次级排序。

## Implementation Phases

- Phase 1: Foundation
  - 加 Tantivy 依赖和最小 tokenizer smoke test。
  - 写 ADR/设计说明，明确“SQLite source of truth + Tantivy derived cache”。
  - 建立搜索 DTO、schema version、index path、reader registry、索引状态模型。
  - 验收：能在临时目录创建索引，中文/英文样例可被查询命中。

- Phase 2: Conversation vertical slice
  - 为 session 与 content card 生成文档，覆盖 `session` 和 `web` 两套表。
  - 替换 `search_conversation_records` 的内部实现，保留 SQL fallback。
  - 替换 Session 搜索路径，避免当前 `instr(lower(...)) + EXISTS` 扫描。
  - 卡片类型过滤保持多选，并从 Tantivy Facet 返回各类型数量。
  - 验收：原 CLI/前端测试不改期望仍通过，新增分词、模糊、同义词、权重排序测试。

- Phase 3: Background index and freshness
  - 新增搜索索引后台任务 registry，支持 rebuild/status/progress。
  - Conversation sync/import、merge/split、translation、delete 等变更后按 session/question 增量更新索引。
  - Tauri 命令继续 `async + spawn_blocking`，不持有 `AppState.lock` 执行查询或重建。
  - UI 在搜索框和结果区展示加载/索引构建状态，不出现系统转圈式卡顿。

- Phase 4: Other local search surfaces
  - Catalog 资产、来源、Skill 分组接入 `search.local`，结果再从 SQLite hydration，前端不再对大数组做主搜索。
  - Prompt notes 先迁入 Engine/SQLite，再接入 Tantivy；迁移前仍只保留 debounced client filter。
  - 手册页和 GitHub Skill discovery 不纳入 Tantivy：前者数据量小，后者是远程 provider 搜索。

- Phase 5: Hardening
  - 对 stale/missing/corrupt index 自动 fallback，并提示用户重建。
  - 限制 `limit <= 500`、分页 offset、fuzzy 复杂度，防止单次查询拖垮 UI。
  - 高亮只返回纯文本片段和位置，前端用 React 渲染，不拼 HTML。

## Test Plan

- Rust unit tests：schema version、document mapping、tokenizer、synonym expansion、query builder、field boost 排序、fuzzy 边界。
- Rust integration tests：临时 SQLite + 临时 Tantivy dir，验证 rebuild、incremental update、delete/reindex、fallback。
- Existing regression tests：`conversation.search`、`conversation.session.list`、web record 搜索结果保持兼容。
- CLI tests：conversation search 参数、compact-json/markdown/prompt 输出不破坏；新增 index status/rebuild contract 测试。
- Frontend tests：搜索框 pending/loading UI、card type 多选 facet、index-building 提示、旧 SQL fallback 提示。
- Performance checks：10k docs p95 查询 < 120ms，100k docs p95 查询 < 250ms；输入期间无 >50ms 主线程长任务，搜索/重建期间无全局 app lock。

## Assumptions

- 复杂全文能力优先服务本地数据，不替代 GitHub Skill discovery。
- v1 不删除现有 SQLite FTS5 表，等 Tantivy 稳定覆盖后再单独迁移清理。
- 同义词和字段权重进入 `settings.search`，默认开启同义词和 auto fuzzy；CLI/Engine 可用参数覆盖。
- Prompt 卡片要获得同等级搜索能力，必须先完成后端持久化迁移。
