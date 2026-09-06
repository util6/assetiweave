# B2-G01：第二阶段完整验收与交付

**Objective:** 用行为、删除、跨 surface、跨平台和依赖证据确认 Issue #24 已真实完成。

**Contracts:** 全部 Contract。

**Files:**

- Modify only stale execution status: this plan package.
- Modify: Issue #24 comments/status through `gh`.
- No production refactor; 发现失败回到 owner 卡。

## Steps

- [x] 逐条读取 `01-contract.md`，为每个 Contract 引用一条行为测试和一条删除/保留证据。
- [x] 运行 Runtime、Event、HostProcess、Settings、Log、Path 和 Store 目标测试。
- [x] 运行全局删除查询，确认 backend runtime bridge、dispatcher thread/Condvar/detach、手工进程树和旧日志 façade 符合零命中要求。
- [x] 运行 G-FULL；记录每个命令、test count、warning count 和 exit code。
- [x] 使用临时路径运行桌面 startup self-check；运行 CLI-to-Engine e2e，确认协议 stdout 没有日志污染。
- [x] 检查最近一次 Windows CI：完整 workspace tests 包含 process-wrap Job Object fixture 且通过。
- [x] 运行 cargo audit、重复依赖和 feature 检查；确认新增依赖均有生产 owner 和删除收益。
- [x] 核对 #1、#2、#22：本 Issue 不反向修改其历史，相关残余项以 #24 当前证据为准。
- [x] 在 Issue #24 发布最终验收矩阵；任一 Contract 为 incomplete/contradicted/missing evidence 时保持 OPEN。

## Global delete queries

```bash
rg -n 'tokio::runtime::Runtime|build_runtime|\.block_on\(|\.run_sync\(' src-tauri/src/backend --glob '*.rs'
rg -n 'Condvar|WakeSignal|struct Completion|detach' src-tauri/src/backend/events
rg -n 'taskkill|setpgid|killpg|stdout_reader = thread::spawn|stderr_reader = thread::spawn' src-tauri/src/backend/host_process.rs src-tauri/src/backend/agents/process.rs
rg -n '\b(log_info|log_warn|log_error|record_info|record_warn|record_error|record_operation|OperationLogLevel)\b' src-tauri/src --glob '*.rs'
rg -n '\b(Canceled|Cancelled)\b' src-tauri/src --glob '*.rs'
```

通过：前三/四条为零；取消查询只含单一规范 variant；Settings/Path/SQLx 的保留命中与 Contract 原因已列入最终评论。

## Full verification

```bash
pnpm typecheck
pnpm lint --quiet
pnpm format:check
pnpm test
pnpm build
cargo fmt --all -- --check
cargo test --workspace
pnpm check:boundaries
pnpm cli:contract
pnpm gen:surface-matrix
pnpm check:surface-matrix
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
cargo audit
```

## Startup self-check

```bash
check_root="$(mktemp -d)"
ASSETIWEAVE_DB_PATH="$check_root/app.db" \
ASSETIWEAVE_LOG_DIR="$check_root/logs" \
cargo run -p assetiweave --bin assetiweave -- --startup-self-check
rm -rf "$check_root"
```

## Final status

- 全部通过：评论 `Status: VERIFIED`，关闭 Issue #24，并提交 `docs(agent): 记录后端基础设施第二阶段验收`（仅在文档有变更时）。
- 任一失败：评论 `Status: INCOMPLETE`，列出失败 Contract、命令和 owner 卡；停止，不关闭 Issue。
