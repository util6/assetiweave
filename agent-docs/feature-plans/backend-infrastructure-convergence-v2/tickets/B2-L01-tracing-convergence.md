# B2-L01：tracing span、rolling 与生产 consumer 收口

**Objective:** 完成 tracing 第二阶段，让生产日志不再依赖手工字段 façade。

**Contracts:** C-LOG-01、C-ERROR-01、C-PRODUCT-01。

**Canonical authority after this card:** tracing subscriber/event/span 负责记录；LogSnapshot 只负责读取产品日志。

**Files:**

- Modify: `src-tauri/src/backend/logging.rs`
- Modify: `src-tauri/src/backend/logs.rs`
- Delete after migration: `src-tauri/src/backend/operation_log.rs`
- Modify: production callers returned by the logging locator command.
- Modify: `src-tauri/src/lib.rs` for guard lifetime and stdout/stderr routing.

## Steps

- [ ] Add RED tests using a local subscriber writer for task/tenant/operation span inheritance, single-line structured output, filter, rolling file discovery, redaction and Engine stdout isolation.
- [ ] Configure `EnvFilter` from the existing runtime config/default, with deterministic INFO fallback.
- [ ] Replace direct append file with `tracing_appender::rolling` and retain a process-lifetime non-blocking `WorkerGuard`.
- [ ] Define spans at AppRuntime task/workflow boundaries so nested events inherit tenant/task/operation; use direct typed fields for source/asset/profile IDs.
- [ ] Migrate every production `log_info/log_warn/log_error/record_*` consumer to tracing macros or `#[instrument]` with explicit field policy.
- [ ] Keep panic emergency writer independent of the non-blocking worker and keep LogSnapshot able to enumerate/read current and rolled managed files.
- [ ] Route all Engine/MCP protocol stdout exclusively through transport writers; tracing uses file/stderr subscriber.
- [ ] Delete `operation_log.rs`, `OperationLogLevel` and generic fields vector façade after last consumer.

## Tests

```bash
cargo test -p assetiweave backend::logging -- --nocapture
cargo test -p assetiweave backend::logs -- --nocapture
cargo test -p assetiweave adapters::engine::transport -- --nocapture
cargo test -p assetiweave adapters::engine -- --nocapture
```

## Delete proof

```bash
rg -n '\b(log_info|log_warn|log_error|record_info|record_warn|record_error|record_operation|OperationLogLevel)\b' src-tauri/src --glob '*.rs'
test ! -f src-tauri/src/backend/operation_log.rs
```

通过：第一条无生产命中；第二条成功；snapshot/rolling/stdout/redaction tests 绿。

**Stop:** 不删除 LogSnapshot、打开目录或 panic 落盘；它们是产品能力，不是 tracing 重复实现。

**Commit:** `refactor(logging): 完成 tracing 生产链路收口`

