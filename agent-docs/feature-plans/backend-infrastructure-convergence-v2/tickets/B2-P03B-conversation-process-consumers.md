# B2-P03B：Conversation 进程 Consumer Async 化

**Authority:** Conversation harvester、external adapter 与 IO helper 只调用 async HostCommand seam。

**Contract:** C-PROCESS-01、C-RUNTIME-01、C-RUNTIME-02。

**Files:**

- Modify/Test: `src-tauri/src/backend/conversations/external.rs`
- Modify/Test: `src-tauri/src/backend/conversations/harvester.rs`
- Modify/Test: `src-tauri/src/backend/conversations/io_utils.rs`
- Modify callers under `src-tauri/src/backend/application/` only when编译器证明属于同一 workflow。

## Steps

- [ ] 运行删除查询并从公开 AppService/adapter 追踪三个 helper 的全部 caller。
- [ ] 扩展现有 Conversation fixture，增加 `conversation_external_timeout_reaps_descendant_pipe` 与 `conversation_harvester_cancel_reaps_descendant_pipe`，断言在总 deadline 内返回 typed timeout/cancel。
- [ ] 运行新测试记录 RED；若旧测试已通过，加入“父进程退出但后代保持 stdout/stderr”模式直到能反驳同步 runner。
- [ ] 将三个模块和必要 caller 改为 async；不得使用 `spawn_blocking` 包裹旧同步 runner。
- [ ] 保持 adapter 输出、无效 UTF-8、非零退出与 output cap 语义。
- [ ] 删除查询对三个目标文件为零。

## Delete query

```bash
rg -n '\b(run_command_with_timeout|run_program_with_timeout|run_program_with_cancellation|run_host_command_with_cancellation|run_command_with_control)\s*\(' \
  src-tauri/src/backend/conversations/external.rs \
  src-tauri/src/backend/conversations/harvester.rs \
  src-tauri/src/backend/conversations/io_utils.rs
```

## Verify

```bash
cargo test -p assetiweave backend::conversations -- --nocapture
cargo test -p assetiweave conversation_external_timeout_reaps_descendant_pipe -- --nocapture
cargo test -p assetiweave conversation_harvester_cancel_reaps_descendant_pipe -- --nocapture
cargo fmt --all -- --check
pnpm check:boundaries
```

提交：`refactor(process): 迁移对话短进程到异步执行边界`
