# B2-R02：Tokio Event Dispatcher 接管调度

**Objective:** 保留 durable outbox 语义，删除 dispatcher 的线程、Condvar、手工 completion 和 detach。

**Contracts:** C-EVENT-01、C-SHUTDOWN-01、C-RUNTIME-02。

**Canonical authority after this card:** `EventDispatcher` 是 async worker；`EventDispatcherHandle` 持有 cancellation、Notify 与受追踪 Tokio task。

**Interfaces:**

- Convert dispatcher initialization, dispatch, retention and pending-count functions to async over `SqlitePool`.
- Keep the object-safe consumer registry with `type ConsumerFuture<'a> = Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + 'a>>`; `backfill` and `handle` return this future instead of adding another runtime.
- `start` must run on a provided runtime handle/tracker and return a handle whose `stop_until(deadline)` is async.
- `notify` remains synchronous and lock-free from caller perspective.

**Files:**

- Modify: `src-tauri/src/backend/events/dispatcher.rs`
- Modify: `src-tauri/src/backend/events/mod.rs`
- Modify: `src-tauri/src/backend/events/consumers.rs`
- Modify: `src-tauri/src/backend/events/tests.rs`
- Modify: `src-tauri/src/backend/runtime/app_runtime.rs`
- Modify: `src-tauri/src/backend/runtime/tests.rs`

## Steps

- [ ] Add RED tests proving Notify interrupts idle wait, cancellation interrupts retry sleep, a failed consumer does not block a later consumer, and stop waits for the tracked worker without detach.
- [ ] Change `ConsumerCx` to use cloned `SqlitePool`; convert Session Memory SQL directly to await. Until R08 migrates search rebuild, its consumer may use one bounded `spawn_blocking` adapter whose ownership is recorded in the R01 allowlist.
- [ ] Replace `WakeSignal` with `tokio::sync::Notify`; replace retry timing with `tokio::time::sleep_until` inside `select!` with notify and cancellation.
- [ ] Replace thread `JoinHandle` and custom Completion with tracked Tokio task completion.
- [ ] Keep per-consumer retry state and exponential cap; shutdown clears backoff once, performs final drain and reports failure/remaining rows.
- [ ] Start dispatcher only for ResidentHost through AppRuntime's existing task execution handle; OneShot remains without dispatcher.
- [ ] Remove all dispatcher `Database::block_on/run_sync` calls by awaiting SQLx directly.

## Tests

```bash
cargo test -p assetiweave backend::events -- --nocapture
cargo test -p assetiweave backend::runtime::tests -- --nocapture
```

## Delete proof

```bash
rg -n 'Condvar|std::thread|thread::|WakeSignal|struct Completion|join: Option<thread::JoinHandle' src-tauri/src/backend/events
```

通过：无生产命中；所有 durable event tests 仍绿。

**Stop:** process-wide async handle 无法确定时只使用 AppRuntime 已有 TaskRuntime handle，不新建第二个 Runtime；consumer trait 需要全仓 async 迁移时先拆 consumer adapter 子卡。

**Commit:** `refactor(events): 使用 Tokio 统一事件派发调度`
