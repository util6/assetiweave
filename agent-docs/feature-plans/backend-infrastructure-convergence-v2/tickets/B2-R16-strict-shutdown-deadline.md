# B2-R16：严格绝对 Shutdown Deadline

**Authority:** `AppRuntime` 创建的单一 `Instant` 是所有关闭阶段唯一时间 Authority。

**Contract:** C-SHUTDOWN-01。

**Files:**

- Modify/Test: `src-tauri/src/backend/events/dispatcher.rs`
- Test only if cross-runtime assertion is needed: `src-tauri/src/backend/runtime/tests.rs`

## Steps

- [ ] 运行 `sed -n '523,565p' src-tauri/src/backend/events/dispatcher.rs`，确认两个 `remaining.max(Duration::from_millis(50))` 命中。
- [ ] 增加 `stop_until_never_extends_an_expired_deadline`：传入已经到达的 deadline，断言返回墙钟时间不超过 25ms，且报告 `timed_out=true`。
- [ ] 增加 `stop_until_awaits_aborted_dispatcher_completion`：让 dispatcher worker 在可观察 drop guard 中挂起，触发 timeout 后断言 guard 已 drop 再返回。
- [ ] 运行两个新测试并记录当前实现的 RED 输出。
- [ ] 把 pending count 查询限制为 `deadline.saturating_duration_since(Instant::now())`；剩余为零时跳过 SQL 查询并使用关闭流程已经持有的计数或安全默认值。
- [ ] `task.abort()` 后继续 `await` 同一 JoinHandle；不得丢弃仍可能运行的 worker。
- [ ] 运行 Gate，确认 `rg -n 'remaining\.max\(Duration::from_millis\(50\)\)' src-tauri/src/backend/events/dispatcher.rs` 为零。

## Verify

```bash
cargo test -p assetiweave stop_until_ -- --nocapture
cargo test -p assetiweave shutdown_deadline_bounds_total_wall_time_across_all_stages -- --nocapture
cargo fmt --all -- --check
pnpm check:boundaries
```

提交：`fix(runtime): 严格约束关闭流程绝对截止时间`
