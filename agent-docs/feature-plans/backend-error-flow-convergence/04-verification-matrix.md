# 错误链 Verification Matrix

| Gate | 行为证据 | 删除/结构证据 |
|---|---|---|
| G-ERR-00 | guard self-test 对新增 String boundary 变 RED | 分类表覆盖全部基线命中 |
| G-ERR-D01 | SQLx/JSON failure 得到正确 code/source | 目标 store/codec 不再映射为 External String |
| G-ERR-A01 | Agent Market Tauri/Engine error parity | 跨模块 `Result<T,String>` 归零 |
| G-ERR-C01 | Projection/LogSnapshot 保持用户行为并返回 typed error | 目标模块跨层 String result 归零 |
| G-ERR-E01 | 全部规范 code/retryable/message/details parity | 基础设施 String variant 与普通 Domain escape hatch 收紧 |
| G-ERR-G01 | 全量 Rust/frontend/Go/CLI 通过 | 全局命中只剩已列明私有 parser/test/outer transport |

每卡至少运行：

```bash
cargo fmt --all -- --check
cargo check -p assetiweave
cargo test -p assetiweave --lib
pnpm check:boundaries
```

公开错误或 Engine handler 变化时追加：

```bash
pnpm cli:contract
git diff --exit-code -- cli/internal/schema/contract.json
pnpm gen:surface-matrix
pnpm check:surface-matrix
go vet -C cli ./...
go test -C cli -race ./...
pnpm cli:test:e2e
```
