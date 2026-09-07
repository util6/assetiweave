# B2-R13：async bootstrap 与 pool-only Database 收口

**Objective:** 删除 backend 内部 Tokio Runtime 所有权和同步桥，使 Database 只持有 `SqlitePool`。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-PRODUCT-01。

**Canonical authority after this card:** 进程最外层建立 runtime；`AppRuntime::bootstrap`、AppService 和 shutdown 全部 async；`Database` 只代理 pool。

**Interfaces:**

```rust
impl AppRuntime {
    pub(crate) async fn bootstrap(db_path: PathBuf, role: RuntimeRole) -> AppResult<Arc<Self>>;
    pub(crate) fn pool(&self) -> &SqlitePool;
}

impl Database {
    pub(crate) fn from_pool(pool: SqlitePool) -> Self;
    pub(crate) fn pool(&self) -> &SqlitePool;
}
```

**Files:**

- Modify: `src-tauri/src/backend/store/database.rs`
- Modify: `src-tauri/src/backend/runtime/app_runtime.rs`
- Modify: `src-tauri/src/backend/runtime/tasks.rs`
- Modify: `src-tauri/src/backend/runtime/tests.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: binaries and remaining Tauri/Engine/MCP process entries.
- Modify: tests that still construct sync `Database`.

## Steps

- [ ] Run global bridge query; every remaining match must be process-entry or test constructor and listed before editing.
- [ ] Add RED tests proving async bootstrap is idempotent on a temporary DB, ResidentHost/OneShot share the same bootstrap logic, and no nested runtime panic occurs under `#[tokio::test]`.
- [ ] Make `AppRuntime::bootstrap` async; replace internal runtime `.block_on` with direct await.
- [ ] Make `Database` contain only cloned `SqlitePool`; replace sync test constructors with async helpers used under Tokio tests.
- [ ] Build one runtime per standalone Engine/Team MCP/Recall MCP/self-check process at the outer function, and use Tauri's runtime at desktop startup.
- [ ] Obtain TaskRuntime spawn handle from the active outer runtime; do not create a second Tokio Runtime.
- [ ] Remove `build_runtime`, `from_parts(pool, runtime)`, all `block_on/run_sync` methods and imports.
- [ ] Convert remaining tests to `#[tokio::test]` or await from an existing runtime fixture.
- [ ] Upgrade the R01 guard from monotonic allowlist to zero-match rule and delete `rust-runtime-bridge-baseline.txt`.

## Tests

```bash
cargo test -p assetiweave backend::runtime::tests -- --nocapture
cargo test -p assetiweave backend::store -- --nocapture
cargo test -p assetiweave adapters::engine -- --nocapture
pnpm check:boundaries
pnpm cli:contract
git diff --exit-code -- cli/internal/schema/contract.json
```

## Delete proof

```bash
rg -n 'tokio::runtime::Runtime|build_runtime|\.block_on\(|\.run_sync\(' src-tauri/src/backend --glob '*.rs'
rg -n 'runtime: Runtime|from_parts\(pool: SqlitePool, runtime' src-tauri/src/backend/store/database.rs
```

通过：两条命令均零命中；process-entry 文件之外没有 runtime 构造或 block_on。

**Stop:** 不通过在新 helper 中改名隐藏 block_on；任何 sync-only backend API 必须继续向外迁移，而非保留桥。

**Commit:** `refactor(runtime): 数据库收口为纯连接池`

