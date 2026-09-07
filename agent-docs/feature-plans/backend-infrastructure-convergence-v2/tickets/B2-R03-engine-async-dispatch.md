# B2-R03：Engine registry 与 transport 建立 async dispatch

**Objective:** 让 Engine handler 能直接 await AppService，为后续逐领域移除同步桥提供单一入口。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-PRODUCT-01。

**Canonical authority after this card:** Engine `CommandSpec` 的 handler 返回 `DispatchFuture`，transport 以 async dispatch 驱动；method/risk/exposure/schema 不变。

**Interfaces:**

```rust
type DispatchFuture = Pin<Box<dyn Future<Output = DispatchResult> + Send>>;
type CommandHandler = fn(Value) -> DispatchFuture;

impl CommandSpec {
    async fn dispatch(&self, params: Value) -> DispatchResult;
}
```

同步 AppService consumer 在本卡可暂时由返回 `DispatchFuture` 的 async block 调用；R04–R12 必须逐域删除这些同步调用，R13 验收零残留。

**Files:**

- Modify: `src-tauri/src/adapters/engine/registry.rs`
- Modify: `src-tauri/src/adapters/engine/transport.rs`
- Modify: `src-tauri/src/adapters/engine/runtime.rs`
- Modify: `src-tauri/src/adapters/engine/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: Engine adapter tests colocated in these modules.

## Steps

- [ ] Convert the registry handler field and `CommandSpec::dispatch` to the exact async interface above.
- [ ] Convert every registered handler to return `DispatchFuture`; keep handler body, typed param validation and WireError conversion unchanged.
- [ ] Convert transport request dispatch to async and await the spec handler.
- [ ] Keep stdio line reading/writing ordering deterministic: one request is fully dispatched before the next response is written.
- [ ] At the process entry, run the async stdio loop through the existing runtime bridge; mark this exact bridge as R13-owned in the baseline allowlist.
- [ ] Convert direct transport tests to `#[tokio::test]` and await dispatch; preserve all existing assertions for aliases, risk, exposure, schema, confirmation and errors.
- [ ] Regenerate the CLI contract and prove there is no schema diff.

## Tests

```bash
cargo test -p assetiweave adapters::engine -- --nocapture
pnpm cli:contract
git diff --exit-code -- cli/internal/schema/contract.json
pnpm check:surface-matrix
```

## Delete proof

```bash
rg -n 'handler: fn\(Value\) -> DispatchResult|pub\(crate\) fn dispatch\(&self, params: Value\)' src-tauri/src/adapters/engine
```

通过：零命中；所有 Engine tests 和 committed contract 一致性通过。

**Stop:** 并发处理多条 stdio request 不在本卡范围；保持现有串行响应顺序。

**Commit:** `refactor(engine): 建立异步命令派发边界`
