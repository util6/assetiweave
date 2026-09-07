# B2-R05：Asset、Source、Profile 与 Catalog 链路 async-first

**Objective:** 移除 Catalog 读写、Source 扫描编排和 Profile 导航内部的同步数据库桥。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-PRODUCT-01。

**Canonical authority after this card:** Catalog AppService workflow await repository/capability；后台扫描仍由 TaskRuntime 管理。

**Files:**

- Modify: `src-tauri/src/backend/application/assets.rs`
- Modify: `src-tauri/src/backend/application/sources.rs`
- Modify: `src-tauri/src/backend/application/profiles_navigation.rs`
- Modify: `src-tauri/src/backend/capabilities/catalog.rs`
- Modify: `src-tauri/src/backend/capabilities/sources.rs`
- Modify: `src-tauri/src/backend/capabilities/profiles.rs`
- Modify: `src-tauri/src/backend/store/asset_repo.rs`
- Modify: `src-tauri/src/backend/store/source_repo.rs`
- Modify: `src-tauri/src/backend/store/profile_repo.rs`
- Modify: related Tauri/Engine handlers and existing Catalog tests.

## Steps

- [ ] Add/extend AppService tests for list/add/update/remove Source, Asset description update, Profile/navigation reads and scan task quick return; assert tenant isolation and persisted side effects.
- [ ] Convert the named repository/capability/AppService functions to async from the storage boundary outward.
- [ ] Preserve Source read-only policy, scan TaskRuntime dedup/conflict/progress and a single post-batch refresh.
- [ ] Await AppService from Tauri and Engine; browser mock behavior remains frontend-only.
- [ ] Remove per-function `block_on/run_sync`; do not introduce `*_async` twins at card completion.
- [ ] Keep Engine method names, params, risk and confirmation metadata unchanged.

## Tests

```bash
cargo test -p assetiweave backend::application::tests -- --nocapture
cargo test -p assetiweave backend::capabilities -- --nocapture
cargo test -p assetiweave backend::scanner -- --nocapture
cargo test -p assetiweave adapters::engine -- --nocapture
```

## Delete proof

```bash
rg -n '\.(block_on|run_sync)\(' src-tauri/src/backend/application/assets.rs src-tauri/src/backend/application/sources.rs src-tauri/src/backend/application/profiles_navigation.rs src-tauri/src/backend/capabilities/catalog.rs src-tauri/src/backend/capabilities/sources.rs src-tauri/src/backend/capabilities/profiles.rs src-tauri/src/backend/store/asset_repo.rs src-tauri/src/backend/store/source_repo.rs src-tauri/src/backend/store/profile_repo.rs
```

通过：零命中；Catalog 与 scanner 行为测试绿。

**Stop:** Mount/Group/Skill 逻辑留给 R06；不要因共享 helper 顺便迁移它们。

**Commit:** `refactor(runtime): 资产来源与目标配置链路改为异步`

