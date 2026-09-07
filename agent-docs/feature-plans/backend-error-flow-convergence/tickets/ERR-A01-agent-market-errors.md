# ERR-A01：Agent Market Typed Error

**Authority:** Agent Market repository、cache、distribution 和 runtime 使用一个领域 typed error family；AppService 完成唯一 AppError 转换。

**Files:**

- Modify/Test: `src-tauri/src/backend/agent_market/repository.rs`
- Modify/Test: `src-tauri/src/backend/agent_market/cache.rs`
- Modify/Test: `src-tauri/src/backend/agent_market/distribution.rs`
- Modify/Test: `src-tauri/src/backend/agent_market/runtime.rs`
- Modify: `src-tauri/src/backend/agent_market/types.rs`
- Modify/Test: `src-tauri/src/backend/application/agent_market.rs`
- Modify/Test only for parity: Engine/Tauri Agent Market adapters.

## Steps

- [x] 列出目标模块全部跨模块 `Result<T,String>`，按 repository、invalid catalog、artifact/distribution、runtime/protocol 分类。
- [x] 增加同一非法 Catalog、缺失 installation、SQL failure、artifact mismatch 和 process timeout 通过 AppService、Tauri、Engine 的 parity tests。
- [x] 断言每场景的 code、retryable、安全 message/details 和内部 source；运行并记录当前字符串错误的 RED。
- [x] 使用 `thiserror` 建立 Agent Market typed errors；repository 保留 SQLx source，Catalog validation 保留结构字段，process 保留 HostProcessError。
- [x] AppService 使用显式 `From` 转成 AppError；adapter 只消费 WireError，不解析 Display 文本。
- [x] 目标模块跨模块公开 `Result<T,String>` 归零；立即消费的私有 parser 可保留，但必须在交接列出。

## Verify

```bash
cargo test -p assetiweave backend::agent_market -- --nocapture
cargo test -p assetiweave agent_market_error_parity -- --nocapture
pnpm cli:contract
pnpm check:surface-matrix
pnpm cli:test:e2e
cargo fmt --all -- --check
```

提交：`refactor(errors): 类型化智能体市场错误链`
