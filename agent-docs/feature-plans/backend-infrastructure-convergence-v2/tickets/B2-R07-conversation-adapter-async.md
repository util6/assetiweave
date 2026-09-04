# B2-R07：Conversation Adapter、Catalog 与同步链路 async-first

**Objective:** 迁移 Conversation Adapter 列表、安装、脚本 catalog 和同步入口，保留 adapter 协议与后台任务行为。

**Contracts:** C-RUNTIME-01、C-RUNTIME-02、C-PRODUCT-01。

**Canonical authority after this card:** Conversation Adapter AppService workflow await store/harvester；TaskRuntime 仍拥有长同步任务状态。

**Files:**

- Modify: `src-tauri/src/backend/application/conversation_adapters.rs`
- Modify: `src-tauri/src/backend/application/conversation_adapter_catalog_v2.rs`
- Modify: `src-tauri/src/backend/application/conversation_adapter_installer.rs`
- Modify: `src-tauri/src/backend/application/conversation_script_catalog.rs`
- Modify: adapter-related code in `src-tauri/src/backend/conversations/`
- Modify: affected conversation repository functions.
- Modify: related Tauri/Engine handlers and tests.

## Steps

- [x] Add/extend tests for built-in adapter materialization, catalog list/refresh, install preview/confirm and conversation sync task quick return.
- [x] Preserve Conversation Adapter identity, trust, source read-only, payload normalization and TaskRuntime progress/error behavior.
- [x] Convert repository and AppService functions for this slice to async; keep external script/process work bounded outside async executor threads.
- [x] Await the canonical methods from both Tauri and Engine.
- [x] Remove this slice's internal SQLx bridges; do not alter Conversation Record/Search methods owned by R08.
- [x] Regenerate surface matrix only if the existing exposure metadata requires a generated refresh; no manual exemption.

## Tests

```bash
cargo test -p assetiweave backend::conversations -- --nocapture
cargo test -p assetiweave backend::application::tests -- --nocapture
cargo test -p assetiweave adapters::engine -- --nocapture
pnpm conversation-adapters:test
```

## Delete proof

```bash
rg -n '\.(block_on|run_sync)\(' src-tauri/src/backend/application/conversation_adapters.rs src-tauri/src/backend/application/conversation_adapter_catalog_v2.rs src-tauri/src/backend/application/conversation_adapter_installer.rs src-tauri/src/backend/application/conversation_script_catalog.rs
```

通过：零命中；adapter package 与同步任务测试绿。

**Stop:** Question/Turn/Part 语义变化不在本卡；发现需要数据模型变化时按 DRIFT 停止。

**Commit:** `refactor(runtime): 对话适配器与同步链路改为异步`

