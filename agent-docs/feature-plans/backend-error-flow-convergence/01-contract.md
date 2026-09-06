# 后端错误链收口 Contract

## C-RESULT-01 — 标准 Result

- `AppResult<T>` 继续是 `std::result::Result<T, AppError>` 的别名。
- `Ok`/`Err` 数量不作为质量指标；验收只检查错误类型、source 保真与边界语义。
- 纯传播使用 `?` 与 `From`，不写等价的 `match Ok/Err` 转发。

## C-SOURCE-01 — Typed Source

- SQLx、I/O、Serde、HostProcess、Extension 和领域错误保持原始 typed source。
- `map_err(AppError::external)`、`AppError::External(error.to_string())` 和 `map_err(|e| e.to_string())` 不得出现在能够静态分类的路径。
- repository 和 runtime 的跨模块公开方法不返回 `Result<T, String>`。

## C-APPERROR-01 — Application Taxonomy

- `AppError` 是 AppService 错误分类 Authority，不是日志字符串容器。
- Storage、Process、Extension 与已知领域错误使用 `#[from]`/`#[source]` 或显式 typed conversion。
- `External`/`Internal` 只接收不能合理归类的最外层故障，公开映射固定为安全 internal/external code。
- `Domain { code: String }` 不作为绕过稳定 taxonomy 的常规入口。

## C-WIRE-01 — Transport Parity

- `WireError` 是唯一公开错误 DTO。
- Adapter 显式完成 `AppError -> WireError`；内部错误类型不直接 Serialize 成不受控结构。
- validation、not-found、conflict、cancelled、timeout、storage、process、extension 与 internal/external 的 code、retryable 和安全 message 在 Tauri/Engine 间一致。
- details 只包含 allowlisted 结构，不包含绝对路径、SQL、token、secret、password、prompt 或 environment。

## C-CONTEXT-01 — Diagnostic Context

- 内部错误通过 source chain 与 tracing fields 保留 operation、tenant、task 和资源 ID。
- 用户路径进入内部日志前遵守 Issue #24 的生产脱敏策略。
- 增加上下文不得改变公开 message，也不得通过字符串拼接重建错误分类。

## C-SCOPE-01 — 范围

- 不替换 Rust `Result`，不引入 ORM，不重写所有领域错误。
- 不为减少 `Ok`/`Err` 计数而改变控制流。
- 不修改数据库 migration、公开成功 DTO、TaskRuntime 或 Agent 生命周期行为。
- 不把所有私有纯 parser 的局部 `Result<T, String>` 机械升级；只有跨模块、持久化、进程、协议或公开错误路径进入本轮。
