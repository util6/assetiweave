# B2-R11：Team workflow 与 repository 链路 async-first

**Objective:** 迁移 Team、成员、运行恢复、mailbox 与工具凭据链路，保持可恢复多成员聊天语义。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-PRODUCT-01。

**Canonical authority after this card:** Team AppService workflow await repository；Team coordinator 和 Team MCP 复用同一 async workflow。

**Files:**

- Modify: `src-tauri/src/backend/application/team.rs`
- Modify: `src-tauri/src/backend/application/team_workflow.rs`
- Modify: `src-tauri/src/backend/application/team_member_workflow.rs`
- Modify: `src-tauri/src/backend/store/team_repo.rs`
- Modify: Team coordinator portions of `src-tauri/src/backend/runtime/app_runtime.rs`
- Modify: Team Tauri/Engine/MCP handlers and tests.

## Steps

- [ ] Extend tests for Team create/update/delete, member start/cancel/recover, mailbox order, leader session continuity, tenant isolation and scoped tool credential.
- [ ] Convert team repository and AppService methods to async from SQLx outward.
- [ ] Convert coordinator reconciliation to a tracked async task; do not spawn an untracked std thread for database work.
- [ ] Await workflow from Tauri/Engine/Team MCP while preserving method exposure and credential validation.
- [ ] Keep SessionStream registry, TaskRuntime identity and durable run recovery semantics unchanged.
- [ ] Remove this slice's runtime bridges and sync database helpers.

## Tests

```bash
cargo test -p assetiweave team -- --nocapture
cargo test -p assetiweave backend::runtime::tests -- --nocapture
cargo test -p assetiweave adapters::engine -- --nocapture
```

## Delete proof

```bash
rg -n '\.(block_on|run_sync)\(' src-tauri/src/backend/application/team.rs src-tauri/src/backend/application/team_workflow.rs src-tauri/src/backend/application/team_member_workflow.rs src-tauri/src/backend/store/team_repo.rs
rg -n 'TeamCoordinatorHandle|thread::Builder|thread::spawn' src-tauri/src/backend/runtime/app_runtime.rs
```

通过：第一条零命中；Team coordinator 不再由 std thread 驱动。

**Stop:** Team UI 与会话产品行为不在本卡；只修改 async/lifecycle 边界。

**Commit:** `refactor(runtime): 团队工作流与协调器改为异步`

