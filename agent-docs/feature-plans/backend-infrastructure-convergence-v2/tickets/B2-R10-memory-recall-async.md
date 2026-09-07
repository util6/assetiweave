# B2-R10：Memory Recall、Search 与 Public 链路 async-first

**Objective:** 迁移 Recall Agent、Memory 搜索、usage/evidence 与 public workflow 的数据库等待。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-SETTINGS-01、C-PRODUCT-01。

**Canonical authority after this card:** Recall/Search AppService workflow await typed repositories；MCP bridge 只调用同一 workflow。

**Files:**

- Modify: `src-tauri/src/backend/application/memory_recall_workflow.rs`
- Modify: `src-tauri/src/backend/application/memory_search.rs`
- Modify: `src-tauri/src/backend/application/memory_public.rs`
- Modify: `src-tauri/src/backend/search/memory_semantic.rs`
- Modify: Memory recall/query/usage repository modules.
- Modify: Recall MCP loop and related Engine/Tauri handlers.
- Test: Memory recall, search, MCP and repository tests.

## Steps

- [ ] Extend tests for fragment query → cited Session/Question/Turn/Part locators, tenant isolation, excluded records, usage recording and no-result behavior.
- [ ] Convert repository/search/AppService methods to async and await them from Tauri/Engine/MCP.
- [ ] Keep Recall MCP read-only surface and scoped credential/session checks unchanged.
- [ ] Ensure semantic/index CPU work and external Agent execution remain bounded; no blocking filesystem/model call on Tokio workers.
- [ ] Preserve safe public errors and evidence locator ordering.
- [ ] Remove internal runtime bridges in this slice.

## Tests

```bash
cargo test -p assetiweave memory_recall -- --nocapture
cargo test -p assetiweave memory_search -- --nocapture
cargo test -p assetiweave memory_usage -- --nocapture
cargo test -p assetiweave adapters::engine -- --nocapture
```

## Delete proof

```bash
rg -n '\.(block_on|run_sync)\(' src-tauri/src/backend/application/memory_recall_workflow.rs src-tauri/src/backend/application/memory_search.rs src-tauri/src/backend/application/memory_public.rs src-tauri/src/backend/search/memory_semantic.rs src-tauri/src/backend/store/memory_recall_query_repo.rs src-tauri/src/backend/store/memory_recall_repo.rs src-tauri/src/backend/store/memory_usage_repo.rs
```

通过：零命中；Recall/Search/MCP 行为绿。

**Stop:** 不扩展 Recall MCP mutation surface，不改变 Memory evidence 模型。

**Commit:** `refactor(runtime): 回忆检索与公开记忆链路改为异步`

