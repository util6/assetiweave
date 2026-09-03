# A-R04：thiserror 错误派生、source 保留与受限 anyhow

**Depends:** A-R03
**Contracts:** C-BASE、C-ERROR
**Outcome:** 删除机械 Error/Display 样板；保留错误源并维持 WireError 契约。

## 执行规则

状态：`PLANNED`。先读总入口、本卡 Contract IDs、`../02-dependencies.md`、`../05-playbook.md`。一轮只做本卡。原有正确行为先 characterization green；随后新增 adoption/deletion guard 得到 red，再迁移。筛选测试先 `-- --list`，零测试不算 green。只用临时目录/内存库/loopback fixture；本卡不授权插件架构或真实用户数据操作。

## 文件

- Modify：`src-tauri/src/backend/ai_execution/error.rs`、`agents/process.rs`、`agents/registry.rs`、`agents/types.rs`、`agents/protocol/acp.rs`、`agent_market/catalog.rs`、`agent_market/installers/mod.rs`、`agent_market/types.rs`；`runtime/error.rs`、`runtime/config.rs`；`src-tauri/Cargo.toml`、`Cargo.lock`。
- Test：上述错误类型内联测试；`runtime/error.rs::tests`、`runtime/config.rs::tests`。Create：无。
- 只迁移各文件的 Error enum/struct、Display/Error impl 和错误构造；本卡不改变 ACP/Agent 执行算法。

## Consumes / Produces

Consumes（已有）：`AppError::view(&self) -> WireError`、`AppError::code(&self) -> String`、`WireError { code, message, retryable, details }`；`AppError::Io(std::io::Error)`、`AppError::Db(sqlx::Error)` 已有 `#[from]`。

Produces：全部既有公有/模块可见函数签名保持；`AiExecutionError`、`ManagedAgentProcessError`、`AgentProbeError`、`AgentRegistryError`、`AgentDefinitionError`、`AcpError`、`CatalogError`、`InstallError`、`AgentMarketError` 使用 `#[derive(thiserror::Error)]`。`RuntimeConfig::from_environment` 内部可用 `anyhow::Context` 获取启动缺省目录诊断；不对外返回 anyhow。

## 步骤

- [ ] 为每个被迁移类型选各错误variant的 Display 样本，并用既有 `to_view/view` 快照确认 code/retryable/details，先 green；不把手写Display生成文字当作可随意修改的提示文案。
- [ ] 新增 derive/source gate；derive缺口测试用编译契约、source用运行断言：

```rust
#[test]
fn io_error_keeps_source_while_wire_message_is_sanitized() {
    use std::error::Error;
    let error = AppError::from(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied, "/private/token-file"));
    assert!(error.source().is_some());
    let wire = error.view();
    assert_eq!(wire.code, "storage_error");
    assert!(!wire.message.contains("token-file"));
}
#[test]
fn ai_execution_error_uses_derive_instead_of_manual_error_impl() {
    let source = include_str!("../ai_execution/error.rs");
    assert!(!source.contains(concat!("impl fmt::Display for ", "AiExecutionError")));
    assert!(source.contains("thiserror::Error"));
}
```

第一个测试可在旧实现已绿，是保真测试；第二个在旧实现必须红。

- [ ] 按旧 match 分支逐个转换为 `#[error(...)]`；例如保持 InstallError 文案时从其当前 Display 原样复制格式串。保留 domain `to_view()` 的分类与脱敏，禁止用 Display 直接序列化对外。

```rust
#[derive(Debug, thiserror::Error)]
pub(crate) enum CatalogError {
    // 以现有同名variant、字段、原Display文字逐项写 #[error]，
    // 实施时保留真实variant集合；不引入新的Catalog错误模型。
}
```

上块仅说明属性落点，执行时使用文件内已经存在的 variant，不创建空enum。关键可直接应用的已存在源错误模式是：

```rust
#[error("{0}")]
Io(#[from] std::io::Error),
#[error("{0}")]
Db(#[from] sqlx::Error),
```

- [ ] 手动 Error::source 实现若存在，将其映射到对应字段 `#[source]`；如 error variant 当前只有 String，就保留公开分类，不假称已能还原丢失源。只对构造点仍有原始 `io::Error/sqlx::Error` 的路径移除多余 `to_string`；若这会改变 `external_error` 为 `storage_error`，保持原外部code并停止扩大该替换。
- [ ] anyhow 限于 `RuntimeConfig::from_environment` 内部启动defaults装配，使用 `dirs::home_dir().context("system home directory is unavailable")?` 和 data_dir 对应Context；边界记录诊断链后转换现有 AppError，调用者仍拿 typed AppResult。
- [ ] 删除每个已迁移类型的手写 Display 与空 Error impl；保留 ID 等非错误类型的 Display，不做全目录正则删除。

## 验证

```bash
cargo test -p assetiweave --lib backend::runtime::error
cargo test -p assetiweave --lib backend::ai_execution
cargo test -p assetiweave --lib backend::agents
cargo test -p assetiweave --lib backend::agent_market
cargo test -p assetiweave --lib runtime::config::tests
cargo fmt --all -- --check
```

成功：derive guard red→green；既有错误code/retryable/序列化与取消分类不变；源错误测试命中；anyhow只出现在批准的启动内部边界。停止：类型派生涉及 Clone/Eq 的实质契约变化、source字段丢失、计划之外的大量 AppService→anyhow 替换。

[官方：thiserror](https://docs.rs/thiserror/latest/thiserror/)；[anyhow Context](https://docs.rs/anyhow/latest/anyhow/trait.Context.html)
