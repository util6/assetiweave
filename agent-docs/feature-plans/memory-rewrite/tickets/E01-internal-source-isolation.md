# E01：内部来源显式标记与统一隔离

## Outcome

在 Conversation 数据模型中引入显式执行来源 (`execution_origin`)、执行用途 (`execution_purpose`) 与用户可见性 (`user_visible`)；在追加 migration 中建立 schema；在普通 Conversation 列表、FTS 搜索、Recent Work、Memory 调度和 Recall 候选列表中统一过滤内部生成记录；持久 Recall 问答记录正常保留可查；提供 dry-run 审计与幂等回放保证。

## Blocked by

E0（已交付，commit `ab349e4`）。

## Read

- Contracts：C-D01、C-A01、C-A02、C-S04、C-X01。
- Seams：`src-tauri/src/backend/store/conversation_repo.rs`、`recent.rs`、`memory_search.rs`、`events/consumers.rs`。
- Gates：G0、G1、G2。

## Authority changed

`conversation_sessions` 增加显式执行来源与可见性字段；普通查询和调度将内部执行与普通会话彻底解耦。

## Red test first

编写测试：
1. 插入一条普通用户会话与一条内部 Agent 执行会话（`execution_origin = 'internal_memory'`, `user_visible = 0`）；
2. 调用普通列表、FTS 搜索、Recent Work 与 Memory 候选列表，断言在旧实现下内部会话会泄露混入；在实施后内部会话被统一排除，分页总数一致。
3. 验证 Recall 问答（`execution_origin = 'recall'`, `user_visible = 1`）在普通列表或 Recall 中保留，但不会被 Memory 调度递归提取。

## Execution steps

1. **追加 Migration**：创建 `20260909000001_conversation_execution_origin.sql`，为 `conversation_sessions` 添加 `execution_origin`、`execution_purpose`、`user_visible` 字段及索引。
2. **模型与仓储更新**：在 `models::ConversationSession` 和 `NormalizedConversationSession` 中支持可选的来源标记；在 `import_canonical_session_sqlx` 中持久化来源标记。
3. **查询收口**：
   - `list_conversation_sessions_sqlx` 与 count 查询增加 `user_visible = 1` 过滤；
   - `list_recent_conversation_sessions_sqlx` 增加 `user_visible = 1` 过滤（保留并补充目录排除作为防御层）；
   - `memory_search.rs` 检索过滤排除内部执行；
   - Memory 提取 Consumer 排除 `execution_origin != 'user'`。
4. **集成测试验证**：运行隔离测试，证明内部记录在全链路上被彻底隔离。

## Acceptance

- [x] 追加 migration 幂等执行，已发布 migration 不修改（使用 `20260909000001_conversation_execution_origin.sql`）。
- [x] 内部执行会话在普通会话列表和总数中不可见（`s.user_visible = 1`）。
- [x] 内部执行会话在 Recent Work 和 FTS 检索中不可见。
- [x] Recall 问答会话保留且不触发递归 Memory 调度（`execution_origin = 'user'` 约束）。
- [x] 隔离测试 PASS（`test_internal_source_isolation_hides_agent_sessions_from_views_and_memory` 3/3 passed）。

## Verification Results

- 测试命令：`cargo test --package assetiweave --lib backend::application::bounded_evidence_baseline_tests -- --nocapture`
- 测试结果：全部 3 个测试通过（包括 baseline 与 E1 isolation）。
- 格式化：`cargo fmt --all` 已执行。

## Non-goals

E2 脱敏算法重构、E4 生产证据裁剪。
