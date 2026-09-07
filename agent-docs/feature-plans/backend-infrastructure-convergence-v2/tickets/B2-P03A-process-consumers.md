# B2-P03A：非 Conversation 进程 Consumer Async 化

**Authority:** `run_host_command` / `run_host_command_async` 与 `process-wrap::tokio::CommandWrap` 是有界短进程唯一执行 seam。

**Contract:** C-PROCESS-01、C-RUNTIME-01。

**Files:**

- Modify: `src-tauri/src/backend/agent_market/installers/mod.rs`
- Modify: `src-tauri/src/backend/agents/registry.rs`
- Modify: `src-tauri/src/backend/application/agent_market.rs`
- Modify: `src-tauri/src/backend/application/conversation_adapter_installer.rs`
- Modify: `src-tauri/src/backend/application/skill_remote.rs`
- Test in the owning modules above; do not modify `host_process.rs` in this card.

## Steps

- [ ] 运行卡片末尾删除查询并保存全部起始命中。
- [ ] 为 Agent probe、installer、adapter install 和 remote skill 各选择现有最高层测试，加入 timeout/cancel/nonzero 的稳定 `AppError` 或领域错误断言。
- [ ] 先把测试指向 async seam，确认当前同步签名导致编译 RED 或行为 RED。
- [ ] 沿调用链把五个模块的进程调用改为 async `.await`；阻塞文件操作继续使用已有 bounded blocking seam，不在 HostProcess 内创建 runtime。
- [ ] 每迁移一个模块运行其目标测试；不得新增 blocking façade。
- [ ] 删除查询在这五个文件中必须为零；其他模块命中留给 B2-P03B。

## Delete query

```bash
rg -n '\b(run_command_with_timeout|run_program_with_timeout|run_program_with_cancellation|run_host_command_with_cancellation|run_command_with_control)\s*\(' \
  src-tauri/src/backend/agent_market/installers/mod.rs \
  src-tauri/src/backend/agents/registry.rs \
  src-tauri/src/backend/application/agent_market.rs \
  src-tauri/src/backend/application/conversation_adapter_installer.rs \
  src-tauri/src/backend/application/skill_remote.rs
```

## Verify

```bash
cargo test -p assetiweave backend::agent_market -- --nocapture
cargo test -p assetiweave backend::agents -- --nocapture
cargo test -p assetiweave conversation_adapter_installer -- --nocapture
cargo test -p assetiweave skill_remote -- --nocapture
cargo fmt --all -- --check
pnpm check:boundaries
```

提交：`refactor(process): 迁移非对话短进程到异步执行边界`
