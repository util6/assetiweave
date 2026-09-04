# 当前基线与再审计命令

> 本页是 2026-09-04 的快照，不是长期 Authority。每张卡以当轮 Preflight 输出为准。

## 审计快照

- 分支：`refactor/ecosystem-task-1`
- 审计 HEAD：`3fba5429`
- 审计时工作树已有用户未提交修改；执行者不得覆盖或重置。
- `Database` 同时持有 `SqlitePool` 与 Tokio `Runtime`，并暴露 `block_on/run_sync`。
- `AppRuntime` 仍暴露重复同步桥；backend 生产与测试代码有大量 `.block_on()`/`.run_sync()`。
- TaskRuntime 已使用 `TaskTracker`、`CancellationToken`，活动计数与错误派生基本完成。
- Event Dispatcher 仍使用线程、`Condvar`、手工 retry/wake/completion，超时路径可 detach worker。
- `host_process` 约 1022 行，仍承担 PATH、reader thread、轮询、Unix process group 和 Windows `taskkill`。
- Settings 核心约 45 处 `.get()`；完整文档仍以 `serde_json::Value` 为主。
- tracing 三件套已安装，直接 tracing 宏调用极少，旧日志 façade 仍有大量生产 consumer。
- store 层约 2 个 `FromRow`、14 个 `query_as`、640 个 `try_get`。
- `AppError` 同时保留 `Canceled` 与 `Cancelled`。
- 最近验证基线：Rust 格式通过、workspace 859 tests 通过、目标测试 76 tests 通过、`cargo check` 通过但约 98 warnings。数字随代码变化，只作审计起点。

## 每轮 Preflight

```bash
git status --short --branch
git rev-parse HEAD
gh issue view 24 --comments
cargo metadata --no-deps --format-version 1 >/dev/null
```

完成标准：输出被复制到工作卡；确认本卡路径不与用户未提交修改重叠。

## 基础设施计数

```bash
printf 'block_on_run_sync='; rg '\.(block_on|run_sync)\(' src-tauri/src --glob '*.rs' | wc -l
printf 'settings_get='; rg '\.get\(' src-tauri/src/backend/app_settings.rs | wc -l
printf 'tracing_direct='; rg 'tracing::(trace|debug|info|warn|error)!|\b(trace|debug|info|warn|error)!' src-tauri/src --glob '*.rs' | wc -l
printf 'legacy_log_calls='; rg '\b(log_info|log_warn|log_error|record_info|record_warn|record_error)\b' src-tauri/src --glob '*.rs' | wc -l
printf 'fromrow='; rg 'derive\([^)]*FromRow|sqlx::FromRow' src-tauri/src/backend/store --glob '*.rs' | wc -l
printf 'query_as='; rg 'query_as' src-tauri/src/backend/store --glob '*.rs' | wc -l
printf 'try_get='; rg 'try_get' src-tauri/src/backend/store --glob '*.rs' | wc -l
wc -l src-tauri/src/backend/host_process.rs
```

完成标准：B2-00 把输出作为 Issue #24 首条 baseline 评论；后续卡只记录与自己有关的计数变化。

## Runtime 分布

```bash
rg -n 'tokio::runtime::Runtime|build_runtime|\.block_on\(|\.run_sync\(' src-tauri/src --glob '*.rs'
rg -n 'Condvar|std::thread|thread::|detach|JoinHandle' src-tauri/src/backend/events src-tauri/src/backend/runtime --glob '*.rs'
```

完成标准：每个命中归属到 B2-R01 至 B2-R09 的一张卡；未知命中在 B2-00 更新 ticket map 后再施工。

## 依赖与平台基线

```bash
cargo tree --duplicates
cargo tree -p assetiweave -e features
cargo info process-wrap@10.0.0
cargo info which@8.0.6
cargo info directories@6.0.0
cargo info camino@1.2.5
```

完成标准：确认四个候选支持 Rust 1.96；P01/F01 仍需本地行为 fixture，crate metadata 不是采用证据。

## 测试基线

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo check -p assetiweave --message-format=short 2>&1 | tee /tmp/assetiweave-cargo-check.txt
pnpm check:boundaries
pnpm check:surface-matrix
go vet -C cli ./...
go test -C cli -race ./...
```

完成标准：B2-00 记录通过/失败与确切数量。失败不得用“与本卡无关”直接忽略；先证明是否为已存在基线。

