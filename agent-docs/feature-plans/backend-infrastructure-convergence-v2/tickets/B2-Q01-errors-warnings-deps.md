# B2-Q01：取消错误、warning 与依赖审计收口

**Objective:** 合并重复取消分类，清理本轮触及模块 warning，并验证最终依赖集。

**Contracts:** C-ERROR-01、C-RUNTIME-01、C-PROCESS-01、C-LOG-01、C-PATH-01。

**Canonical authority after this card:** 一个 Rust cancellation variant 映射稳定 `cancelled` wire code；Cargo.lock 是经审计依赖事实。

**Files:**

- Modify: `src-tauri/src/backend/runtime/error.rs`
- Modify: callers matching `Canceled|Cancelled`.
- Modify: 本轮触及且仍产生 warning 的模块。
- Modify only if required: `.github/workflows/ci.yml` for a reproducible audit command.
- Modify: Cargo manifests/lock only to remove unused dependencies/features found by evidence.

## Steps

- [ ] Add wire parity tests for cancellation from TaskRuntime, HostProcess, Tauri and Engine; assert code=`cancelled`, retryable semantics and safe message/details.
- [ ] Choose one existing Rust spelling, migrate all constructors/patterns and delete the other variant.
- [ ] Run `cargo check` with short output; classify every warning in modules touched by Issue #24 and make those modules warning-free without unrelated refactor.
- [ ] Run `cargo tree --duplicates` and `cargo tree -e features`; remove unused direct dependencies/features only when `cargo check --all-targets` and tests prove they are unnecessary.
- [ ] Install the pinned audit tool outside the repo if absent, then audit Cargo.lock.
- [ ] Record each remaining duplicate dependency and advisory with its transitive owner and disposition; unresolved vulnerable production dependency prevents VERIFIED.
- [ ] Verify process-wrap/which/directories/camino remain only when their production replacement and delete proof passed.

## Tests

```bash
cargo test -p assetiweave runtime::error -- --nocapture
cargo test -p assetiweave cancellation -- --nocapture
cargo check -p assetiweave --all-targets --message-format=short
cargo tree --duplicates
cargo tree -p assetiweave -e features
if ! command -v cargo-audit >/dev/null 2>&1; then cargo install cargo-audit --version 0.22.2 --locked; fi
cargo audit
```

## Delete proof

```bash
rg -n '\b(Canceled|Cancelled)\b' src-tauri/src --glob '*.rs'
```

通过：只剩选择的单一 variant；所有公开边界仍输出 `cancelled`；触及模块 warning 为零；audit exit 0。

**Stop:** 不通过 `#[allow]` 隐藏本轮新增 warning；第三方 advisory 无升级路径时提交 BLOCKED 证据，不静默忽略。

**Commit:** `refactor(errors): 统一取消错误并审计基础设施依赖`
