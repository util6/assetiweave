# 2026-09-06 审计基线

- Revision：`b39932fe124781973fadfbd9f99ddfc5dc1efdcf`
- Branch：`refactor/ecosystem-task-1`
- Issue #24：历史状态 `CLOSED/VERIFIED`，本次审计后应为 `OPEN/INCOMPLETE`

## 已证伪 Contract

| Contract | 当前事实 | 重放命令 |
|---|---|---|
| C-PROCESS-01 | 同步 runner 使用 `std::process::Command`、reader threads、15ms polling、Unix `libc::kill`；Windows 同步路径无 Job Object | `rg -n 'std::process::Command|std::thread::spawn|thread::sleep|libc::kill|process_group' src-tauri/src/backend/host_process.rs` |
| C-SHUTDOWN-01 | dispatcher 在剩余时长为 0 时仍以 `max(50ms)` 查询，并在 abort 后不 await JoinHandle | `sed -n '523,565p' src-tauri/src/backend/events/dispatcher.rs` |
| C-SETTINGS-01 | `load_backend_settings_for_database` 忽略 Database，并在无 process runtime 时读取 legacy JSON | `sed -n '282,340p' src-tauri/src/backend/app_settings.rs` |
| C-LOG-01 | deployment 保留 `Vec<LogField>`；dispatcher 使用 `eprintln!`；Tauri 记录完整用户路径；脱敏 helper 只在 test 编译 | `rg -n 'LogField|eprintln!|root_path =|path = %path|cfg\(test\).*sanitize' src-tauri/src/backend src-tauri/src/adapters --glob '*.rs'` |
| C-PATH-01 | backend 有 348 个 `to_string_lossy` 命中，且部分参与身份、比较与持久化 | `rg -n 'to_string_lossy' src-tauri/src/backend --glob '*.rs'` |
| C-SQLX-01 | store 仍有 310 个 `try_get`，分布于 16 个文件 | `rg -n 'try_get' src-tauri/src/backend/store --glob '*.rs'` |
| B2-G01 | 当前 HEAD 无 Actions 记录；最近可见 Windows job 的 Rust workspace 测试失败 | `gh run list --limit 10` |

## 已确认保持成立

- backend runtime bridge 查询为零。
- `Database` 只持有 `SqlitePool`。
- Event Dispatcher 已使用 Tokio、Notify 与 CancellationToken。
- `AppError::Canceled` 已统一为 `AppError::Cancelled`。
- CLI contract 与 surface matrix 当前生成后无 diff。
- Rust、Frontend、Go、CLI E2E 主体测试在审计轮次通过。

## 独立发布阻断

Agent Catalog 属于 Issue #1。当前 bundled Catalog SHA256 为
`2953ae1a57e5a9b3d2c3586ce97fb15647291ae35b39d0580bae5c2032e65ef6`，release evidence
仍记录 `69628e68a5a52900e9216934cc475444ee8944d936f220e740866021137a23a3`。

```bash
node scripts/check-agent-catalog-release.mjs --release
node --test scripts/check-agent-catalog-release.test.mjs
```

两条命令当前分别失败和 `8 passed / 2 failed`。B2-G03 必须等待 Issue #1 修复该门禁。
