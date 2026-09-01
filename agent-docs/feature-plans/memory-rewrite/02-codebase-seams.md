# Memory 重写：代码接缝索引

本文件记录需要昂贵定位的入口和约束，不缓存可由单次搜索得到的实现细节。开始工作后先用 `rg` 验证路径和符号仍存在。

## 生产接缝

### S01 — AppService workflow

- `src-tauri/src/backend/application/mod.rs`：AppService 聚合与公开应用工作流入口。
- `src-tauri/src/backend/application/memory*.rs`：旧 Memory 工作流，仅用于识别可复用基础设施与待删除表面。
- 新 Memory mutation 和 read workflow 在 AppService 收口；adapter 不保存业务分支。

### S02 — Conversation 事实、revision 与 delta

- `src-tauri/src/backend/application/conversation_records.rs`
- `src-tauri/src/backend/application/conversation_adapters.rs`
- `src-tauri/src/backend/store/conversation_repo.rs`
- `src-tauri/src/backend/models/conversation.rs`
- `src-tauri/migrations/202607290001_conversation_sync_deltas.sql`
- 关注 Session last activity、完成信号、cwd/project hints、source revision、sync run delta 和删除/缺失语义。

### S03 — Outbox 与 Consumer

- `src-tauri/src/backend/events/mod.rs`
- `src-tauri/src/backend/events/dispatcher.rs`
- `src-tauri/src/backend/events/consumers.rs`
- `src-tauri/src/backend/events/tests.rs`
- 复用 `ConversationSourceCommitted` 的 transaction outbox。新 Consumer 需要 durable enqueue、backfill/cutoff 和大批 delta 读取。

### S04 — TaskRuntime 与应用生命周期

- `src-tauri/src/backend/runtime/tasks.rs`
- `src-tauri/src/backend/runtime/app_runtime.rs`
- `src-tauri/src/backend/runtime/tests.rs`
- `src-tauri/src/adapters/tauri/background_tasks.rs`
- TaskRuntime 是活动投影；启动恢复、shutdown、close guard 与持久 Job 调度在 AppRuntime/应用生命周期接入。

### S05 — 新 Memory 模型、repository 与 migration

- 当前 legacy：`src-tauri/src/backend/models/memory.rs`、`src-tauri/src/backend/store/memory_repo.rs`。
- 旧 schema：`src-tauri/migrations/202607230001_memory_domain.sql`、`202607240001_memory_recall_query_indexes.sql`、`202608250003_memory_evidence_question_remap.sql`。
- 新表只通过新的追加 migration 创建；已发布 migration 保持不变。
- 新模型应按领域命名进入既有 backend 分层，不创建 `v2`/`new` 平行树。

### S06 — AI action 与执行 runtime

- `src-tauri/src/backend/ai_execution/composition.rs`：action registration/assignment。
- `src-tauri/src/backend/ai_execution/executor.rs`：`AgentExecutionRuntime`。
- `src-tauri/src/backend/ai_execution/types.rs`、`error.rs`：session mode、事件与错误。
- Session/Project/Global/Recall action 独立；Fake runtime 测试从公开 workflow 注入。

### S07 — ACP 与 Persistent Session

- `src-tauri/src/backend/agents/protocol/acp.rs`
- `src-tauri/src/backend/ai_execution/backends/acp.rs`
- `src-tauri/src/backend/ai_execution/backends/acp_aggregator.rs`
- `src-tauri/src/backend/agent_market/runtime.rs`
- Translation 参考：`src-tauri/src/backend/card_translation.rs`、`application/card_translation.rs`。
- Recall 复用 process/protocol/runtime primitives，建立它自己的交互聚合与恢复合同；OneShot aggregator 不承担 Recall。

### S08 — Conversation 搜索与派生索引

- `src-tauri/src/backend/application/conversation_search.rs`
- `src-tauri/src/backend/search/conversation/`
- `src-tauri/src/backend/store/search_index_repo.rs`
- `src-tauri/src/backend/projection/conversation_content_nodes.rs`
- lexical、semantic 和 rerank 汇合为 read-only AppService 工具；索引失效追随 Conversation revision。

### S09 — 精确导航与内容渲染

- `src-tauri/src/backend/dto/types.rs`：`ConversationContentNodeLocator` 与内容 DTO。
- `frontend/src/router/navigationTargets.ts`
- `frontend/src/pages/conversations/ConversationsPage.tsx`
- `frontend/src/components/conversations/useConversationContentController.ts`
- `frontend/src/components/conversations/ConversationContentCards.tsx`
- 跳转复用现有 Session → Question → Content block 链；扩展 locator 时保持 Card 为表现层。

### S10 — Tauri command 与事件

- `src-tauri/src/adapters/tauri/commands.rs`
- `src-tauri/src/adapters/tauri/background_tasks.rs`
- `src-tauri/src/adapters/tauri/mod.rs`
- 长任务命令快速返回 snapshot；事件与 polling 读取同一 TaskRuntime/持久 Job 投影。

### S11 — Engine contract

- `src-tauri/src/adapters/engine/registry.rs`
- `src-tauri/src/adapters/engine/surface_mapping.rs`
- `src-tauri/src/adapters/engine/runtime.rs`
- `cli/internal/schema/contract.json` 是生成物。改公开方法后运行 `pnpm cli:contract`，不手工修生成文件。

### S12 — Go CLI

- `cli/cmd/memory.go`、`cli/cmd/memory_test.go`：旧命令删除/改写目标。
- CLI 只通过 Engine client；至少覆盖 recent、context、rebuild、task 和 recall。

### S13 — Frontend contract boundary

- `frontend/src/services/memory.ts`、`memory.test.ts`
- `frontend/src/schemas/memory.ts`、`memory.test.ts`
- `frontend/src/types/memory.ts`
- 页面、hooks、schema 和组件只通过 service 边界接入 Tauri/Engine。

### S14 — Memory 后台状态

- `frontend/src/app/backgroundTasks/MemoryTaskProvider.tsx` 与测试。
- `frontend/src/app/backgroundTasks/BackgroundTaskRuntime.tsx` 与测试。
- Memory 状态进入现有全局 provider/event+polling 机制；UI 只禁用冲突操作。

### S15 — Memory 路由与页面

- `frontend/src/router/menu.ts`、`routes.ts`、`routeLoaders.ts`、`AppRouter.tsx`
- `frontend/src/pages/memory/MemoryPage.tsx` 与测试。
- `frontend/src/components/memory/`：legacy 页面为删除清单。
- 最终只保留「近期」「回忆」，共享 foundation/common 组件与正确 Markdown renderer。

### S16 — 设置与 action assignment

- `frontend/src/store/settings/settingsSchema.ts` 与 provider 测试。
- `src-tauri/src/backend/ai_execution/composition.rs`
- 生成/使用开关、排除规则、四类 action assignment 均通过持久 settings/AppService，不硬编码在页面。

### S17 — 内置 Memory Skill

- `builtin-assets/skills/assetiweave-memory/SKILL.md`
- `builtin-assets/skills/assetiweave-memory/assetiweave.skill.json`
- `builtin-assets/skills/assetiweave-memory/scripts/recall.py`
- `scripts/memory-skill-recall.test.py`
- Skill 只描述/调用新工具合同，不承担工具权限，也不引用 Dream/candidate/旧 Evidence。

## 测试接缝

### TS01 — 主验收：AppService + 临时 SQLite

使用公开 AppService workflow、确定性 Conversation fixtures、可控时钟和 Fake AgentExecutor。断言数据库重开后的外部结果，不锁 SQL 或内部调用次数。

### TS02 — Outbox + TaskRuntime 恢复

扩展 `backend/events/tests.rs`、`backend/runtime/tests.rs` 及相邻集成测试，覆盖重复事件、cutoff/backfill、lease、heartbeat、retry、cancel、restart 和 shutdown。

### TS03 — Fake ACP

沿 `ai_execution`/`agent_market` 现有 fake server 测试接缝验证 Persistent Recall、多轮、工具活动、取消、恢复、进程退出和权限拒绝；无真实网络。

### TS04 — Search fixtures

在 conversation search/index 的现有 harness 中放入精确词、同义表达、错误项目、相邻时间和重复 locator，验证过滤、lexical、semantic、rerank 和失效。

### TS05 — Engine/CLI surface

Registry 暴露测试、surface mapping 测试、生成 contract diff、Go command tests 和可选 CLI-to-Engine e2e 共同证明公共接口一致。

### TS06 — Frontend service/component/navigation

以 frontend service 为边界，覆盖 schema、两个页面、空/错/取消、Markdown、引用组件、任务响应性与精确跳转。浏览器 mock 只作 UI 数据，不成为规则引擎。

### TS07 — Migration/legacy archive

临时旧 schema fixture 执行升级，断言新表追加、旧数据只读归档、公开旧方法退出且新读路径不依赖旧表。

## 外部实现参考

以下 Codex 源码只用于理解设计，不是 AssetIWeave 权威：

- `~/fork-code/codex/codex-rs/memories/write/src/` 与 `state/src/runtime/memories.rs`：Phase 1/Phase 2、运行控制、持久状态和 last-success。
- `~/fork-code/codex/codex-rs/ext/memories/src/`、`memories/read/src/` 与 `core/src/context_manager/`：渐进式搜索/读取和上下文编排。
- 借鉴：per-session 提取、串行 consolidation、last-success、受限 Agent、usage feedback。
- AssetIWeave 特有：按项目目录结构化 Project Memory、Outbox 主动触发、SQLite 权威、多宿主 Conversation 聚合。
