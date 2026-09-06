# B2-P03C：删除同步 Process Supervisor

**Authority:** `process-wrap` 持有短进程的跨平台进程树、KillOnDrop 和 reader lifecycle。

**Contract:** C-PROCESS-01。

**Files:**

- Modify/Test: `src-tauri/src/backend/host_process.rs`
- Modify/Test: `scripts/check-module-boundaries.sh`
- Modify/Test: `scripts/check-module-boundaries.test.sh`

## Steps

- [ ] 运行全仓 sync consumer 查询；除 `host_process.rs` 中待删除定义与 test fixture 外必须为零。
- [ ] 确认现有 `contract_process_wrap_*` 八类测试实际调用生产 async wrapper，而不是平行 test-only helper。
- [ ] 增加 Windows 条件测试 `contract_windows_job_object_reaps_descendant_held_pipe`；该测试必须检查 Job Object wrapper 参与生产构造。
- [ ] 删除 `build_std_host_command`、`read_sync_capped_and_drain`、`kill_process_group`、同步 polling loop、reader threads 和所有同步 wrapper。
- [ ] 扩大边界守卫，直接拒绝 `libc::kill`、`process_group(0)`、HostProcess 内 `std::thread::spawn`、`thread::sleep`、`std::process::Command`，并为每种模式加入 RED self-test。
- [ ] 运行 macOS/Linux 本地测试；Windows 项写 `NOT RUN`，留给 B2-G02，不得本地伪造 PASS。

## Delete queries

```bash
rg -n '\b(run_command_with_timeout|run_program_with_timeout|run_program_with_cancellation|run_host_command_with_cancellation|run_command_with_control)\s*\(' src-tauri/src --glob '*.rs'
rg -n 'libc::kill|process_group\(0\)|std::thread::spawn|thread::sleep|std::process::Command' src-tauri/src/backend/host_process.rs
```

两条均必须为零；测试 fixture 通过 async `tokio::process::Command` 构造。

## Verify

```bash
cargo test -p assetiweave backend::host_process -- --nocapture
bash scripts/check-module-boundaries.test.sh
pnpm check:boundaries
cargo fmt --all -- --check
```

提交：`refactor(process): 删除手工同步进程监督器`
