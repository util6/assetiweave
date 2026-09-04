# B2-R04：Settings、System 与 Tenant 链路 async-first

**Objective:** 把设置/系统/租户垂直切片从 repository 迁移到 Tauri/Engine，验证 async 模式后再扩展其他领域。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-SETTINGS-01。

**Canonical authority after this card:** 本领域 AppService 方法直接 await SQLx；Tauri/Engine 只做 async adapter。

**Files:**

- Modify: `src-tauri/src/backend/app_settings.rs`
- Modify: `src-tauri/src/backend/application/system.rs`
- Modify: `src-tauri/src/backend/application/tenants.rs`
- Modify: `src-tauri/src/backend/store/settings_repo.rs`
- Modify: `src-tauri/src/backend/store/tenant_repo.rs`
- Modify: relevant handlers in `src-tauri/src/adapters/tauri/commands.rs`
- Modify: relevant handlers in `src-tauri/src/adapters/engine/registry.rs`
- Test: settings, application, Tauri and Engine adapter tests.

## Steps

- [ ] Add/extend high-level tests that load/save settings, initialize locale and activate tenant through AppService, then exercise one Tauri adapter and one Engine method against a temporary database.
- [ ] Convert repository-facing helpers and AppService methods in this slice to `async fn`; pass `&SqlitePool` or cloned pool explicitly.
- [ ] Convert Tauri commands to async and await AppService; remove `spawn_blocking` used only to accommodate sync SQLx in this slice.
- [ ] Await the same AppService methods from Engine handlers; aliases continue delegating canonical methods.
- [ ] Preserve locale first-writer, active tenant atomic switch, typed Settings and WireError payloads.
- [ ] Remove this slice's `Database::block_on/run_sync` and `AppRuntime::block_on/run_sync` calls.
- [ ] Regenerate contract; internal async conversion must leave the committed JSON unchanged.

## Tests

```bash
cargo test -p assetiweave backend::app_settings -- --nocapture
cargo test -p assetiweave backend::application::tests -- --nocapture
cargo test -p assetiweave adapters::engine -- --nocapture
pnpm cli:contract
git diff --exit-code -- cli/internal/schema/contract.json
pnpm check:surface-matrix
```

## Delete proof

```bash
rg -n '\.(block_on|run_sync)\(' src-tauri/src/backend/app_settings.rs src-tauri/src/backend/application/system.rs src-tauri/src/backend/application/tenants.rs src-tauri/src/backend/store/settings_repo.rs src-tauri/src/backend/store/tenant_repo.rs
```

通过：零命中；AppService、Tauri、Engine 行为测试绿。

**Stop:** async 转换本身不授权公开 command schema 变化；发现变化即按 DRIFT 停止。

**Commit:** `refactor(runtime): 设置与租户链路改为异步`

