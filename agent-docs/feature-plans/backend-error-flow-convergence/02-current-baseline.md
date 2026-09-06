# 当前错误链基线

- Revision：`b39932fe124781973fadfbd9f99ddfc5dc1efdcf`
- `AppResult<T>`：`Result<T, AppError>` 标准类型别名。
- `AppError`：已使用 `thiserror::Error`，并保留 `sqlx::Error`、`io::Error` source。
- `WireError`：已有稳定 `code/message/retryable/details` 与公开脱敏测试。

## 当前计数

```text
Result<..., String> signature lines in backend/adapters: 83
map_err(AppError::external) in backend: 984
AppError::External(...to_string()) in backend: 77
map_err(...to_string()) in backend: 301
```

重放：

```bash
rg -n --pcre2 '(?:std::result::)?Result<[^\n]*,\s*String\s*>' src-tauri/src/backend src-tauri/src/adapters --glob '*.rs'
rg -n -F 'map_err(AppError::external)' src-tauri/src/backend --glob '*.rs'
rg -n --pcre2 'AppError::External\([^\n]*\.to_string\(\)' src-tauri/src/backend --glob '*.rs'
rg -n --pcre2 'map_err\([^\n]*\.to_string\(\)' src-tauri/src/backend --glob '*.rs'
```

## 优先迁移范围

1. SQLx row/codec 错误被转成 `External(String)`。
2. Agent Market repository、runtime、cache、distribution 跨模块返回 `String`。
3. LogSnapshot 与 Conversation projection 跨模块返回 `String`。
4. `AppError` 的 String 型基础设施 variant 和 `Domain` escape hatch。

测试 helper、进程最外层 stdout/stderr transport 和立即消费的私有 parser 必须分类后才可保留；“改动太多”不是保留原因。
