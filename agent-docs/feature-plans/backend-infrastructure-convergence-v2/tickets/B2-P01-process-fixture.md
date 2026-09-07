# B2-P01：验证 process-wrap 与 which 行为契约

**Objective:** 在切换生产代码前证明候选 crate 覆盖 AssetIWeave 的进程树和可执行文件发现需求。

**Contracts:** C-PROCESS-01、C-ERROR-01。

**Canonical authority after this card:** 本地 fixture 是 process-wrap/which 采用门；生产 HostProcess 尚不切换。

**Files:**

- Modify: `src-tauri/Cargo.toml`
- Modify: `Cargo.lock`
- Modify tests in: `src-tauri/src/backend/host_process.rs`
- Create only if fixture cannot stay colocated: `src-tauri/tests/process_wrap_contract.rs`

## Dependency change

```toml
process-wrap = { version = "=10.0.0", features = ["tokio1"] }
which = "=8.0.6"
```

## Steps

- [ ] Add test-only helpers that spawn the repository's existing child-process fixture through `process_wrap::tokio::CommandWrap`.
- [ ] On Unix, wrap with `ProcessGroup::leader()` or `ProcessSession` plus `KillOnDrop`; choose the smaller option that reaps the fixture's descendant tree.
- [ ] On Windows, wrap with `JobObject` plus `KillOnDrop`; do not call `taskkill` in the new fixture path.
- [ ] Test normal exit, timeout, pre-cancel, mid-flight cancel, parent exit with descendant-held pipe, ignored graceful termination, bounded stdout/stderr and nonzero exit.
- [ ] Test `which::which` on a temporary PATH executable; then test that the existing desktop fallback is invoked only when standard lookup misses.
- [ ] Record the exact wrapper combination and child methods used; P02 must reuse them rather than re-research the API.
- [ ] Run macOS/Linux local tests. Windows-specific tests must pass in the repository's existing Windows CI job before this card is VERIFIED.

## Tests

```bash
cargo test -p assetiweave host_process -- --nocapture
cargo check -p assetiweave --all-targets
cargo tree -p assetiweave -e features | rg 'process-wrap|which'
```

## Gate G-B2-P01

通过需同时满足：

1. 当前平台 fixture 全绿。
2. Windows CI 的 `cargo test --workspace -- --test-threads=1` 全绿。
3. process-wrap feature 包含 Tokio 与目标平台 wrapper。
4. 未修改生产 HostProcess consumer。

**Stop:** 任一核心 fixture 无法由 process-wrap 10.0.0 满足时，删除本卡新增依赖，提交 DRIFT 证据并停止；不在 crate 外重写同等复杂机制来“让验证通过”。

**Commit:** `test(process): 验证跨平台进程包装契约`

