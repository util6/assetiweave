# A-R03：用 tracing 接管日志写入和上下文

**Depends:** A-R02
**Contracts:** C-BASE、C-CONFIG、C-ERROR
**Outcome:** 普通日志经 tracing/subscriber/appender 输出，旧逐条开文件写入器退出；日志查看和紧急 panic 保底继续可用。

## 执行规则

- 状态：`PLANNED`。本文件只授权本卡范围；实施前读取 `../00-execution-router.md`、本卡 Contract IDs、`../02-dependencies.md` 和 `../05-playbook.md`。
- 版本/features 只取依赖锁定表；本卡列出的新签名是**待创建接口**，不是已实现事实。
- 原有正确行为先 characterization green；随后运行本卡 adoption/deletion guard 得到 red，再替换实现。编译错误只可证明新接口未创建，不能冒充行为回归证据。
- 测试只用临时目录、内存库或 loopback fixture；不读写真实用户数据库。筛选测试先 `-- --list` 确认命中；零测试不算 green。
- 结束只交付本卡证据与下一张 ready 卡；任务二由独立目录的入场门控制。

## 文件

- Create：`src-tauri/src/backend/logging.rs`（初始化、Guard、内联测试）。
- Modify：`backend/mod.rs`、`backend/logs.rs`、`backend/operation_log.rs`、`src-tauri/src/lib.rs`、`src-tauri/Cargo.toml`、`Cargo.lock`。
- Test：`backend/logging.rs::tests`、`backend/logs.rs::tests`；`frontend/src/utils/logViewer.test.ts`、`frontend/src/utils/logViewer.ts` 仅在补充历史/新格式样本时修改。
- 本卡只替换已有 `operation_log` 的日志机制；其资产/来源字段组装属于领域薄适配，保留原调用签名，避免同时编辑十四个业务模块。

## 接口

Consumes：`runtime_config() -> AppResult<Arc<RuntimeConfig>>`；已有 `OperationLogLevel`、`LogField = (&'static str, String)`。

Produces（创建）：

```rust
pub(crate) struct LoggingGuard {
    _worker: tracing_appender::non_blocking::WorkerGuard,
}
pub(crate) fn init_logging(
    config: &crate::backend::runtime::config::RuntimeConfig,
) -> crate::backend::runtime::AppResult<LoggingGuard>;
```

初始化每个生产进程一次，Guard 由 `run`/`run_engine_stdio`/MCP入口栈持有到正常退出。显式 `process::exit` 分支先 drop guard；不把 Guard 存在临时局部初始化函数。测试使用局部 subscriber，不重复设置全局 subscriber。

## 步骤与代码

- [ ] 跑现有 `logs::tests` 和 `logViewer` 测试，留存旧日志转义、文件选择、panic备用路径行为 green。
- [ ] 添加删除 guard 得到 red：

```rust
#[test]
fn ordinary_logs_no_longer_open_file_for_each_event() {
    let source = include_str!("logs.rs");
    assert!(!source.contains(concat!("fn append_app_", "log_line(")));
    assert!(!source.contains(concat!("fn format_operation_", "log_line(")));
}
```

- [ ] 用官方 writer 和 subscriber，关闭 ANSI/target，保留第一列时间第二列级别供 UI 解析。时间格式选默认 RFC3339；库负责转义字段：

```rust
let file = std::fs::OpenOptions::new()
    .create(true).append(true).open(config.log_dir.join("app.log"))?;
let (writer, worker) = tracing_appender::non_blocking::NonBlockingBuilder::default()
    .lossy(false).finish(file);
tracing_subscriber::fmt()
    .with_ansi(false).with_target(false).with_writer(writer)
    .with_max_level(tracing::Level::INFO)
    .try_init().map_err(crate::backend::runtime::AppError::external)?;
let guard = LoggingGuard { _worker: worker };
```

- [ ] 初始化前创建 log_dir。`record_operation` 将三种 Level 明确 match 到 `tracing::info!/warn!/error!`；operation和fields作为结构化字段；message 先经既有 sanitize_log_text，动态 operation/字段名经 sanitize_log_key（不是依赖 fmt 自动清洗 Display 字符串）。保持既有字段清洗策略，尤其单事件不出现未转义换行；领域字段组装函数不重写。

```rust
tracing::info!(target: "assetiweave.operation",
    operation = operation, fields = ?fields, "{}", message);
```

- [ ] 删除普通 `write_operation_log_to_dir/append_app_log_line/format_operation_log_line`。`logs_write_operation` 保留输入校验并走同一日志入口；该接口表示日志已接受，测试不将其解释成 fsync。磁盘写失败的用户可见语义若有明确调用方断言则停止并交回契约裁定，不另造ack协议。
- [ ] `write_fatal_panic_log` 与备用目录保留：初始化失败或崩溃时仍有独立保底，不能依赖被破坏的异步队列。日志枚举、tail、打开目录不迁移到 logging.rs。
- [ ] 新测试用临时文件、局部 subscriber 写事件后 drop worker，断言文件有 operation、文本、级别，且每事件一行；历史日志样本和新格式都能被 `filterLogContent(content, level)` 现有API识别。
- [ ] 对 Engine 单独运行协议测试，stdout 不含日志；正常退出前 Guard flush 后文件有最后一条事件。默认先不增加日志轮转策略。

## 验证

```bash
cargo test -p assetiweave --lib backend::logging::tests -- --nocapture
cargo test -p assetiweave --lib backend::logs::tests
cargo test -p assetiweave --lib engine::transport -- --test-threads=1
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/utils/logViewer.test.ts
cargo fmt --all -- --check
```

成功：新Guard测试和旧读取测试均命中；普通每次open/手写格式化函数删净；panic保底保留；不新增第二套日志写入管线。停止：生产多入口重复全局初始化、Engine stdout污染、既有日志接口依赖逐条文件写错误而未裁定。

[官方 API：tracing-appender](https://docs.rs/tracing-appender/latest/tracing_appender/non_blocking/index.html)
