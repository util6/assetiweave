# B2-R06：Mount、Group、Skill 与 Backup 链路 async-first

**Objective:** 移除挂载规划/执行、资产分组、Skill 与备份 workflow 的同步数据库桥，同时保留挂载意图和后台任务语义。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-PRODUCT-01。

**Canonical authority after this card:** AppService workflow 统一持久化挂载意图、规划和物理执行；所有 SQLx await 由同一请求链驱动。

**Files:**

- Modify: `src-tauri/src/backend/application/skills.rs`
- Modify: `src-tauri/src/backend/application/skill_remote.rs`
- Modify: `src-tauri/src/backend/capabilities/groups.rs`
- Modify: `src-tauri/src/backend/capabilities/mounts.rs`
- Modify: `src-tauri/src/backend/planner/`
- Modify: `src-tauri/src/backend/executor/`
- Modify: mount/group/skill/backup repository modules and `data_backup.rs`
- Modify: related Tauri/Engine handlers and tests.

## Steps

- [ ] Add/extend AppService tests for explicit/group/exclusive mount, unmount, Skill source operations, backup metadata and cancellation between physical actions.
- [ ] Assert `asset_mounts` remains the single intent source, Source remains read-only and batch workflows refresh once after completion.
- [ ] Convert the named workflow/capability/repository chain to async; blocking filesystem work remains inside bounded `spawn_blocking` owned by the async workflow.
- [ ] Await AppService from Tauri/Engine; OneShot executes the same canonical workflow without resident dispatcher.
- [ ] Remove internal SQLx runtime bridges and duplicate sync wrappers.
- [ ] Preserve item-level error, partial failure, dedup/conflict and rollback behavior.

## Tests

```bash
cargo test -p assetiweave backend::planner -- --nocapture
cargo test -p assetiweave backend::executor -- --nocapture
cargo test -p assetiweave backend::capabilities -- --nocapture
cargo test -p assetiweave backend::data_backup -- --nocapture
cargo test -p assetiweave backend::application::tests -- --nocapture
```

## Delete proof

```bash
rg -n '\.(block_on|run_sync)\(' src-tauri/src/backend/application/skills.rs src-tauri/src/backend/application/skill_remote.rs src-tauri/src/backend/capabilities/groups.rs src-tauri/src/backend/capabilities/mounts.rs src-tauri/src/backend/planner src-tauri/src/backend/executor src-tauri/src/backend/data_backup.rs
```

通过：零命中；mount/group/skill/backup 高层行为保持。

**Stop:** Remote Skill 网络 client 重构不在本卡；只迁移 await 边界。

**Commit:** `refactor(runtime): 挂载技能与备份链路改为异步`

