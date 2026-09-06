# B2-L02：生产日志脱敏与单一 tracing Authority

**Authority:** `tracing` event/span 是普通生产日志唯一入口；共享字段策略在生产编译中执行。

**Contract:** C-LOG-01。

**Files:**

- Modify/Test: `src-tauri/src/backend/logging.rs`
- Modify: `src-tauri/src/backend/executor/deployment.rs`
- Modify: `src-tauri/src/backend/events/dispatcher.rs`
- Modify: `src-tauri/src/adapters/tauri/commands.rs`
- Test: `src-tauri/src/backend/logs.rs`

## Steps

- [ ] 运行 `rg -n 'LogField|eprintln!|root_path =|expanded_root_path =|path = %path|source_path|target_path'` 对目标文件建立命中表。
- [ ] 把 `sanitize_for_log` 与 `redact_sensitive_data` 移出 `#[cfg(test)]`，合并为一个生产可用、单行、有界的字段值策略。
- [ ] 增加 `production_events_redact_absolute_paths_and_sensitive_values`：通过真实 subscriber writer 写入 Unix 路径、Windows 路径、token、secret、password、prompt 和 environment，断言输出不含原值但保留 action 与非敏感 ID。
- [ ] 运行新测试；当前 helper 不进入生产和直接路径字段应形成 RED。
- [ ] 删除 deployment 的 `LogField`、`Vec<(&str, String)>` 和三个转发函数，改为直接 typed tracing fields；路径只记录稳定资源 ID、portable anchor 或脱敏 display。
- [ ] dispatcher 两个 `eprintln!` 改为 tracing event，并继承 tenant/consumer span。
- [ ] backup/reveal command 不记录完整绝对路径；成功记录资源类别，失败记录安全错误 code。
- [ ] panic 紧急落盘和 Engine/MCP stderr 失败出口继续保留，但普通领域事件不得旁路 tracing。

## Delete query

```bash
rg -n 'type LogField|Vec<\(&str, String\)>|fn log_action_(info|warn|error)|eprintln!' \
  src-tauri/src/backend/executor/deployment.rs \
  src-tauri/src/backend/events/dispatcher.rs
```

必须为零。

## Verify

```bash
cargo test -p assetiweave backend::logging -- --nocapture
cargo test -p assetiweave backend::logs -- --nocapture
cargo test -p assetiweave production_events_redact_absolute_paths_and_sensitive_values -- --nocapture
pnpm cli:test:e2e
cargo fmt --all -- --check
```

提交：`refactor(logging): 统一生产日志脱敏与结构化字段`
