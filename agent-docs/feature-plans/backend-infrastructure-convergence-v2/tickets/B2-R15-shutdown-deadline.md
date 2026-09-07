# B2-R14：统一 shutdown 绝对 deadline

**Objective:** 让 worker、coordinator、dispatcher 和 pool close 共用一个 deadline，并准确报告未完成资源。

**Contracts:** C-SHUTDOWN-01、C-EVENT-01、C-RUNTIME-01。

**Canonical authority after this card:** `AppRuntime::shutdown_until(deadline)` 是所有 resident resource 的唯一关闭协调入口。

**Interfaces:**

- Accept one `tokio::time::Instant` absolute deadline.
- `ShutdownReport` includes unfinished task IDs, dispatcher drain state/remaining count and named unfinished resource stages.
- `shutdown_with_grace` may remain only as a thin outer conversion from `Duration` to one absolute deadline.

**Files:**

- Modify: `src-tauri/src/backend/runtime/app_runtime.rs`
- Modify: `src-tauri/src/backend/runtime/tasks.rs`
- Modify: `src-tauri/src/backend/events/dispatcher.rs`
- Modify: `src-tauri/src/backend/runtime/tests.rs`
- Modify: desktop/Engine close hooks in `src-tauri/src/lib.rs`

## Steps

- [ ] Add RED tests with a non-cooperative task, slow coordinator, pending event consumer and delayed pool close; assert total wall time is bounded by one grace plus CI tolerance, not grace multiplied by stages.
- [ ] Pass one absolute deadline through every shutdown stage and derive remaining time immediately before each await.
- [ ] Stop acceptance before cancellation; let task terminal events publish before dispatcher drain.
- [ ] Await all tracked coordinators and dispatcher; deadline expiry records their stage names instead of dropping join handles.
- [ ] Wrap pool close in timeout using remaining deadline.
- [ ] Make repeated shutdown calls idempotent and return a stable already-finished report.
- [ ] Ensure Tauri close warning uses the report and does not bootstrap a second AppRuntime.

## Tests

```bash
cargo test -p assetiweave backend::runtime::tests -- --nocapture
cargo test -p assetiweave backend::events -- --nocapture
cargo test -p assetiweave shutdown -- --nocapture
```

## Delete proof

```bash
rg -n 'detach|join\.take\(\).*timed_out|pool\(\)\.close\(\)' src-tauri/src/backend/runtime src-tauri/src/backend/events
```

通过：无 detach 路径；pool close 位于 deadline timeout 内；总时长测试稳定。

**Stop:** 不通过扩大 grace 或测试 sleep 容差掩盖阶段重复计时；先修正绝对 deadline 传播。

**Commit:** `refactor(runtime): 统一应用关闭截止时间`

