# B2-R12：Agent、Market、AI 与 HTTP 链路 async-first

**Objective:** 迁移 Agent lifecycle、Agent Market、AI execution 和共享 HTTP client 的数据库等待与任务协调。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-ERROR-01、C-PRODUCT-01。

**Canonical authority after this card:** Agent/Market AppService workflow await manager/repository/client；Extension Kernel 保留 lifecycle 领域契约。

**Files:**

- Modify: `src-tauri/src/backend/application/agent.rs`
- Modify: `src-tauri/src/backend/application/agent_market.rs`
- Modify: `src-tauri/src/backend/agent_market/`
- Modify: `src-tauri/src/backend/ai_execution/`
- Modify: `src-tauri/src/backend/http_client.rs`
- Modify: relevant Extension Kernel launcher/lifecycle consumers.
- Modify: Agent Tauri/Engine handlers and tests.

## Steps

- [ ] Extend tests for catalog refresh, install/update/uninstall, runtime check, assignment cleanup, AI execution error phase, cancellation and tenant isolation.
- [ ] Convert repository/manager/AppService database interactions to async; keep blocking process calls inside bounded process layer until P02.
- [ ] Ensure shared HTTP client async/blocking choice remains one implementation and does not create a private runtime.
- [ ] Await the canonical workflow from Tauri/Engine; preserve version-as-observation and artifact/conformance gate decisions from prior Issues.
- [ ] Preserve TaskRuntime dedup/conflict/progress and structured extension errors.
- [ ] Remove all internal runtime bridges in this slice.

## Tests

```bash
cargo test -p assetiweave agent_market -- --nocapture
cargo test -p assetiweave ai_execution -- --nocapture
cargo test -p assetiweave extension_kernel -- --nocapture
cargo test -p assetiweave adapters::engine -- --nocapture
pnpm agent-catalog:check
```

## Delete proof

```bash
rg -n '\.(block_on|run_sync)\(' src-tauri/src/backend/application/agent.rs src-tauri/src/backend/application/agent_market.rs src-tauri/src/backend/agent_market src-tauri/src/backend/ai_execution src-tauri/src/backend/http_client.rs src-tauri/src/backend/extension_kernel
```

通过：零命中；Agent/Market/AI 行为测试绿。

**Stop:** P02 才替换进程树实现；本卡不引入 process-wrap 或改 PATH 策略。

**Commit:** `refactor(runtime): 智能体市场与执行链路改为异步`

