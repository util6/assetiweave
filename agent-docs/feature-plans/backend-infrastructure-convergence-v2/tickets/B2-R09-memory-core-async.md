# B2-R09：Session、Project 与 Global Memory 链路 async-first

**Objective:** 迁移 Memory 生成、持久化与 Recent Work 聚合的核心链路，保留 Memory 领域模型和后台生成语义。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-SETTINGS-01、C-PRODUCT-01。

**Canonical authority after this card:** Session/Project/Global Memory AppService workflow await repository；TaskRuntime 继续拥有生成任务状态。

**Files:**

- Modify: `src-tauri/src/backend/application/session_memory.rs`
- Modify: `src-tauri/src/backend/application/project_memory.rs`
- Modify: `src-tauri/src/backend/application/global_memory.rs`
- Modify: `src-tauri/src/backend/application/recent.rs`
- Modify: corresponding session/project/global Memory repository modules.
- Modify: related Tauri/Engine handlers and Memory tests.

## Steps

- [ ] Add/extend high-level tests for Session Memory generation, Project Memory aggregation, Global Memory persistence, excluded session/source settings and Recent Work ordering.
- [ ] Assert generation/usage disabled settings prevent work without corrupting existing records.
- [ ] Convert repository and AppService methods in this slice to async; pass request tenant and typed settings snapshot explicitly.
- [ ] Keep AI execution and filesystem work in their established bounded worker; only database waiting becomes async.
- [ ] Await methods from Tauri/Engine and preserve TaskSnapshot/WireError behavior.
- [ ] Remove internal `block_on/run_sync` for this slice; Recall/Search remains R10-owned.

## Tests

```bash
cargo test -p assetiweave backend::application::session_memory -- --nocapture
cargo test -p assetiweave backend::application::project_memory -- --nocapture
cargo test -p assetiweave backend::application::global_memory -- --nocapture
cargo test -p assetiweave backend::application::recent -- --nocapture
cargo test -p assetiweave backend::store::session_memory_repo -- --nocapture
```

## Delete proof

```bash
rg -n '\.(block_on|run_sync)\(' src-tauri/src/backend/application/session_memory.rs src-tauri/src/backend/application/project_memory.rs src-tauri/src/backend/application/global_memory.rs src-tauri/src/backend/application/recent.rs src-tauri/src/backend/store/session_memory_repo.rs src-tauri/src/backend/store/project_memory_repo.rs src-tauri/src/backend/store/global_memory_repo.rs
```

通过：零命中；Memory core 和 Recent Work 行为绿。

**Stop:** Recall Agent、Context Resolver 和 Memory evidence 搜索留给 R10；不改变 Memory 产品语义。

**Commit:** `refactor(runtime): 核心记忆链路改为异步`

