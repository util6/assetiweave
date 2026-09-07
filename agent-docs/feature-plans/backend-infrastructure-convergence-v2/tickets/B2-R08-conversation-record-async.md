# B2-R08：Conversation Record、Search 与 Maintenance 链路 async-first

**Objective:** 迁移 Conversation Session/Turn/Part/Question 的读取、搜索、维护和 card translation 数据链路。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-PRODUCT-01。

**Canonical authority after this card:** AppService Conversation workflow await repository/search；Card 继续只是 Content Node 的前端展示投影。

**Files:**

- Modify: `src-tauri/src/backend/application/conversation_records.rs`
- Modify: `src-tauri/src/backend/application/conversation_search.rs`
- Modify: `src-tauri/src/backend/application/conversation_maintenance.rs`
- Modify: `src-tauri/src/backend/application/card_translation.rs`
- Modify: `src-tauri/src/backend/store/conversation_repo.rs`
- Modify: `src-tauri/src/backend/store/web_record_repo.rs`
- Modify: `src-tauri/src/backend/search/conversation/`
- Modify: related Tauri/Engine handlers and tests.

## Steps

- [x] Extend AppService tests for hierarchical Session→Question→Turn→Part reads, content locator search, maintenance/reparse and translation setting resolution.
- [x] Assert tenant isolation, stable ordering, Question membership and Content Node locator behavior before implementation.
- [x] Convert repository/search/AppService methods to async outward from SQLx.
- [x] Await the same workflow from Tauri and Engine; preserve DTOs and error codes.
- [x] Remove internal block_on/run_sync and sync database wrappers in this slice.
- [x] Keep CPU-heavy parsing or filesystem reads explicitly bounded; async conversion does not move blocking I/O onto Tokio workers.

## Tests

```bash
cargo test -p assetiweave backend::conversations -- --nocapture
cargo test -p assetiweave backend::search::conversation -- --nocapture
cargo test -p assetiweave backend::application::tests -- --nocapture
cargo test -p assetiweave adapters::engine -- --nocapture
```

## Delete proof

```bash
rg -n '\.(block_on|run_sync)\(' src-tauri/src/backend/application/conversation_records.rs src-tauri/src/backend/application/conversation_search.rs src-tauri/src/backend/application/conversation_maintenance.rs src-tauri/src/backend/application/card_translation.rs src-tauri/src/backend/store/conversation_repo.rs src-tauri/src/backend/store/web_record_repo.rs src-tauri/src/backend/search/conversation
```

通过：零命中；Conversation 领域高层测试绿。

**Stop:** 不新增 card 数据库实体，不改变 Question–Turn membership。

**Commit:** `refactor(runtime): 对话记录搜索与维护链路改为异步`

