# B2-P02：生产 HostProcess 切换到 process-wrap

**Objective:** 用 P01 已验证组合接管进程树生命周期并删除手工 OS supervisor。

**Contracts:** C-PROCESS-01、C-SHUTDOWN-01、C-ERROR-01。

**Canonical authority after this card:** 一个 async HostProcess core 负责 spawn/read/timeout/cancel/cleanup；process-wrap 负责 OS 进程树。

**Files:**

- Modify: `src-tauri/src/backend/host_process.rs`
- Modify: `src-tauri/src/backend/agents/process.rs`
- Modify: `src-tauri/src/backend/extension_kernel/launcher.rs`
- Modify: affected `ai_execution` and `agent_market` process consumers.
- Modify: process fixture and lifecycle tests.

## Steps

- [ ] Add RED integration tests at HostProcess public seam for the P01 matrix, including cleanup report and stable AppError mapping.
- [ ] Build `process_wrap::tokio::CommandWrap` with the exact platform wrappers proven in P01.
- [ ] Use Tokio async pipe readers with explicit caps while continuing to drain after cap; coordinate child wait and both readers under one deadline.
- [ ] Use `CancellationToken` directly in `select!`; pre-cancel returns without spawn, mid-flight cancel terminates the process tree and reaps.
- [ ] Switch `ManagedAgentProcess` and Extension launcher to the same child lifecycle; preserve stderr tail/watch behavior as a domain projection, not a second supervisor.
- [ ] Replace standard executable search with `which`; retain AssetIWeave desktop fallback after a miss.
- [ ] Map missing program, spawn, timeout, cancel, output limit, nonzero exit and cleanup failure without `to_string()` reclassification.
- [ ] Delete hand-written process group/session setup, Windows taskkill tree logic, 10ms polling and duplicate reader threads.

## Tests

```bash
cargo test -p assetiweave host_process -- --nocapture
cargo test -p assetiweave agents::process -- --nocapture
cargo test -p assetiweave extension_kernel -- --nocapture
cargo test -p assetiweave ai_execution -- --nocapture
```

## Delete proof

```bash
rg -n 'taskkill|setpgid|killpg|CREATE_NEW_PROCESS_GROUP|AtomicBool|thread::sleep\(Duration::from_millis\(10\)\)|stdout_reader = thread::spawn|stderr_reader = thread::spawn' src-tauri/src/backend/host_process.rs src-tauri/src/backend/agents/process.rs
```

通过：旧 OS supervisor 零命中；P01+production consumer tests 全绿；Windows CI 全绿。

**Stop:** 不保留“旧实现 fallback”；平台 wrapper 失败时回到 P01 修订，不能双轨上线。

**Commit:** `refactor(process): 使用成熟库接管进程树生命周期`

