# SPEC-BA-01：AppRuntime、结构化错误与依赖边界收口

- 状态：Proposed v1
- 优先级：P0/P1
- 前置：SPEC-BA-00
- 交付物：唯一 `AppResult`、transport error view、Runtime bootstrap 下沉、边界守卫

## 1. 当前证据

### 1.1 AppError 未接管 Application

- `backend/runtime/error.rs` 定义 `AppResult<T> = Result<T, AppError>`。
- `backend/dto/types.rs` 仍定义 `AppResult<T> = Result<T, String>`。
- `backend/application/prelude.rs` 从 DTO 导入旧别名。
- 审计基线中 Application 约有 276 处相关返回签名，24 个模块使用 prelude。
- `check-module-boundaries.sh` 只禁止显式 `Result<..., String>`，别名可绕过守卫。

### 1.2 Runtime 反向依赖 Application

`AppRuntime::bootstrap` 调用
`application::bootstrap::materialize_and_seed_builtin_adapters`，违反
Runtime → Infrastructure、Application → Runtime 的目标方向。

### 1.3 同步桥仍是全局计数基线

`check_max 333 'block_on' src-tauri/src` 只能限制总数，不限制新增位置；删除 A 模块一处后，
在 B 模块新增一处仍可能通过。

## 2. 目标接口

### 2.1 唯一 AppResult

```rust
// backend/runtime/error.rs
pub(crate) type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error("operation conflicts with active state: {0}")]
    Conflict(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error("operation timed out: {0}")]
    Timeout(String),
    #[error("storage failure: {0}")]
    Storage(String),
    #[error("host process failure: {0}")]
    Process(String),
    #[error("extension failure: {0}")]
    Extension(String),
    #[error("external service failure: {0}")]
    External(String),
    #[error("legacy failure: {0}")]
    Legacy(String),
}
```

要求：

- `dto` MUST NOT 定义结果别名。
- `application/prelude.rs` MUST 从 `runtime` 导入 `AppError/AppResult`。
- 新增业务方法 MUST NOT 返回裸 `String` 错误。
- `Legacy` 只允许出现在明确登记的迁移清单中，数量必须单调下降。

### 2.2 Transport error

Transport 不得把 `AppError` 的 Debug 文本直接作为协议。定义稳定 view：

```rust
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: Option<serde_json::Value>,
}
```

映射规则：

| AppError | code | retryable |
|---|---|---:|
| Validation | `validation_error` | false |
| NotFound | `not_found` | false |
| Conflict | `conflict` | true，除非领域明确不可重试 |
| Cancelled | `cancelled` | true |
| Timeout | `timeout` | true |
| Storage | `storage_error` | true |
| Process | `process_error` | 由子类型决定 |
| Extension | `extension_error` | 由子类型决定 |
| External | `external_error` | true |
| Legacy | `legacy_error` | false |

领域已有更具体错误 view（如 Agent Market）MAY 保留，但转换入口必须唯一，且不得先
`to_string()` 再反向解析错误码。

## 3. 迁移策略

### 3.1 垂直切片顺序

必须按完整调用链迁移，禁止只改函数签名后在边界统一 `.map_err(String::from)`：

1. Agent Market preview/lifecycle。
2. source scan 与 mount workflow。
3. conversation package lifecycle。
4. Memory 与 AI workflow。
5. 其余 CRUD/read workflow。

每个切片必须同时覆盖：

```text
Store/Host error → Domain/Application AppError → Tauri/Engine WireError → Frontend error display
```

### 3.2 Legacy 清单

在边界脚本旁维护精确 allowlist，而不是只有数量：

```text
path|symbol|owner|removal_task
```

新增 `AppError::Legacy` 使用必须导致 CI 失败。删除一项时同步删除 allowlist 行。

### 3.3 禁止的转换

```rust
// MUST NOT
some_typed_error.map_err(|e| e.to_string())?;

// MUST NOT
AppError::Legacy(format!("{error:?}"));

// MUST
some_operation().map_err(AppError::from)?;
```

无法直接 `From` 的领域错误必须有命名转换函数，并保留稳定分类。

## 4. Runtime bootstrap 分层

### 4.1 目标模块

```text
backend/bootstrap/
├── mod.rs
├── prepared_assets.rs
└── startup.rs
```

职责：

- 在 DB transaction 之外准备内置文件资产。
- 调用 Store 的纯持久化 seed API。
- 不依赖 Tauri。
- 不导入 `backend/application`。

`AppRuntime::bootstrap` 可以调用 `backend::bootstrap::startup`，但 Runtime MUST NOT 调用
Application。`AppService` 构造必须发生在 Runtime 建立之后。

### 4.2 Bootstrap 顺序

```text
build tokio runtime
→ open/migrate pool
→ ensure app-owned directories
→ prepare builtin assets outside transaction
→ seed defaults using prepared values
→ load request context
→ recover extension installations
→ build registry snapshots
→ construct AppRuntime
→ ResidentHost starts dispatcher
```

失败时不得发布半初始化的全局 Runtime。

## 5. block_on 政策

- `Database::block_on` 在迁移期 MAY 保留为同步 Application 的桥。
- Application 新增 `block_on` MUST 记录在按目录基线中。
- Tauri async command MUST NOT 在 async runtime worker 中直接执行长时间阻塞 I/O。
- HostProcess async、TaskRuntime worker、网络刷新必须使用 async API 或专用
  `spawn_blocking`，不得持有全局锁。

边界基线必须至少拆为：

```text
backend/application
backend/store
backend/runtime
adapters/tauri
adapters/engine
```

## 6. CI 边界规则

新增或强化以下检查：

```bash
# Application 不得导入旧 AppResult
rg 'dto::.*AppResult|dto::\{[^}]*AppResult' src-tauri/src/backend/application

# Runtime 不得依赖 Application
rg 'backend::application|crate::backend::application' src-tauri/src/backend/runtime

# Application 不得显式声明 String error result
rg 'Result<[^>]+,\s*String>' src-tauri/src/backend/application

# DTO 不得定义 AppResult
rg 'type AppResult' src-tauri/src/backend/dto
```

上述命令命中即失败；不得使用新的别名或 re-export 绕过文本检查。后续 SHOULD 用
`cargo metadata`/AST 工具补强，但不能删除简单守卫。

## 7. 测试要求

必须增加：

1. `application_error_preserves_validation_code_across_tauri_view`
2. `engine_error_preserves_same_code_as_tauri`
3. `legacy_error_never_exposes_debug_payload`
4. `runtime_bootstrap_does_not_import_or_construct_app_service`
5. `failed_bootstrap_does_not_publish_process_runtime`
6. `bootstrap_prepares_files_before_seed_transaction`

## 8. 验收标准

- `dto::AppResult` 不存在。
- Application 的公开 workflow 全部返回 `runtime::AppResult` 或领域 typed result。
- `AppError::Legacy` 只有 allowlist 中的明确兼容点。
- Runtime 目录对 Application 的引用数为 0。
- Tauri 与 Engine 对同一业务失败返回相同 `code/retryable`。
- 边界检查通过，且守卫能在故意加入旧别名/反向依赖的 fixture 中失败。
